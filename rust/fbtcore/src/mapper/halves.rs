//! THE HALVES, each run where its parser is and each read into one collector: the script halves in the host
//! that owns their language, rust and markdown in this process, C# through the caller (Roslyn), and a
//! project's own plugins. A HALF THAT WILL NOT LAUNCH IS A FINDING, NEVER A SKIP - its reason is written
//! against every file it would have covered, because a map missing every python file looks exactly like a
//! map of a tree with no python in it.

use super::protocol::{read, Collector};
use crate::graph::{Computed, Place};
use crate::hosts::plugins;
use super::kept::Kept;
use crate::{gomap, mdmap, rsmap};
use rayon::prelude::*;
use serde_json::Value;
use std::path::Path;

pub use super::script::{background, finish, script, Script};

/// A half that did not run leaves its reason on EVERY file it would have covered - a reader looking at one
/// path must see that the blank beside it is a missing tool, not a file that imports nothing.
pub fn mark(into: &mut Collector, prefix: &str, files: &[(String, String)], reason: &str) {
    if reason.is_empty() {
        return;
    }
    for (rel, _) in files {
        if let Some(file) = into.files.get_mut(&format!("{prefix}{rel}")) {
            file.unmapped = reason.into();
        }
    }
}

/// THE RUST HALF, in process: `syn` parses, and a `mod name;` is an exact edge.
pub fn rust(into: &mut Collector, root: &str, prefix: &str, files: &[(String, String)], kept: &mut Option<Kept>) {
    in_process(into, root, prefix, files, kept, ("rust", "rust in-process (syn)", "rust".into()), rsmap::map_file);
}

/// THE GO HALF, in process by `gosyn` (`gomap/`). Its answer about a file also reads the `go.mod` that says where
/// an import points, so every `go.mod` of the tree is in the key a kept answer is found by.
pub fn go(into: &mut Collector, root: &str, prefix: &str, files: &[(String, String)], kept: &mut Option<Kept>) {
    if files.is_empty() {
        return;
    }
    let modules = kept.as_ref().and_then(|k| k.whole("go.mod", &[], |path| path.ends_with("go.mod"))).unwrap_or_default();
    in_process(into, root, prefix, files, kept, ("go", "go in-process (gosyn)", format!("go|{modules}")), gomap::map_file);
}

/// A half parsed INSIDE this process, over one root's files on every core - rust by `syn`, Go by `gosyn` - each
/// answering per file in one shape. `half` is (language, how the halves line names it, the key a kept answer is under).
fn in_process(into: &mut Collector, root: &str, prefix: &str, files: &[(String, String)], kept: &mut Option<Kept>,
              half: (&str, &str, String), map: fn(&Path, &str, &Path) -> Value) {
    if files.is_empty() {
        return;
    }
    let (language, label, stamp) = half;
    let stage = crate::trace::stage(&format!("map: {language}"));
    stage.set("structuregate.files", files.len() as i64);
    for (rel, _) in files {
        let file = into.row(&format!("{prefix}{rel}"), language);
        file.language = language.into();
        file.reports_external = false;
    }
    let root_path = Path::new(root);
    // A FILE THAT DID NOT MOVE IS ANSWERED FROM THE LAST RUN (`kept.rs`): its answer is the file and the file set,
    // and re-parsing every `.rs` on every turn was 127 ms of each one in this repo. Only a clean answer is kept.
    let keys: Vec<Option<String>> = files.iter().map(|(rel, _)| kept.as_ref().and_then(|k| k.key(&stamp, rel))).collect();
    let mut rows: Vec<Option<Value>> = keys.iter()
        .map(|key| key.as_deref().and_then(|k| kept.as_mut()?.get(k)).and_then(|text| serde_json::from_str(&text).ok()))
        .collect();
    let asked: Vec<usize> = (0..files.len()).filter(|&i| rows[i].is_none()).collect();
    stage.set("structuregate.parsed", asked.len() as i64);
    let fresh: Vec<Value> = asked.par_iter().map(|&i| map(root_path, &files[i].0, Path::new(&files[i].1))).collect();
    for (i, row) in asked.into_iter().zip(fresh) {
        if let (Some(kept), Some(key), None) = (kept.as_mut(), &keys[i], row.get("error")) {
            kept.put(key.clone(), row.to_string());
        }
        rows[i] = Some(row);
    }
    // A WHOLE PACKAGE TAKEN AT ONCE - a Go `.` or `_` import - reaches every file of its folder but a test.
    let mut by_folder: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
    for (rel, _) in files {
        if !rel.ends_with("_test.go") {
            by_folder.entry(rel.rfind('/').map_or(String::new(), |at| rel[..at].to_string())).or_default().push(format!("{prefix}{rel}"));
        }
    }
    for row in rows.into_iter().flatten() {
        let rel = format!("{prefix}{}", row["rel"].as_str().unwrap_or(""));
        if let Some(lines) = row["lines"].as_i64() {
            into.row(&rel, language).lines = lines;
        }
        if let Some(error) = row.get("error") {
            into.errors.push(format!("UNPARSED  {rel}:{}: {}", error["line"].as_i64().unwrap_or(1), error["message"].as_str().unwrap_or("")));
            continue;
        }
        let file = into.row(&rel, language);
        file.summary = row["summary"].as_str().unwrap_or("").into();
        file.entry = row["entry"].as_bool().unwrap_or(false);
        file.generated = row["generated"].as_bool().unwrap_or(false);
        file.declares.extend(strings(&row["declares"]));
        file.registered.extend(strings(&row["registered"]));
        // A NAME QUALIFIED BY ITS FOLDER (Go's `dir::Name`) carries the root's prefix too.
        // A package at the module's root is folder "", so its qualified name starts `::` and takes the prefix bare.
        let qualify = |u: String| match u.strip_prefix("::") {
            Some(_) => format!("{}{u}", prefix.trim_end_matches('/')),
            None => format!("{prefix}{u}"),
        };
        file.uses.extend(strings(&row["uses"]).into_iter().map(|u| if u.contains("::") && language == "go" { qualify(u) } else { u }));
        file.uses_path.extend(strings(&row["uses_path"]).into_iter().map(|p| format!("{prefix}{p}")));
        for folder in strings(&row["uses_dirs"]) {
            file.uses_path.extend(by_folder.get(&folder).into_iter().flatten().filter(|p| **p != rel).cloned());
        }
        for body in row["bodies"].as_array().into_iter().flatten() {
            let place = Place(format!("{prefix}{}", body["where"].as_str().unwrap_or("")), body["size"].as_u64().unwrap_or(0) as usize);
            into.bodies.entry(body["digest"].as_str().unwrap_or("").into()).or_default().push(place);
        }
    }
    into.halves.insert(label.into());
}

