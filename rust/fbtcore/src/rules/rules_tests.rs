use super::*;

/// A C#-shaped map: one source file and one test file, the columns the rules read.
fn map() -> Connection {
    let db = Connection::open_in_memory().unwrap();
    db.execute_batch(
        "CREATE TABLE files (id INTEGER, path TEXT, lang TEXT);
         CREATE TABLE functions (file INTEGER, line INTEGER, end_line INTEGER, name TEXT, qualname TEXT, kind TEXT, type TEXT,
                                 is_async INTEGER, awaits INTEGER, body INTEGER, inherited INTEGER);
         CREATE TABLE calls (file INTEGER, line INTEGER, callee TEXT, type TEXT, dropped INTEGER);
         CREATE TABLE parameters (file INTEGER, line INTEGER, qualname TEXT, type TEXT);
         INSERT INTO files VALUES (1, 'src/Demo/Orders.cs', 'csharp'), (2, 'src/Demo.Tests/OrdersTests.cs', 'csharp'), (3, 'a.py', 'python');
         INSERT INTO functions VALUES
           (1, 10, 12, 'Load', 'Demo.Orders.Load', 'method', 'System.Threading.Tasks.Task<int>', 0, 0, 1, 0),
           (1, 20, 24, 'Save', 'Demo.Orders.Save', 'method', 'System.Threading.Tasks.Task', 1, 0, 1, 0),
           (1, 30, 34, 'Send', 'Demo.Orders.Send', 'method', 'System.Threading.Tasks.ValueTask', 1, 2, 1, 1),
           (1, 40, 44, 'All', 'Demo.Orders.All', 'method', 'System.Threading.Tasks.Task<System.Collections.Generic.IEnumerable<int>>', 1, 1, 1, 0),
           (1, 50, 54, 'Fine', 'Demo.Orders.Fine', 'method', 'System.Threading.Tasks.Task', 1, 1, 1, 0),
           (1, 60, 60, 'Shape', 'Demo.IOrders.Shape', 'method', 'System.Threading.Tasks.Task', 0, 0, 0, 0),
           (1, 70, 72, 'Count', 'Demo.Orders.Count', 'method', 'int', 0, 0, 1, 0),
           (2, 10, 14, 'Test', 'Demo.Tests.OrdersTests.Test', 'method', 'System.Threading.Tasks.Task', 0, 0, 1, 0);
         INSERT INTO parameters VALUES (1, 50, 'Demo.Orders.Fine', 'System.Threading.CancellationToken'),
           (1, 41, 'Demo.Orders.Fine', 'System.Threading.CancellationToken');
         INSERT INTO calls VALUES (1, 22, 'Save', 'System.Threading.Tasks.Task', 1), (1, 23, 'Load', 'System.Threading.Tasks.Task<int>', 0),
           (1, 31, 'ConfigureAwait', 'System.Runtime.CompilerServices.ConfiguredTaskAwaitable', 1), (1, 32, 'Count', 'int', 1),
           (2, 11, 'Save', 'System.Threading.Tasks.Task', 1);",
    )
    .unwrap();
    db
}

fn found(id: &str) -> Vec<String> {
    let rule = all().iter().find(|r| r.id == id).unwrap();
    findings(&map(), rule, "", &mut HashMap::new()).unwrap().into_iter().map(|f| format!("{}:{}", f.line, f.name)).collect()
}

#[test]
fn a_dropped_task_is_found_and_a_taken_one_or_a_dropped_int_is_not() {
    assert_eq!(found("cs-task-dropped"), ["22:Save", "31:ConfigureAwait"]);
}

#[test]
fn a_task_method_that_is_not_async_with_a_body_is_found() {
    // `Shape` has no body (an interface member); `Count` returns no task; the test file is left out.
    assert_eq!(found("cs-task-not-async"), ["10:Demo.Orders.Load"]);
}

#[test]
fn an_async_method_without_an_await_is_found() {
    assert_eq!(found("cs-async-no-await"), ["20:Demo.Orders.Save"]);
}

#[test]
fn a_task_method_without_its_own_cancellation_token_is_found() {
    // `Fine` takes one; `Send` is inherited; the token at line 41 is `Fine`'s qualname, not `All`'s, so `All` is found.
    assert_eq!(found("cs-task-no-cancellation"), ["10:Demo.Orders.Load", "20:Demo.Orders.Save", "40:Demo.Orders.All", "60:Demo.IOrders.Shape"]);
}

#[test]
fn a_task_of_ienumerable_is_found() {
    assert_eq!(found("cs-task-ienumerable"), ["40:Demo.Orders.All"]);
}

#[test]
fn a_map_without_the_columns_finds_nothing() {
    let db = Connection::open_in_memory().unwrap();
    db.execute_batch("CREATE TABLE files (id INTEGER, path TEXT, lang TEXT); CREATE TABLE functions (file INTEGER, line INTEGER);").unwrap();
    for rule in all() {
        assert!(findings(&db, rule, "", &mut HashMap::new()).unwrap().is_empty(), "{}", rule.id);
    }
}

#[test]
fn a_waiver_on_the_line_or_the_line_above_skips_it() {
    let lines: Vec<String> = ["a", "// async-ok: fire and forget by design", "Save();", "Save();"].map(String::from).to_vec();
    assert!(waived(Some(&lines), 3));
    assert!(waived(Some(&lines), 2));
    assert!(!waived(Some(&lines), 4));
    assert!(!waived(None, 3));
}

fn config(text: &str) -> Config {
    let dir = std::env::temp_dir().join(format!("fbt-rules-{}", blake3::hash(text.as_bytes()).to_hex()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(FILE_NAME), text).unwrap();
    read(None, &dir.to_string_lossy())
}

fn level(config: &Config, id: &str) -> Level {
    config.levels.iter().find(|(r, _)| r.id == id).unwrap().1
}

#[test]
fn every_rule_is_off_without_a_config_and_each_is_switched_alone() {
    let none = read(None, "no-such-dir");
    assert!(none.problems.is_empty() && none.levels.iter().all(|(_, l)| *l == Level::Off));
    let some = config(r#"{"rules": {"cs-task-dropped": "error", "cs-task-ienumerable": "warn", "cs-task-not-async": true, "cs-async-no-await": false}}"#);
    assert!(some.problems.is_empty(), "{:?}", some.problems);
    assert_eq!(level(&some, "cs-task-dropped"), Level::Error);
    assert_eq!(level(&some, "cs-task-ienumerable"), Level::Warn);
    assert_eq!(level(&some, "cs-task-not-async"), Level::Error);
    assert_eq!(level(&some, "cs-async-no-await"), Level::Off);
    assert_eq!(level(&some, "cs-task-no-cancellation"), Level::Off);
}

#[test]
fn an_unknown_rule_a_wrong_level_or_a_missing_file_is_a_problem() {
    let wrong = config(r#"{"rules": {"cs-no-such": "error", "cs-task-dropped": "loud"}}"#);
    assert_eq!(wrong.problems.len(), 2, "{:?}", wrong.problems);
    assert!(!read(Some("no-such-file.json"), ".").problems.is_empty());
}
