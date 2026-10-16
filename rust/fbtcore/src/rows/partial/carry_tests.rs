use super::*;
use crate::rows::{apply, Batch};
use serde_json::json;

fn tmp(name: &str) -> crate::rows::TempDb {
    crate::rows::TempDb::new("carry", name)
}

/// Two files, rows that hang off each by `owner_file`, and one derived table that hangs
/// off nothing.
fn seed(db: &Path) {
    let mut b = Batch {
        all: true,
        first: true,
        final_: true,
        lang: "typescript".to_string(),
        half: Some("typescript".to_string()),
        ..Default::default()
    };
    b.shas.insert("a.ts".to_string(), json!("sha-a"));
    b.shas.insert("b.ts".to_string(), json!("sha-b"));
    b.tables.insert("files".to_string(), json!([
        {"id": "f:1", "path": "a.ts", "sha": "sha-a"},
        {"id": "f:2", "path": "b.ts", "sha": "sha-b"}
    ]));
    b.tables.insert("bindings".to_string(), json!([
        {"id": "b:1", "owner_file": "f:1", "name": "one", "note": null,
         "gates_json": "[\"g:1\", \"g:2\"]"},
        {"id": "b:2", "owner_file": "f:2", "name": "two", "note": null, "gates": ["g:3"], "at": {"line": 4, "open": true}}
    ]));
    // No `owner_file`: a rollup node rebuilds whole.
    b.tables.insert("key_reach".to_string(), json!([
        {"key": "menu.home", "route": "template"}
    ]));
    apply(db, ".", &b).unwrap();
}

fn carry(db: &Path, affected: &[&str]) -> (Carried, Value) {
    let target = db.with_extension("carry.json");
    let list: Vec<String> = affected.iter().map(|s| s.to_string()).collect();
    let out = carry_over(db, &list, &target, "typescript", None).unwrap();
    (out, collected(&target))
}

/// THE FILE AS ITS READER PUTS IT BACK TOGETHER. It is one row a line now - see `carry_over` -
/// so a test that parsed it as one document would be asserting the shape of the wire rather
/// than the rows on it. This is what `TsMap.mjs` builds from the same lines.
fn collected(target: &Path) -> Value {
    let text = std::fs::read_to_string(target).unwrap();
    let mut tables: Map<String, Value> = Map::new();
    let mut claims = Value::Object(Map::new());
    let mut rollup = Value::Null;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let one: Value = serde_json::from_str(line).unwrap();
        if let Some(found) = one.get("claims") {
            claims = found.clone();
            rollup = one.get("rollup").cloned().unwrap_or(Value::Null);
            continue;
        }
        let table = one["t"].as_str().unwrap().to_string();
        let entry = tables.entry(table).or_insert_with(|| Value::Array(Vec::new()));
        entry.as_array_mut().unwrap().push(one["r"].clone());
    }
    let mut out = Map::new();
    out.insert("tables".to_string(), Value::Object(tables));
    out.insert("claims".to_string(), claims);
    out.insert("rollup".to_string(), rollup);
    Value::Object(out)
}

#[test]
fn the_rows_of_a_file_being_re_extracted_are_not_sent_because_they_are_about_to_be_replaced() {
    let db = tmp("replaced");
    seed(&db);
    let (out, payload) = carry(&db, &["a.ts"]);
    let bindings = payload["tables"]["bindings"].as_array().unwrap();
    assert_eq!(bindings.len(), 1);
    assert_eq!(bindings[0]["id"], json!("b:2"));
    // ...BUT THE FILE ROW ITSELF IS, because the file is the same file.
    let files = payload["tables"]["files"].as_array().unwrap();
    assert_eq!(files.len(), 2, "a re-extracted file keeps its identity");
    assert_eq!(files[0]["id"], json!("f:1"));
    assert_eq!(out.rows, 3);
}

#[test]
fn a_carried_row_takes_its_structures_back_from_their_nodes_and_no_node_table_goes_out() {
    let db = tmp("nodes");
    seed(&db);
    let (out, payload) = carry(&db, &["a.ts"]);
    let two = &payload["tables"]["bindings"][0];
    assert_eq!((two["gates"].clone(), two["at"].clone()), (json!(["g:3"]), json!({"line": 4, "open": true})));
    assert!(payload["tables"].as_object().unwrap().keys().all(|t| !t.contains("__")), "{payload}");
    assert_eq!(out.tables, 2);
}

/// What the last full run measured about which ids must survive, stored where it stores it.
fn with_identity(db: &Path, identity: Value) {
    let conn = Connection::open(db).unwrap();
    crate::rows::meta::put_value(&conn, "spec:typescript", &serde_json::json!({"identity": identity})).unwrap();
}

