//! THE MAP, RUN WHOLE: the walk, which half owns which file, whether anything changed since the map was
//! written, every half, the plugins, the deep map, and the finish - in this library, calling back into the
//! caller for what only .NET can parse: a C# file's graph and deep rows (Roslyn), and T-SQL (ScriptDom).
//!
//! IT IS DERIVED, NEVER WRITTEN. Every row is read from a parse tree by the parser authoritative for that
//! language - Roslyn, the repo's own TypeScript compiler, the python that will run the code, `syn`,
//! `pulldown-cmark`. A hand-written map would be a second copy of the source, and a second copy drifts.
//!
//! IT PRINTS NOTHING: the lines come back tagged for stdout or stderr, and the caller writes them in the
//! console's code page, which is what MSBuild reads.

mod check;
mod docroot;
pub(crate) mod deep;
pub(crate) use deep::{stored_nothing, UNRESTORED};
mod halves;
mod kept;
mod protocol;
mod script;
mod stale;

use crate::graph::{self, Options};
use crate::{count, sources};
use protocol::Collector;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::ffi::{c_char, CStr, CString};
use std::path::{Path, PathBuf, MAIN_SEPARATOR};
use std::time::SystemTime;

pub type CsMap = extern "C" fn(abs: *const c_char, rel: *const c_char) -> *mut c_char;
pub type Deep = extern "C" fn(input: *const c_char) -> *mut c_char;
pub type Free = extern "C" fn(text: *mut c_char);

/// What the halves own. `.js`/`.mjs` go to the TypeScript compiler on purpose: it parses them.
const SCRIPT: [&str; 8] = [".ts", ".tsx", ".mts", ".cts", ".js", ".mjs", ".cjs", ".jsx"];
const SHELL: [&str; 3] = [".ps1", ".psm1", ".psd1"];
/// Read by the deep map only; the file-level graph lists them.
const DEEP_ONLY: [&str; 3] = [".sql", ".razor", ".cshtml"];

#[derive(Deserialize, Default)]
#[serde(default)]
struct Run {
    roots: Vec<String>,
    extensions: Vec<String>,
    skip: Vec<String>,
    tracked: bool,
    include_untracked: bool,
    context_docs_only: bool,
    /// `--doc-skip`: docs that are DATA (a corpus, a vendored model card) - neither mapped nor measured.
    doc_skip: Vec<String>,
    /// `--doc-root`: where the docs are mapped from, instead of the code roots (`docroot.rs`).
    doc_root: Option<String>,
    map: bool,
    map_sqlite: Option<String>,
    map_out: String,
    map_if_stale: bool,
    map_check: bool,
    baseline_path: Option<String>,
    update_baseline: bool,
    map_plugins: Vec<String>,
    ts_host: String,
    py_host: String,
    ps_host: String,
    ts_node_modules: Vec<String>,
    ts_config: Option<String>,
    ts_html: Option<String>,
    map_row_fts: bool,
    map_atlas_dir: Option<String>,
    map_exclude: Vec<String>,
    map_reread: Vec<String>,
    sql_config: Option<String>,
    facts_config: Option<String>,
    base_dir: String,
    build: String,
}

/// One `--root`'s share of the walk: the script halves take a root of their own and answer relative to it.
#[derive(Default)]
struct Bucket {
    root: String,
    prefix: String,
    script: Vec<(String, String)>,
    python: Vec<(String, String)>,
    shell: Vec<(String, String)>,
    rust: Vec<(String, String)>,
    go: Vec<(String, String)>,
    markdown: Vec<(String, String)>,
    tree: Vec<String>,
}

/// `{out: [["o"|"e", line], ...], exit}` - or `{error}` when the input is not the map's.
pub(crate) fn run_json(input: Value, cs: CsMap, deep: Deep, free: Free) -> Value {
    match serde_json::from_value::<Run>(input) {
        Ok(input) => run(&input, cs, deep, free),
        Err(e) => json!({ "error": format!("the map input is not JSON: {e}") }),
    }
}

