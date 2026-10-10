//! A FILE THAT DID NOT MOVE IS NOT PARSED AGAIN BY THE FILE MAP. `--map-if-stale` re-parsed EVERY file the moment one
//! was newer than the map: a Stop hook after a one-line edit started PowerShell over all 75 scripts of this repo,
//! 5.1 s of a 6.5 s turn, to learn one file's answer. A half's lines about a file are kept under that file's CONTENT
//! hash - the tree map's, from the snapshot `tree::look` has just refreshed, never a hash taken here - and a half
//! is launched only for the files whose key moved; with none, it is not started at all.
//!
//! WHAT AN ANSWER DEPENDS ON IS THE KEY, beside the content: the BUILD of this tool, the half, its host and its
//! staged script (`stamp`), and EVERY PATH THE WALK SAW - a half resolves an import against the file set, and
//! PowerShell asks the disk whether a dot-sourced path exists, so a file added or gone anywhere asks every file
//! again, as before. C# is the exception: Roslyn reads the one file, and the names are joined here (`own_key`). TypeScript also reads `tsconfig` `paths`, so its stamp carries the project files (`gate::counts`).
//!
//! ONLY HALVES WHOSE LINES ABOUT A FILE COME FROM THAT FILE are kept. Python's do not - a name is bound by reading
//! the module it comes from, and a `declares` check opens the file it names - so it parses everything, as before.
//! Only a run that finished is kept, and only one root: with several, a path is keyed by its root prefix.

use rusqlite::{params, Connection};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub struct Kept {
    store: PathBuf,
    /// Path key -> the tree map's content hash.
    hashes: HashMap<String, String>,
    base: String,
    /// The BUILD alone, for an answer that reads its own file and nothing else (C#): the file set is not in it.
    own: String,
    kept: HashMap<String, String>,
    now: Vec<(String, String)>,
    read: usize,
    /// The map this run is about to replace: who imports what, as the last run resolved it - read once, on demand.
    previous: PathBuf,
    importers: Option<Option<HashMap<String, Vec<String>>>>,
}

