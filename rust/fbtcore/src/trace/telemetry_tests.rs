use super::*;
use crate::trace::Span;

fn span(name: &str, parent: Option<&str>, start_ns: u128, end_ns: u128, attributes: Vec<(String, Value)>) -> Span {
    Span { id: name.into(), parent: parent.map(String::from), name: name.into(), start_ns, end_ns, attributes }
}

fn run_spans() -> Vec<Span> {
    vec![
        span("structuregate", None, 0, 9_000_000, vec![("structuregate.mode".into(), json!("deep"))]),
        span("walk", Some("structuregate"), 0, 2_000_000, Vec::new()),
        span("walk", Some("structuregate"), 0, 4_000_000, Vec::new()),
        span("csharp project: Demo.Orders", Some("structuregate"), 0, 1_000_000, Vec::new()),
        span("file: csharp", Some("structuregate"), 0, 1_000_000, vec![("code.filepath".into(), json!("src/Demo.cs"))]),
    ]
}

/// The kept panic is the process's: the cases that read it take turns.
static CRASHES: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn parsed(text: &str) -> Vec<Value> {
    text.lines().map(|l| serde_json::from_str(l).unwrap()).collect()
}

#[test]
fn a_word_holding_a_path_is_scrubbed_and_the_rest_kept() {
    assert_eq!(scrub("cannot read C:\\repo\\Foo.cs: denied"), "cannot read <path> denied");
    assert_eq!(scrub("no /home/me/a.py here"), "no <path> here");
}

#[test]
fn a_stage_named_after_the_tree_counts_under_its_prefix_and_files_are_no_stage() {
    let spans = run_spans();
    let names: Vec<Option<String>> = spans.iter().map(public_name).collect();
    assert_eq!(names, vec![None, Some("walk".into()), Some("walk".into()), Some("csharp project".into()), None]);
}

#[test]
fn stages_of_one_name_are_one_series_and_nothing_of_the_tree_is_written() {
    let spans = run_spans();
    let resource = vec![("service.version".to_string(), json!("v9.9.9")), ("host.name".to_string(), json!("secret-host"))];
    let run = Finished::new("t1", &resource, &spans, 0, 9_000_000);
    let text = records(&run, "id-1");
    for secret in ["Demo.Orders", "Demo.cs", "secret-host"] {
        assert!(!text.contains(secret), "{secret} left the machine: {text}");
    }
    let lines = parsed(&text);
    let kinds: Vec<String> = lines.iter().map(|r| format!("{}/{}", r["k"].as_str().unwrap(), r["n"].as_str().unwrap())).collect();
    assert_eq!(kinds, ["session/start", "machine/machine", "event/run", "metric/run", "session/end"]);
    let walk = lines[3]["d"]["stats"].as_array().unwrap().iter().find(|s| s["name"] == "stage.walk").unwrap().clone();
    assert_eq!((walk["n"].as_u64(), walk["sum"].as_f64(), walk["min"].as_f64(), walk["max"].as_f64()), (Some(2), Some(6.0), Some(2.0), Some(4.0)));
    assert!(lines.iter().all(|r| r["v"] == "v9.9.9" && r["i"] == "id-1" && r["s"] == "t1"));
}

#[test]
fn a_panic_is_a_crash_record_without_this_sinks_frames() {
    let _turn = CRASHES.lock().unwrap_or_else(|p| p.into_inner());
    catch_panics();
    let caught = std::panic::catch_unwind(|| panic!("cannot open /home/me/secret.cs now"));
    assert!(caught.is_err());
    escaped();
    let spans = run_spans();
    let run = Finished::new("t2", &[], &spans, 2, 9_000_000);
    let lines = parsed(&records(&run, "id"));
    let crash = lines.iter().find(|r| r["k"] == "crash").expect("no crash record");
    assert_eq!(crash["n"], "panic");
    assert_eq!(crash["d"]["message"], "cannot open <path> now");
    assert!(crash["d"]["location"].as_str().unwrap().contains("telemetry_tests.rs"));
    assert!(!crash["d"]["backtrace"].as_str().unwrap().contains(OWN_FRAMES), "the hook's own frames were kept");
    assert!(crash["d"]["uptime_s"].as_f64().is_some());
}

#[test]
fn an_exit_2_without_a_panic_is_a_crash_named_after_the_mode() {
    let _turn = CRASHES.lock().unwrap_or_else(|p| p.into_inner());
    catch_panics();
    let _ = std::panic::catch_unwind(|| panic!("a parser's panic, caught and answered"));
    let spans = run_spans();
    let run = Finished::new("t3", &[], &spans, 2, 9_000_000);
    let lines = parsed(&records(&run, "id"));
    let crash = lines.iter().find(|r| r["k"] == "crash").expect("no crash record");
    assert_eq!((crash["n"].as_str(), crash["d"]["message"].as_str()), (Some("exit_2"), Some("a deep run exited 2")));
}

#[test]
fn the_install_id_is_a_version_4_uuid() {
    let id = random_uuid();
    let parts: Vec<&str> = id.split('-').collect();
    assert_eq!(parts.iter().map(|p| p.len()).collect::<Vec<_>>(), [8, 4, 4, 4, 12]);
    assert!(parts[2].starts_with('4'), "{id}");
    assert!(matches!(parts[3].as_bytes()[0], b'8' | b'9' | b'a' | b'b'), "{id}");
    assert_ne!(id, random_uuid());
}
