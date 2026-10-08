//! THE GATE'S JUDGEMENT - three rules over counts a host already took: N source lines per file, N files
//! per folder, N non-blank lines per doc, plus no dangling local `.md` link, judged against the size
//! ratchet. The counting stays where the parsers are (Roslyn for C#, the hosts for PowerShell and
//! TypeScript); everything decided from the counts is here.
//!
//! THE LAST TENTH OF EVERY LIMIT IS NOT SPENDABLE. A `NEAR LIMIT` warning under a headline that said OK was
//! a warning nobody acted on, so the share IS the ceiling - 450 of 500 source lines, 14 of 15 files, 180 of
//! 200 doc lines - and a file inside that last tenth fails while the split is still small.

pub(crate) mod cache;
mod counts;
pub(crate) use counts::projectish;
pub(crate) mod docs;
mod ratchet;
pub(crate) mod run;

use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;

const CEILING_SHARE: f64 = 0.9;

/// One doc: where it is, and how many non-blank lines it has.
#[derive(Deserialize)]
pub(crate) struct Doc {
    pub rel: String,
    pub abs: String,
    pub lines: i64,
}

#[derive(Deserialize, Default)]
#[serde(default)]
pub(crate) struct Input {
    /// `[rel, n]` IN THE ORDER THE WALK MET THEM - ties in every "largest first" list keep that order.
    pub files: Vec<(String, i64)>,
    pub folders: Vec<(String, i64)>,
    pub docs: Vec<Doc>,
    /// Docs per folder - never failed, only consulted so the split advice knows whether a sibling fits.
    pub doc_folders: BTreeMap<String, i64>,
    pub max_source_lines: i64,
    pub max_files_per_dir: i64,
    pub max_doc_lines: i64,
    pub baseline_path: Option<String>,
    pub strict: bool,
    pub update_baseline: bool,
    pub dump: bool,
    pub worst: bool,
}


struct Entry {
    kind: &'static str,
    rel: String,
    n: i64,
    limit: i64,
}