#[test]
fn a_declaration_that_is_still_there_keeps_the_id_it_already_had() {
    // Minting it a fresh number makes every reference to it from another file dangle, the
    // reference passes resolve them onto the new number, and those are per-file rows a partial run
    // may not rewrite. Measured on a large workspace: dozens of `calls`, `expressions` and
    // `assignments`, every one the same fact under a different number.
    let db = tmp("claims");
    seed(&db);
    with_identity(&db, json!({"bindings": ["name"]}));
    let (_, payload) = carry(&db, &["a.ts"]);

    let claims = payload["claims"]["bindings"].as_object().unwrap();
    assert_eq!(claims.len(), 1, "one row is being replaced, so one id is kept");
    assert_eq!(claims["f:1\u{0}one"], json!("b:1"));
}

#[test]
fn a_row_an_earlier_alternative_cannot_name_is_claimed_by_the_next_and_numbered_as_node_numbers_it() {
    // A template expression has no line: the sparse key every other expression is named by leaves it out, and only
    // an ALTERNATIVE keeps its id. The number in front is the alternative's, as `identityOf` writes it.
    let db = tmp("alternatives");
    seed(&db);
    with_identity(&db, json!({"bindings": [["name", "note"], ["name"]]}));
    let (_, payload) = carry(&db, &["a.ts"]);
    let claims = payload["claims"]["bindings"].as_object().unwrap();
    assert_eq!(claims["f:1\u{0}1\u{1}one"], json!("b:1"), "{claims:?}");
}

#[test]
fn a_rollups_ids_go_back_in_the_order_it_made_them() {
    // A ROLLUP REBUILT FROM THE SAME ROWS MAKES THE SAME ROWS: handed its old ids in order, it keeps them, and the
    // closure that reads them does not move for nothing. Every row of a table node rebuilds whole; of a per-file table,
    // only the rows no file owns - the rest are carried with their own ids.
    let db = tmp("rollup");
    let mut b = Batch { all: true, first: true, final_: true, lang: "typescript".to_string(), half: Some("typescript".to_string()), ..Default::default() };
    b.shas.insert("a.ts".to_string(), json!("sha-a"));
    b.tables.insert("files".to_string(), json!([{"id": "f:1", "path": "a.ts", "sha": "sha-a"}]));
    b.tables.insert("routes".to_string(), json!([
        {"id": "rt:2", "owner_file": "f:1", "path": "b"}, {"id": "rt:1", "owner_file": "f:1", "path": "a"}
    ]));
    b.tables.insert("renders".to_string(), json!([
        {"id": "rd:1", "owner_file": "f:1", "to_class": "c:1"}, {"id": "rd:7", "to_class": "c:2", "call": "cl:9"}
    ]));
    apply(&db, ".", &b).unwrap();
    let conn = Connection::open(&*db).unwrap();
    crate::rows::meta::put_value(&conn, "spec:typescript", &json!({"rebuilt": ["routes", "projects"]})).unwrap();
    drop(conn);
    let (_, payload) = carry(&db, &[]);
    assert_eq!(payload["rollup"]["routes"], json!(["rt:2", "rt:1"]), "the order they were made in, not their numbers'");
    assert_eq!(payload["rollup"]["renders"], json!(["rd:7"]), "the per-file render is carried, not handed back");
    assert!(payload["rollup"].get("files").is_none());
}

#[test]
fn a_row_that_belongs_to_no_file_is_derived_and_is_never_sent() {
    // The same rule as the table-level one, a level down. Carrying these put a copy beside each
    // freshly derived one, naming the same edge through a different call.
    let db = tmp("ownerless");
    let mut b = Batch {
        all: true,
        first: true,
        final_: true,
        lang: "typescript".to_string(),
        half: Some("typescript".to_string()),
        ..Default::default()
    };
    b.shas.insert("a.ts".to_string(), json!("sha-a"));
    b.tables.insert("files".to_string(), json!([{"id": "f:1", "path": "a.ts", "sha": "sha-a"}]));
    b.tables.insert("renders".to_string(), json!([
        {"id": "rd:1", "owner_file": "f:1", "to_class": "c:1"},
        {"id": "rd:2", "to_class": "c:2", "call": "cl:9"}
    ]));
    apply(&db, ".", &b).unwrap();

    let (out, payload) = carry(&db, &[]);
    let rows = payload["tables"]["renders"].as_array().unwrap();
    assert_eq!(rows.len(), 1, "the one a rollup derives is left to be derived again");
    assert_eq!(rows[0]["id"], json!("rd:1"));
    assert_eq!(out.rows, 2, "it and the file row");
}

