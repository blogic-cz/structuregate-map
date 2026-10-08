//! The tests of `store.rs`, apart from it only so that file stays under the line ceiling.
//! It is still `store::tests`, so `super::*` is the store.
use super::*;

fn tmp(name: &str) -> crate::rows::TempDb {
    crate::rows::TempDb::new("fts", name)
}

fn request<'a>(
    texts: &'a Texts<'a>,
    tables: &'a Map<String, Value>,
    shas: &'a Map<String, Value>,
    empty: &'a Map<String, Value>,
    gone: &'a BTreeSet<String>,
    extra: &'a BTreeMap<String, String>,
    keep_existing: bool,
) -> WriteRequest<'a> {
    WriteRequest {
        tables,
        texts,
        root: ".",
        shas,
        gone,
        full: false,
        counters: empty,
        lang: "typescript",
        half: Some("typescript"),
        setup: None,
        extra,
        keep_existing,
        counts: true,
    }
}

fn texts_of(paths: &[&str]) -> BTreeMap<String, String> {
    paths
        .iter()
        .map(|p| ((*p).to_string(), format!("the source of {p}")))
        .collect()
}

fn held(db: &Path, path: &str) -> i64 {
    let held = Connection::open(db).unwrap();
    held.query_row("SELECT count(*) FROM file_text WHERE path = ?1", [path], |r| r.get(0))
        .unwrap()
}

#[test]
fn a_file_read_twice_is_stored_once() {
    // `file_text` IS AN FTS5 TABLE AND HAS NO KEY TO CONFLICT ON. A second write of the same
    // path appends unless the path is cleared first, and the half that replaces its rows
    // whole never reaches the per-file drop that would have done it - so every file a partial
    // run re-read was stored twice, and a search returned each hit twice.
    let db = tmp("twice");
    let (tables, shas, empty) = (Map::new(), Map::new(), Map::new());
    let (gone, extra) = (BTreeSet::new(), BTreeMap::new());
    let texts = texts_of(&["src/a.ts", "src/b.ts"]);
    write(&db, &request(&Texts::Held(&texts), &tables, &shas, &empty, &gone, &extra, true)).unwrap();
    write(&db, &request(&Texts::Held(&texts), &tables, &shas, &empty, &gone, &extra, true)).unwrap();
    assert_eq!(held(&db, "src/a.ts"), 1, "one text per path, however often it is read");
    assert_eq!(held(&db, "src/b.ts"), 1);
}

#[test]
fn a_file_nobody_re_read_keeps_the_text_it_had() {
    // The other side of the same rule: a PARTIAL run names a few files, and clearing more
    // than those would leave the rest of the map searchable only by its rows.
    let db = tmp("kept");
    let (tables, shas, empty) = (Map::new(), Map::new(), Map::new());
    let (gone, extra) = (BTreeSet::new(), BTreeMap::new());
    write(
        &db,
        &request(&Texts::Held(&texts_of(&["src/a.ts", "src/b.ts"])), &tables, &shas, &empty, &gone, &extra, true),
    )
    .unwrap();
    write(
        &db,
        &request(&Texts::Held(&texts_of(&["src/b.ts"])), &tables, &shas, &empty, &gone, &extra, true),
    )
    .unwrap();
    assert_eq!(held(&db, "src/a.ts"), 1, "it was not re-read, so it was not touched");
    assert_eq!(held(&db, "src/b.ts"), 1);
}

#[test]
fn nothing_is_cleared_when_there_is_nothing_to_clear() {
    // THE SKIP IS THE WHOLE POINT. `path` cannot be indexed on a virtual table, so each
    // clear reads every document; a full run has an empty table and must not pay for it.
    let db = tmp("fresh");
    let (tables, shas, empty) = (Map::new(), Map::new(), Map::new());
    let (gone, extra) = (BTreeSet::new(), BTreeMap::new());
    let texts = texts_of(&["src/a.ts"]);
    let applied =
        write(&db, &request(&Texts::Held(&texts), &tables, &shas, &empty, &gone, &extra, true)).unwrap();
    assert!(applied.fts, "the table is there");
    assert_eq!(held(&db, "src/a.ts"), 1);
}

