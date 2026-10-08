//! WHY THE DEEP C# HALF RE-READS A FILE, told apart and counted. A pull changed a few hundred `.cs` files, the next
//! refresh re-read thousands and took minutes, and the output said only the count: a file's recorded sha folds its content,
//! its project's fingerprint and the rows version into one hash, so the hash alone cannot say which of them moved.
//!
//! SO THE FINGERPRINT'S PARTS ARE KEPT, by project, in `_meta` as `fingerprints:csharp` - written only when one moved,
//! so a run that changed nothing writes nothing. A moved file's sha is then folded again from its content NOW and its
//! project's parts THEN: equal to what was recorded, its content did not move and its project did - and the parts that
//! differ say which input it was. Nothing is guessed and nothing is read: one SHA-256 per moved file. A database with no
//! kept parts (the first run of this build) says "its content or project moved", as it always did.
//!
//! The key is not one the passes over the finished rows read (`derived.rs`): they key on `counter:`, `setup:`, `schema`.

use super::driven::folded;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
use std::path::Path;

const KEPT: &str = "fingerprints:csharp";
/// How many projects the note names; `_meta` keeps more.
const NAMED: usize = 5;
const KEPT_PROJECTS: usize = 20;

/// Every project's fingerprint parts `(name, hash)`, keyed by its path as spelled on disk.
pub(super) type Parts = BTreeMap<String, Vec<(String, String)>>;

/// THE SHA A FILE IS RECORDED WITH: its content, its project's fingerprint, the rows version - and whether
/// `--map-exclude` matches it. The one place it is folded, so the reasons below fold it the same way.
pub(super) fn salted(content: &str, fingerprint: Option<&str>, version: &str, excluded: bool) -> String {
    let salted = match fingerprint {
        None => folded(&format!("{content}|{version}")),
        Some(fingerprint) => folded(&format!("{content}|{fingerprint}|{version}")),
    };
    if excluded { folded(&format!("{salted}|excluded")) } else { salted }
}

/// A project's fingerprint, as its parts join into it.
pub(super) fn joined(parts: &[(String, String)]) -> String {
    parts.iter().map(|(_, hash)| hash.as_str()).collect::<Vec<_>>().join("+")
}

/// What one C# file was hashed from on this run.
pub(super) struct File {
    pub content: String,
    /// Its project's path - the key of `Parts`.
    pub project: Option<String>,
    pub excluded: bool,
}

/// What the last run kept: the rows version and every project's parts.
pub(super) struct Before {
    version: String,
    projects: Parts,
}

pub(super) fn before(db: &str) -> Option<Before> {
    let conn = rusqlite::Connection::open_with_flags(db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).ok()?;
    let text: String = conn.query_row("SELECT value FROM _meta WHERE key = ?1", [KEPT], |r| r.get(0)).ok()?;
    let kept: Value = serde_json::from_str(&text).ok()?;
    let pair = |p: &Value| (p[0].as_str().unwrap_or("").to_string(), p[1].as_str().unwrap_or("").to_string());
    let projects = kept["projects"].as_object()?.iter()
        .map(|(project, parts)| (project.clone(), parts.as_array().into_iter().flatten().map(pair).collect()))
        .collect();
    Some(Before { version: kept["version"].as_str()?.to_string(), projects })
}

/// THE PARTS, KEPT FOR THE NEXT RUN - only when they moved: a run that changed nothing writes nothing.
pub(super) fn keep(db: &str, version: &str, now: &Parts, before: Option<&Before>) {
    if before.is_some_and(|b| b.version == version && &b.projects == now) || !Path::new(db).is_file() {
        return;
    }
    let value = json!({ "version": version, "projects": now }).to_string();
    let Ok(conn) = rusqlite::Connection::open(db) else { return };
    let _ = conn.busy_timeout(std::time::Duration::from_secs(5));
    let _ = conn.execute_batch("CREATE TABLE IF NOT EXISTS _meta (key TEXT, value TEXT)");
    let _ = conn.execute("DELETE FROM _meta WHERE key = ?1", [KEPT]);
    let _ = conn.execute("INSERT INTO _meta (key, value) VALUES (?1, ?2)", [KEPT, value.as_str()]);
}

/// Why one moved file is read again.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub(super) enum Cause {
    Content,
    Project,
    New,
    Version,
    Exclusion,
    Rebuilt,
    Unknown,
}

impl Cause {
    fn key(self) -> &'static str {
        match self {
            Cause::Content => "content",
            Cause::Project => "project",
            Cause::New => "new",
            Cause::Version => "rows_version",
            Cause::Exclusion => "exclusion",
            Cause::Rebuilt => "rebuilt",
            Cause::Unknown => "content_or_project",
        }
    }
    fn said(self) -> &'static str {
        match self {
            Cause::Content => "whose content moved",
            Cause::Project => "whose project's inputs moved",
            Cause::New => "not recorded before",
            Cause::Version => "written by an older C# extractor",
            Cause::Exclusion => "whose --map-exclude match moved",
            Cause::Rebuilt => "in a database that is rebuilt",
            Cause::Unknown => "whose content or project moved (the run before kept no fingerprints to tell which - from this run on it does)",
        }
    }
}

