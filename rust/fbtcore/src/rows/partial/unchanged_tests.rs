//! The tests of `unchanged.rs`, apart from it so that file stays under the line ceiling.
use super::*;
use crate::rows::tsapply::{apply, TsPayload};
use serde_json::{json, Map, Value};

fn tmp(name: &str) -> crate::rows::TempDb {
    crate::rows::TempDb::new("unchanged", name)
}

/// A table `t` with a column `c` written as nodes, one row per `(id, a, b, c)`.
fn table(db: &Connection, rows: &[(&str, i64, &str, Value)]) {
    db.execute_batch("CREATE TABLE t (id TEXT, a, b, half TEXT)").unwrap();
    let mut writer = cells::Writer::new(db, "t", "c").unwrap();
    for (id, a, b, c) in rows {
        db.execute("INSERT INTO t (id, a, b, half) VALUES (?1, ?2, ?3, 'typescript')", (id, a, b)).unwrap();
        writer.tree(id, c).unwrap();
    }
}

fn hash(rows: &[(&str, i64, &str, Value)], columns: &[&str]) -> String {
    let db = Connection::open_in_memory().unwrap();
    table(&db, rows);
    let columns: BTreeSet<String> = columns.iter().map(|c| c.to_string()).collect();
    let all = Scope { all: true, owners: &[], paths: &[], kind: None, above: None };
    digest(&db, "t", &columns, "typescript", &all).unwrap()
}

#[test]
fn a_digest_sees_every_read_cell_and_nothing_else() {
    let rows = [("x:1", 1, "one", json!(["p", {"k": 1}])), ("x:2", 2, "two", json!([]))];
    let same = hash(&rows, &["a", "c"]);
    assert_eq!(same, hash(&rows, &["a", "c"]), "the same rows hash the same");
    // A COLUMN NO PASS READ may differ: the closure never saw it.
    let other_b = [("x:1", 1, "uno", json!(["p", {"k": 1}])), ("x:2", 2, "dos", json!([]))];
    assert_eq!(same, hash(&other_b, &["a", "c"]));
    // ...but one it read may not, a node of a structure included, nor the order the rows come in.
    let other_a = [("x:1", 1, "one", json!(["p", {"k": 1}])), ("x:2", 3, "two", json!([]))];
    assert_ne!(same, hash(&other_a, &["a", "c"]));
    let other_node = [("x:1", 1, "one", json!(["p", {"k": 2}])), ("x:2", 2, "two", json!([]))];
    assert_ne!(same, hash(&other_node, &["a", "c"]));
    let swapped = [rows[1].clone(), rows[0].clone()];
    assert_ne!(same, hash(&swapped, &["a", "c"]));
    // A TYPE IS PART OF A CELL: 1 and "1" are two answers.
    let db = Connection::open_in_memory().unwrap();
    db.execute_batch("CREATE TABLE u (a, half TEXT); INSERT INTO u VALUES (1, 'typescript')").unwrap();
    let text = Connection::open_in_memory().unwrap();
    text.execute_batch("CREATE TABLE u (a, half TEXT); INSERT INTO u VALUES ('1', 'typescript')").unwrap();
    let columns: BTreeSet<String> = ["a".to_string()].into();
    let all = Scope { all: true, owners: &[], paths: &[], kind: None, above: None };
    assert_ne!(digest(&db, "u", &columns, "typescript", &all).unwrap(), digest(&text, "u", &columns, "typescript", &all).unwrap());
    // A COLUMN A LATER WRITE ADDED, empty in every row, is what it was before it existed: NULL.
    let widened = Connection::open_in_memory().unwrap();
    widened.execute_batch("CREATE TABLE u (a, z, half TEXT); INSERT INTO u VALUES (1, NULL, 'typescript')").unwrap();
    let both: BTreeSet<String> = ["a".to_string(), "z".to_string()].into();
    assert_eq!(digest(&db, "u", &both, "typescript", &all).unwrap(), digest(&widened, "u", &both, "typescript", &all).unwrap());
}

