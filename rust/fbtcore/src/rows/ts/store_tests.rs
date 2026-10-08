//! The tests of `store.rs`, apart from it only so that file stays under the line ceiling.
//! It is still `store::tests`, so `super::*` is the store.

use super::*;
use serde_json::json;

fn db_with(spec: Option<&str>, rows: &[(&str, &str, i64)]) -> Connection {
    let db = Connection::open_in_memory().unwrap();
    db.execute_batch(
        "CREATE TABLE _meta (key TEXT, value TEXT);
         CREATE TABLE renders (id TEXT, gate_chain TEXT, n INTEGER, half TEXT);",
    )
    .unwrap();
    if let Some(spec) = spec {
        db.execute("INSERT INTO _meta VALUES ('spec:typescript', ?1)", [spec])
            .unwrap();
    }
    for (id, chain, n) in rows {
        db.execute(
            "INSERT INTO renders (id, gate_chain, n, half) VALUES (?1, ?2, ?3, 'typescript')",
            rusqlite::params![id, chain, n],
        )
        .unwrap();
    }
    db
}

#[test]
fn rows_come_back_in_the_order_they_went_in_not_by_id() {
    // `r:1000` sorts before `r:2` as a STRING, and the walk below this is
    // order-sensitive, so an ORDER BY id would quietly reorder the DAG.
    let db = db_with(None, &[("r:1000", "[]", 1), ("r:2", "[]", 2)]);
    let store = Store::from_db(&db, "typescript").unwrap();
    let rows = store.table("renders");
    assert_eq!(rows[0]["id"], json!("r:1000"));
    assert_eq!(rows[1]["id"], json!("r:2"));
}

#[test]
fn a_boolean_column_the_spec_names_comes_back_a_boolean_and_not_the_integer_sqlite_stored() {
    // `gate_cases` asks `is_default == true`; read back as 1, every switch's default case lost its key.
    let spec = r#"{"bool_columns": {"renders": ["n"]}}"#;
    let db = db_with(Some(spec), &[("r:1", "[]", 1), ("r:2", "[]", 0), ("r:3", "[]", 7)]);
    let store = Store::from_db(&db, "typescript").unwrap();
    let rows = store.table("renders");
    assert_eq!(rows[0]["n"], json!(true));
    assert_eq!(rows[1]["n"], json!(false));
    assert_eq!(rows[2]["n"], json!(7), "only 0 and 1 are a boolean's");
    let bare = db_with(None, &[("r:1", "[]", 1)]);
    let plain = Store::from_db(&bare, "typescript").unwrap();
    assert_eq!(plain.table("renders")[0]["n"], json!(1), "a column the spec does not name stays as stored");
}

