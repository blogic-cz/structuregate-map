//! `suggest.rs` over a map built here and an API spec in Markdown: routes in running text and code spans, a table of
//! property names, a table of seeded codes - each found against the right set, a column of prose against none.
use super::*;
use crate::facts::config::Source;

const SPEC: &str = "# Items API\n\nList them with `GET /api/items`, read one by GET /api/items/{itemId} and add one by\n\
`POST /api/items`. Old clients call /api/legacy.\n\n## Fields\n\n| Field | Meaning |\n|---|---|\n| itemId | the key |\n\
| displayName | what a person reads |\n| price | in cents |\n\n## Codes\n\n| Code | Note |\n|---|---|\n| 7 | laptop |\n\
| 8 | phone |\n| 9 | not seeded |\n";

fn map(path: &std::path::Path) {
    let db = Connection::open(path).unwrap();
    let put = |table: &str, rows: Vec<Value>| crate::rows::store::insert_rows(&db, table, &rows).unwrap();
    db.execute_batch(
        "CREATE TABLE http_endpoints (id, file, cls, method, line, verb, template, route, func);
         INSERT INTO http_endpoints VALUES ('he:1', 'Items.cs', 'ItemsController', 'List', 3, 'GET', 'api/items', 'api/items', ''),
           ('he:2', 'Items.cs', 'ItemsController', 'Get', 5, 'GET', 'api/items/{id:int}', 'api/items/{}', ''),
           ('he:3', 'Items.cs', 'ItemsController', 'Add', 7, 'POST', 'api/items', 'api/items', ''),
           ('he:4', 'Items.cs', 'ItemsController', 'Drop', 9, 'DELETE', 'api/items/{id}', 'api/items/{}', '');",
    )
    .unwrap();
    put("members", vec![
        json!({"id": "m:1", "file": "f:1", "line": 2, "kind": "property", "name": "ItemId", "cls": "ItemDto", "class_symbol": "Demo.ItemDto"}),
        json!({"id": "m:2", "file": "f:1", "line": 3, "kind": "property", "name": "Title", "cls": "ItemDto", "class_symbol": "Demo.ItemDto"}),
        json!({"id": "m:3", "file": "f:1", "line": 4, "kind": "property", "name": "Price", "cls": "ItemDto", "class_symbol": "Demo.ItemDto"}),
    ]);
    put("decorators", vec![json!({"id": "d:1", "file": "f:1", "line": 3, "cls": "ItemDto", "name": "JsonPropertyName", "target": "Title",
                                  "target_kind": "property", "arg_values": ["displayName"]})]);
    put("sql_seeds", vec![
        json!({"id": "sd:1", "file": "f:2", "line": 1, "condition": "", "schema": "", "name": "Items", "status": "bound", "columns": ["Code", "Name"], "row_values": [7, "Pen"]}),
        json!({"id": "sd:2", "file": "f:2", "line": 2, "condition": "", "schema": "", "name": "Items", "status": "bound", "columns": ["Code", "Name"], "row_values": [8, "Cup"]}),
    ]);
}

#[test]
fn each_part_of_a_spec_is_found_against_the_set_sharing_its_values() {
    let dir = std::env::temp_dir().join(format!("fbt-suggest-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("spec.md"), SPEC).unwrap();
    let db = dir.join("map.sqlite");
    map(&db);
    let config = Config {
        path: dir.join("structuregate.facts.json"),
        sources: vec![Source { name: "spec".into(), kind: "file".into(), id: String::new(), path: dir.join("spec.md"), shown: "spec.md".into() }],
        ..Default::default()
    };
    let lines = run(db.to_str().unwrap(), &config).unwrap();
    let said = lines.join("\n");
    assert!(said.contains("spec: routes the whole document: 3/4 values = endpoint every endpoint (4 code values)"), "{said}");
    assert!(said.contains("spec: table 1 under \"Fields\", column 1 \"Field\": 3/3 values = property Demo.ItemDto (3 code values)"), "{said}");
    assert!(said.contains("spec: table 1 under \"Codes\", column 1 \"Code\": 2/3 values = seed dbo.Items.Code (2 code values)"), "{said}");
    assert!(!said.contains("\"Meaning\""), "{said}");
    let written: Value = serde_json::from_str(&std::fs::read_to_string(dir.join(FILE_NAME)).unwrap()).unwrap();
    let routes = written["candidates"].as_array().unwrap().iter().find(|c| c["link"]["to"]["kind"] == "endpoint").unwrap();
    assert_eq!(routes["facts"], json!({"source": "spec", "section": "", "from": "routes", "name": routes["link"]["facts"]}));
    assert_eq!(routes["unmatched"], json!(["/api/legacy"]));
    let codes = written["candidates"].as_array().unwrap().iter().find(|c| c["link"]["to"]["kind"] == "seed").unwrap();
    assert_eq!(codes["link"]["to"], json!({"kind": "seed", "object": "dbo.Items", "column": "Code"}));
    assert_eq!(codes["facts"]["key"], "Code");
    let _ = std::fs::remove_dir_all(&dir);
}
