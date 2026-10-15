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

/// LAB HARNESS (not committed): SG_LAB_DB=<map> SG_LAB_OUT=<dir> SG_LAB_WS=<workspace> - the closure and its
/// partial replace + write over a copy of a stored map.
#[test]
#[ignore]
fn lab() {
    let (Ok(src), Ok(out), Ok(ws)) = (std::env::var("SG_LAB_DB"), std::env::var("SG_LAB_OUT"), std::env::var("SG_LAB_WS")) else { return };
    let work = Path::new(&out).join("work.sqlite");
    std::fs::create_dir_all(&out).unwrap();
    std::fs::copy(&src, &work).unwrap();
    if let Ok(child) = std::env::var("SG_LAB_MICRO") {
        let db = Connection::open(&work).unwrap();
        let owners: Vec<rusqlite::types::Value> = {
            let mut s = db.prepare(&format!("SELECT DISTINCT owner FROM \"{child}\" LIMIT 50000")).unwrap();
            s.query_map([], |r| r.get(0)).unwrap().map(|r| r.unwrap()).collect()
        };
        let keys: Vec<String> = owners.iter().map(|o| match o { rusqlite::types::Value::Text(t) => t.clone(), rusqlite::types::Value::Integer(i) => i.to_string(), _ => String::new() }).collect();
        let source = crate::rows::cells::Source::new(&db, &child).unwrap();
        let t = std::time::Instant::now();
        let mut nodes = 0usize;
        for k in &keys { if source.get(k).is_some() { nodes += 1; } }
        eprintln!("LAB micro get {} owners {} ms", nodes, t.elapsed().as_millis());
        let c2 = Connection::open(&work).unwrap();
        let t = std::time::Instant::now();
        let mut rows = 0usize;
        let mut s = c2.prepare(&format!("SELECT n, parent, role, node, value FROM \"{child}\" WHERE owner = ?1 ORDER BY n")).unwrap();
        for o in &owners { let mut q = s.query([o]).unwrap(); while let Some(_r) = q.next().unwrap() { rows += 1; } }
        eprintln!("LAB micro query-only {} rows {} ms", rows, t.elapsed().as_millis());
        let t = std::time::Instant::now();
        let mut s = c2.prepare(&format!("SELECT owner, n, parent, role, node, value FROM \"{child}\" ORDER BY owner, n")).unwrap();
        let mut q = s.query([]).unwrap(); let mut all = 0usize; while let Some(_r) = q.next().unwrap() { all += 1; }
        eprintln!("LAB micro scan-all {} rows {} ms", all, t.elapsed().as_millis());
        return;
    }
    let t = std::time::Instant::now();
    let emitted = {
        let db = Connection::open(&work).unwrap();
        let mut held = tsstore::Store::from_db(&db, LANG).unwrap();
        let (stats, _) = keyreach::build_closure(&mut held, &[], "", Path::new(&ws), &mut |_| {});
        eprintln!("closure: {} paths, {} keys", stats.path_rows, stats.keys);
        let mut out = Map::new();
        for (name, rows) in held.emitted {
            out.insert(name, Value::Array(rows.into_iter().map(|r| Value::Object(r.into_map())).collect()));
        }
        out
    };
    eprintln!("LAB derive the closure {} ms", t.elapsed().as_millis());
    eprintln!("{}", crate::rows::cells::probe::report());
    if let Ok(tag) = std::env::var("SG_LAB_TAG") {
        let sorted: BTreeMap<&String, &Value> = emitted.iter().collect();
        std::fs::write(Path::new(&out).join(format!("emitted-{tag}.json")), serde_json::to_string(&sorted).unwrap()).unwrap();
    }
    if std::env::var("SG_LAB_ONLY_CLOSURE").is_ok() {
        return;
    }
    let t = std::time::Instant::now();
    {
        let db = Connection::open(&work).unwrap();
        let whole: Vec<String> = CLOSURE_TABLES.iter().map(|t| t.to_string()).collect();
        replace_affected(&db, &[], &whole).unwrap();
    }
    eprintln!("LAB replace {} ms", t.elapsed().as_millis());
    let t = std::time::Instant::now();
    let gone = BTreeSet::new();
    let applied = store::write(&work, &WriteRequest {
        tables: &emitted, stream: None, texts: &store::Texts::Held(&BTreeMap::new()), root: &ws, shas: &Map::new(), gone: &gone,
        full: false, counters: &Map::new(), lang: LANG, half: Some(LANG), setup: None, extra: &BTreeMap::new(), keep_existing: true, counts: true,
    }).unwrap();
    eprintln!("LAB write the closure {} ms {:?}", t.elapsed().as_millis(), applied.steps);
}
