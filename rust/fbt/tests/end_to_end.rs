//! End to end checks over a real temporary tree.
//!
//! The journal fast path is switched off here on purpose. It needs administrator
//! rights and an NTFS volume, so it cannot be a condition for the test suite to
//! pass. `usn_matches_full_scan` covers it when the environment allows it.

use fbt::diff::ChangeKind;
use fbt::pipeline::{Engine, Method, Refresh};
use fbt::scan::ScanOptions;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

struct Tree {
    root: PathBuf,
}

impl Tree {
    fn new(name: &str) -> Tree {
        let root = std::env::temp_dir().join(format!("fbt-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        Tree { root }
    }

    fn write(&self, rel: &str, content: &str) {
        let p = self.root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, content).unwrap();
    }

    fn remove(&self, rel: &str) {
        std::fs::remove_file(self.root.join(rel)).unwrap();
    }

    fn rename(&self, from: &str, to: &str) {
        let to = self.root.join(to);
        std::fs::create_dir_all(to.parent().unwrap()).unwrap();
        std::fs::rename(self.root.join(from), to).unwrap();
    }

    fn engine(&self) -> Engine {
        let opts = ScanOptions {
            // A temporary tree has no .gitignore, and the default skip list
            // would hide directories the tests create, such as `build`.
            use_gitignore: false,
            no_default_skip: true,
            ..Default::default()
        };
        Engine::open(&self.db(), &self.root, opts).unwrap()
    }

    fn db(&self) -> PathBuf {
        self.root.join(".fbt").join("map.db")
    }
}

impl Drop for Tree {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn by_path(r: &Refresh) -> HashMap<String, ChangeKind> {
    r.changes
        .iter()
        .map(|c| (c.path.replace('\\', "/"), c.kind))
        .collect()
}

/// Full scan with the journal path disabled, storing the result.
fn update(e: &mut Engine) -> Refresh {
    e.refresh(false, true, false).unwrap()
}

#[test]
fn first_scan_reports_no_changes_and_a_root_hash() {
    let t = Tree::new("first");
    t.write("src/a.rs", "fn a() {}");
    t.write("src/b.rs", "fn b() {}");

    let mut e = t.engine();
    let r = update(&mut e);

    assert_eq!(r.method, Method::FirstScan);
    assert!(r.changes.is_empty(), "a first scan has nothing to compare against");
    assert!(r.root_hash.is_some());
    assert!(r.snapshot_id.is_some());
}

#[test]
fn an_unchanged_tree_produces_no_changes_and_the_same_root_hash() {
    let t = Tree::new("stable");
    t.write("src/a.rs", "fn a() {}");

    let mut e = t.engine();
    let first = update(&mut e);
    let second = update(&mut e);

    assert!(second.changes.is_empty(), "nothing moved, so nothing may be reported");
    assert_eq!(first.root_hash, second.root_hash);
}

#[test]
fn edit_add_and_delete_are_each_reported_once() {
    let t = Tree::new("basic");
    t.write("src/a.rs", "fn a() {}");
    t.write("src/gone.rs", "fn gone() {}");

    let mut e = t.engine();
    let before = update(&mut e);

    t.write("src/a.rs", "fn a() { changed }");
    t.write("src/new.rs", "fn new() {}");
    t.remove("src/gone.rs");

    let r = update(&mut e);
    let m = by_path(&r);

    assert_eq!(m.get("src/a.rs"), Some(&ChangeKind::Modified));
    assert_eq!(m.get("src/new.rs"), Some(&ChangeKind::Added));
    assert_eq!(m.get("src/gone.rs"), Some(&ChangeKind::Deleted));
    assert_eq!(m.len(), 3, "no extra rows: {:?}", r.changes);
    assert_ne!(before.root_hash, r.root_hash, "the root hash must follow the content");
}

#[test]
fn a_move_is_reported_as_one_rename_not_an_add_and_a_delete() {
    let t = Tree::new("rename");
    t.write("src/old.rs", "fn same() {}");

    let mut e = t.engine();
    update(&mut e);

    t.rename("src/old.rs", "src/moved/new.rs");

    let r = update(&mut e);
    let renames: Vec<_> = r
        .changes
        .iter()
        .filter(|c| c.kind == ChangeKind::Renamed)
        .collect();

    assert_eq!(renames.len(), 1, "expected one rename, got {:?}", r.changes);
    assert_eq!(renames[0].path.replace('\\', "/"), "src/moved/new.rs");
    assert_eq!(
        renames[0].from.as_deref().map(|s| s.replace('\\', "/")),
        Some("src/old.rs".to_string())
    );
}

#[test]
fn touching_a_file_without_changing_bytes_is_not_a_change() {
    let t = Tree::new("touch");
    t.write("src/a.rs", "fn a() {}");

    let mut e = t.engine();
    let first = update(&mut e);

    // Rewrite identical content. Size and mtime move, the content hash does not.
    std::thread::sleep(std::time::Duration::from_millis(20));
    t.write("src/a.rs", "fn a() {}");

    let r = update(&mut e);
    assert!(
        r.changes.is_empty(),
        "identical bytes must not be a change: {:?}",
        r.changes
    );
    assert_eq!(first.root_hash, r.root_hash);
    assert_eq!(r.read_from_disk, 1, "the file had to be read again to prove it is the same");
}

#[test]
fn the_root_hash_returns_to_its_old_value_when_the_content_does() {
    let t = Tree::new("revert");
    t.write("src/a.rs", "original");

    let mut e = t.engine();
    let first = update(&mut e);

    t.write("src/a.rs", "edited");
    let edited = update(&mut e);
    assert_ne!(first.root_hash, edited.root_hash);

    t.write("src/a.rs", "original");
    let back = update(&mut e);
    assert_eq!(
        first.root_hash, back.root_hash,
        "the same tree content must always hash the same"
    );
}

#[test]
fn a_skipped_directory_never_reaches_the_map() {
    let t = Tree::new("skip");
    t.write("src/a.rs", "fn a() {}");
    t.write("junk/b.rs", "fn b() {}");

    let opts = ScanOptions {
        use_gitignore: false,
        no_default_skip: true,
        extra_skip: vec!["junk".to_string()],
        ..Default::default()
    };
    let mut e = Engine::open(&t.db(), &t.root, opts).unwrap();
    let r = e.refresh(false, true, false).unwrap();

    let paths: Vec<String> = fbt::db::load_entries(&e.conn, r.snapshot_id.unwrap())
        .unwrap()
        .into_keys()
        .collect();
    assert!(paths.iter().any(|p| p == "src/a.rs"));
    assert!(
        !paths.iter().any(|p| p.starts_with("junk")),
        "a skipped directory must not appear at all: {paths:?}"
    );
}

#[test]
fn dirty_targets_include_everything_that_depends_on_the_change() {
    let t = Tree::new("targets");
    t.write("core/a.rs", "fn a() {}");
    t.write("app/b.rs", "fn b() {}");
    t.write(
        "targets.json",
        r#"{"targets":[
             {"name":"core","inputs":["core/**"],"deps":[]},
             {"name":"app","inputs":["app/**"],"deps":["core"]},
             {"name":"other","inputs":["other/**"],"deps":[]}
           ]}"#,
    );