#[test]
fn a_table_nothing_outside_a_file_names_hands_back_no_id_at_all() {
    // The measurement says which tables those are; a table absent from it is one whose rows are
    // named only from inside their own file, and a fresh id for one of those breaks nothing.
    let db = tmp("noclaims");
    seed(&db);
    with_identity(&db, json!({}));
    let (_, payload) = carry(&db, &["a.ts"]);
    assert!(payload["claims"].as_object().unwrap().is_empty());
}

#[test]
fn a_row_whose_key_is_not_whole_is_left_unclaimable_rather_than_matched_on_less() {
    // A key that is not the whole key would match a row that merely looks alike, and the reference
    // it then kept would name the wrong declaration.
    let db = tmp("partialkey");
    seed(&db);
    with_identity(&db, json!({"bindings": ["name", "note"]}));
    let (_, payload) = carry(&db, &["a.ts"]);
    assert!(payload["claims"].as_object().unwrap().is_empty(),
            "`note` is null on that row, so it has no key");
}

#[test]
fn a_table_with_no_owner_file_is_derived_and_is_never_sent() {
    // node rebuilds those whole and would throw them away. `files` is the exception.
    let db = tmp("derived");
    seed(&db);
    let (_, payload) = carry(&db, &[]);
    let tables = payload["tables"].as_object().unwrap();
    assert!(tables.contains_key("files"), "a file row IS a file");
    assert!(tables.contains_key("bindings"));
    assert!(!tables.contains_key("key_reach"), "derived, so rebuilt whole");
}

#[test]
fn nothing_affected_carries_everything_that_is_not_derived() {
    let db = tmp("nothing");
    seed(&db);
    let (out, payload) = carry(&db, &[]);
    assert_eq!(payload["tables"]["files"].as_array().unwrap().len(), 2);
    assert_eq!(payload["tables"]["bindings"].as_array().unwrap().len(), 2);
    assert_eq!((out.rows, out.tables), (4, 2));
}

