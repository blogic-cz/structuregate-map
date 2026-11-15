//! ASYNC DISCIPLINE FOR RUST (`--async-discipline`), the rule C# has (`src/AsyncRules.cs`): inside an `async fn` or an
//! `async` block nothing may block the executor's thread. A blocking call in async code does not fail - it stalls every
//! task queued on that worker - and reads as ordinary code; no test shows it.
//!
//! A CALL IS JUDGED BY THE PATH IT RESOLVES TO, never by its spelling: the file's `use` lines (`use std::fs;`,
//! `use std::thread::sleep as nap;`, a glob) turn `fs::read(..)` into `std::fs::read`, so `tokio::fs::read` is never
//! one. Blocking: `std::thread::sleep`, anything in `std::fs`, `std::net::TcpStream::connect`, a `std::process::Command`
//! chain's `output()`/`status()`/`wait()`, `block_on` in any spelling, anything in a `blocking` module. A method whose
//! receiver no syntax can type is judged only where the async twin would be AWAITED: `recv()`, `join()` and `wait()`
//! with no argument and no `.await` after them (`std::sync::mpsc`, a thread's `JoinHandle`, a child process).
//! `std::sync::Mutex::lock` is NOT refused: a short critical section under a std mutex is the pattern tokio itself
//! recommends.
//!
//! A SYNC CLOSURE OR FN INSIDE ASYNC CODE IS SYNC: the body of `spawn_blocking(|| std::fs::read(p))` is exactly where
//! the blocking call belongs. `// async-ok` on the line or the line above waives it, so the reason sits beside it.

use proc_macro2::Span;
use std::collections::{BTreeSet, HashMap};
use syn::visit::Visit;

const WAIVER: &str = "async-ok";

/// A rust file's line count, as every other count of it is made, and its violations - the gate's answer for it.
pub fn measured(abs: &str, rel: &str) -> Result<super::counts::Answer, String> {
    let lines = crate::count::count(std::path::Path::new(abs), false, false).map_err(|e| e.to_string())?;
    let bytes = std::fs::read(abs).map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&bytes));
    Ok((lines as i64, inspect(rel, &text)))
}

/// Every violation of a rust file, `<rel>:<line>: <what> — <remedy> (`<source>`)` as the C# rule writes them. A file
/// the parser refuses has none here: its count already falls back, and a rule must not guess.
pub fn inspect(rel: &str, text: &str) -> Vec<String> {
    let Ok(file) = syn::parse_file(text) else { return vec![] };
    let mut names = Names::default();
    for item in &file.items {
        match item {
            syn::Item::Use(u) => {
                for import in crate::rsmap::paths::flatten(&u.tree) {
                    let full: Vec<String> = import.segs.iter().cloned().chain((!import.glob).then(|| import.leaf.clone())).collect();
                    match (import.glob, import.alias) {
                        (true, _) => names.globs.push(full),
                        (false, Some(alias)) => drop(names.alias.insert(alias, full)),
                        (false, None) => drop(names.alias.insert(import.leaf.clone(), full)),
                    }
                }
            }
            syn::Item::Fn(f) => drop(names.own.insert(f.sig.ident.to_string())),
            _ => {}
        }
    }
    let lines: Vec<&str> = text.lines().collect();
    let mut walk = Walk { names: &names, in_async: false, found: Vec::new() };
    walk.visit_file(&file);
    let mut out = Vec::new();
    for (span, what, remedy) in walk.found {
        let line = span.start().line;
        let waived = (line.saturating_sub(1).max(1)..=line).any(|l| lines.get(l - 1).is_some_and(|t| t.contains(WAIVER)));
        if !waived {
            let source: String = lines.get(line - 1).map(|t| t.trim()).unwrap_or("").chars().take(60).collect();
            out.push(format!("{rel}:{line}: {what} — {remedy} (`{source}`)"));
        }
    }
    proc_macro2::extra::invalidate_current_thread_spans();
    out
}

/// What a name in this file stands for: its `use` imports (by the name they bind), its globs, its own fns.
#[derive(Default)]
struct Names {
    alias: HashMap<String, Vec<String>>,
    globs: Vec<Vec<String>>,
    own: BTreeSet<String>,
}

impl Names {
    /// The path a written path resolves to: its first segment through an import, a bare name through a glob that
    /// names a blocking item (an own fn shadows a glob), `crate::`-rooted and unknown names as written.
    fn resolve(&self, path: &syn::Path) -> Vec<String> {
        let segs: Vec<String> = path.segments.iter().map(|s| s.ident.to_string()).collect();
        if path.leading_colon.is_some() || segs.is_empty() {
            return segs;
        }
        if let Some(full) = self.alias.get(&segs[0]) {
            return full.iter().cloned().chain(segs[1..].iter().cloned()).collect();
        }
        if segs.len() == 1 && !self.own.contains(&segs[0]) {
            for glob in &self.globs {
                let candidate: Vec<String> = glob.iter().cloned().chain(segs.iter().cloned()).collect();
                if blocking_path(&candidate).is_some() {
                    return candidate;
                }
            }
        }
        segs
    }
}