impl Kept {
    /// None when the tree map cannot say what the files hold - the map then parses every file, as it always did.
    pub fn open(root: &str, skip: &[String], tracked: bool, build: &str, tree: &[String], outputs: &[&str]) -> Option<Kept> {
        let at = Path::new(root);
        crate::tree::look(at, skip, tracked).ok()?;
        let (store, hashes) = crate::tree::contents(at, skip, tracked)?;
        // THIS TOOL'S OWN FILES ARE NOT THE TREE: `.fbt/` and the maps this run writes appear after the first run, and
        // a set that held them would ask every file again on the second.
        let ours: Vec<String> = outputs.iter()
            .filter_map(|out| Path::new(out).strip_prefix(at).ok().map(|p| p.to_string_lossy().replace('\\', "/")))
            .filter(|rel| !rel.is_empty())
            .collect();
        let mut paths: Vec<String> = tree.iter()
            .filter(|rel| !rel.starts_with(".fbt/") && !ours.iter().any(|out| *rel == out || rel.starts_with(&format!("{out}-"))))
            .cloned()
            .collect();
        paths.sort();
        let base = blake3::hash(format!("{build}\n{}", paths.join("\n")).as_bytes()).to_hex().to_string();
        let own = blake3::hash(format!("{build}\nown").as_bytes()).to_hex().to_string();
        let mut kept = HashMap::new();
        if let Ok(conn) = Connection::open(&store)
            && let Ok(mut rows) = conn.prepare("SELECT key, lines FROM map_kept")
        {
            let read = rows.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)));
            kept.extend(read.into_iter().flatten().flatten());
        }
        let previous = outputs.first().map(PathBuf::from).unwrap_or_default();
        Some(Kept { store, hashes, base, own, kept, now: Vec::new(), read: 0, previous, importers: None })
    }

    /// What a TypeScript answer depends on beyond the file: every file that says which compiler or which project.
    pub fn project(&self) -> String {
        let mut project: Vec<(&String, &String)> = self.hashes.iter().filter(|(path, _)| crate::gate::projectish(path)).collect();
        project.sort();
        let mut hash = blake3::Hasher::new();
        hash.update(std::env::var("NODE_PATH").unwrap_or_default().as_bytes());
        for (path, content) in project {
            hash.update(format!("{path}\t{content}\n").as_bytes());
        }
        hash.finalize().to_hex()[..16].to_string()
    }

    /// This file's key under `stamp`, or None when the snapshot does not name it - it is then parsed.
    pub fn key(&self, stamp: &str, rel: &str) -> Option<String> {
        self.keyed(&self.base, stamp, rel)
    }

    /// This file's key under `stamp` for an answer read off the file ALONE: a file added or gone elsewhere does not
    /// move it. Keyed by the whole file set, a handful of new files asked every C# file of a large tree again.
    pub fn own_key(&self, stamp: &str, rel: &str) -> Option<String> {
        self.keyed(&self.own, stamp, rel)
    }

    fn keyed(&self, base: &str, stamp: &str, rel: &str) -> Option<String> {
        let path = fbt::entry::key_of(&fbt::entry::normalize(rel));
        let content = self.hashes.get(&path)?;
        Some(blake3::hash(format!("{base}\t{stamp}\t{path}\t{content}").as_bytes()).to_hex().to_string())
    }

    /// ONE KEY FOR A WHOLE HALF, for a half whose answer about a file reads other files (python binds a name by
    /// reading the module it comes from): every file it is given, by content, and every file `also` names - what it
    /// reads beyond them (`pyproject.toml`, a `.spec`). Nothing moved, nothing is parsed. None when the snapshot
    /// does not name one of them - the half then runs.
    pub fn whole(&self, stamp: &str, files: &[(String, String)], also: fn(&str) -> bool) -> Option<String> {
        let mut named: Vec<(String, &String)> = Vec::new();
        for (rel, _) in files {
            let path = fbt::entry::key_of(&fbt::entry::normalize(rel));
            let content = self.hashes.get(&path)?;
            named.push((path, content));
        }
        named.extend(self.hashes.iter().filter(|(path, _)| also(path)).map(|(path, content)| (path.clone(), content)));
        named.sort();
        named.dedup();
        let mut hash = blake3::Hasher::new();
        hash.update(format!("{}\twhole\t{stamp}\n", self.base).as_bytes());
        for (path, content) in named {
            hash.update(format!("{path}\t{content}\n").as_bytes());
        }
        Some(hash.finalize().to_hex().to_string())
    }

    /// WHICH TYPESCRIPT COMPILER THESE FILES WOULD GET, as one hash: every `node_modules/typescript/package.json` the
    /// half resolves from (each file's package folder, the root and its ancestors, `NODE_PATH`). The tree map skips
    /// `node_modules`, so without this a compiler installed after a failed run would never be looked for again.
    pub fn compilers(root: &str, files: &[(String, String)]) -> String {
        let top = Path::new(root);
        let mut dirs: Vec<PathBuf> = top.ancestors().map(Path::to_path_buf).collect();
        for (_, abs) in files {
            if let Some(dir) = Path::new(abs).ancestors().skip(1).take_while(|at| at.starts_with(top))
                .find(|at| at.join("package.json").is_file())
            {
                dirs.push(dir.to_path_buf());
            }
        }
        let mut found: Vec<PathBuf> = dirs.into_iter().map(|d| d.join("node_modules")).collect();
        found.extend(std::env::split_paths(&std::env::var_os("NODE_PATH").unwrap_or_default()));
        found.sort();
        found.dedup();
        let mut hash = blake3::Hasher::new();
        for modules in found {
            let package = modules.join("typescript").join("package.json");
            if let Ok(bytes) = std::fs::read(&package) {
                hash.update(package.to_string_lossy().as_bytes()).update(blake3::hash(&bytes).as_bytes());
            }
        }
        hash.finalize().to_hex()[..16].to_string()
    }

    /// Whether lines are kept under `key` - asked before deciding, so a file that is read again anyway is not also
    /// answered from the last run.
    pub fn has(&self, key: &str) -> bool {
        self.kept.contains_key(key)
    }

    /// WHO IMPORTS EACH FILE, as the last map resolved it (`imported_by`, `imported_softly_by`): the files whose
    /// answer can move when another file does. None when there is no last map to ask - every file is then read.
    pub fn importers(&mut self) -> Option<&HashMap<String, Vec<String>>> {
        if self.importers.is_none() {
            let read = std::fs::read_to_string(&self.previous).ok()
                .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
                .map(|map| {
                    let mut by: HashMap<String, Vec<String>> = HashMap::new();
                    for section in ["imported_by", "imported_softly_by"] {
                        for (target, from) in map[section].as_object().into_iter().flatten() {
                            by.entry(target.clone()).or_default()
                                .extend(from.as_array().into_iter().flatten().filter_map(|v| v.as_str().map(String::from)));
                        }
                    }
                    by
                });
            self.importers = Some(read);
        }
        self.importers.as_ref().and_then(Option::as_ref)
    }

    /// The lines kept under `key`, carried on to the next run.
    pub fn get(&mut self, key: &str) -> Option<String> {
        let lines = self.kept.get(key)?.clone();
        self.now.push((key.to_string(), lines.clone()));
        Some(lines)
    }

    /// Lines this run parsed, kept for the next.
    pub fn put(&mut self, key: String, lines: String) {
        self.read += 1;
        self.now.push((key, lines));
    }

    /// Keep what this run knows, and nothing it no longer asked.
    pub fn keep(self) {
        if self.read == 0 && self.now.len() == self.kept.len() {
            return;
        }
        let Ok(mut conn) = Connection::open(&self.store) else { return };
        let Ok(tx) = conn.transaction() else { return };
        let _ = tx.execute_batch("CREATE TABLE IF NOT EXISTS map_kept (key TEXT PRIMARY KEY, lines TEXT); DELETE FROM map_kept;");
        if let Ok(mut insert) = tx.prepare("INSERT OR REPLACE INTO map_kept (key, lines) VALUES (?1, ?2)") {
            for (key, lines) in &self.now {
                let _ = insert.execute(params![key, lines]);
            }
        }
        let _ = tx.commit();
    }
}