/// THE MARKDOWN HALF, in process: what each doc points a reader at, looked for beside it, above it and under
/// it (`tree`: every file the walk saw here). A doc is an ENTRY - opened by a reader, never imported.
/// `place` names a path the doc resolved (from `root`) as the map keys it - the root's prefix, or, under
/// `--doc-root`, the code root it lands in (`docroot.rs`).
pub fn markdown(into: &mut Collector, root: &str, prefix: &str, files: &[(String, String)], tree: &[String], place: &dyn Fn(&str) -> String, anywhere: bool, also: &[std::path::PathBuf]) {
    if files.is_empty() {
        return;
    }
    let stage = crate::trace::stage("map: markdown");
    stage.set("structuregate.files", files.len() as i64);
    for (rel, abs) in files {
        let lines = crate::count::count(Path::new(abs), true, true).unwrap_or(0) as i64;
        let file = into.row(&format!("{prefix}{rel}"), "markdown");
        file.language = "markdown".into();
        file.reports_external = false;
        file.entry = true;
        file.lines = lines;
    }
    let names = mdmap::names::Tree::new(tree);
    let root_path = Path::new(root);
    let rows: Vec<Value> = files.par_iter().map(|(rel, abs)| mdmap::map_file(root_path, rel, Path::new(abs), &names, anywhere, also)).collect();
    for row in rows {
        let rel = format!("{prefix}{}", row["rel"].as_str().unwrap_or(""));
        if let Some(error) = row.get("error") {
            into.errors.push(format!("UNPARSED  {rel}:{}: {}", error["line"].as_i64().unwrap_or(1), error["message"].as_str().unwrap_or("")));
            continue;
        }
        let file = into.row(&rel, "markdown");
        file.summary = row["summary"].as_str().unwrap_or("").into();
        for mention in row["mentions"].as_array().into_iter().flatten() {
            let path = mention["path"].as_str().unwrap_or("");
            // A path inside the root goes back under the root's prefix; an absolute one names another tree.
            let rooted = Path::new(path).is_absolute() || path.as_bytes().get(1) == Some(&b':');
            // UNDER A DOC ROOT an absolute path is a code root's file too (`also`): `place` keys it by that root.
            file.mentions.insert(if rooted && !anywhere { path.to_string() } else { place(path) });
        }
        for gone in row["missing"].as_array().into_iter().flatten() {
            file.missing.push((gone["line"].as_i64().unwrap_or(0), gone["text"].as_str().unwrap_or("").into()));
        }
    }
    into.halves.insert("markdown in-process (pulldown-cmark)".into());
}

