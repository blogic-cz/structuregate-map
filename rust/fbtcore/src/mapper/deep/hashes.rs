//! WHAT EACH FILE HOLDS, WITHOUT READING THE FILES THAT DID NOT MOVE. The deep C# and SQL halves stamp every
//! file's rows with a hash of its content, and taking that hash by reading every file made a per-turn run pay
//! for thousands of files to find the few that changed. The tree map (`fbt`) already answers it: from the NTFS
//! journal where it can, from a walk that re-reads only what size and mtime say moved where it cannot - the
//! same engine that keys the gate's pass cache.
//!
//! ITS SNAPSHOT LIVES IN THE TREE, under `.fbt/deep.sqlite`, beside the gate's, and is bounded by the map's
//! own `--skip`, the root's `.gitignore` and the walk's default skips. The map's database is pruned from the walk by path with its SQLite siblings: it changes
//! BECAUSE of the run, so a tree holding it could never read as unchanged.
//!
//! A FILE THE SNAPSHOT DOES NOT NAME - outside every root, reached through a junction, pruned by a skip - is
//! read and hashed as before. Nothing is ever guessed: the hash is either the tree map's or the file's own.

use fbt::pipeline::Engine;
use fbt::scan::ScanOptions;
use std::collections::HashMap;
use std::path::Path;

pub struct Hashes {
    /// `(root label, folded as the tree map keys it, ending in '/') -> {relative path key -> content hash}`.
    roots: Vec<(String, HashMap<String, String>)>,
    /// Every root's merkle hash, in order - None when any root could not be snapshotted.
    tree: Option<String>,
    /// What bringing the snapshots up to date cost, over every root.
    pub read: Read,
    /// Each root that could not be snapshotted, and why - said in the run's notes, never dropped in silence.
    pub failed: Vec<String>,
}

/// HOW MUCH THE HASH READ (#144): the entries the snapshots name, the files read from disk and what those held. The
/// timing note says it, so a hash phase that dwarfs every half shows whether a few large files or many small ones did it.
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Read {
    pub entries: usize,
    pub files: usize,
    pub bytes: u64,
}

impl Hashes {
    /// Every root's snapshot, brought up to date. A root the tree map cannot open answers nothing, and its
    /// files are read instead.
    pub fn of(roots: &[String], skip: &[String], avoid: &[&str]) -> Hashes {
        let mut failed = Vec::new();
        let taken: Vec<(String, HashMap<String, String>, String, Read)> = roots.iter()
            .filter_map(|root| snapshot(root, skip, avoid).map_err(|why| failed.push(format!("{root} - {why}"))).ok())
            .collect();
        let tree = (taken.len() == roots.len() && !roots.is_empty()).then(|| taken.iter().map(|(_, _, h, _)| h.as_str()).collect::<Vec<_>>().join("+"));
        let read = taken.iter().fold(Read::default(), |sum, (_, _, _, r)| Read { entries: sum.entries + r.entries, files: sum.files + r.files, bytes: sum.bytes + r.bytes });
        Hashes { roots: taken.into_iter().map(|(r, f, _, _)| (r, f)).collect(), tree, read, failed }
    }

    /// WHAT THE WHOLE TREE IS NOW, as one string: the merkle hash of every root. Equal to what a half's
    /// rows were written against, nothing under any root has moved since.
    pub fn tree(&self) -> Option<&str> {
        self.tree.as_deref()
    }