/// THE CAUSE OF ONE RE-READ, from the sha it was recorded with.
pub(super) fn cause(file: &File, recorded: Option<&str>, rebuild: bool, before: Option<&Before>, now: &Parts) -> Cause {
    let Some(recorded) = recorded else { return if rebuild { Cause::Rebuilt } else { Cause::New } };
    if rebuild {
        return Cause::Rebuilt;
    }
    // NO FINGERPRINTS KEPT - the first run of a build that keeps them, as one upgrade to v1.5.11 was: a file whose
    // content and project are what they are NOW and whose sha an OLDER rows version made was re-read for the version
    // alone, and that is said rather than "content or project" for every file.
    let Some(before) = before else { return if older_version(file, recorded, now) { Cause::Version } else { Cause::Unknown } };
    let then = match &file.project {
        None => None,
        Some(project) => match before.projects.get(project) {
            Some(parts) => Some(joined(parts)),
            None => return Cause::Unknown,
        },
    };
    if salted(&file.content, then.as_deref(), &before.version, file.excluded) == recorded {
        // THE CONTENT IS WHAT IT WAS: the version or the project moved under it.
        return if before.version != super::driven::CS_ROWS_VERSION { Cause::Version } else if then.is_some() { Cause::Project } else { Cause::Unknown };
    }
    if salted(&file.content, then.as_deref(), &before.version, !file.excluded) == recorded {
        return Cause::Exclusion;
    }
    Cause::Content
}

/// Whether the recorded sha is this file's content and project folded with an EARLIER rows version.
fn older_version(file: &File, recorded: &str, now: &Parts) -> bool {
    let fingerprint = match &file.project {
        None => None,
        Some(project) => match now.get(project) {
            Some(parts) => Some(joined(parts)),
            None => return false,
        },
    };
    let current: u32 = super::driven::CS_ROWS_VERSION.parse().unwrap_or(0);
    (1..current).rev().any(|version| salted(&file.content, fingerprint.as_deref(), &version.to_string(), file.excluded) == recorded)
}

/// What the half says about its re-reads: the note, the note by project, and the facts `_meta` keeps.
pub(super) struct Explained {
    pub notes: Vec<String>,
    pub facts: Value,
}

/// EVERY MOVED FILE'S CAUSE, counted, and the projects that forced the most, with the inputs of theirs that moved.
#[allow(clippy::too_many_arguments)]
pub(super) fn explain(moved: &[&String], files: &HashMap<String, File>, recorded: &HashMap<String, String>, rebuild: bool,
    before: Option<&Before>, now: &Parts, gone: usize, root: &str) -> Explained {
    let mut causes: BTreeMap<Cause, usize> = BTreeMap::new();
    let mut by_project: HashMap<&str, usize> = HashMap::new();
    let mut named = Vec::new();
    for rel in moved {
        let Some(file) = files.get(rel.as_str()) else { continue };
        let cause = cause(file, recorded.get(rel.as_str()).map(String::as_str), rebuild, before, now);
        *causes.entry(cause).or_default() += 1;
        if let (Cause::Project, Some(project)) = (cause, &file.project) {
            *by_project.entry(project.as_str()).or_default() += 1;
        }
        if named.len() < 3 {
            named.push(format!("{rel} ({})", cause.said()));
        }
    }
    let counted: Vec<String> = causes.iter().map(|(cause, n)| format!("{n} {}", cause.said())).collect();
    let mut notes = vec![format!("the deep C# half re-reads {} file(s){}: {} - e.g. {}{}", moved.len(),
        if gone > 0 { format!(" and drops {gone}") } else { String::new() }, counted.join(", "), named.join(", "),
        if moved.len() > named.len() { ", …" } else { "" })];
    let mut projects: Vec<(&str, usize)> = by_project.into_iter().collect();
    projects.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    let kept: Vec<Value> = projects.iter().take(KEPT_PROJECTS)
        .map(|(project, n)| json!({ "project": relative(root, project), "files": n, "moved": moved_parts(project, now, before, root) }))
        .collect();
    if !projects.is_empty() {
        let listed: Vec<String> = kept.iter().take(NAMED)
            .map(|p| format!("{} {} ({})", p["project"].as_str().unwrap_or(""), p["files"], strings(&p["moved"]).join(", ")))
            .collect();
        let more = projects.len().saturating_sub(NAMED);
        notes.push(format!("the C# files re-read because their project's inputs moved, by project: {}{}", listed.join(", "),
            if more > 0 { format!(", … and {more} more project(s)") } else { String::new() }));
    }
    let causes: serde_json::Map<String, Value> = causes.iter().map(|(cause, n)| (cause.key().to_string(), Value::from(*n))).collect();
    let facts = json!({ "reread": moved.len(), "dropped": gone, "causes": causes, "projects": kept, "examples": named });
    Explained { notes, facts }
}

