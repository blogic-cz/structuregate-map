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
    assert!(said.contains("spec: table 1 under \"Codes\", column 1 \"Code\": 2/3 values = seed dbo.Items.Code (2 code values, ~2 by chance)"), "{said}");
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

#[test]
fn the_link_whose_rows_agree_outranks_a_dense_small_integer_set() {
    let dir = std::env::temp_dir().join(format!("fbt-suggest-rank-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let names = ["Pen", "Cup", "Bag", "Hat", "Mug", "Box", "Kit", "Jar"];
    let mut doc = String::from("# Goods\n\n| ID | Product |\n|---|---|\n");
    for (i, n) in names.iter().enumerate() {
        doc.push_str(&format!("| {} | {n} |\n", i + 1));
    }
    std::fs::write(dir.join("p.md"), doc).unwrap();
    let db = dir.join("map.sqlite");
    {
        let conn = Connection::open(&db).unwrap();
        let put = |table: &str, rows: Vec<Value>| crate::rows::store::insert_rows(&conn, table, &rows).unwrap();
        // THE TRUE LINK: ids 1..7 (one product the document has is not seeded, one name differs), sparse beside 9000.
        let mut seeds: Vec<Value> = names.iter().take(7).enumerate().map(|(i, n)| {
            let name = if i == 3 { "Cap" } else { n };
            json!({"id": format!("sd:{i}"), "file": "f:1", "line": i, "condition": "", "schema": "", "name": "Goods", "status": "bound",
                   "columns": ["ItemNo", "Name"], "row_values": [i + 1, name]})
        }).collect();
        seeds.push(json!({"id": "sd:99", "file": "f:1", "line": 99, "condition": "", "schema": "", "name": "Goods", "status": "bound",
                          "columns": ["ItemNo", "Name"], "row_values": [9000, "Far"]}));
        put("sql_seeds", seeds);
        // A DENSE SMALL-INT ENUM sharing every id and none of the names.
        let members: Vec<Value> = (1..=12).map(|n| json!({"name": format!("M{n}"), "value": n})).collect();
        put("enums", vec![json!({"id": "en:1", "file": "f:2", "line": 1, "name": "Tier", "symbol": "Demo.Tier", "members": members})]);
    }
    let config = Config {
        path: dir.join("structuregate.facts.json"),
        sources: vec![Source { name: "p".into(), kind: "file".into(), id: String::new(), path: dir.join("p.md"), shown: "p.md".into() }],
        ..Default::default()
    };
    let lines = run(db.to_str().unwrap(), &config).unwrap();
    let at = |what: &str| lines.iter().position(|l| l.contains(what)).unwrap_or_else(|| panic!("no {what} in {lines:#?}"));
    let seed = at("values = seed dbo.Goods.ItemNo");
    assert!(seed < at("values = enum Demo.Tier"), "{lines:#?}");
    assert!(lines[seed].contains("7/8 values = seed dbo.Goods.ItemNo (8 code values, 6 rows agree: column 2 = Name, ~7 by chance)"), "{}", lines[seed]);
    assert!(lines[at("values = enum Demo.Tier (12")].contains("~8 by chance"), "{lines:#?}");
    let written: Value = serde_json::from_str(&std::fs::read_to_string(dir.join(FILE_NAME)).unwrap()).unwrap();
    let keyed = written["candidates"].as_array().unwrap().iter().find(|c| c["link"]["to"]["column"] == "ItemNo").unwrap();
    assert_eq!((keyed["facts"]["key"].as_str(), keyed["facts"]["label"].as_str()), (Some("ID"), Some("Product")), "{keyed}");
    assert!(keyed["facts"].get("headers").is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_field_table_under_an_endpoints_heading_ranks_the_type_that_endpoint_returns_first() {
    let dir = std::env::temp_dir().join(format!("fbt-suggest-uses-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let doc = "# Auth\n\n## 2.1 /api/ticket POST\n\nReturns the ticket.\n\n### Response\n\n| Name | Meaning |\n|---|---|\n| ticketCode | the ticket |\n\
               | expiresIn | seconds |\n| userName | who |\n";
    std::fs::write(dir.join("a.md"), doc).unwrap();
    let db = dir.join("map.sqlite");
    {
        let conn = Connection::open(&db).unwrap();
        let put = |table: &str, rows: Vec<Value>| crate::rows::store::insert_rows(&conn, table, &rows).unwrap();
        conn.execute_batch(
            "CREATE TABLE http_endpoints (id, file, cls, method, line, verb, template, route, func);
             INSERT INTO http_endpoints VALUES ('he:1', 'f:1', 'AuthController', 'Ticket', 3, 'POST', 'api/ticket', 'api/ticket', 'fn:1'),
               ('he:2', 'f:1', 'AuthController', 'Me', 9, 'GET', 'api/me', 'api/me', 'fn:2');",
        )
        .unwrap();
        put("functions", vec![
            json!({"id": "fn:1", "file": "f:1", "name": "Ticket", "qualname": "AuthController.Ticket", "line": 3, "end_line": 7,
                   "type": "System.Threading.Tasks.Task<Microsoft.AspNetCore.Mvc.ActionResult<Demo.Envelope<Demo.TicketDto>>>"}),
            json!({"id": "fn:2", "file": "f:1", "name": "Me", "qualname": "AuthController.Me", "line": 9, "end_line": 10, "type": "Demo.ProfileDto"}),
        ]);
        put("parameters", vec![json!({"id": "p:1", "file": "f:1", "qualname": "AuthController.Ticket", "line": 4, "type": "Demo.LoginDto"})]);
        // THE DECOY FIRST: the same three names on a class another endpoint returns.
        let props = |cls: &str, names: &[(&str, &str)]| -> Vec<Value> {
            names.iter().enumerate().map(|(i, (n, t))| json!({"id": format!("m:{cls}:{i}"), "file": "f:2", "line": i, "kind": "property", "name": n,
                                                             "cls": cls, "class_symbol": format!("Demo.{cls}"), "type": t})).collect()
        };
        let three = [("TicketCode", "string"), ("ExpiresIn", "int"), ("UserName", "string")];
        let mut members = props("ProfileDto", &three);
        members.extend(props("TicketDto", &three));
        members.extend(props("LoginDto", &[("UserName", "string"), ("Password", "string")]));
        let mut envelope = props("Envelope<T>", &[("Data", "T"), ("Code", "int")]);
        envelope.iter_mut().for_each(|m| m["class_symbol"] = json!("Demo.Envelope<T>"));
        members.extend(envelope);
        put("members", members);
    }
    let config = Config {
        path: dir.join("structuregate.facts.json"),
        sources: vec![Source { name: "a".into(), kind: "file".into(), id: String::new(), path: dir.join("a.md"), shown: "a.md".into() }],
        ..Default::default()
    };
    let lines = run(db.to_str().unwrap(), &config).unwrap();
    let at = |what: &str| lines.iter().position(|l| l.contains(what)).unwrap_or_else(|| panic!("no {what} in {lines:#?}"));
    let (ticket, profile) = (at("= property Demo.TicketDto"), at("= property Demo.ProfileDto"));
    assert!(ticket < profile, "{lines:#?}");
    assert!(lines[ticket].contains("3/3 values = property Demo.TicketDto (3 code values, the response type of POST /api/ticket)"), "{}", lines[ticket]);
    assert!(lines[profile].contains("(3 code values, no type of POST /api/ticket)"), "{}", lines[profile]);
    let written: Value = serde_json::from_str(&std::fs::read_to_string(dir.join(FILE_NAME)).unwrap()).unwrap();
    let of = |symbol: &str| written["candidates"].as_array().unwrap().iter().find(|c| c["link"]["to"]["symbol"] == symbol).cloned().unwrap();
    assert_eq!((of("Demo.TicketDto")["endpoint_type"].as_str(), of("Demo.ProfileDto")["endpoint_type"].as_str()), (Some("response"), None));
    assert_eq!(of("Demo.TicketDto")["section_endpoints"], json!(["POST /api/ticket"]));
    assert_eq!((of("Demo.ProfileDto")["endpoints_using"].clone(), of("Demo.ProfileDto")["endpoints_using_count"].clone()), (json!(["GET /api/me (response)"]), json!(1)));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_place_is_under_the_routes_its_headings_name_and_its_own_text_not_an_overview_or_a_siblings() {
    let doc = "# API\n\nAll calls start /api.\n\n## Items\n\nCall GET /api/items.\n\n### Old\n\nNot /api/old.\n\n### 2.1 /api/items/{id} DELETE\n\n\
               #### Response\n\n| a | b |\n|---|---|\n| 1 | 2 |\n";
    let blocks = reader::from_markdown(doc);
    let table = blocks.iter().position(|b| matches!(b, reader::Block::Table { .. })).unwrap();
    assert_eq!(routes_at(&blocks, table), ["DELETE /api/items/{id}"]);
    let under = blocks.iter().position(|b| matches!(b, reader::Block::Heading { text, .. } if text == "Response")).unwrap();
    let mut blocks = blocks;
    blocks.insert(under + 2, reader::Block::Text { at: 0, text: "Answers GET /api/items/{id}/full.".into(), code: false });
    assert_eq!(routes_at(&blocks, table + 1), ["GET /api/items/{id}/full", "DELETE /api/items/{id}"]);
}

#[test]
fn a_tie_goes_to_the_smaller_code_set_then_the_name_whatever_the_order_the_sets_were_read_in() {
    let group = Group {
        source: "d".into(),
        shown: "d: column".into(),
        fact: json!({}),
        values: vec!["Yes".into(), "No".into()],
        table: None,
        routes: vec![],
        bound: Bound::default(),
    };
    let set = |target: &str, values: &[&str]| Set {
        link: Link { facts: String::new(), kind: "consts".into(), target: target.into(), column: String::new(), by: "value".into() },
        codes: values.iter().map(|v| Code { value: v.to_string(), ..Default::default() }).collect(),
        others: HashMap::new(),
    };
    let winner = |sets: Vec<Set>| {
        let found = pairs(std::slice::from_ref(&group), &sets);
        found.iter().map(|(_, s, _)| sets[*s].link.target.clone()).collect::<Vec<_>>()
    };
    let (big, small, twin) = (|| set("Demo.Big", &["Yes", "No", "Maybe"]), || set("Demo.Small", &["Yes", "No"]), || set("Demo.Also", &["Yes", "No"]));
    // EACH SET KEEPS ITS BEST GROUP TOO, so every set is listed - the order says who won.
    assert_eq!(winner(vec![big(), small(), twin()]), ["Demo.Also", "Demo.Small", "Demo.Big"]);
    assert_eq!(winner(vec![twin(), small(), big()]), ["Demo.Also", "Demo.Small", "Demo.Big"]);
}