#[test]
fn an_encoded_column_is_passed_through_as_TEXT_and_never_decoded() {
    // Turning it back into a structure only to re-encode it one line later cost more
    // than twice as long, on a run whose whole purpose is to be shorter than a rebuild.
    let db = tmp("text");
    seed(&db);
    let (_, payload) = carry(&db, &["a.ts"]);
    let row = &payload["tables"]["bindings"][0];
    assert_eq!(row["name"], json!("two"));
    let db2 = tmp("text2");
    seed(&db2);
    let (_, all) = carry(&db2, &[]);
    let first = all["tables"]["bindings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == json!("b:1"))
        .unwrap();
    assert_eq!(first["gates_json"], json!("[\"g:1\", \"g:2\"]"), "still a string");
}

#[test]
fn a_null_column_is_left_out_rather_than_carried_as_null() {
    let db = tmp("null");
    seed(&db);
    let (_, payload) = carry(&db, &[]);
    let row = payload["tables"]["bindings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == json!("b:2"))
        .unwrap()
        .as_object()
        .unwrap();
    assert!(!row.contains_key("note"));
}

#[test]
fn the_half_column_never_goes_out_because_the_reader_has_no_use_for_it() {
    let db = tmp("half");
    seed(&db);
    let (_, payload) = carry(&db, &[]);
    for table in payload["tables"].as_object().unwrap().values() {
        for row in table.as_array().unwrap() {
            assert!(!row.as_object().unwrap().contains_key(HALF_COLUMN));
        }
    }
}

#[test]
fn carrying_from_a_database_that_is_not_there_says_so_rather_than_writing_an_empty_map() {
    // An empty carry file reads as "this map has nothing else in it", which is the one
    // answer a partial run must never be given.
    let missing = std::env::temp_dir().join("fbt-carry-nothing-at-all.db");
    let _ = std::fs::remove_file(&missing);
    let target = std::env::temp_dir().join("fbt-carry-nothing-at-all.json");
    let _ = std::fs::remove_file(&target);
    let err = carry_over(&missing, &[], &target, "typescript", None).unwrap_err();
    assert!(format!("{err:#}").contains("no database"));
    assert!(!target.exists(), "and it writes nothing");
}

#[test]
fn another_halfs_rows_are_not_this_halfs_to_carry() {
    let db = tmp("other");
    let mut b = Batch {
        all: true,
        first: true,
        final_: true,
        lang: "csharp".to_string(),
        half: None,
        ..Default::default()
    };
    b.shas.insert("a.cs".to_string(), json!("sha-c"));
    b.tables.insert("files".to_string(), json!([{"id": "f:9", "path": "a.cs", "sha": "sha-c"}]));
    apply(&db, ".", &b).unwrap();
    seed(&db);

    let (_, payload) = carry(&db, &[]);
    let paths: Vec<&str> = payload["tables"]["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["path"].as_str().unwrap())
        .collect();
    assert_eq!(paths, vec!["a.ts", "b.ts"]);
}

#[test]
fn the_file_is_spelled_the_way_python_spells_it() {
    // Same separators as python's `json.dump`, so two carry files can be compared byte for
    // byte rather than only after a parse.
    //
    // ONE ROW A LINE, and the last line the claims - see `carry_over` for why it is no longer
    // one document. The reader is `TsMap.mjs`, and nothing else reads this file.
    let db = tmp("spelling");
    seed(&db);
    let target = db.with_extension("carry.json");
    carry_over(&db, &[], &target, "typescript", None).unwrap();
    let text = std::fs::read_to_string(&target).unwrap();
    let mut lines = text.lines();
    let first = lines.next().unwrap();
    assert!(first.starts_with("{\"t\": \"bindings\", \"r\": {"), "{first}");
    assert!(first.ends_with('}'), "a line is one row and nothing else: {first}");
    assert!(text.contains("\"id\": \"b:1\""));
    assert!(!text.contains("\"id\":\"b:1\""));
    let last = text.lines().next_back().unwrap();
    assert!(last.starts_with("{\"claims\": {"), "the claims come last: {last}");
}

/// The carry file's lines as they are, `o` and all - `collected` puts the rows back together the way node reads them.
fn lines(db: &Path, affected: &[&str], uncarried: Value) -> Vec<Value> {
    let target = db.with_extension("carry.json");
    let list: Vec<String> = affected.iter().map(|s| s.to_string()).collect();
    carry_over(db, &list, &target, "typescript", Some(&uncarried)).unwrap();
    std::fs::read_to_string(&target).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect()
}

#[test]
fn a_cell_node_never_reads_goes_out_as_its_kind_alone_and_only_on_the_rows_it_names() {
    // REBUILDING A STRUCTURE node only holds was most of the carry: such a cell is its kind (`a` a list, `o` an object),
    // and a `[column, value]` stub leaves the rows whose column holds anything else whole.
    let db = tmp("stubs");
    seed(&db);
    let out = lines(&db, &["a.ts"], json!({"bindings": {"gates": true, "at": ["name", "one"]}}));
    let two = out.iter().find(|l| l["r"]["id"] == json!("b:2")).unwrap();
    assert_eq!(two["o"], json!({"gates": "a"}), "{two}");
    assert!(two["r"].get("gates").is_none(), "a stubbed cell is not rebuilt: {two}");
    assert_eq!(two["r"]["at"], json!({"line": 4, "open": true}), "b:2 is not named `one`: its cell is carried whole");
    let named = lines(&db, &["a.ts"], json!({"bindings": {"at": ["name", "two"]}}));
    let two = named.iter().find(|l| l["r"]["id"] == json!("b:2")).unwrap();
    assert_eq!(two["o"], json!({"at": "o"}), "{two}");
    // A ROW WITH NO STRUCTURE THERE goes out with none, stubbed or not.
    let all = lines(&db, &[], json!({"bindings": {"at": true}}));
    let one = all.iter().find(|l| l["r"]["id"] == json!("b:1")).unwrap();
    assert!(one.get("o").is_none(), "b:1 holds no `at`: {one}");
}

#[test]
fn a_table_node_rebuilds_whole_sends_no_rows_but_its_ids_and_claims_still_go() {
    // node puts back no row of a `rebuilt` table (`TsMap.mjs`); `projects` it reads first, to re-point the files.
    let db = tmp("rebuilt");
    seed(&db);
    let conn = Connection::open(&*db).unwrap();
    crate::rows::meta::put_value(&conn, "spec:typescript", &json!({"rebuilt": ["bindings"], "identity": {"bindings": ["name"]}})).unwrap();
    drop(conn);
    let (out, payload) = carry(&db, &["a.ts"]);
    assert!(payload["tables"].get("bindings").is_none(), "{payload}");
    assert_eq!(payload["rollup"]["bindings"], json!(["b:1", "b:2"]));
    assert_eq!(payload["claims"]["bindings"]["f:1\u{0}one"], json!("b:1"));
    assert_eq!(out.tables, 1, "files alone");
}