/// The names of a project's parts that moved since the last run - a part gone counts as moved.
fn moved_parts(project: &str, now: &Parts, before: Option<&Before>, root: &str) -> Vec<String> {
    let was = before.and_then(|b| b.projects.get(project));
    let parts = now.get(project).map(Vec::as_slice).unwrap_or_default();
    let mut moved: Vec<String> = parts.iter()
        .filter(|(name, hash)| was.is_none_or(|w| !w.iter().any(|(n, h)| n == name && h == hash)))
        .map(|(name, _)| relative(root, name))
        .collect();
    moved.extend(was.into_iter().flatten().filter(|(name, _)| !parts.iter().any(|(n, _)| n == name)).map(|(name, _)| format!("{} (gone)", relative(root, name))));
    moved
}

/// A path under the root as `a/b`; anything else as it is.
fn relative(root: &str, path: &str) -> String {
    let path = path.replace('\\', "/");
    let root = root.replace('\\', "/").to_lowercase();
    let root = root.trim_end_matches('/');
    match path.get(..root.len()) {
        Some(head) if head.to_lowercase() == root && path[root.len()..].starts_with('/') => path[root.len() + 1..].to_string(),
        _ => path,
    }
}

fn strings(value: &Value) -> Vec<String> {
    value.as_array().into_iter().flatten().filter_map(|v| v.as_str().map(String::from)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parts(assets: &str) -> Vec<(String, String)> {
        vec![("Demo.csproj".into(), "c1".into()), ("obj/project.assets.json".into(), assets.into())]
    }

    fn setup(assets_then: &str) -> (Before, File) {
        let mut projects = Parts::new();
        projects.insert("/t/demo.csproj".into(), parts(assets_then));
        let before = Before { version: super::super::driven::CS_ROWS_VERSION.into(), projects };
        (before, File { content: "aaaa".into(), project: Some("/t/demo.csproj".into()), excluded: false })
    }

    #[test]
    fn a_file_whose_project_moved_and_content_did_not_is_the_projects() {
        let (before, file) = setup("a1");
        let v = super::super::driven::CS_ROWS_VERSION;
        let recorded = salted("aaaa", Some(&joined(&parts("a1"))), v, false);
        assert_eq!(cause(&file, Some(&recorded), false, Some(&before), &Parts::new()), Cause::Project);
    }

    #[test]
    fn a_file_whose_content_moved_is_its_contents() {
        let (before, file) = setup("a1");
        let v = super::super::driven::CS_ROWS_VERSION;
        let recorded = salted("bbbb", Some(&joined(&parts("a1"))), v, false);
        assert_eq!(cause(&file, Some(&recorded), false, Some(&before), &Parts::new()), Cause::Content);
    }

    #[test]
    fn without_fingerprints_an_older_rows_version_is_still_told() {
        // A consumer's upgrade: nothing kept, every file's content and project as they were, an older rows version.
        let (before, file) = setup("a1");
        let older = salted("aaaa", Some(&joined(&parts("a1"))), "3", false);
        assert_eq!(cause(&file, Some(&older), false, None, &before.projects), Cause::Version);
        let moved = salted("bbbb", Some(&joined(&parts("a1"))), "3", false);
        assert_eq!(cause(&file, Some(&moved), false, None, &before.projects), Cause::Unknown);
    }

    #[test]
    fn new_rebuilt_unknown_version_and_exclusion_are_told_apart() {
        let (before, file) = setup("a1");
        let fp = joined(&parts("a1"));
        assert_eq!(cause(&file, None, false, Some(&before), &Parts::new()), Cause::New);
        assert_eq!(cause(&file, Some("x"), true, Some(&before), &Parts::new()), Cause::Rebuilt);
        assert_eq!(cause(&file, Some("x"), false, None, &Parts::new()), Cause::Unknown);
        let old = Before { version: "0".into(), projects: before.projects.clone() };
        assert_eq!(cause(&file, Some(&salted("aaaa", Some(&fp), "0", false)), false, Some(&old), &Parts::new()), Cause::Version);
        let excluded = salted("aaaa", Some(&fp), super::super::driven::CS_ROWS_VERSION, true);
        assert_eq!(cause(&file, Some(&excluded), false, Some(&before), &Parts::new()), Cause::Exclusion);
    }

    #[test]
    fn a_project_note_names_the_input_that_moved_under_the_root() {
        let (before, file) = setup("a1");
        let mut now = Parts::new();
        now.insert("/t/demo.csproj".into(), parts("a2"));
        let recorded: HashMap<String, String> = [("A.cs".to_string(), salted("aaaa", Some(&joined(&parts("a1"))), super::super::driven::CS_ROWS_VERSION, false))].into();
        let files: HashMap<String, File> = [("A.cs".to_string(), file)].into();
        let rel = "A.cs".to_string();
        let said = explain(&[&rel], &files, &recorded, false, Some(&before), &now, 0, "/t");
        assert_eq!(said.facts["causes"]["project"], 1);
        assert_eq!(said.facts["projects"][0]["project"], "demo.csproj");
        assert_eq!(said.facts["projects"][0]["moved"][0], "obj/project.assets.json");
        assert!(said.notes[1].contains("demo.csproj 1 (obj/project.assets.json)"), "{:?}", said.notes);
    }
}