/// One C# file's answer from the caller (Roslyn), into the collector.
pub fn csharp(into: &mut Collector, rel: &str, answer: &Value) {
    // LISTED BEFORE IT IS READ: a file that will not open is still in the map, with the reason beside it.
    let file = into.row(rel, "csharp");
    file.reports_external = false;
    if let Some(error) = answer["error"].as_str() {
        into.errors.push(format!("UNPARSED  {rel}: could not be read ({error})"));
        return;
    }
    into.errors.extend(strings(&answer["errors"]));
    let file = into.row(rel, "csharp");
    file.lines = answer["lines"].as_i64().unwrap_or(0);
    file.generated = answer["generated"].as_bool().unwrap_or(false);
    file.summary = answer["summary"].as_str().unwrap_or("").into();
    file.entry = answer["entry"].as_bool().unwrap_or(false);
    file.declares.extend(strings(&answer["declares"]));
    file.uses.extend(strings(&answer["uses"]));
    file.registered.extend(strings(&answer["registered"]));
    for pair in answer["computed"].as_array().into_iter().flatten() {
        into.computed.push(Computed { rel: rel.into(), line: text(&pair[0]), what: text(&pair[1]) });
    }
    for (key, list) in [("bodies", true), ("expressions", false)] {
        for item in answer[key].as_array().into_iter().flatten() {
            let place = Place(text(&item[1]), item[2].as_u64().unwrap_or(0) as usize);
            let found = if list { &mut into.bodies } else { &mut into.expressions };
            found.entry(text(&item[0])).or_default().push(place);
        }
    }
}

/// WHAT THE DEEP C# MAP BOUND, back into the file map: each mapped C# file's `file_refs` targets (`File::bound`). A
/// half that failed is not read - its rows cannot be trusted - and neither is a target the file map does not hold.
pub fn bound(into: &mut Collector, db: &str) {
    if into.errors.iter().any(|e| e.starts_with("HALF      csharp rows")) {
        return;
    }
    let Ok(conn) = rusqlite::Connection::open_with_flags(db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY) else { return };
    let Ok(mut query) = conn.prepare("SELECT f.path, r.target FROM file_refs r JOIN files f ON f.id = r.file WHERE f.lang = 'csharp'") else { return };
    let Ok(rows) = query.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))) else { return };
    let pairs: Vec<(String, String)> = rows.flatten().collect();
    for (path, target) in pairs {
        if into.files.contains_key(&target)
            && let Some(file) = into.files.get_mut(&path)
        {
            file.bound.insert(target);
        }
    }
}

/// A project's own half: handed EVERY file this map holds, so its scan agrees with this one. A map plugin is
/// not read for its EXIT CODE - it states its own findings - so its rows are read whatever became of it.
pub fn plugins(into: &mut Collector, commands: &[String], cwd: &str, files: Vec<(String, String)>) {
    if commands.is_empty() {
        return;
    }
    let _stage = crate::trace::stage("map: plugins");
    let answer = plugins::map(&plugins::Input { commands: commands.to_vec(), cwd: cwd.into(), files });
    if let Some(error) = answer["error"].as_str() {
        into.errors.push(error.into());
        return;
    }
    for run in answer["runs"].as_array().into_iter().flatten() {
        read(run["stdout"].as_str().unwrap_or(""), into, "plugin", "");
        match run["error"].as_str() {
            Some(error) => into.errors.push(error.into()),
            None => {
                into.plugins.insert(run["command"].as_str().unwrap_or("").into());
            }
        }
    }
}

/// What a deep half the caller parses (C#, T-SQL) adds: its errors, notes, halves, the tables it wrote and how
/// many files it re-read - ADDED to what the other halves re-read.
pub fn deep(into: &mut Collector, answer: &Value) {
    into.errors.extend(strings(&answer["errors"]));
    into.notes.extend(strings(&answer["notes"]));
    into.halves.extend(strings(&answer["halves"]));
    for (table, rows) in answer["database"].as_object().into_iter().flatten() {
        into.database.insert(table.clone(), rows.as_i64().unwrap_or(0));
    }
    if let Some(reread) = answer["reread"].as_i64().filter(|n| *n >= 0) {
        into.reread = into.reread.max(0) + reread;
    }
}

pub fn strings(value: &Value) -> Vec<String> {
    value.as_array().into_iter().flatten().map(|v| v.as_str().unwrap_or("").to_string()).collect()
}

fn text(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        _ => String::new(),
    }
}
