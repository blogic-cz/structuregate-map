//! THE RUST HALF - the gate's line count and the map's file-level graph, parsed by `syn` inside
//! this process.
//!
//! Every other half is run by the host that owns its parser: node for TypeScript, python for its
//! `ast`, `powershell.exe` for the PowerShell AST. C# is parsed in-process by Roslyn, and rust is
//! the second in-process half for the same reason: its parser can live in the exe. This library is
//! already linked into it, so the half costs no new host, no second process and nothing that has
//! to reach a consumer beside the two files it already receives.
//!
//! ONE ENTRY POINT, `fbt_rs_map`: the map for a whole root at once, in parallel, because the module tree
//! is only knowable with the filesystem in view. The gate's count of a rust file is `count::`'s, which
//! calls `lines::count` here.

pub(crate) mod consts;
pub(crate) mod handlers;
mod items;
pub(crate) mod lines;
pub(crate) mod literals;
mod modpath;

use serde_json::{json, Value};
use std::path::Path;

/// The map rows of every file in `files_json` - `[[rel, abs], ...]`, `rel` relative to `root` -
/// as `{"files": [...]}`, one object per file in the order given.
///
/// # Safety
/// `root` and `files_json` must be null or NUL-terminated UTF-8. The result is the caller's to
/// free with `fbt_string_free`.
#[unsafe(no_mangle)]


pub(crate) fn map_file(root: &Path, rel: &str, abs: &Path) -> Value {
    let row = match std::fs::read_to_string(abs) {
        Ok(text) => read(root, rel, text.trim_start_matches('\u{feff}')),
        Err(e) => json!({ "rel": rel, "error": { "line": 1, "message": format!("could not be read ({e})") } }),
    };
    forget_spans();
    row
}

fn read(root: &Path, rel: &str, text: &str) -> Value {
    let counted = lines::count(text).ok();
    let source = lines::without_shebang(text);
    let file = match syn::parse_file(&source) {
        Ok(file) => file,
        Err(e) => {
            let at = e.span().start().line.max(1);
            return json!({ "rel": rel, "lines": counted,
                           "error": { "line": at, "message": format!("does not parse as rust: {e}") } });
        }
    };
    let found = items::read(&file, &lines::LineStarts::new(&source));
    let uses_path: Vec<String> = found.mods.iter().map(|m| modpath::resolve(root, rel, m)).collect();
    let bodies: Vec<Value> = found
        .bodies
        .iter()
        .map(|b| json!({ "digest": b.digest, "where": format!("{rel}:{}:{}", b.line, b.name), "size": b.size }))
        .collect();
    json!({
        "rel": rel,
        "lines": counted,
        "summary": found.summary,
        "declares": found.declares,
        "uses": found.uses,
        "uses_path": uses_path,
        "entry": found.has_main || modpath::is_crate_root(rel),
        "bodies": bodies,
    })
}

/// The deep map's rows of one file, off ONE parse.
pub(crate) struct Rows {
    pub consts: Vec<consts::Const>,
    pub handlers: Vec<handlers::Handler>,
    pub literals: Vec<literals::Literal>,
}

/// The deep map's rows of one file - its constants, its error handling and its literals - or `Err` with the
/// line and the reason when it does not parse. The spans are forgotten before this returns.
pub(crate) fn rows_of(text: &str) -> Result<Rows, (usize, String)> {
    let source = lines::without_shebang(text.trim_start_matches('\u{feff}'));
    let found = match syn::parse_file(&source) {
        Ok(file) => {
            let starts = lines::LineStarts::new(&source);
            Ok(Rows { consts: consts::read(&file, &starts), handlers: handlers::read(&file), literals: literals::read(&file, &starts) })
        }
        Err(e) => Err((e.span().start().line.max(1), format!("does not parse as rust: {e}"))),
    };
    forget_spans();
    found
}

/// SPANS ARE NEVER FREED UNLESS ASKED. Outside a proc macro, `proc-macro2` keeps every lexed
/// file's text in a thread-local map for the life of the thread, so a map over many thousands of
/// files would hold every byte of them until the process exits. Nothing lexed may be alive when
/// this runs - every `syn` tree above has been dropped by then.
pub(crate) fn forget_spans() {
    proc_macro2::extra::invalidate_current_thread_spans();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_that_does_not_parse_is_an_error_with_its_line_and_keeps_its_count() {
        let row = read(Path::new("."), "src/a.rs", "fn a() {}\nfn b( {}\n");
        assert_eq!(row["error"]["line"], 2);
        assert!(row["declares"].is_null());
        forget_spans();
    }

    #[test]
    fn a_crate_root_is_an_entry_and_its_mods_are_resolved_to_paths() {
        let row = read(Path::new("no-such-root"), "src/lib.rs", "mod scan;\npub fn go() {}\n");
        assert_eq!(row["entry"], true);
        assert_eq!(row["uses_path"][0], "src/scan.rs");
        assert_eq!(row["declares"][0], "go");
        forget_spans();
    }
}