fn run(input: &Run, cs: CsMap, deep: Deep, free: Free) -> Value {
    // THE SCRIPTS EACH HALF IS STARTED WITH, staged out of this library: `language -> path`, or why not.
    let (mut scripts, mut stage_errors) = (HashMap::new(), HashMap::new());
    for set in crate::embedded::for_map(input.map_sqlite.is_some()) {
        match crate::embedded::stage(set) {
            Ok(path) => scripts.insert(set.tag.to_string(), path),
            Err(why) => stage_errors.insert(set.tag.to_string(), why),
        };
    }
    let mut out: Vec<(&str, String)> = Vec::new();
    let mut into = Collector::new();
    let extensions: Vec<String> = input.extensions.iter().map(|e| e.to_lowercase()).collect();
    let is_doc = |rel: &str| {
        extension(rel) == ".md"
            && (!input.context_docs_only || crate::gate::docs::context_doc(rel))
            && !crate::sources::glob::excluded(&input.doc_skip, rel)
    };
    // WITH A DOC ROOT the code roots map no doc: each is mapped once, from there.
    let in_scope = |rel: &str| input.doc_root.is_none() && !extensions.contains(&extension(rel)) && is_doc(rel);
    let mut buckets: Vec<Bucket> = Vec::new();
    let (mut sharp, mut sql, mut markup) = (Vec::new(), Vec::new(), Vec::new());
    let mut listed: Vec<(String, String, String, String)> = Vec::new();
    let (mut walked, mut deleted, mut unreadable) = (Vec::<String>::new(), Vec::<String>::new(), Vec::<String>::new());
    let mut newest = SystemTime::UNIX_EPOCH;
    // THE NEWEST MTIME IS ASKED ONLY WHERE IT IS READ - `--map-if-stale`. A deep map alone never reads it, and a call to
    // the file system for each of a large tree's files was most of its `walk`.
    let dated = input.map && input.map_if_stale;
    // WHAT A HALF COULD HAVE READ AND WAS NOT GIVEN, per extension as first spelled.
    let mut passed_over: Vec<(String, i64)> = Vec::new();

    let walk = crate::trace::stage("walk");
    for root in &input.roots {
        let prefix = prefix(input, root);
        let mut bucket = Bucket { root: root.clone(), prefix: prefix.clone(), ..Default::default() };
        let found = sources::walked(&sources::Input {
            root: root.clone(),
            skip: input.skip.clone(),
            tracked: input.tracked,
            include_untracked: input.include_untracked,
            generated_cs: true,
        });
        deleted.extend(found.deleted.into_iter().filter(|rel| extensions.contains(&extension(rel)) || in_scope(rel)));
        unreadable.extend(found.unreadable);
        for (abs, rel) in found.found {
            bucket.tree.push(rel.clone());
            let spelled = extension_as_spelled(&abs);
            let lower = spelled.to_lowercase();
            let mtime = || if dated { std::fs::metadata(&abs).and_then(|m| m.modified()).ok() } else { None };
            // A DOC IS MAPPED WHEREVER THE GATE MEASURES IT, so it never needs `--ext`.
            if in_scope(&rel) {
                walked.push(format!("{prefix}{rel}"));
                newest = newest.max(mtime().unwrap_or(SystemTime::UNIX_EPOCH));
                bucket.markdown.push((rel, abs));
                continue;
            }
            if !extensions.contains(&lower) {
                if mappable(&lower) {
                    match passed_over.iter_mut().find(|(e, _)| e.eq_ignore_ascii_case(&spelled)) {
                        Some(entry) => entry.1 += 1,
                        None => passed_over.push((spelled, 1)),
                    }
                }
                continue;
            }
            let full = format!("{prefix}{rel}");
            walked.push(full.clone());
            newest = newest.max(mtime().unwrap_or(SystemTime::UNIX_EPOCH));
            match lower.as_str() {
                ".cs" => sharp.push((full, abs)),
                e if SCRIPT.contains(&e) => bucket.script.push((rel, abs)),
                ".py" => bucket.python.push((rel, abs)),
                e if SHELL.contains(&e) => bucket.shell.push((rel, abs)),
                ".rs" => bucket.rust.push((rel, abs)),
                ".go" => bucket.go.push((rel, abs)),
                _ => {
                    if lower == ".sql" {
                        sql.push((full.clone(), abs.clone()));
                    }
                    if lower == ".razor" || lower == ".cshtml" {
                        markup.push((full.clone(), abs.clone()));
                    }
                    // A LANGUAGE WITH NO HALF IS LISTED, NOT DROPPED - and counted only once the map is known to
                    // be built: a current map, and the deep map alone, read none of these.
                    listed.push((full, abs, lower, spelled));
                }
            }
        }
        buckets.push(bucket);
    }

    let docs = input.doc_root.as_deref().map(|root| docroot::walk(root, &input.skip, input.tracked, input.include_untracked, &is_doc));
    walked.extend(docs.iter().flat_map(|d| d.markdown.iter().map(|(rel, _)| rel.clone())));
    walk.set("structuregate.files", walked.len() as i64);
    drop(walk);

    // SAID ONCE, WITH THE FLAG THAT FIXES IT.
    if !passed_over.is_empty() {
        let mut ordered = passed_over.clone();
        ordered.sort_by(|a, b| b.1.cmp(&a.1));
        let named: Vec<String> = ordered.iter().map(|(e, n)| format!("{n} {e}")).collect();
        let mut asked = input.extensions.clone();
        asked.sort();
        into.notes.push(format!(
            "the map passed over {} file(s) a half can read: `--ext` is {} here, so pass --ext with the extensions you want mapped",
            named.join(", "),
            asked.join(",")
        ));
    }

    let lists = |buckets: &[Bucket], pick: fn(&Bucket) -> &Vec<(String, String)>| -> Vec<(String, String)> {
        buckets.iter().flat_map(|b| pick(b).iter().map(|(rel, abs)| (format!("{}{rel}", b.prefix), abs.clone()))).collect()
    };
    let deep_files = || deep::Files {
        python: lists(&buckets, |b| &b.python),
        sharp: sharp.iter().chain(&markup).cloned().collect(),
        script: lists(&buckets, |b| &b.script),
        sql: sql.clone(),
        rust: lists(&buckets, |b| &b.rust),
    };
    let deep_options = || deep::Deep {
        db: input.map_sqlite.clone().unwrap_or_default(),
        root: input.roots[0].clone(),
        roots: input.roots.iter().map(|r| (prefix(input, r), r.clone())).collect(),
        ts_host: input.ts_host.clone(),
        py_host: input.py_host.clone(),
        skip: input.skip.clone(),
        ts_node_modules: input.ts_node_modules.clone(),
        ts_config: input.ts_config.clone(),
        ts_html: input.ts_html.clone(),
        row_fts: input.map_row_fts,
        atlas_dir: input.map_atlas_dir.clone(),
        exclude: input.map_exclude.clone(),
        reread: input.map_reread.clone(),
        sql_config: input.sql_config.clone(),
        facts_config: input.facts_config.clone(),
        base_dir: input.base_dir.clone(),
        map_out: input.map.then(|| input.map_out.clone()),
        build: input.build.clone(),
        scripts: scripts.clone(),
        stage_errors: stage_errors.clone(),
    };
    let mut ask = |question: &Value| callback(deep, free, &question.to_string());

    // `--map-sqlite` WITHOUT `--map`: the deep rows and none of the file-level pass - what a per-turn hook runs.
    if !input.map {
        let _deep = crate::trace::stage("deep map");
        let exit = deep::only(&mut into, &deep_options(), &deep_files(), &mut ask, &mut out);
        return json!({ "out": out, "exit": exit });
    }

    // NOTHING IS PARSED WHEN NOTHING CHANGED - AND NOTHING CAME OR WENT, AND THE GATE IS THE SAME: a moved
    // file keeps its mtime, so the file SET is compared as well, and a newer exe is a changed rule set over
    // the same sources, so its own mtime counts as one more input (`stale.rs`).
    into.inventory = inventory(&walked);
    let map_path = Path::new(&input.map_out);
    if input.map_if_stale && map_path.is_file() {
        let map_modified = std::fs::metadata(map_path).and_then(|m| m.modified()).ok();
        let baseline = input.baseline_path.as_ref().and_then(|b| std::fs::metadata(b).and_then(|m| m.modified()).ok());
        let kept = if input.map_check { check::kept(map_path) } else { Some(Vec::new()) };
        match stale::why(map_modified, newest, stale::exe_modified(), &recorded(map_path), &into.inventory, baseline) {
            None if kept.is_some() => {
                let sources = sharp.len() + buckets.iter().map(|b| b.script.len() + b.python.len() + b.shell.len() + b.rust.len() + b.markdown.len()).sum::<usize>();
                crate::trace::set("structuregate.map.current", true);
                out.push(("o", format!("structuregate map: current ({sources} source file(s) older than {}, none added or gone, gate unchanged) — nothing re-parsed", input.map_out)));
                let exit = if input.map_check { check::report(&kept.unwrap_or_default(), &mut out) } else { 0 };
                return json!({ "out": out, "exit": exit });
            }
            // SAID OUT LOUD, because nothing in the tree explains this re-parse: no file moved.
            Some(stale::Why::Exe) => out.push(("o", "structuregate map: the gate is newer than the map — every file re-parsed under its rules".to_string())),
            // A stale map, or a current one whose findings could not be read back: parsed again, never passed unchecked.
            _ => {}
        }
    }

    for (full, abs, lower, spelled) in listed {
        let lines = match count::count(Path::new(&abs), false, true) {
            Ok(n) => n as i64,
            Err(error) => {
                unreadable.push(format!("{full} ({error})"));
                0
            }
        };
        let unmapped = if DEEP_ONLY.contains(&lower.as_str()) {
            "read by the deep map (--map-sqlite), not graphed here".to_string()
        } else {
            format!("no {spelled} parser in this tool — the file is counted, not graphed")
        };
        let file = into.row(&full, lower.trim_start_matches('.'));
        file.reports_external = false;
        file.lines = lines;
        file.unmapped = unmapped;
    }

    // THE DEEP MAP'S PYTHON HOST STARTS NOW, beside the file map's own (`deep::Early`), not after it.
    let early = input.map_sqlite.is_some().then(|| deep::early(&deep_options(), &deep_files()));

    // ONE ROOT ONLY: with several, a path is keyed by its root prefix, and the tree map is one root's (`kept.rs`).
    let mut kept = (input.roots.len() == 1).then(|| kept::Kept::open(&input.roots[0], &input.skip, input.tracked, &input.build, &buckets[0].tree,
        &[input.map_out.as_str(), input.map_sqlite.as_deref().unwrap_or("")])).flatten();
    let csharp = crate::trace::stage("map: csharp");
    let (answers, parsed) = kept::answers(&mut kept, &sharp, |rel, abs| cs_file(cs, free, abs, rel));
    csharp.set("structuregate.files", sharp.len() as i64);
    csharp.set("structuregate.parsed", parsed as i64);
    for ((rel, _), answer) in sharp.iter().zip(&answers) {
        halves::csharp(&mut into, rel, answer);
    }
    drop(csharp);
    if !sharp.is_empty() {
        into.halves.insert("csharp in-process (Roslyn)".into());
    }

    let staged = |language: &str| -> Result<&str, &str> {
        match scripts.get(language) {
            Some(path) => Ok(path.as_str()),
            None => Err(stage_errors.get(language).map_or("not staged", String::as_str)),
        }
    };
    let powershell_before: Vec<String> = ["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"].map(String::from).to_vec();
    let powershell = || halves::Script { language: "powershell", host: &input.ps_host, script: staged("powershell"), flag: "--ps-host",
        before: powershell_before.clone(), after: Vec::new(), list_flag: "-ListFile", root_flag: "-Root", per_file: true, known_flag: "-KnownFile", whole: None, dependents: None, early: None };
    let mut pending = Vec::new();
    for bucket in &buckets {
        for (rel, abs) in &bucket.script {
            let dialect = if [".ts", ".tsx", ".mts", ".cts"].contains(&extension(abs).as_str()) { "typescript" } else { "javascript" };
            into.row(&format!("{}{rel}", bucket.prefix), dialect).language = dialect.into();
        }
        for (rel, _) in &bucket.python {
            into.row(&format!("{}{rel}", bucket.prefix), "python");
        }
        for (rel, _) in &bucket.shell {
            // NOT reports_external: a PowerShell use is a command NAME - hundreds of cmdlets per file.
            into.row(&format!("{}{rel}", bucket.prefix), "powershell").reports_external = false;
        }
        let halves_here = [
            (halves::Script { language: "typescript", host: &input.ts_host, script: staged("typescript"), flag: "--ts-host",
                before: Vec::new(), after: vec!["--map".into()], list_flag: "--list-file", root_flag: "--root", per_file: true, known_flag: "--known-file", whole: None, dependents: None, early: None }, &bucket.script),
            (halves::Script { language: "python", host: &input.py_host, script: staged("python"), flag: "--py-host",
                before: Vec::new(), after: Vec::new(), list_flag: "--list-file", root_flag: "--root", per_file: true, known_flag: "--known-file", whole: None, dependents: Some(python_reads), early: None }, &bucket.python),
        ];
        for (half, files) in halves_here {
            let reason = halves::script(&mut into, &half, files, &bucket.root, &bucket.prefix, &mut kept);
            halves::mark(&mut into, &bucket.prefix, files, &reason);
        }
        // POWERSHELL RUNS ON BESIDE EVERYTHING AFTER IT - the other halves, the deep map - and is read last: `pwsh` is
        // seconds on a cold run, and nothing after it needs its answer.
        pending.push((bucket, halves::background(&mut into, &powershell(), &bucket.shell, &bucket.root, &bucket.prefix, &mut kept)));
        // IN PROCESS, like C#: `syn` and `pulldown-cmark` are linked into this exe.
        halves::rust(&mut into, &bucket.root, &bucket.prefix, &bucket.rust, &mut kept);
        halves::go(&mut into, &bucket.root, &bucket.prefix, &bucket.go, &mut kept);
        let prefix = bucket.prefix.clone();
        halves::markdown(&mut into, &bucket.root, &bucket.prefix, &bucket.markdown, &bucket.tree, &|path| format!("{prefix}{path}"), false, &[]);
    }
    if let Some(docs) = &docs {
        let roots: Vec<(String, String)> = input.roots.iter().map(|r| (r.clone(), prefix(input, r))).collect();
        let mapped: HashSet<&str> = docs.markdown.iter().map(|(rel, _)| rel.as_str()).collect();
        // WHERE ELSE A DOC WRITES A PATH FROM: a code root (a flat-import tree names its modules so) or the repository top.
        let also: Vec<PathBuf> = input.roots.iter().map(PathBuf::from)
            .chain(crate::sources::repository(Path::new(&docs.root)))
            .collect();
        halves::markdown(&mut into, &docs.root, "", &docs.markdown, &docs.tree, &|path| docroot::keyed(&docs.root, &roots, &mapped, path), true, &also);
    }

    // A DOC UNDER THE DOC ROOT is where that root says, not under the code roots' parent.
    let doc_at = |rel: &str| docs.as_ref().and_then(|d| d.markdown.iter().find(|(r, _)| r == rel).map(|(_, abs)| abs.clone()));
    let every: Vec<(String, String)> = into.files.keys().map(|rel| (rel.clone(), doc_at(rel).unwrap_or_else(|| absolute(input, rel)))).collect();
    halves::plugins(&mut into, &input.map_plugins, &input.roots[0], every);

    // THE DEEP MAP, with every path already carrying its root prefix.
    if input.map_sqlite.is_some() {
        let _deep = crate::trace::stage("deep map");
        deep::run(&mut into, &deep_options(), &deep_files(), &mut ask, early);
        halves::bound(&mut into, input.map_sqlite.as_deref().unwrap_or(""));
    }
    for (bucket, running) in pending {
        let reason = halves::finish(&mut into, &powershell(), &bucket.shell, &bucket.prefix, &mut kept, running);
        halves::mark(&mut into, &bucket.prefix, &bucket.shell, &reason);
    }
    if let Some(kept) = kept {
        kept.keep();
    }

    if !unreadable.is_empty() {
        let first: Vec<&str> = unreadable.iter().take(5).map(String::as_str).collect();
        out.push(("e", format!("NOTE: {} path(s) could not be read and are NOT in the map: {}{}", unreadable.len(), first.join(", "),
            if unreadable.len() > 5 { " …" } else { "" })));
    }
    if !deleted.is_empty() {
        let first: Vec<&str> = deleted.iter().take(5).map(String::as_str).collect();
        out.push(("e", format!("NOTE: {} tracked file(s) are deleted in the working tree and are NOT in the map — stage the deletion: {}",
            deleted.len(), first.join(", "))));
    }

    let writing = crate::trace::stage("write the map");
    let answer = graph::finish(into.into_input(Options {
        map_path: input.map_out.clone(),
        baseline_path: input.baseline_path.clone(),
        update_baseline: input.update_baseline,
    }));
    drop(writing);
    if let Some(why) = answer["write_error"].as_str() {
        out.push(("e", format!("structuregate: the map could not be written to {} ({why})", input.map_out)));
        return json!({ "out": out, "exit": 2 });
    }
    for line in halves::strings(&answer["report"]) {
        out.push(("o", line));
    }
    if input.update_baseline {
        if let Some(why) = answer["baseline_error"].as_str() {
            out.push(("e", format!("structuregate: the map baseline could not be written ({why})")));
            return json!({ "out": out, "exit": 2 });
        }
        out.push(("o", answer["baseline_written"].as_str().unwrap_or("").to_string()));
        return json!({ "out": out, "exit": 0 });
    }
    if !input.map_check {
        return json!({ "out": out, "exit": 0 });
    }
    let findings: Vec<(String, bool)> = answer["findings"].as_array().into_iter().flatten()
        .map(|f| (f["text"].as_str().unwrap_or("").to_string(), f["error"].as_bool().unwrap_or(false))).collect();
    let exit = check::report(&findings, &mut out);
    json!({ "out": out, "exit": exit })
}