    /// WHAT ONE FOLDER IS NOW: every file the snapshots name under `dir`, with its hash, as one string. None when
    /// a root could not be snapshotted or no root holds the folder - the caller then keys by the whole tree.
    pub fn under(&self, dir: &Path) -> Option<String> {
        self.tree.as_ref()?;
        let at = format!("{}/", label(dir)?.trim_end_matches('/'));
        let mut files: Vec<(String, &str)> = Vec::new();
        let mut held = false;
        for (root, entries) in &self.roots {
            // A ROOT INSIDE THE FOLDER IS WHOLLY IN IT; a folder inside a root is the part of it under the folder.
            let inside = if root.starts_with(&at) { Some(&root[at.len()..]) } else { None };
            let part = if inside.is_none() { at.strip_prefix(root.as_str()) } else { None };
            if inside.is_none() && part.is_none() {
                continue;
            }
            held = true;
            for (rel, hash) in entries {
                match (inside, part) {
                    (Some(prefix), _) => files.push((format!("{prefix}{rel}"), hash)),
                    (None, Some(part)) if rel.starts_with(part) => files.push((rel[part.len()..].to_string(), hash)),
                    _ => {}
                }
            }
        }
        if !held {
            return None;
        }
        files.sort();
        let mut hasher = blake3::Hasher::new();
        for (rel, hash) in files {
            hasher.update(rel.as_bytes()).update(b"	").update(hash.as_bytes()).update(b"
");
        }
        Some(hasher.finalize().to_hex().to_string())
    }

    /// WHAT A HALF READS, rather than the whole tree: every PATH under every root (so a file added or gone - an
    /// `angular.json` among them - still moves it), the CONTENT of the files the half is given, and the content of
    /// every file `also` names (the project files). Keyed by the whole tree, a `.cs` edit started node twice to
    /// hear "unchanged" on every turn. None when a root or a given file is not in a snapshot.
    ///
    /// `every_path` false leaves the paths out but the project files: a half that re-reads only by its GIVEN files (python's
    /// `PyDeps.rebound`, the plain TypeScript half's `rebound`, both over the given set) started its host for any file
    /// added anywhere, to re-read nothing. The given LIST is in the caller's key.
    pub fn scoped(&self, given: &[(String, String)], also: fn(&str) -> bool, every_path: bool) -> Option<String> {
        self.tree.as_ref()?;
        let mut hasher = blake3::Hasher::new();
        let mut paths: Vec<(&str, &str, &str)> = self.roots.iter()
            .flat_map(|(root, files)| files.iter().map(move |(rel, hash)| (root.as_str(), rel.as_str(), hash.as_str())))
            .collect();
        paths.sort();
        // `.fbt/` IS THIS TOOL'S OWN, written by the first run: in the set, the second run read as a changed tree and
        // started node to hear "unchanged".
        for (root, rel, hash) in paths.into_iter().filter(|(_, rel, _)| !rel.starts_with(".fbt/") && (every_path || also(rel))) {
            hasher.update(root.as_bytes()).update(rel.as_bytes()).update(b"\t");
            if also(rel) {
                hasher.update(hash.as_bytes());
            }
            hasher.update(b"\n");
        }
        let mut contents: Vec<(&str, &str)> = Vec::new();
        for (rel, abs) in given {
            contents.push((rel.as_str(), self.get(abs)?));
        }
        contents.sort();
        for (rel, hash) in contents {
            hasher.update(rel.as_bytes()).update(b"\t").update(hash.as_bytes()).update(b"\n");
        }
        Some(format!("scoped:{}", hasher.finalize().to_hex()))
    }

    /// The tree map's hash of a file's content, or None when no snapshot names it.
    pub fn get(&self, abs: &str) -> Option<&str> {
        let key = label(Path::new(abs))?;
        self.roots.iter().find_map(|(root, files)| key.strip_prefix(root.as_str()).and_then(|rel| files.get(rel)).map(String::as_str))
    }
}

/// One root's snapshot, or why the tree map could not take it.
fn snapshot(root: &str, skip: &[String], avoid: &[&str]) -> Result<(String, HashMap<String, String>, String, Read), String> {
    let root_path = std::fs::canonicalize(root).map_err(|e| format!("the root could not be resolved: {e}"))?;
    let prefix = format!("{}/", label_of(&root_path).trim_end_matches('/'));
    let mut skip_paths = Vec::new();
    // WHAT THIS TOOL WRITES INTO THE TREE - the map's own database with its SQLite siblings, the file-level
    // map - changes because of a run, so a tree holding it could never read as unchanged.
    for written in avoid {
        if let Some(rel) = label(Path::new(written)).and_then(|d| d.strip_prefix(&prefix).map(String::from)) {
            // AND THE LAST RUN'S TRACE beside it (`trace::LAST_RUN`), rewritten by every run.
            for suffix in ["", "-wal", "-shm", "-journal", crate::trace::LAST_RUN] {
                skip_paths.push(format!("{rel}{suffix}"));
            }
        }
    }
    // Nor is the tree map's own state, which the gate rewrites on its own schedule.
    let mut extra_skip = skip.to_vec();
    extra_skip.push(".fbt".into());
    // NOR WHAT GIT IGNORES OR NO TREE MAPS (#148): `.git`, `node_modules`, build output, an ignored dataset. Walking them
    // stored every entry of a folder no half reads, and a few ignored folders made a tree of two source files hash for
    // seconds; a file a half does read from there (`obj/project.assets.json`) is one the snapshot does not name, hashed itself.
    let options = ScanOptions { use_gitignore: true, no_default_skip: false, extra_skip, skip_paths, rehash_all: false };
    let store = root_path.join(".fbt").join("deep.sqlite");
    let mut engine = Engine::open(&store, &root_path, options).map_err(|e| format!("{} could not be opened: {e:#}", store.display()))?;
    crate::tree::ignored(&root_path);
    let stage = crate::trace::stage(&format!("deep: hash {}", root_path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()));
    let refreshed = engine.refresh(true, true, false).map_err(|e| format!("the snapshot was not taken: {e:#}"))?;
    // HOW THE SNAPSHOT WAS BROUGHT UP TO DATE, said in the trace: the journal or a walk, over how many entries, and how
    // many files it had to read - a large tree spent seconds here on a run that changed nothing, and nothing said why.
    stage.set("structuregate.hash.method", refreshed.method.label());
    stage.set("structuregate.hash.entries", refreshed.entry_count as i64);
    stage.set("structuregate.hash.read", refreshed.read_from_disk as i64);
    stage.set("structuregate.hash.bytes", refreshed.bytes_read as i64);
    stage.set("structuregate.hash.changes", refreshed.changes.len() as i64);
    if let Some(note) = &refreshed.usn_note {
        stage.set("structuregate.hash.journal", note.as_str());
    }
    let merkle = refreshed.root_hash.as_ref().map(fbt::hash::hex).ok_or("the snapshot has no root hash")?;
    let latest = engine.latest().ok().flatten().ok_or("no snapshot was stored")?;
    let entries = fbt::db::load_entries(&engine.conn, latest.id).map_err(|e| format!("the snapshot could not be read: {e:#}"))?;
    let files = entries
        .into_iter()
        .filter(|(_, e)| e.kind == fbt::entry::Kind::File)
        .filter_map(|(key, e)| e.hash.map(|h| (key, fbt::hash::hex(&h))))
        .collect();
    let read = Read { entries: refreshed.entry_count, files: refreshed.read_from_disk, bytes: refreshed.bytes_read };
    Ok((prefix, files, merkle, read))
}

/// A path as the tree map keys it: absolute, `/`-separated, folded as its keys are (`fbt::entry::fold`), no `\\?\` prefix.
fn label(path: &Path) -> Option<String> {
    Some(label_of(&std::path::absolute(path).ok()?))
}

fn label_of(path: &Path) -> String {
    fbt::entry::fold(&path.to_string_lossy().trim_start_matches("\\\\?\\").replace('\\', "/"))
}