/// C# files' answers from the caller (Roslyn), kept like a half's lines - each a pure function of its own file and the
/// build, so a file added or gone elsewhere asks none of them again (`own_key`). The ones not kept are asked ON EVERY
/// CORE: a cold run over a large tree was one Roslyn parse after another, minutes of it. In `files`' order, with how
/// many were asked. One that failed is asked again next run.
pub fn answers(kept: &mut Option<Kept>, files: &[(String, String)], ask: impl Fn(&str, &str) -> serde_json::Value + Sync) -> (Vec<serde_json::Value>, usize) {
    let keys: Vec<Option<String>> = files.iter().map(|(rel, _)| kept.as_ref().and_then(|k| k.own_key("csharp", rel))).collect();
    let mut found: Vec<Option<serde_json::Value>> = keys.iter()
        .map(|key| key.as_deref().and_then(|key| kept.as_mut()?.get(key)).and_then(|text| serde_json::from_str(&text).ok()))
        .collect();
    let missing: Vec<usize> = (0..files.len()).filter(|&at| found[at].is_none()).collect();
    let workers = std::thread::available_parallelism().map_or(1, |n| n.get()).min(missing.len()).max(1);
    let next = std::sync::atomic::AtomicUsize::new(0);
    let asked: Vec<(usize, serde_json::Value)> = std::thread::scope(|scope| {
        let running: Vec<_> = (0..workers).map(|_| scope.spawn(|| {
            let mut mine = Vec::new();
            while let Some(&at) = missing.get(next.fetch_add(1, std::sync::atomic::Ordering::Relaxed)) {
                let (rel, abs) = &files[at];
                let started = std::time::Instant::now();
                mine.push((at, ask(rel, abs)));
                crate::trace::file("csharp", rel, started);
            }
            mine
        })).collect();
        running.into_iter().flat_map(|worker| worker.join().unwrap_or_else(|panic| std::panic::resume_unwind(panic))).collect()
    });
    for (at, answer) in asked {
        if let (Some(kept), Some(key)) = (kept.as_mut(), keys[at].clone())
            && answer.get("error").is_none()
        {
            kept.put(key, answer.to_string());
        }
        found[at] = Some(answer);
    }
    (found.into_iter().map(Option::unwrap_or_default).collect(), missing.len())
}