/// What the python half reads beyond the `.py` files it is given: the launchers it takes entry points from
/// (`PyLaunch.py`) - kept whole by it, so a `.jsx` edit no longer re-reads every module (`kept.rs`).
fn python_reads(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path).to_lowercase();
    name == "pyproject.toml" || name.ends_with(".spec")
}

/// Whether SOME half could read this extension, whatever `--ext` says - so a file passed over is named.
fn mappable(extension: &str) -> bool {
    extension == ".cs" || extension == ".py" || extension == ".rs" || extension == ".go" || SCRIPT.contains(&extension) || SHELL.contains(&extension)
        || DEEP_ONLY.contains(&extension)
}

/// The folder every path under this root is named with - "" with one root.
fn prefix(input: &Run, root: &str) -> String {
    if input.roots.len() > 1 && !input.tracked {
        let trimmed = root.trim_end_matches(MAIN_SEPARATOR);
        Path::new(trimmed).file_name().map(|n| format!("{}/", n.to_string_lossy())).unwrap_or_default()
    } else {
        String::new()
    }
}

/// A map key back as a path on disk: with several roots the key carries the folder that tells them apart.
fn absolute(input: &Run, rel: &str) -> String {
    let first = &input.roots[0];
    let base = if input.roots.len() > 1 && !input.tracked {
        Path::new(first.trim_end_matches(MAIN_SEPARATOR)).parent().map_or_else(|| PathBuf::from(first), Path::to_path_buf)
    } else {
        PathBuf::from(first)
    };
    let joined = base.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
    joined.to_string_lossy().into_owned()
}

