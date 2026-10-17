use super::*;
use serde_json::json;

fn ids_of(rows: Value) -> IndexSet<String> {
    super::super::digest::Digest::over(rows.as_object().unwrap(), &[]).ids
}

fn ask(db: &Connection, replaced: &[&str], whole: &[&str], kept: &IndexSet<String>, named: &IndexMap<String, String>, deps: Value, nested: bool) -> Option<(usize, String)> {
    let replaced: Vec<String> = replaced.iter().map(|s| s.to_string()).collect();
    let whole: Vec<String> = whole.iter().map(|s| s.to_string()).collect();
    let plan = Replace { half: "typescript", replaced: &replaced, whole: &whole, kept, named };
    dangling(db, &plan, deps.as_object().unwrap(), nested).unwrap()
}

fn base() -> Connection {
    let db = Connection::open_in_memory().unwrap();
    db.execute_batch(
        "CREATE TABLE files (id TEXT, path TEXT, half TEXT);
         CREATE TABLE classes (id TEXT, file TEXT, owner_file TEXT, half TEXT);
         CREATE TABLE calls (id TEXT, file TEXT, owner_file TEXT, target TEXT, half TEXT);
         INSERT INTO files VALUES ('f:1', 'a.ts', 'typescript'), ('f:2', 'b.ts', 'typescript'), ('f:3', 'c.ts', 'typescript');
         INSERT INTO classes VALUES ('c:1', 'f:2', 'f:2', 'typescript'), ('c:2', 'f:2', 'f:2', 'typescript');
         INSERT INTO calls VALUES ('x:1', 'f:1', 'f:1', 'c:1', 'typescript');",
    )
    .unwrap();
    db
}

#[test]
fn a_kept_row_naming_an_id_the_run_replaces_away_is_found_and_one_kept_is_not() {
    let db = base();
    let none = IndexMap::new();
    // b.ts is read again and keeps c:1: nothing a.ts holds is left pointing at nothing.
    let kept = ids_of(json!({"classes": [{"id": "c:1", "file": "f:2"}]}));
    assert_eq!(ask(&db, &["b.ts"], &[], &kept, &none, json!({"b.ts": ["a.ts"]}), false), None);
    // b.ts comes back with c:1 under a new id: a.ts's call would name a row that is gone.
    let renumbered = ids_of(json!({"classes": [{"id": "c:9", "file": "f:2"}, {"id": "c:2", "file": "f:2"}]}));
    assert_eq!(ask(&db, &["b.ts"], &[], &renumbered, &none, json!({"b.ts": ["a.ts"]}), false), Some((1, "calls -> c:1".to_string())));
}

#[test]
fn an_id_a_kept_row_holds_inside_a_structure_is_found_where_the_run_stopped_short_and_nowhere_else() {
    // c.ts depends on nothing the graph knows of: the id sits inside a member's folded value, which is NODE rows.
    let db = base();
    let members = [json!({"id": "m:1", "file": "f:3", "owner_file": "f:3", "half": "typescript", "value": {"$call": {"args": [{"$expr_id": "c:1"}]}}})];
    crate::rows::store::insert_rows(&db, "members", &members).unwrap();
    let ids = ids_of(json!({"classes": [{"id": "c:9", "file": "f:2"}, {"id": "c:2", "file": "f:2"}]}));
    let none = IndexMap::new();
    let found = ask(&db, &["b.ts"], &[], &ids, &none, json!({"b.ts": ["a.ts"]}), true);
    // a.ts's plain call and c.ts's nested value both name it.
    assert_eq!(found.map(|(n, _)| n), Some(2));
    assert_eq!(ask(&db, &["b.ts"], &[], &ids, &none, json!({}), true).unwrap().1, "members.value -> c:1");
    // A run over every hop never asked it of a structure, and still does not.
    assert_eq!(ask(&db, &["b.ts"], &[], &ids, &none, json!({}), false), None);
}

#[test]
fn an_id_of_a_whole_table_the_payload_does_not_hold_again_is_found_in_a_kept_row_and_a_sent_one() {
    // `routes` is rebuilt whole: r:1 comes back as r:5. A kept call names r:1 in a plain cell, a kept member inside its
    // value, and a whole table's row the payload sends names it too - each would name nothing once the replace is done.
    let db = base();
    db.execute_batch(
        "CREATE TABLE routes (id TEXT, path TEXT, half TEXT);
         INSERT INTO routes VALUES ('r:1', 'shop', 'typescript'), ('r:2', 'cart', 'typescript');
         INSERT INTO calls VALUES ('x:2', 'f:3', 'f:3', 'r:1', 'typescript');",
    )
    .unwrap();
    let kept = ids_of(json!({"routes": [{"id": "r:5"}, {"id": "r:2"}]}));
    let none = IndexMap::new();
    assert_eq!(ask(&db, &["a.ts"], &["routes"], &kept, &none, json!({}), true), Some((1, "calls -> r:1".to_string())));
    let named: IndexMap<String, String> = [("r:1".to_string(), "component_reach.routes".to_string())].into_iter().collect();
    db.execute("DELETE FROM calls WHERE id = 'x:2'", []).unwrap();
    assert_eq!(ask(&db, &["a.ts"], &["routes"], &kept, &named, json!({}), true), Some((1, "component_reach.routes -> r:1".to_string())));
    // THE SAME ID BACK, and nothing is gone.
    let same = ids_of(json!({"routes": [{"id": "r:1"}, {"id": "r:2"}]}));
    assert_eq!(ask(&db, &["a.ts"], &["routes"], &same, &named, json!({}), true), None);
}

#[test]
fn a_kept_row_of_a_table_with_no_id_is_asked_too_and_a_string_that_only_looks_like_an_id_is_not_a_reason() {
    let db = base();
    db.execute_batch(
        "CREATE TABLE diagnostics (kind TEXT, target TEXT, owner_file TEXT, half TEXT);
         INSERT INTO diagnostics VALUES ('unresolved', 'c:1', 'f:3', 'typescript'), ('note', 'zz:77', 'f:3', 'typescript');",
    )
    .unwrap();
    let none = IndexMap::new();
    let renumbered = ids_of(json!({"classes": [{"id": "c:9", "file": "f:2"}, {"id": "c:2", "file": "f:2"}]}));
    db.execute("DELETE FROM calls", []).unwrap();
    assert_eq!(ask(&db, &["b.ts"], &[], &renumbered, &none, json!({}), true), Some((1, "diagnostics -> c:1".to_string())));
    // `zz:77` names no row that ever was: nothing vanished, nothing to read again for.
    let kept = ids_of(json!({"classes": [{"id": "c:1", "file": "f:2"}, {"id": "c:2", "file": "f:2"}]}));
    assert_eq!(ask(&db, &["b.ts"], &[], &kept, &none, json!({}), true), None);
}

#[test]
fn the_ids_a_whole_table_sends_are_gathered_however_deep_and_only_shaped_like_nodes_ids() {
    let tables = json!({"component_reach": [{"id": "cr:1", "owner_file": "f:9", "routes": ["r:1", {"via": ["r:2", "Zz:3", "abcde:4", "r:"]}], "label": "shop"}]});
    let mut digest = super::super::digest::Digest::new(&[]).naming(&["component_reach".to_string()]);
    for row in tables["component_reach"].as_array().unwrap() {
        digest.first("component_reach", row.as_object().unwrap());
    }
    let named: Vec<&String> = digest.named.keys().collect();
    assert_eq!(named, vec!["r:1", "r:2"]);
    assert_eq!(digest.named["r:2"], "component_reach.routes");
}