#[test]
fn more_files_than_one_statement_can_name_are_still_all_replaced() {
    // The clear is BATCHED under SQLITE_MAX_VARIABLE_NUMBER. A batch boundary that dropped
    // the tail would leave the last files of a big re-read stored twice.
    let db = tmp("batched");
    let (tables, shas, empty) = (Map::new(), Map::new(), Map::new());
    let (gone, extra) = (BTreeSet::new(), BTreeMap::new());
    let names: Vec<String> = (0..2100).map(|i| format!("src/f{i}.ts")).collect();
    let texts: BTreeMap<String, String> =
        names.iter().map(|n| (n.clone(), format!("the source of {n}"))).collect();
    write(&db, &request(&Texts::Held(&texts), &tables, &shas, &empty, &gone, &extra, true)).unwrap();
    write(&db, &request(&Texts::Held(&texts), &tables, &shas, &empty, &gone, &extra, true)).unwrap();
    for probe in ["src/f0.ts", "src/f899.ts", "src/f900.ts", "src/f1800.ts", "src/f2099.ts"] {
        assert_eq!(held(&db, probe), 1, "{probe} is stored once");
    }
}
#[test]
fn a_named_file_is_read_by_the_store_rather_than_handed_to_it() {
    // THE WHOLE POINT OF `OnDisk`. The half names its files; the text never passes through
    // memory in bulk. The rows must come out the same as if it had been handed over.
    let db = tmp("ondisk");
    let root = std::env::temp_dir().join(format!("fbt-src-{}", std::process::id()));
    let _ = std::fs::create_dir_all(root.join("src"));
    std::fs::write(root.join("src/a.ts"), "export const a = 1;").unwrap();
    std::fs::write(root.join("src/b.ts"), "export const b = 2;").unwrap();

    let (tables, shas, empty) = (Map::new(), Map::new(), Map::new());
    let (gone, extra) = (BTreeSet::new(), BTreeMap::new());
    let rels: BTreeSet<String> =
        ["src/a.ts", "src/b.ts"].iter().map(|p| (*p).to_string()).collect();
    let texts = Texts::OnDisk { root: &root, rels: &rels };
    write(&db, &request(&texts, &tables, &shas, &empty, &gone, &extra, true)).unwrap();

    let open = Connection::open(&db).unwrap();
    let got: String = open
        .query_row("SELECT content FROM file_text WHERE path = 'src/a.ts'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(got, "export const a = 1;");
    assert_eq!(held(&db, "src/b.ts"), 1);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_named_file_that_cannot_be_read_is_skipped_rather_than_fatal() {
    // The half named it and it has since gone. One missing source must not lose the map.
    let db = tmp("missing");
    let root = std::env::temp_dir().join(format!("fbt-gone-{}", std::process::id()));
    let _ = std::fs::create_dir_all(root.join("src"));
    std::fs::write(root.join("src/here.ts"), "export const here = 1;").unwrap();

    let (tables, shas, empty) = (Map::new(), Map::new(), Map::new());
    let (gone, extra) = (BTreeSet::new(), BTreeMap::new());
    let rels: BTreeSet<String> =
        ["src/here.ts", "src/vanished.ts"].iter().map(|p| (*p).to_string()).collect();
    let texts = Texts::OnDisk { root: &root, rels: &rels };
    let applied =
        write(&db, &request(&texts, &tables, &shas, &empty, &gone, &extra, true)).unwrap();
    assert_eq!(applied.written.get("file_text"), Some(&1), "the one that is there");
    assert_eq!(held(&db, "src/vanished.ts"), 0);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_drop_by_one_language_leaves_another_languages_rows_for_the_same_path() {
    // The plain TypeScript half hands `src/main.ts` over to the Angular one: the Angular
    // half writes its rows, then the plain half drops its own. By path alone that took the
    // Angular rows, its `files` record and the shared source text with it.
    let db = tmp("langs");
    let open = Connection::open(&db).unwrap();
    open.execute_batch(
        "CREATE TABLE files (id, path, lang); CREATE TABLE calls (id, file);
         CREATE VIRTUAL TABLE file_text USING fts5(path, content);
         INSERT INTO files VALUES ('f:1', 'src/main.ts', 'typescript'), ('f:2', 'src/main.ts', 'ts');
         INSERT INTO calls VALUES ('call:1', 'f:1'), ('call:2', 'f:2');
         INSERT INTO file_text VALUES ('src/main.ts', 'export const x = 1;');",
    )
    .unwrap();
    let rels: BTreeSet<String> = ["src/main.ts".to_string()].into_iter().collect();
    drop_files(&open, &rels, "ts").unwrap();
    let count = |sql: &str| -> i64 { open.query_row(sql, [], |r| r.get(0)).unwrap() };
    assert_eq!(count("SELECT count(*) FROM files WHERE lang = 'typescript'"), 1, "the other half's file stays");
    assert_eq!(count("SELECT count(*) FROM files WHERE lang = 'ts'"), 0, "this half's file goes");
    assert_eq!(count("SELECT count(*) FROM calls"), 1, "only this half's row went");
    assert_eq!(held(&db, "src/main.ts"), 1, "the text stays while a half still records the file");
    drop_files(&open, &rels, "typescript").unwrap();
    assert_eq!(held(&db, "src/main.ts"), 0, "and goes with the last one");
}