/// WHICH FILES A MAP COVERS, as one hash written FIRST in the map, so `--map-if-stale` reads it without
/// parsing the rest. A MOVE KEEPS A FILE'S MTIME and a deletion leaves nothing to be newer, so the set of
/// files is what is compared.
fn inventory(walked: &[String]) -> String {
    let mut sorted = walked.to_vec();
    sorted.sort();
    blake3::hash(sorted.join("\n").as_bytes()).to_hex()[..16].to_string()
}

/// What the map at `path` recorded - its first property, read off the head of the file - or "".
fn recorded(path: &Path) -> String {
    // THE HEAD ONLY: the map runs to megabytes, and its first property is all this asks.
    let mut bytes = Vec::with_capacity(4096);
    let Ok(file) = std::fs::File::open(path) else { return String::new() };
    if std::io::Read::read_to_end(&mut std::io::Read::take(file, 4096), &mut bytes).is_err() {
        return String::new();
    }
    let head = String::from_utf8_lossy(&bytes).into_owned();
    let rest = head.trim_start();
    let Some(rest) = rest.strip_prefix('{') else { return String::new() };
    let Some(rest) = rest.trim_start().strip_prefix("\"inventory\"") else { return String::new() };
    let Some(rest) = rest.trim_start().strip_prefix(':') else { return String::new() };
    let Some(rest) = rest.trim_start().strip_prefix('"') else { return String::new() };
    rest.find('"').map(|end| rest[..end].to_string()).unwrap_or_default()
}

