//! WHETHER THE TREE MOVED ON SINCE THE MAP WAS WRITTEN, said by `--map-query` as ONE note on stderr (`cli/lens.rs`):
//! a tree not wired to rebuild its map kept answering from an old snapshot - a renamed method still found under its old
//! name - and nothing in the output said so. Stdout is parsed, so the note never goes there, and it never fails a query.
//!
//! THE RULE `--map-if-stale` JUDGES THE FILE MAP BY (`mapper/stale.rs`): a source newer than the map, or a file added or
//! gone. The deep map records its file SET as rows rather than as a hash, so the set is compared file by file: every
//! folder the map holds a file in is listed once - a listing carries each file's time, so this is one call per FOLDER
//! on Windows - and a mapped file not there is gone, one newer than the database changed, and an unmapped file of a
//! mapped extension newer than the database added. A file added in a folder the map holds nothing in is not seen.
//!
//! A FILE THE LAST RUN ALREADY SAW IS NOT NEWS. A run that re-read nothing leaves the database untouched (`refresh.rs`),
//! so a file touched and not changed stays newer than it after every rebuild. The tree map's snapshot that run took
//! (`<root>/.fbt/deep.sqlite`, `mapper/deep/hashes.rs`) is dated when it LOOKED, moved or not, and a file last written
//! before then was hashed by that run - what it changed, if anything, is already in the rows, and a file it did not map
//! it left out on purpose. Without that a rebuild could never clear the note.
//!
//! BOUNDED: a map of a large tree is listed for at most `BUDGET`, and a check cut short says how much it covered. A map
//! whose root is gone, or that names none, says nothing - there is no tree left to compare.

use rusqlite::{Connection, OpenFlags};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

/// How long the check may list folders before it stops and says how far it got.
const BUDGET: Duration = Duration::from_secs(2);
/// How many paths the note names.
const NAMED: usize = 3;

/// What moved, as map paths, and how much of the map was looked at.
#[derive(Debug, Default)]
pub struct Moved {
    pub changed: Vec<String>,
    pub added: Vec<String>,
    pub gone: Vec<String>,
    pub folders: usize,
    pub listed: usize,
}

/// The note `--map-query` prints on stderr, or None when the map is current or cannot be compared.
pub fn note(db: &Path) -> Option<String> {
    said(&check(db, BUDGET)?)
}

/// The tree against the map at `db`. None when there is nothing to compare against.
pub fn check(db: &Path, budget: Duration) -> Option<Moved> {
    let built = std::fs::metadata(db).ok()?.modified().ok()?;
    let conn = Connection::open_with_flags(db, OpenFlags::SQLITE_OPEN_READ_ONLY).ok()?;
    let roots = roots_of(&conn);
    if roots.is_empty() || roots.iter().any(|root| !root.is_dir()) {
        return None;
    }
    let workspace = workspace_of(&conn).map(|fe| roots[0].join(fe));
    let mut stmt = conn.prepare("SELECT DISTINCT path, lang FROM files").ok()?;
    let mapped: Vec<(String, String)> = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?.unwrap_or_default())))
        .ok()?
        .flatten()
        .collect();
    let extensions: Vec<String> = {
        let mut found: Vec<String> = mapped.iter().filter_map(|(path, _)| extension(path)).collect();
        found.sort();
        found.dedup();
        found
    };
    // FOLDER BY FOLDER: `abs folder -> (the map's spelling of it, {name on disk -> map path})`.
    let mut folders: BTreeMap<PathBuf, (String, HashMap<String, String>)> = BTreeMap::new();
    let mut located: HashMap<(String, bool), Option<PathBuf>> = HashMap::new();
    for (path, lang) in &mapped {
        let (above, name) = path.rsplit_once('/').map_or(("", path.as_str()), |(above, name)| (above, name));
        let angular = lang == "typescript";
        // AN ANGULAR PATH IS RELATIVE TO ITS WORKSPACE, not the root; with none recorded it cannot be placed.
        let folder = located
            .entry((above.to_string(), angular))
            .or_insert_with(|| if angular { workspace.as_ref().map(|fe| fe.join(above)) } else { Some(locate(&roots, above)) })
            .clone();
        let Some(folder) = folder else { continue };
        let spelled = if above.is_empty() { String::new() } else { format!("{above}/") };
        folders.entry(folder).or_insert_with(|| (spelled, HashMap::new())).1.insert(fold(name), path.clone());
    }
    let seen = Seen::open(&roots);
    let started = Instant::now();
    let mut moved = Moved { folders: folders.len(), ..Moved::default() };
    for (folder, (spelled, names)) in &folders {
        if moved.listed > 0 && started.elapsed() > budget {
            break;
        }
        moved.listed += 1;
        let listing = list(folder);
        let mut held: Vec<(&String, &String)> = names.iter().collect();
        held.sort();
        for (name, path) in held {
            match listing.get(name) {
                None => moved.gone.push(path.clone()),
                Some((disk, at)) if *at > built && !seen.saw(&folder.join(disk), *at) => moved.changed.push(path.clone()),
                Some(_) => {}
            }
        }
        let mut fresh: Vec<&String> = listing
            .iter()
            .filter(|(name, (disk, at))| {
                !names.contains_key(*name)
                    && *at > built
                    && extension(disk).is_some_and(|e| extensions.contains(&e))
                    && !seen.saw(&folder.join(disk), *at)
            })
            .map(|(_, (disk, _))| disk)
            .collect();
        fresh.sort();
        moved.added.extend(fresh.into_iter().map(|disk| format!("{spelled}{disk}")));
    }
    Some(moved)
}

