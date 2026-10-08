//! WHAT A DEEP HALF HASHES BEFORE IT KNOWS WHETHER ANYTHING MOVED - and how little of it reads a disk.
//!
//! A file the tree map's snapshot names has its content hash there, and it is NOT OPENED: the C# half opened every one
//! of a large tree's files on every run to prove it still could, most of a run whose cache was cold after a pause.
//! Only a file that MOVED is opened, by the half, before it is read.
//!
//! A file OUTSIDE the snapshot - `obj/project.assets.json`, a props file above the root - is read only when its size or
//! time moved: its digest is kept with both, in the database (`_digests`). The digest is the same SHA-256 it always was,
//! so no recorded sha changes and an upgrade re-reads nothing.

use super::hashes::Hashes;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::Read;
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

/// `path -> (size, mtime in ns, digest)`.
type Known = HashMap<String, (u64, i64, String)>;

pub(super) struct Inputs<'a> {
    hashes: &'a Hashes,
    db: String,
    known: Mutex<Known>,
    changed: AtomicBool,
}

impl<'a> Inputs<'a> {
    pub(super) fn open(db: &str, hashes: &'a Hashes) -> Inputs<'a> {
        let mut known = Known::new();
        if let Ok(conn) = rusqlite::Connection::open_with_flags(db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            && let Ok(mut rows) = conn.prepare("SELECT path, size, mtime, digest FROM _digests")
        {
            let read = rows.query_map([], |r| Ok((r.get::<_, String>(0)?, (r.get::<_, i64>(1)? as u64, r.get::<_, i64>(2)?, r.get::<_, String>(3)?))));
            known.extend(read.into_iter().flatten().flatten());
        }
        Inputs { hashes, db: db.to_string(), known: Mutex::new(known), changed: AtomicBool::new(false) }
    }

    /// A file's content hash: the tree map's when it names the file - not opened - else its digest, read only when
    /// its size or time moved.
    pub(super) fn content(&self, path: &str) -> std::io::Result<String> {
        if let Some(known) = self.hashes.get(path) {
            return Ok(known.to_string());
        }
        let meta = std::fs::metadata(path)?;
        let size = meta.len();
        let mtime = meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_nanos() as i64);
        if let Some((s, m, d)) = self.known.lock().ok().and_then(|k| k.get(path).cloned())
            && s == size
            && m == mtime
        {
            return Ok(d);
        }
        let digest = digest(path)?;
        if let Ok(mut known) = self.known.lock() {
            known.insert(path.to_string(), (size, mtime, digest.clone()));
            self.changed.store(true, Ordering::Relaxed);
        }
        Ok(digest)
    }

    /// A file's content hash, `-` when it is not there and `?` when it cannot be read.
    pub(super) fn hash(&self, path: &str) -> String {
        if !Path::new(path).is_file() {
            return "-".into();
        }
        self.content(path).unwrap_or_else(|_| "?".into())
    }

    /// WHAT A PROJECT COMPILES AGAINST: the `.csproj`, `obj/project.assets.json` and the nearest
    /// `Directory.Build.props`/`Directory.Packages.props`. A restore changes a row's `symbol` without touching
    /// its source - one tree restored many projects and the next run re-read 0 files.
    ///
    /// IN NAMED PARTS - the project file, its assets, a props file by its path, what a generator emitted - so a run
    /// that re-reads a project's files can say which of them moved. Joined with `+` (`reasons::joined`) they are
    /// the fingerprint exactly as it always was: no recorded sha changes.
    pub(super) fn fingerprint_parts(&self, project: &str) -> Vec<(String, String)> {
        let folder = Path::new(project).parent().unwrap_or(Path::new(""));
        let name = Path::new(project).file_name().map_or_else(|| project.to_string(), |n| n.to_string_lossy().into_owned());
        let mut parts = vec![(name, self.hash(project)),
            ("obj/project.assets.json".to_string(), self.hash(&folder.join("obj").join("project.assets.json").to_string_lossy()))];
        for name in ["Directory.Build.props", "Directory.Packages.props"] {
            if let Some(found) = folder.ancestors().map(|at| at.join(name)).find(|candidate| candidate.is_file()) {
                parts.push((found.to_string_lossy().replace('\\', "/"), self.hash(&found.to_string_lossy())));
            }
        }
        // AND WHAT A GENERATOR EMITTED, by size and time - read off `obj`, which the tree map does not hash: a build
        // that newly writes a Mapperly body must re-read the files that call it, not leave their CS8795 standing.
        let mut emitted: Vec<String> = crate::csproj::emitted_of(project).into_iter()
            .map(|file| {
                let stamp = std::fs::metadata(&file).ok().map(|m| (m.len(), m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_nanos())));
                format!("{file}={stamp:?}")
            })
            .collect();
        if !emitted.is_empty() {
            emitted.sort();
            parts.push(("what its generators emitted".to_string(), blake3::hash(emitted.join("|").as_bytes()).to_hex()[..16].to_string()));
        }
        parts
    }
}

/// THE DIGESTS THIS RUN TOOK, kept for the next one - only when one was taken.
impl Drop for Inputs<'_> {
    fn drop(&mut self) {
        if !self.changed.load(Ordering::Relaxed) || !Path::new(&self.db).is_file() {
            return;
        }
        let Ok(known) = self.known.lock() else { return };
        let Ok(mut conn) = rusqlite::Connection::open(&self.db) else { return };
        let _ = conn.busy_timeout(std::time::Duration::from_secs(5));
        let Ok(tx) = conn.transaction() else { return };
        let _ = tx.execute_batch("CREATE TABLE IF NOT EXISTS _digests (path TEXT PRIMARY KEY, size INTEGER, mtime INTEGER, digest TEXT); DELETE FROM _digests;");
        if let Ok(mut insert) = tx.prepare("INSERT INTO _digests (path, size, mtime, digest) VALUES (?1, ?2, ?3, ?4)") {
            for (path, (size, mtime, digest)) in known.iter() {
                let _ = insert.execute(rusqlite::params![path, *size as i64, mtime, digest]);
            }
        }
        let _ = tx.commit();
    }
}

/// SHA-256 of a file off the stream, as 16 lowercase hex digits.
fn digest(path: &str) -> std::io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1 << 16];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    Ok(super::driven::hex16(&hasher.finalize()))
}