pub(crate) fn judge(input: &Input) -> Value {
    let doc_counts: Vec<(String, i64)> = input.docs.iter().map(|d| (d.rel.clone(), d.lines)).collect();
    if input.dump {
        return json!({ "dump": {
            "files": ratchet::counts(&input.files),
            "dirs": ratchet::counts(&input.folders),
            "docs": ratchet::counts(&doc_counts),
        }});
    }
    let over_files: Vec<(String, i64)> = input.files.iter().filter(|(_, n)| *n > input.max_source_lines).cloned().collect();
    let over_dirs: Vec<(String, i64)> = input.folders.iter().filter(|(_, n)| *n > input.max_files_per_dir).cloned().collect();
    if input.update_baseline {
        // FROZEN FROM THE CEILING, which is where the gate stops - frozen from the limit, a file in the last tenth
        // that predates the gate left the tree red on the day it was wired in.
        let at_ceiling = |counts: &[(String, i64)], limit: i64| -> Vec<(String, i64)> {
            counts.iter().filter(|(_, n)| *n >= ceiling(limit)).cloned().collect()
        };
        let (files, dirs) = (at_ceiling(&input.files, input.max_source_lines), at_ceiling(&input.folders, input.max_files_per_dir));
        let path = input.baseline_path.clone().unwrap_or_default();
        return match ratchet::write(&path, input.max_source_lines, input.max_files_per_dir, &files, &dirs) {
            Ok(()) => json!({ "baseline_written": format!(
                "baseline written: {} file(s), {} folder(s) -> {path}", files.len(), dirs.len()) }),
            Err(why) => json!({ "error": why }),
        };
    }

    let mut entries: Vec<Entry> = Vec::new();
    entries.extend(input.files.iter().map(|(rel, n)| Entry { kind: "file", rel: rel.clone(), n: *n, limit: input.max_source_lines }));
    entries.extend(input.folders.iter().map(|(rel, n)| Entry { kind: "folder", rel: format!("{rel}/"), n: *n, limit: input.max_files_per_dir }));
    entries.extend(input.docs.iter().map(|d| Entry { kind: "doc", rel: d.rel.clone(), n: d.lines, limit: input.max_doc_lines }));

    let mut problems = Vec::new();
    let recorded = if input.strict { ratchet::Recorded::default() } else { ratchet::load(input.baseline_path.as_deref(), &mut problems) };
    ratchet::judge(&mut problems, &input.files, &over_files, &recorded.files, ceiling(input.max_source_lines), "file",
        &format!("over the {}-source-line limit — split them into logical modules", input.max_source_lines));
    ratchet::judge(&mut problems, &input.folders, &over_dirs, &recorded.dirs, ceiling(input.max_files_per_dir), "folder",
        &format!("holding more than {} source files — group them into logical subfolders", input.max_files_per_dir));

    // NO RATCHET FOR DOCS: a doc is always splittable, so there is no legacy debt that cannot be paid the day
    // it is found. The folder's own count decides WHERE the split goes.
    let folders: BTreeMap<&str, i64> = input.folders.iter().map(|(f, n)| (f.as_str(), *n)).collect();
    let mut long: Vec<&Doc> = input.docs.iter().filter(|d| d.lines > input.max_doc_lines).collect();
    long.sort_by(|a, b| b.lines.cmp(&a.lines));
    for doc in long {
        let folder = folder(&doc.rel);
        let crowd = docs::Crowding {
            files: folders.get(folder.as_str()).copied().unwrap_or(0) + input.doc_folders.get(&folder).copied().unwrap_or(0),
            limit: input.max_files_per_dir,
        };
        let text = std::fs::read_to_string(&doc.abs).unwrap_or_default();
        problems.push(docs::too_long(&doc.abs, &doc.rel, doc.lines, &text, input.max_doc_lines, &crowd));
    }
    for doc in &input.docs {
        let text = std::fs::read_to_string(&doc.abs).unwrap_or_default();
        problems.extend(docs::broken_links(&doc.rel, &doc.abs, &text));
    }

    // AFTER the ratchet, so a file the baseline holds is named once: this is the band UP TO the limit.
    let ratio = |e: &Entry| e.n as f64 / e.limit as f64;
    let mut band: Vec<&Entry> =
        entries.iter().filter(|e| e.n as f64 >= e.limit as f64 * CEILING_SHARE && e.n <= e.limit).collect();
    band.sort_by(|a, b| ratio(b).total_cmp(&ratio(a)));
    for e in band {
        // DEBT THE BASELINE HOLDS is the ratchet's to judge: held while it does not grow, and GREW once it does.
        let held = match e.kind {
            "file" => recorded.files.get(&e.rel),
            "folder" => recorded.dirs.get(e.rel.trim_end_matches('/')),
            _ => None,
        };
        if let Some(&was) = held {
            if e.n > was {
                problems.push(format!("{} (baseline {was})  {}   +{} — baseline {} GREW; existing debt may only shrink",
                    e.n, e.rel, e.n - was, e.kind));
            }
            continue;
        }
        problems.push(format!(
            "{}: {}/{} is inside the last tenth of the limit (ceiling {}) — {}",
            e.rel, e.n, e.limit, ceiling(e.limit), split(e.kind)
        ));
    }

    if input.worst {
        let mut ranked: Vec<&Entry> = entries.iter().collect();
        ranked.sort_by(|a, b| ratio(b).total_cmp(&ratio(a)));
        let lines: Vec<String> =
            ranked.iter().take(15).map(|e| format!("  {:>4}/{}  {:<6} {}", e.n, e.limit, e.kind, e.rel)).collect();
        return json!({ "worst": lines });
    }

    let largest = |kind: &str| entries.iter().filter(|e| e.kind == kind).map(|e| e.n).max().unwrap_or(0);
    let count = |kind: &str| entries.iter().filter(|e| e.kind == kind).count();
    // The CEILING is what the run enforced, so the green line names the ceiling and the limit it comes from.
    let ok = format!(
        "structuregate OK: {} source files within {} lines (90 % of {}; largest {}{}), {} folders within {} files{}, \
         {} docs within {} lines (largest {}).",
        count("file"), ceiling(input.max_source_lines), input.max_source_lines, largest("file"), held(recorded.files.len()),
        input.folders.len(), ceiling(input.max_files_per_dir), held(recorded.dirs.len()),
        count("doc"), ceiling(input.max_doc_lines), largest("doc")
    );
    json!({ "problems": problems, "ok": ok })
}

/// The number a limit really stops at, printed so the author can aim at it.
fn ceiling(limit: i64) -> i64 {
    (limit as f64 * CEILING_SHARE).ceil() as i64
}

/// What to do about it, which differs by what was measured.
fn split(kind: &str) -> &'static str {
    match kind {
        "doc" => "split it into a sibling doc in the SAME folder and link it",
        "folder" => "group the files into logical subfolders",
        _ => "split it into logical modules",
    }
}

fn held(n: usize) -> String {
    if n > 0 { format!(", {n} held in the baseline") } else { String::new() }
}

/// A file's folder with `/`, or `.` at the top - the key the folder counts are taken under.
pub(crate) fn folder(rel: &str) -> String {
    let rel = rel.replace('\\', "/");
    match rel.rfind('/') {
        Some(at) if at > 0 => rel[..at].to_string(),
        _ => ".".to_string(),
    }
}