/// The note, or None when nothing moved.
pub fn said(moved: &Moved) -> Option<String> {
    let counts: Vec<String> = [(moved.changed.len(), "changed"), (moved.added.len(), "added"), (moved.gone.len(), "gone")]
        .iter()
        .filter(|(n, _)| *n > 0)
        .map(|(n, what)| format!("{n} {what}"))
        .collect();
    let cut = moved.listed < moved.folders;
    // A CHECK CUT SHORT THAT FOUND NOTHING SAYS SO TOO: silence would read as "current" for folders never looked at.
    if counts.is_empty() {
        return cut.then(|| format!(
            "note: whether this map is current was checked in {} of its {} folders before giving up - nothing there moved since it was written",
            moved.listed, moved.folders
        ));
    }
    let named: Vec<&String> = moved.changed.iter().chain(&moved.added).chain(&moved.gone).take(NAMED).collect();
    let total = moved.changed.len() + moved.added.len() + moved.gone.len();
    let more = if total > named.len() { ", ..." } else { "" };
    let named: Vec<&str> = named.iter().map(|p| p.as_str()).collect();
    let partly = if cut {
        format!(" (checked {} of the map's {} folders before giving up; there may be more)", moved.listed, moved.folders)
    } else {
        String::new()
    };
    Some(format!(
        "note: the tree moved on since this map was written - {} ({}{more}){partly}; its answers may be stale - rebuild it with the --map-sqlite run that wrote it",
        counts.join(", "),
        named.join(", ")
    ))
}

/// `_meta.roots` (every root), or `_meta.root` for a map written before it was kept.
fn roots_of(conn: &Connection) -> Vec<PathBuf> {
    let meta = |key: &str| conn.query_row("SELECT value FROM _meta WHERE key = ?1", [key], |r| r.get::<_, String>(0)).ok();
    if let Some(listed) = meta("roots").and_then(|text| serde_json::from_str::<Vec<String>>(&text).ok())
        && !listed.is_empty()
    {
        return listed.into_iter().map(PathBuf::from).collect();
    }
    meta("root").map(PathBuf::from).into_iter().collect()
}

/// The Angular workspace the last run mapped, relative to the first root - kept with its key (`mapper/deep/ts.rs`).
fn workspace_of(conn: &Connection) -> Option<String> {
    let kept: String = conn.query_row("SELECT value FROM _meta WHERE key = 'skip:typescript'", [], |r| r.get(0)).ok()?;
    let first = kept.split('\n').next()?;
    serde_json::from_str::<serde_json::Value>(first).ok()?["fe"].as_str().map(String::from)
}

/// A map folder on disk. With several roots a path carries the folder that tells them apart (`src/...` for a root
/// `/t/src`), so that name is taken off where it leads; the first spelling that exists wins.
fn locate(roots: &[PathBuf], above: &str) -> PathBuf {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if roots.len() > 1 {
        for root in roots {
            let Some(name) = root.file_name().map(|n| n.to_string_lossy().into_owned()) else { continue };
            if above == name {
                candidates.push(root.clone());
            } else if let Some(rest) = above.strip_prefix(&format!("{name}/")) {
                candidates.push(root.join(rest));
            }
        }
    }
    candidates.extend(roots.iter().map(|root| if above.is_empty() { root.clone() } else { root.join(above) }));
    if candidates.len() == 1 {
        return candidates.remove(0);
    }
    candidates.iter().find(|c| c.is_dir()).cloned().unwrap_or_else(|| candidates.remove(0))
}

/// Every file in a folder: `{folded name -> (name, modified)}`. A folder that cannot be listed holds nothing.
fn list(folder: &Path) -> HashMap<String, (String, SystemTime)> {
    let Ok(entries) = std::fs::read_dir(folder) else { return HashMap::new() };
    entries
        .flatten()
        .filter_map(|entry| {
            let meta = entry.metadata().ok().filter(|m| m.is_file())?;
            let name = entry.file_name().to_string_lossy().into_owned();
            Some((fold(&name), (name, meta.modified().ok()?)))
        })
        .collect()
}

/// A file name as compared: Windows names are not case-sensitive.
fn fold(name: &str) -> String {
    if cfg!(windows) { name.to_lowercase() } else { name.to_string() }
}

/// `.py`, lowercased, or None for a name with no extension.
fn extension(path: &str) -> Option<String> {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.rfind('.').filter(|at| *at > 0).map(|at| name[at..].to_lowercase())
}

