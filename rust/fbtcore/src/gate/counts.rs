//! A FILE THAT DID NOT MOVE IS NOT PARSED AGAIN - NOR COUNTED. When the pass cache misses - one file edited, or a tree that
//! failed - the gate still counted every `.cs` with Roslyn and ran the async rule over it, and started
//! PowerShell and node over every script: thousands of parses to learn three new answers. Each answer is a pure
//! function of the file's CONTENT, its PATH (the problems name it), the rules asked and the BUILD of this tool,
//! so it is kept under exactly that, in `.fbt/gate.sqlite` beside the passes.
//!
//! A HOST'S ANSWER ALSO DEPENDS ON THE HOST, so its path is in the key - and a TypeScript 7 compiler answers per
//! PROJECT, so the TypeScript key holds every `tsconfig*.json`, `package.json` and lock file of the tree too: an
//! upgraded compiler or a changed `include` asks every file again, and so does another `NODE_PATH`.
//!
//! THE CONTENT HASH IS THE TREE MAP'S, from the snapshot the pass cache has just brought up to date - never a hash
//! taken here, which would read every file to save reading every file. A file that snapshot does not name is
//! counted as before, and so is every file of a run that did not refresh it (`--no-gate-cache`, several roots).
//!
//! Only what this run counted is kept: the table is rewritten when anything was counted or anything went unused.

use rusqlite::{params, Connection};
use std::collections::HashMap;
use std::path::Path;

/// A file's answer: its line count and the problems found in it.
pub(crate) type Answer = (i64, Vec<String>);

pub(crate) struct Counts {
    store: std::path::PathBuf,
    /// Relative path key -> the tree map's content hash.
    hashes: HashMap<String, String>,
    stamp: String,
    kept: HashMap<String, Answer>,
    now: Vec<(String, i64, Vec<String>)>,
    counted: usize,
}

impl Counts {
    /// None when the tree map cannot say what the files hold; the gate then counts every file.
    pub(crate) fn open(root: &str, skip: &[String], tracked: bool, build: &str, async_rules: bool) -> Option<Counts> {
        let (store, hashes) = crate::tree::contents(Path::new(root), skip, tracked)?;
        let mut kept = HashMap::new();
        if let Ok(conn) = Connection::open(&store) {
            if let Ok(mut rows) = conn.prepare("SELECT key, lines, problems FROM gate_counts") {
                let read = rows.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, String>(2)?)));
                for (key, lines, problems) in read.into_iter().flatten().flatten() {
                    kept.insert(key, (lines, serde_json::from_str(&problems).unwrap_or_default()));
                }
            }
        }
        Some(Counts { store, hashes, stamp: format!("{build}|{async_rules}"), kept, now: Vec::new(), counted: 0 })
    }

    /// What a host's answers depend on besides the file: which host, and for TypeScript every file that says
    /// which compiler and which project.
    pub(crate) fn host(&self, tag: &str, host: &str) -> String {
        let mut stamp = format!("{tag}|{host}");
        if tag == "ts" {
            let mut project: Vec<(&String, &String)> = self.hashes.iter().filter(|(path, _)| projectish(path)).collect();
            project.sort();
            let mut hash = blake3::Hasher::new();
            // NODE_PATH TOO: a compiler resolved through it is outside the tree, as a hoisted install is.
            hash.update(std::env::var("NODE_PATH").unwrap_or_default().as_bytes());
            for (path, content) in project {
                hash.update(format!("{path}\t{content}\n").as_bytes());
            }
            stamp.push('|');
            stamp.push_str(&hash.finalize().to_hex()[..16]);
        }
        stamp
    }

    /// The kept answer for this file under these rules, or None when it has to be asked.
    pub(crate) fn get(&mut self, rel: &str, rules: &str) -> Option<Answer> {
        let key = self.key(rel, rules)?;
        let found = self.kept.get(&key)?.clone();
        self.now.push((key, found.0, found.1.clone()));
        Some(found)
    }

    /// Keep an answer that was asked for this run.
    pub(crate) fn put(&mut self, rel: &str, rules: &str, answer: &Answer) {
        if let Some(key) = self.key(rel, rules) {
            self.counted += 1;
            self.now.push((key, answer.0, answer.1.clone()));
        }
    }

    /// The kept answer for this C# file, or `count`'s - which is kept for the next run when it succeeded.
    pub(crate) fn csharp(&mut self, rel: &str, count: impl FnOnce() -> Result<Answer, String>) -> Result<Answer, String> {
        if let Some(found) = self.get(rel, "cs") {
            return Ok(found);
        }
        let answer = count()?;
        self.put(rel, "cs", &answer);
        Ok(answer)
    }

    /// The kept line count of any other file - a source (`lines`) or a doc (`doc`) - or `count`'s. Counting the files
    /// that did not move was ~86 ms of every edit turn on this repo's 345 files, and grows with the tree.
    pub(crate) fn lines(&mut self, rel: &str, rules: &str, count: impl FnOnce() -> Result<i64, String>) -> Result<i64, String> {
        if let Some((lines, _)) = self.get(rel, rules) {
            return Ok(lines);
        }
        let lines = count()?;
        self.put(rel, rules, &(lines, Vec::new()));
        Ok(lines)
    }

    fn key(&self, rel: &str, rules: &str) -> Option<String> {
        let path = fbt::entry::key_of(&fbt::entry::normalize(rel));
        let content = self.hashes.get(&path)?;
        Some(blake3::hash(format!("{path}\t{content}\t{}\t{rules}", self.stamp).as_bytes()).to_hex().to_string())
    }

    /// Keep what this run knows, and nothing it no longer asked.
    pub(crate) fn keep(self) {
        if self.counted == 0 && self.now.len() == self.kept.len() {
            return;
        }
        let Ok(mut conn) = Connection::open(&self.store) else { return };
        let Ok(tx) = conn.transaction() else { return };
        let _ = tx.execute_batch("CREATE TABLE IF NOT EXISTS gate_counts (key TEXT PRIMARY KEY, lines INTEGER, problems TEXT); DELETE FROM gate_counts;");
        if let Ok(mut insert) = tx.prepare("INSERT OR REPLACE INTO gate_counts (key, lines, problems) VALUES (?1, ?2, ?3)") {
            for (key, lines, problems) in &self.now {
                let _ = insert.execute(params![key, lines, serde_json::to_string(problems).unwrap_or_default()]);
            }
        }
        let _ = tx.commit();
    }
}

/// A file that decides which TypeScript compiler runs, or which project a file is in.
pub(crate) fn projectish(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    (name.starts_with("tsconfig") && name.ends_with(".json"))
        || ["package.json", "package-lock.json", "pnpm-lock.yaml", "yarn.lock", "npm-shrinkwrap.json"].contains(&name)
}