#[test]
fn a_column_the_spec_names_is_decoded_and_one_it_does_not_is_left_alone() {
    let spec = r#"{"json_columns": {"renders": ["gate_chain"]}}"#;
    let db = db_with(Some(spec), &[("r:1", r#"["a","b"]"#, 1)]);
    let store = Store::from_db(&db, "typescript").unwrap();
    let rows = store.table("renders");
    assert_eq!(rows[0]["gate_chain"], json!(["a", "b"]));

    // With no spec the same cell stays the text it is stored as: a source string can
    // SPELL a structure, and deciding per value would decode it.
    let bare = db_with(None, &[("r:1", r#"["a","b"]"#, 1)]);
    let plain = Store::from_db(&bare, "typescript").unwrap();
    assert_eq!(plain.table("renders")[0]["gate_chain"], json!(r#"["a","b"]"#));
}

#[test]
fn text_that_opens_like_a_structure_but_does_not_parse_stays_text() {
    let spec = r#"{"json_columns": {"renders": ["gate_chain"]}}"#;
    let db = db_with(Some(spec), &[("r:1", "{not json", 1)]);
    let store = Store::from_db(&db, "typescript").unwrap();
    assert_eq!(store.table("renders")[0]["gate_chain"], json!("{not json"));
}

#[test]
fn the_half_column_never_reaches_a_pass() {
    let db = db_with(None, &[("r:1", "[]", 1)]);
    let store = Store::from_db(&db, "typescript").unwrap();
    assert_eq!(store.table("renders")[0].get("half"), None);
}

#[test]
fn every_emitted_row_carries_the_half_that_made_it() {
    // Without the stamp `drop_half` never finds the table, and every rebuild APPENDS.
    let db = db_with(None, &[]);
    let mut store = Store::from_db(&db, "typescript").unwrap();
    store.emit("render_path", Row::Built(json!({"component": "c:1"}).as_object().unwrap().clone()));
    assert_eq!(store.emitted["render_path"][0]["half"], json!("typescript"));
}

#[test]
fn a_table_nobody_wrote_is_empty_rather_than_an_error() {
    let db = db_with(None, &[]);
    let store = Store::from_db(&db, "typescript").unwrap();
    assert!(store.table("nothing_wrote_this").is_empty());
}

#[test]
fn the_payload_and_the_database_serve_the_same_shape() {
    let tables = serde_json::from_str::<Map<String, Value>>(
        r#"{"renders": [{"id": "r:1", "n": 1, "half": "typescript"}]}"#,
    )
    .unwrap();
    let store = Store::from_payload(tables, "typescript");
    let rows = store.table("renders");
    assert_eq!(rows[0]["id"], json!("r:1"));
    assert_eq!(rows[0].get("half"), None, "the stamp is housekeeping, not a fact");
}

#[test]
fn the_rows_handed_in_come_back_out_in_the_order_they_arrived() {
    // THE TABLE ORDER IS THE PAYLOAD'S. `cache` is a `HashMap`, so a store that gave the
    // tables back in its own order would write them in an order that changes between runs.
    let tables = serde_json::from_str::<Map<String, Value>>(
        r#"{"zebra": [{"id": "z:1"}], "alpha": [{"id": "a:1"}], "middle": [{"id": "m:1"}]}"#,
    )
    .unwrap();
    let mut store = Store::from_payload(tables, "typescript");
    let back = store.take_tables();
    assert_eq!(
        back.keys().collect::<Vec<_>>(),
        vec!["zebra", "alpha", "middle"],
        "the payload's order, not the cache's"
    );
    assert_eq!(back["alpha"][0]["id"], json!("a:1"));
}

#[test]
fn a_table_read_during_the_walk_still_comes_back_whole() {
    // `Rc::try_unwrap` is the fast path and it FAILS while somebody still holds the table.
    // Falling back to a copy is what keeps the answer right when it does.
    let tables = serde_json::from_str::<Map<String, Value>>(
        r#"{"renders": [{"id": "r:1"}, {"id": "r:2"}]}"#,
    )
    .unwrap();
    let mut store = Store::from_payload(tables, "typescript");
    let still_held = store.table("renders");
    let back = store.take_tables();
    assert_eq!(back["renders"].as_array().unwrap().len(), 2);
    assert_eq!(still_held.len(), 2, "the holder's copy is untouched");
}

#[test]
fn the_rows_given_back_do_not_carry_the_stamp_they_arrived_with() {
    // The storing side stamps `half` itself. A row that handed its own back would be trusted,
    // and a row that had been read out of a DIFFERENT half would then keep that half's name.
    let tables = serde_json::from_str::<Map<String, Value>>(
        r#"{"renders": [{"id": "r:1", "half": "csharp"}]}"#,
    )
    .unwrap();
    let mut store = Store::from_payload(tables, "typescript");
    let back = store.take_tables();
    assert!(back["renders"][0].get("half").is_none());
}

#[test]
fn the_tables_are_handed_back_once() {
    // A second call must not serve the same rows again: they have been moved out, and a
    // caller that wrote them twice would double every table.
    let tables = serde_json::from_str::<Map<String, Value>>(r#"{"renders": [{"id": "r:1"}]}"#)
        .unwrap();
    let mut store = Store::from_payload(tables, "typescript");
    assert_eq!(store.take_tables().len(), 1);
    assert_eq!(store.take_tables().len(), 0);
}
#[test]
fn a_column_a_row_never_had_is_not_a_column_holding_null() {
    // THE WHOLE RISK OF A SHARED SCHEMA. Every row gets every column of the table, so a row
    // that said nothing about `sha` must still read as saying nothing - not as saying null.
    // The passes ask `contains_key` and `get(...).is_none()` to mean "this row is silent".
    let tables = serde_json::from_str::<Map<String, Value>>(
        r#"{"files": [{"id": "f:1", "sha": null}, {"id": "f:2"}]}"#,
    )
    .unwrap();
    let store = Store::from_payload(tables, "typescript");
    let rows = store.table("files");

    assert_eq!(rows[0].get("sha"), Some(&Value::Null), "it carried sha, holding null");
    assert_eq!(rows[1].get("sha"), None, "it never carried sha at all");
}

#[test]
fn a_column_only_the_last_row_carries_is_still_a_column_of_the_table() {
    // The schema is the union over every row, in first-seen order. Taking it from the first
    // row alone would drop a column the extractor only writes sometimes.
    let tables = serde_json::from_str::<Map<String, Value>>(
        r#"{"calls": [{"id": "c:1"}, {"id": "c:2"}, {"id": "c:3", "new": true}]}"#,
    )
    .unwrap();
    let store = Store::from_payload(tables, "typescript");
    let rows = store.table("calls");
    assert_eq!(rows[2].get("new"), Some(&Value::Bool(true)));
    assert_eq!(rows[0].get("new"), None, "the earlier rows are silent about it");
}


#[test]
fn a_row_handed_back_says_exactly_what_it_arrived_saying() {
    // What the storing side writes comes through `into_map`. A column that was absent must
    // not reappear as null, or every silent row would start claiming a value.
    let tables = serde_json::from_str::<Map<String, Value>>(
        r#"{"files": [{"id": "f:1", "sha": null}, {"id": "f:2"}]}"#,
    )
    .unwrap();
    let mut store = Store::from_payload(tables, "typescript");
    let back = store.take_tables();
    let rows = back["files"].as_array().unwrap();
    assert_eq!(rows[0], serde_json::json!({"id": "f:1", "sha": null}));
    assert_eq!(rows[1], serde_json::json!({"id": "f:2"}), "still silent about sha");
}

#[test]
fn setting_a_column_on_a_row_does_not_reach_the_table() {
    // A row is handed out by value. Writing to one must not edit the shape every other row
    // of that table shares.
    let tables = serde_json::from_str::<Map<String, Value>>(
        r#"{"calls": [{"id": "c:1"}, {"id": "c:2"}]}"#,
    )
    .unwrap();
    let store = Store::from_payload(tables, "typescript");
    let mut mine = store.table("calls")[0].clone();
    mine.insert("mine".to_string(), Value::Bool(true));
    assert_eq!(mine.get("mine"), Some(&Value::Bool(true)));
    assert_eq!(store.table("calls")[0].get("mine"), None, "the table is untouched");
    assert_eq!(store.table("calls")[1].get("mine"), None);
}