/// When the last deep run LOOKED at each root: the tree map's snapshot it took (`<root>/.fbt/deep.sqlite`) is moved
/// forward to that time even when nothing in the tree moved (`fbt::pipeline`). A root without one saw nothing.
struct Seen {
    roots: Vec<(PathBuf, Option<SystemTime>)>,
}

impl Seen {
    fn open(roots: &[PathBuf]) -> Seen {
        let looked = |root: &PathBuf| {
            let store = root.join(".fbt").join("deep.sqlite");
            let conn = Connection::open_with_flags(&store, OpenFlags::SQLITE_OPEN_READ_ONLY).ok()?;
            let ns: i64 = conn.query_row("SELECT max(created_ns) FROM snapshot", [], |r| r.get(0)).ok()?;
            Some(SystemTime::UNIX_EPOCH + Duration::from_nanos(u64::try_from(ns).ok()?))
        };
        Seen { roots: roots.iter().map(|root| (root.clone(), looked(root))).collect() }
    }

    /// Whether the last deep run looked at this file AS IT IS NOW: it was last written before that run began.
    fn saw(&self, file: &Path, at: SystemTime) -> bool {
        self.roots.iter().any(|(root, looked)| looked.is_some_and(|looked| at <= looked) && file.starts_with(root))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tree under the temp folder with a map in it holding `paths`, the map dated an hour back so every file is newer.
    fn tree(name: &str, roots: &[&str], files: &[&str], paths: &[&str]) -> (PathBuf, PathBuf) {
        let dir = std::env::temp_dir().join(format!("sg-fresh-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for file in files {
            let at = dir.join(file);
            std::fs::create_dir_all(at.parent().unwrap()).unwrap();
            std::fs::write(&at, "x = 1\n").unwrap();
        }
        let db = dir.join("map.sqlite");
        let conn = Connection::open(&db).unwrap();
        let listed: Vec<String> = roots.iter().map(|r| dir.join(r).to_string_lossy().into_owned()).collect();
        conn.execute_batch("CREATE TABLE _meta (key TEXT, value TEXT); CREATE TABLE files (id TEXT, path TEXT, lang TEXT);").unwrap();
        conn.execute("INSERT INTO _meta VALUES ('root', ?1)", [&listed[0]]).unwrap();
        conn.execute("INSERT INTO _meta VALUES ('roots', ?1)", [&serde_json::Value::from(listed.clone()).to_string()]).unwrap();
        for (i, path) in paths.iter().enumerate() {
            conn.execute("INSERT INTO files VALUES (?1, ?2, 'python')", [&format!("f:{i}"), &path.to_string()]).unwrap();
        }
        drop(conn);
        let hour_ago = SystemTime::now() - Duration::from_secs(3600);
        std::fs::File::options().write(true).open(&db).unwrap().set_modified(hour_ago).unwrap();
        (dir, db)
    }

    #[test]
    fn changed_added_and_gone_are_counted_and_named_under_several_roots() {
        let (dir, db) = tree("several", &["src", "test"], &["src/demo/order.py", "src/demo/new.py", "test/check.py"],
            &["src/demo/order.py", "src/demo/old.py", "test/check.py"]);
        let moved = check(&db, BUDGET).unwrap();
        assert_eq!(moved.changed, vec!["src/demo/order.py", "test/check.py"]);
        assert_eq!(moved.added, vec!["src/demo/new.py"]);
        assert_eq!(moved.gone, vec!["src/demo/old.py"]);
        let note = said(&moved).unwrap();
        assert!(note.contains("2 changed, 1 added, 1 gone") && note.contains("src/demo/order.py, test/check.py, src/demo/new.py, ..."), "{note}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_map_newer_than_its_files_says_nothing_and_a_map_with_no_root_is_not_checked() {
        let (dir, db) = tree("current", &["."], &["a.py"], &["a.py"]);
        std::fs::File::options().write(true).open(&db).unwrap().set_modified(SystemTime::now() + Duration::from_secs(60)).unwrap();
        assert!(said(&check(&db, BUDGET).unwrap()).is_none());
        let conn = Connection::open(&db).unwrap();
        conn.execute("UPDATE _meta SET value = '[\"/no/such/root\"]' WHERE key = 'roots'", []).unwrap();
        drop(conn);
        assert!(check(&db, BUDGET).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_check_out_of_time_says_how_much_it_covered() {
        let (dir, db) = tree("budget", &["."], &["a/x.py", "b/y.py"], &["a/x.py", "b/y.py"]);
        let moved = check(&db, Duration::ZERO).unwrap();
        assert_eq!((moved.listed, moved.folders), (1, 2));
        assert!(said(&moved).unwrap().contains("checked 1 of the map's 2 folders"));
        // Cut short with nothing found: still said, since the folders never listed were never compared.
        let quiet = Moved { folders: 2, listed: 1, ..Moved::default() };
        assert!(said(&quiet).unwrap().contains("checked in 1 of its 2 folders"));
        assert!(said(&Moved { folders: 2, listed: 2, ..Moved::default() }).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