    let mut e = t.engine();
    update(&mut e);
    fbt::targets::import(&mut e.conn, &t.root.join("targets.json")).unwrap();

    t.write("core/a.rs", "fn a() { changed }");
    let r = update(&mut e);
    let dirty = fbt::targets::dirty(&e.conn, &r.changed_paths()).unwrap();

    assert!(dirty.contains(&"core".to_string()), "the owning target must rebuild");
    assert!(dirty.contains(&"app".to_string()), "a dependent target must rebuild too");
    assert!(!dirty.contains(&"other".to_string()), "an unrelated target must not");
}

#[test]
fn a_changed_path_no_target_claims_is_reported() {
    let t = Tree::new("unclaimed");
    t.write("core/a.rs", "fn a() {}");
    t.write("stray.rs", "fn stray() {}");
    t.write(
        "targets.json",
        r#"{"targets":[{"name":"core","inputs":["core/**"],"deps":[]}]}"#,
    );

    let mut e = t.engine();
    update(&mut e);
    fbt::targets::import(&mut e.conn, &t.root.join("targets.json")).unwrap();

    t.write("stray.rs", "fn stray() { changed }");
    let r = update(&mut e);

    let unclaimed = fbt::targets::unclaimed(&e.conn, &r.changed_paths()).unwrap();
    assert!(
        unclaimed.iter().any(|p| p.replace('\\', "/") == "stray.rs"),
        "a change outside every input pattern must be visible: {unclaimed:?}"
    );
}

