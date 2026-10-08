//! The tests of `tsapply.rs`, apart from it so that file stays under the line ceiling. It is still
//! `tsapply::tests`, so `super::*` is the store of this half.
use super::*;
use serde_json::json;

fn tmp(name: &str) -> crate::rows::TempDb {
    crate::rows::TempDb::new("tsapply", name)
}

fn payload(partial: bool, files: Value) -> TsPayload {
    let mut tables = Map::new();
    tables.insert("files".to_string(), files);
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
        ..Default::default()
    }
}

#[test]
fn a_partial_store_says_where_its_time_went() {
    // THE PARTIAL BRANCH WAS UNTIMED: only `remember the graph` reached the note, so a partial run slower
    // than a full one could not say which of its steps was.
    let db = tmp("phases");
    let root = std::env::temp_dir();
    let root = root.to_string_lossy();
    let files = json!([
        {"id": "f:1", "path": "a.ts", "ext": ".ts", "owner_file": "f:1"},
        {"id": "f:2", "path": "b.ts", "ext": ".ts", "owner_file": "f:2"}
    ]);
    apply(&db, &root, payload(false, files)).unwrap();
    let edited = json!([{"id": "f:3", "path": "a.ts", "ext": ".ts", "owner_file": "f:3"}]);
    let receipt = apply(&db, &root, payload(true, edited)).unwrap();
    assert!(receipt.partial);
    for phase in ["check the kept rows", "replace the affected rows", "write the rows", "derive the closure", "write the closure"] {
        assert!(receipt.phases.contains_key(phase), "no `{phase}` in {:?}", receipt.phases.keys().collect::<Vec<_>>());
    }
}

#[test]
fn the_source_is_read_from_the_workspace_below_the_root() {
    // `files.path` IS RELATIVE TO THE WORKSPACE, not the root. Read off the root, a workspace one folder down
    // (like `web` here) put no `.ts` or `.html` into `file_text` at all, and `--cat` could not show one.
    let db = tmp("workspace");
    let root = std::env::temp_dir().join(format!("fbt-tsapply-root-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("web")).unwrap();
    std::fs::write(root.join("web").join("a.ts"), "export const a = 1;\n").unwrap();
    std::fs::write(root.join("web").join("b.ts"), "export const b = 2;\n").unwrap();
    let files = json!([
        {"id": "f:1", "path": "a.ts", "ext": ".ts", "owner_file": "f:1"},
        {"id": "f:2", "path": "b.ts", "ext": ".ts", "owner_file": "f:2"}
    ]);
    let mut full = payload(false, files);
    full.fe = Some("web".to_string());
    apply(&db, &root.to_string_lossy(), full).unwrap();
    let held: Vec<String> = {
        let db = Connection::open(&db).unwrap();
        let mut stmt = db.prepare("SELECT path FROM file_text ORDER BY path").unwrap();
        stmt.query_map([], |r| r.get(0)).unwrap().map(|r| r.unwrap()).collect()
    };
    let _ = std::fs::remove_dir_all(&root);
    assert_eq!(held, vec!["a.ts", "b.ts"]);
}