/// Why a resolved path blocks, and what to use instead - None when it does not.
fn blocking_path(full: &[String]) -> Option<(&'static str, &'static str)> {
    let at = |n: usize| full.get(n).map(String::as_str).unwrap_or("");
    let std = matches!(at(0), "std" | "core");
    if std && at(1) == "thread" && at(2) == "sleep" {
        return Some(("std::thread::sleep inside an async body holds the executor's thread", "await tokio::time::sleep (or your runtime's timer)"));
    }
    if std && at(1) == "fs" {
        return Some(("std::fs inside an async body blocks on the disk", "use tokio::fs (or spawn_blocking)"));
    }
    if std && at(1) == "net" && at(2) == "TcpStream" && at(3) == "connect" {
        return Some(("std::net::TcpStream::connect inside an async body blocks on the network", "await tokio::net::TcpStream::connect"));
    }
    if full.last().is_some_and(|s| s == "block_on") {
        return Some(("block_on inside an async body blocks on a future from within one", "await the future"));
    }
    if full.iter().any(|s| s == "blocking") {
        return Some(("a `blocking` API inside an async body blocks on I/O", "use its async client"));
    }
    None
}

struct Walk<'a> {
    names: &'a Names,
    in_async: bool,
    found: Vec<(Span, &'static str, &'static str)>,
}

impl Walk<'_> {
    fn scoped(&mut self, now_async: bool, inner: impl FnOnce(&mut Self)) {
        let was = std::mem::replace(&mut self.in_async, now_async);
        inner(self);
        self.in_async = was;
    }

    /// A method call that blocks unless its async twin is awaited: no argument, no `.await` after it.
    fn method(&mut self, call: &syn::ExprMethodCall, awaited: bool) {
        let name = call.method.to_string();
        if name == "block_on" {
            self.found.push((call.method.span(), "block_on inside an async body blocks on a future from within one", "await the future"));
            return;
        }
        if awaited || !call.args.is_empty() {
            return;
        }
        if matches!(name.as_str(), "output" | "status" | "wait") && self.from_std_command(&call.receiver) {
            self.found.push((call.method.span(), "a std::process::Command waited on inside an async body", "use tokio::process::Command and await it"));
        } else if matches!(name.as_str(), "recv" | "join" | "wait") {
            self.found.push((call.method.span(), "a blocking recv()/join()/wait() inside an async body", "await the async twin (tokio::sync, a task's JoinHandle) or move it to spawn_blocking"));
        }
    }

    /// Whether a method chain starts at `std::process::Command::new(..)`.
    fn from_std_command(&self, mut receiver: &syn::Expr) -> bool {
        loop {
            match receiver {
                syn::Expr::MethodCall(m) => receiver = &m.receiver,
                syn::Expr::Call(c) => {
                    let syn::Expr::Path(p) = &*c.func else { return false };
                    let full = self.names.resolve(&p.path);
                    return full.len() >= 3 && full[0] == "std" && full[1] == "process" && full[2] == "Command";
                }
                _ => return false,
            }
        }
    }
}

impl<'ast> Visit<'ast> for Walk<'_> {
    fn visit_item_fn(&mut self, f: &'ast syn::ItemFn) {
        self.scoped(f.sig.asyncness.is_some(), |w| syn::visit::visit_item_fn(w, f));
    }

    fn visit_impl_item_fn(&mut self, f: &'ast syn::ImplItemFn) {
        self.scoped(f.sig.asyncness.is_some(), |w| syn::visit::visit_impl_item_fn(w, f));
    }

    fn visit_trait_item_fn(&mut self, f: &'ast syn::TraitItemFn) {
        self.scoped(f.sig.asyncness.is_some(), |w| syn::visit::visit_trait_item_fn(w, f));
    }

    fn visit_expr_async(&mut self, e: &'ast syn::ExprAsync) {
        self.scoped(true, |w| syn::visit::visit_expr_async(w, e));
    }

    fn visit_expr_closure(&mut self, c: &'ast syn::ExprClosure) {
        self.scoped(c.asyncness.is_some(), |w| syn::visit::visit_expr_closure(w, c));
    }

    fn visit_expr_call(&mut self, call: &'ast syn::ExprCall) {
        if self.in_async
            && let syn::Expr::Path(p) = &*call.func
            && let Some((what, remedy)) = blocking_path(&self.names.resolve(&p.path))
        {
            self.found.push((p.path.segments.last().map_or_else(Span::call_site, |s| s.ident.span()), what, remedy));
        }
        syn::visit::visit_expr_call(self, call);
    }

    fn visit_expr_await(&mut self, e: &'ast syn::ExprAwait) {
        // THE CALL AWAITED is its async twin: judged as awaited, and its own receiver and arguments as usual.
        if let syn::Expr::MethodCall(call) = &*e.base {
            if self.in_async {
                self.method(call, true);
            }
            syn::visit::visit_expr_method_call(self, call);
            return;
        }
        syn::visit::visit_expr_await(self, e);
    }

    fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
        if self.in_async {
            self.method(call, false);
        }
        syn::visit::visit_expr_method_call(self, call);
    }
}

#[cfg(test)]
#[path = "rsasync_tests.rs"]
mod tests;