/// The journal fast path must produce exactly what a full walk produces. This is
/// the check that keeps the shortcut honest.
///
/// It is skipped, not failed, where the journal cannot be read: the fast path is
/// an optimisation, and the fallback is the correct behaviour there.
#[test]
#[cfg(windows)]
fn usn_matches_full_scan() {
    let t = Tree::new("usn");
    t.write("src/a.rs", "fn a() {}");
    t.write("src/b.rs", "fn b() {}");
    t.write("src/gone.rs", "fn gone() {}");

    let mut e = t.engine();
    let first = e.refresh(true, true, false).unwrap();
    if first.usn_note.is_some() {
        eprintln!("skipped: journal unavailable ({:?})", first.usn_note);
        return;
    }

    t.write("src/a.rs", "fn a() { changed }");
    t.write("src/added/deep.rs", "fn deep() {}");
    t.remove("src/gone.rs");

    // Compare without storing, so both methods see the same starting snapshot.
    let via_usn = e.refresh(true, false, false).unwrap();
    let via_walk = e.refresh(false, false, false).unwrap();

    if via_usn.method != Method::Usn {
        eprintln!("skipped: journal unavailable ({:?})", via_usn.usn_note);
        return;
    }

    assert_eq!(
        via_usn.root_hash, via_walk.root_hash,
        "the journal path must reach the same tree state as a full walk"
    );
    assert_eq!(normalise(&via_usn), normalise(&via_walk));
}

#[cfg(windows)]
fn normalise(r: &Refresh) -> Vec<(String, ChangeKind)> {
    let mut v: Vec<(String, ChangeKind)> = r
        .changes
        .iter()
        .map(|c| (c.path.replace('\\', "/"), c.kind))
        .collect();
    v.sort();
    v
}

#[test]
fn a_missing_root_is_an_error_not_an_empty_map() {
    let missing = Path::new("this-directory-does-not-exist-fbt");
    let db = std::env::temp_dir().join("fbt-missing.db");
    assert!(Engine::open(&db, missing, ScanOptions::default()).is_err());
    let _ = std::fs::remove_file(db);
}

/// How many snapshots, and how many entry rows, the store holds.
fn held(tree: &Tree) -> (i64, i64) {
    let conn = rusqlite::Connection::open(tree.db()).unwrap();
    let snapshots: i64 = conn.query_row("SELECT count(*) FROM snapshot", [], |r| r.get(0)).unwrap();
    let entries: i64 = conn.query_row("SELECT count(*) FROM entry", [], |r| r.get(0)).unwrap();
    (snapshots, entries)
}

#[test]
fn an_unchanged_tree_is_not_written_again_and_old_snapshots_go() {
    let tree = Tree::new("advance");
    tree.write("a.txt", "one");
    tree.write("sub/b.txt", "two");
    let mut engine = tree.engine();
    let first = engine.refresh(false, true, true).unwrap();
    let after_first = held(&tree);
    // NOTHING MOVED: the same snapshot answers, no second copy of every entry.
    let second = engine.refresh(false, true, true).unwrap();
    assert_eq!(second.snapshot_id, first.snapshot_id, "an unchanged tree is answered by the snapshot it already has");
    assert_eq!(held(&tree), after_first, "an unchanged refresh writes no entries");
    // SOMETHING MOVED, three times: only the newest two are kept.
    for n in 0..3 {
        tree.write("a.txt", &format!("changed {n}"));
        engine.refresh(false, true, true).unwrap();
    }
    assert_eq!(held(&tree).0, 2, "only the newest snapshots are kept");
}