/// A workspace of two files, and the payload that maps it: `parsed` is what the dead-key trace reads off a file.
fn payload(partial: bool, parsed: bool) -> TsPayload {
    let mut tables = Map::new();
    // A PARTIAL PAYLOAD holds the re-extracted file's rows only; the rest stays in the database.
    let mut files = vec![json!({"id": "f:1", "path": "a.ts", "ext": "ts", "owner_file": "f:1", "parsed": parsed})];
    if !partial {
        files.push(json!({"id": "f:2", "path": "b.ts", "ext": "ts", "owner_file": "f:2", "parsed": true}));
        tables.insert("translations".to_string(), json!([{"id": "tr:1", "key": "demo.title", "file": "f:2", "owner_file": "f:2"}]));
    }
    tables.insert("files".to_string(), Value::Array(files));
    let shas = match json!({"a.ts": "1", "b.ts": "2"}) {
        Value::Object(m) => m,
        _ => unreachable!(),
    };
    TsPayload {
        all: !partial,
        partial,
        affected: if partial { vec!["a.ts".to_string()] } else { Vec::new() },
        shas,
        tables,
        setup: Some("setup-a".to_string()),
        ..Default::default()
    }
}

/// The note a partial run left about its closure.
fn verdict(notes: &[String]) -> String {
    notes.iter().find(|n| n.contains("closure it stored") || n.contains("derives its closure again")).cloned().unwrap_or_default()
}

struct Workspace(std::path::PathBuf);

impl Workspace {
    fn new(name: &str) -> Workspace {
        let root = std::env::temp_dir().join(format!("fbt-unchanged-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("a.ts"), "export const a = 1;\n").unwrap();
        std::fs::write(root.join("b.ts"), "export const title = 'demo.title';\n").unwrap();
        Workspace(root)
    }

    fn root(&self) -> String {
        self.0.to_string_lossy().into_owned()
    }

    fn edit(&self, text: &str) {
        std::fs::write(self.0.join("a.ts"), text).unwrap();
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn a_partial_run_keeps_its_closure_only_while_nothing_it_read_moved() {
    let db = tmp("verdict");
    let ws = Workspace::new("verdict");
    apply(&db, &ws.root(), payload(false, true)).unwrap();

    // A COMMENT: the text moved, the rows and what the trace makes of the text did not.
    ws.edit("export const a = 1;\n// a remark\n");
    let receipt = apply(&db, &ws.root(), payload(true, true)).unwrap();
    assert!(verdict(&receipt.notes).contains("kept the closure it stored"), "{:?}", receipt.notes);
    assert!(!receipt.phases.contains_key("derive the closure"));

    // A DOTTED NAME IN LIVE CODE is something the trace reads.
    ws.edit("export const a = 'demo.other';\n// a remark\n");
    let receipt = apply(&db, &ws.root(), payload(true, true)).unwrap();
    assert!(verdict(&receipt.notes).contains("the text of a.ts moved"), "{:?}", receipt.notes);

    // A CELL THE CLOSURE READ: `files.parsed`, read by the trace.
    let receipt = apply(&db, &ws.root(), payload(true, false)).unwrap();
    assert!(verdict(&receipt.notes).contains("`files` moved"), "{:?}", receipt.notes);

    // ANOTHER SETUP is another closure, whatever the rows say.
    let mut other = payload(true, false);
    other.setup = Some("setup-b".to_string());
    let receipt = apply(&db, &ws.root(), other).unwrap();
    assert!(verdict(&receipt.notes).contains("no closure was recorded"), "{:?}", receipt.notes);
}

#[test]
fn a_run_that_fails_before_its_closure_leaves_nothing_to_keep() {
    // `forget` comes first: a record left by the last derivation would vouch for rows this run already replaced.
    let db = tmp("forget");
    let ws = Workspace::new("forget");
    apply(&db, &ws.root(), payload(false, true)).unwrap();
    let conn = Connection::open(&*db).unwrap();
    assert!(cells::meta::get(&conn, "closure:typescript").is_some(), "a derivation records what it read");
    forget(&conn, "typescript").unwrap();
    drop(conn);
    let receipt = apply(&db, &ws.root(), payload(true, true)).unwrap();
    assert!(verdict(&receipt.notes).contains("no closure was recorded"), "{:?}", receipt.notes);
}