fn cs_file(cs: CsMap, free: Free, abs: &str, rel: &str) -> Value {
    let (Ok(abs), Ok(rel)) = (CString::new(abs), CString::new(rel)) else { return json!({ "error": "IOException" }) };
    let reply = cs(abs.as_ptr(), rel.as_ptr());
    take(reply, free)
}

/// A deep half's answer, as the caller wrote it: JSON, or a JSON line followed by a batch's payload.
fn callback(deep: Deep, free: Free, input: &str) -> String {
    let Ok(input) = CString::new(input) else { return r#"{"errors": ["HALF      deep: the question held a NUL"]}"#.into() };
    let reply = deep(input.as_ptr());
    if reply.is_null() {
        return "{}".into();
    }
    let text = unsafe { CStr::from_ptr(reply) }.to_string_lossy().into_owned();
    free(reply);
    text
}

/// A caller's answer, copied out and handed back to be freed by the allocator that made it.
fn take(reply: *mut c_char, free: Free) -> Value {
    if reply.is_null() {
        return json!({ "error": "IOException" });
    }
    let text = unsafe { CStr::from_ptr(reply) }.to_string_lossy().into_owned();
    free(reply);
    serde_json::from_str(&text).unwrap_or_else(|_| json!({ "error": "IOException" }))
}

fn extension(path: &str) -> String {
    extension_as_spelled(path).to_lowercase()
}

/// `.Ext` as the file spells it - `Path.GetExtension` of the last segment, "" when there is none.
fn extension_as_spelled(path: &str) -> String {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    name.rfind('.').map(|at| name[at..].to_string()).unwrap_or_default()
}
