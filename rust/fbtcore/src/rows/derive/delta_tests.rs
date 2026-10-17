//! The tests of `delta.rs`, apart from it so that file stays under the line ceiling.
use super::*;
use serde_json::json;

const HALF: &str = "typescript";

fn rows(values: Value) -> Vec<Value> {
    values.as_array().unwrap().clone()
}

/// A table written the way the store writes one, every row stamped.
fn written(rows: &[Value]) -> Connection {
    let db = Connection::open_in_memory().unwrap();
    crate::rows::store::insert_rows(&db, "key_reach", rows).unwrap();
    db
}

/// What the table holds, as a multiset of row digests.
fn content(db: &Connection) -> Vec<Digest> {
    let mut out: Vec<Digest> = stored(db, "key_reach", HALF).unwrap().into_iter().flat_map(|(d, at)| at.into_iter().map(move |_| d)).collect();
    out.sort();
    out
}

fn first() -> Vec<Value> {
    rows(json!([
        {"id": "kr:1", "key": "demo.a", "paths": [["c:1", "c:2"]], "half": HALF},
        {"id": "kr:2", "key": "demo.b", "paths": [], "half": HALF},
        {"id": "kr:3", "key": "demo.c", "depth": 2, "half": HALF}
    ]))
}

#[test]
fn rows_already_stored_are_kept_and_nothing_is_written() {
    let db = written(&first());
    let (fresh, delta) = reconcile(&db, "key_reach", first(), HALF).unwrap();
    assert!(fresh.is_empty());
    assert_eq!(delta, Delta { kept: 3, deleted: 0, inserted: 0 });
}

#[test]
fn only_the_rows_that_differ_are_deleted_and_written_and_the_table_ends_as_emitted() {
    let db = written(&first());
    // kr:1 moves in a NODE, kr:3 in a scalar, kr:2 is gone, kr:4 is new and kr:5 is emitted twice.
    let emitted = rows(json!([
        {"id": "kr:1", "key": "demo.a", "paths": [["c:1", "c:3"]], "half": HALF},
        {"id": "kr:3", "key": "demo.c", "depth": 3, "half": HALF},
        {"id": "kr:4", "key": "demo.d", "half": HALF},
        {"key": "demo.e", "half": HALF},
        {"key": "demo.e", "half": HALF}
    ]));
    let (fresh, delta) = reconcile(&db, "key_reach", emitted.clone(), HALF).unwrap();
    assert_eq!(delta, Delta { kept: 0, deleted: 3, inserted: 5 });
    crate::rows::store::insert_rows(&db, "key_reach", &fresh).unwrap();
    assert_eq!(content(&db), content(&written(&emitted)), "what is stored is what was emitted, nodes and all");
    let orphans: i64 = db
        .query_row("SELECT count(*) FROM key_reach__paths WHERE owner NOT IN (SELECT id FROM key_reach WHERE id IS NOT NULL)", [], |r| r.get(0))
        .unwrap();
    assert_eq!(orphans, 0, "a deleted row's nodes go with it");
}

#[test]
fn an_absent_cell_and_a_null_one_are_the_same_stored_row() {
    // A table holds a column for every row once any row has it, so the stored row says NULL where the emitted
    // one said nothing: they are one row, not a delete and an insert.
    let db = written(&first());
    let emitted = rows(json!([
        {"id": "kr:1", "key": "demo.a", "paths": [["c:1", "c:2"]], "depth": null, "half": HALF},
        {"id": "kr:2", "key": "demo.b", "paths": [], "half": HALF},
        {"id": "kr:3", "key": "demo.c", "depth": 2, "half": HALF}
    ]));
    let (_, delta) = reconcile(&db, "key_reach", emitted.clone(), HALF).unwrap();
    assert_eq!(delta.kept, 3);
}

#[test]
fn another_halfs_rows_are_never_matched_or_deleted() {
    let mut mine = first();
    mine.push(json!({"id": "kr:9", "key": "demo.z", "half": "csharp"}));
    let db = written(&mine);
    let (_, delta) = reconcile(&db, "key_reach", Vec::new(), HALF).unwrap();
    assert_eq!(delta.deleted, 3);
    let left: i64 = db.query_row("SELECT count(*) FROM key_reach", [], |r| r.get(0)).unwrap();
    assert_eq!(left, 1);
}
