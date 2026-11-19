//! THE MARKDOWN HALF OF THE DEEP MAP: a `files` row per doc the gate measures (`lang = 'markdown'`, its title or
//! frontmatter summary in `doc`), a `md_sections` row per heading, and a `md_names` row per name a reader would
//! follow - a path it claims (a link target, a code span, a word of a shell block) or a dotted symbol a code span
//! names. Stored through the same door the other halves use (`rows::apply`), so what is recorded, what has gone and
//! what disagrees are all scoped by its `lang`.
//!
//! A PATH IS RESOLVED HERE, by the file map's own rules (`super::resolve`), so `md_names` and the JSON map's
//! `mentions` say the same of the same name. Whether a path is THERE depends on the tree as well as the doc, so a
//! doc's sha carries the paths the walk saw: a file added or gone reads every doc again - a doc is cheap to parse.
//! A SYMBOL is only named here; which row it is, is the pass over the finished rows (`links.rs`), since every other
//! half has to have written its rows first.

use super::{names, read, resolve, Found, Place};
use crate::mapper::protocol::Collector;
use crate::rows;
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const LANG: &str = "markdown";
/// Part of every doc's sha: changing what a doc's rows hold changes this, and every doc is read again.
const ROWS_VERSION: &str = "markdown-rows 2";

/// The docs of one root - or of the `--doc-root` - with what their names resolve against.
pub struct Docs {
    pub root: String,
    /// What a path found under this root is keyed by in the map: the root's prefix.
    pub prefix: String,
    /// `(rel, abs)`, `rel` from `root`, without the prefix.
    pub files: Vec<(String, String)>,
    /// Every file the walk saw under `root` (`names::Tree`).
    pub tree: Vec<String>,
    /// Under `--doc-root`: the code roots with their prefixes, and the docs it maps - a found path is keyed by
    /// `docroot::keyed`, exactly as the file map keys it.
    pub doc_root: Option<(Vec<(String, String)>, Vec<String>)>,
    /// Every code root with its prefix: a path found through one of them is keyed by it (`docroot::under_roots`).
    pub roots: Vec<(String, String)>,
    /// The folders tried last (`Place::also`).
    pub also: Vec<PathBuf>,
}

impl Docs {
    /// A found path as the map keys it: under the doc root by `docroot::keyed`, else the root's prefix before it;
    /// an absolute path names another tree and stays as it is.
    fn keyed(&self, path: &str) -> String {
        if let Some((roots, mapped)) = &self.doc_root {
            let mapped: std::collections::HashSet<&str> = mapped.iter().map(String::as_str).collect();
            return crate::mapper::docroot::keyed(&self.root, roots, &mapped, path);
        }
        if Path::new(path).is_absolute() || path.as_bytes().get(1) == Some(&b':') {
            return crate::mapper::docroot::under_roots(&self.roots, path);
        }
        format!("{}{path}", self.prefix)
    }
}

pub fn run(into: &mut Collector, db: &str, root: &str, all: &[Docs]) {
    let state = rows::state(Path::new(db), LANG, false);
    let count: usize = all.iter().map(|d| d.files.len()).sum();
    // NO DOC HERE AND NONE RECORDED: nothing to write, and no database to create for it.
    if count == 0 && state.shas.is_empty() {
        return;
    }
    // (docs, key, abs, text), one per doc that could be read.
    let mut texts: Vec<(&Docs, String, &str, &str, String)> = Vec::new();
    for docs in all {
        // NOT WHAT THIS RUN WRITES: the database, its sidecars and the `.fbt` cache appear after a first run, and in
        // the stamp they read every doc again on the second.
        let ours = |rel: &&str| {
            let abs = Path::new(&docs.root).join(rel);
            rel.starts_with(".fbt/") || abs.to_string_lossy().starts_with(db)
        };
        let mut seen: Vec<&str> = docs.tree.iter().map(String::as_str).filter(|rel| !ours(rel)).collect();
        seen.sort_unstable();
        let stamp = folded(&seen.join("\n"));
        for (rel, abs) in &docs.files {
            // An unreadable doc is named by the file-level pass over the same tree; here it has no rows.
            let Ok(text) = std::fs::read_to_string(abs) else { continue };
            let text = text.trim_start_matches('\u{feff}').to_string();
            let sha = folded(&format!("{ROWS_VERSION}\n{stamp}\n{text}"));
            texts.push((docs, sha, rel, abs, text));
        }
    }
    let key = |d: &Docs, rel: &str| if d.doc_root.is_some() { rel.to_string() } else { format!("{}{rel}", d.prefix) };
    let shas: Map<String, Value> = texts.iter().map(|(d, sha, rel, _, _)| (key(d, rel), Value::String(sha.clone()))).collect();
    for reset in [false, true] {
        let held = if state.rebuild || reset { BTreeMap::new() } else { state.shas.clone() };
        let stale: Vec<&(&Docs, String, &str, &str, String)> =
            texts.iter().filter(|(d, sha, rel, _, _)| held.get(&key(d, rel)) != Some(sha)).collect();
        let mut counters: BTreeMap<String, i64> = if state.rebuild { BTreeMap::new() } else { state.counters.clone() };
        let mut tables: BTreeMap<&str, Vec<Value>> = BTreeMap::new();
        for (docs, sha, rel, _, text) in &stale {
            write_doc(&mut tables, &mut counters, docs, &key(docs, rel), rel, text, sha);
        }
        let batch = rows::Batch {
            all: true,
            first: true,
            final_: true,
            reset,
            shas: shas.clone(),
            read: stale.iter().map(|(d, _, rel, abs, _)| vec![key(d, rel), abs.to_string()]).collect(),
            counters: counters.into_iter().map(|(k, v)| (k, Value::from(v))).collect(),
            tables: tables.into_iter().map(|(k, v)| (k.to_string(), Value::Array(v))).collect(),
            lang: LANG.to_string(),
            ..Default::default()
        };
        let receipt = match rows::apply(Path::new(db), root, &batch) {
            Ok(receipt) => receipt,
            Err(error) => {
                into.errors.push(format!("HALF      markdown rows: the rows were not stored — {error}"));
                return;
            }
        };
        for (table, rows) in receipt.written {
            into.database.insert(table, rows);
        }
        if receipt.retry == 0 {
            return;
        }
        if reset {
            into.errors.push("HALF      markdown rows: doc(s) still disagree with the tree after a full rebuild".into());
            return;
        }
        into.notes.push(format!("the markdown half is re-reading everything: {} doc(s) disagreed with the tree after an incremental pass", receipt.retry));
    }
}

/// The half as the deep map runs it - in process, by `pulldown-cmark`, beside the others; a tree whose last doc has
/// gone still runs it, so its rows go too - in its own trace stage, and how long it took, in ms.
pub fn timed(into: &mut Collector, db: &str, root: &str, all: &[Docs]) -> u64 {
    let started = std::time::Instant::now();
    let stage = crate::trace::stage("deep: markdown");
    stage.set("structuregate.files", all.iter().map(|d| d.files.len()).sum::<usize>() as i64);
    run(into, db, root, all);
    // WHERE A DOC ROOT'S DOCS ARE KEYED FROM: the stale check (`query/fresh.rs`) finds them there, not under a code root.
    let doc_root = all.iter().find(|d| d.doc_root.is_some()).map(|d| std::path::absolute(&d.root).unwrap_or_else(|_| PathBuf::from(&d.root)));
    crate::mapper::deep::refresh::keep(into, db, "doc_root", &doc_root.map(|p| p.to_string_lossy().into_owned()).unwrap_or_default());
    started.elapsed().as_millis() as u64
}

/// A text's hash as 16 hex digits, the width every half's sha has.
fn folded(text: &str) -> String {
    blake3::hash(text.as_bytes()).to_hex()[..16].to_string()
}

/// Ids are a prefix and a counter continued from the database, as every half hands them out.
fn next(counters: &mut BTreeMap<String, i64>, prefix: &str) -> String {
    let n = counters.entry(prefix.to_string()).or_insert(0);
    *n += 1;
    format!("{prefix}:{n}")
}

fn write_doc(tables: &mut BTreeMap<&str, Vec<Value>>, counters: &mut BTreeMap<String, i64>, docs: &Docs, path: &str, rel: &str, text: &str, sha: &str) {
    let doc = read::walk(text);
    let file = next(counters, "f");
    let module = Path::new(rel).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    tables.entry("files").or_default().push(json!({
        "id": file, "path": path, "module": module, "sha": sha, "doc": summary(&doc),
        "lines": text.lines().filter(|l| !l.trim().is_empty()).count() as i64, "errors": 0,
    }));
    // EACH SECTION ITS HEADINGS DOWN TO IT: `Orders > Submitting`.
    let mut ids = vec![String::new()];
    let mut above: Vec<(usize, String)> = Vec::new();
    for (n, section) in doc.sections.iter().enumerate() {
        above.retain(|(level, _)| *level < section.level);
        above.push((section.level, section.heading.clone()));
        let id = next(counters, "ms");
        let trail: Vec<&str> = above.iter().map(|(_, h)| h.as_str()).collect();
        tables.entry("md_sections").or_default().push(json!({
            "id": id, "file": file, "n": n as i64 + 1, "level": section.level as i64, "heading": section.heading,
            "path": trail.join(" > "), "line": section.line as i64,
        }));
        ids.push(id);
    }
    let tree = names::Tree::new(&docs.tree);
    let at = Place { root: Path::new(&docs.root), tree: &tree, anywhere: docs.doc_root.is_some(), also: &docs.also };
    for named in &doc.named {
        let resolved = resolve(&at, rel, named);
        // A SYMBOL is a code span's, and only one that claims no place on disk - `config.json` is a file.
        let placed = matches!(resolved, Some((_, Found::At(_) | Found::Missing)));
        let symbol = (named.kind == "code" && !placed && !names::file_named(&named.text)).then(|| names::symbol_named(&named.text)).flatten();
        let (claimed, target, status) = match resolved {
            Some((path, Found::At(found))) => (path, docs.keyed(&found), "bound"),
            Some((path, Found::Missing)) => (path, String::new(), "missing in code"),
            Some((path, Found::Nothing)) if symbol.is_some() => (path, String::new(), ""),
            None if symbol.is_some() => (String::new(), String::new(), ""),
            _ => continue,
        };
        let id = next(counters, "mn");
        tables.entry("md_names").or_default().push(json!({
            "id": id, "file": file, "section": ids.get(named.section).cloned().unwrap_or_default(), "line": named.line as i64,
            "kind": named.kind, "text": named.text, "path": claimed, "target": target,
            "target_line": if status == "bound" { names::line_named(&named.text) } else { None },
            "status": status, "symbol": symbol.unwrap_or_default(),
        }));
    }
}

/// What a reader sees first: a skill's or an agent's frontmatter name, else its first heading.
fn summary(doc: &read::Doc) -> String {
    match &doc.name {
        Some(name) => name.clone(),
        None => doc.title.clone(),
    }
}
