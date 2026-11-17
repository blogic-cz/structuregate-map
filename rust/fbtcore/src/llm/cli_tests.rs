//! `cli.rs`: the CLI's answer read by its object and its exit code, and (unix) a fake CLI that keeps what it
//! was handed - the question on stdin, never in its arguments.
use super::*;

#[test]
fn an_answer_is_its_data_and_a_failure_says_which_by_its_exit_code() {
    let log = b"INFO starting\nWARN slow\n";
    assert_eq!(answer(Some(0), b"{\"ok\": true, \"data\": {\"accept\": [1]}}\n", log), Ok(Answer { data: json!({"accept": [1]}), tool_calls: None }));
    let counted = |out: &[u8]| answer(Some(0), out, b"").unwrap().tool_calls;
    assert_eq!(counted(br#"{"ok": true, "data": {}, "tool_calls": 3}"#), Some(3));
    assert_eq!(counted(br#"{"ok": true, "data": {}, "usage": {"tool_calls": 2}}"#), Some(2));
    let invalid = answer(Some(3), br#"{"ok": false, "error_type": "BadReply", "message": "not json", "errors": ["at /accept"]}"#, b"");
    assert_eq!(invalid, Err(Failed::Invalid("BadReply: not json (at /accept)".into())));
    let after = answer(Some(3), br#"{"ok": false, "error_type": "BadReply", "message": "not json", "tool_calls": 8}"#, b"");
    assert_eq!(after, Err(Failed::Invalid("BadReply: not json [after 8 tool call(s)]".into())));
    assert!(matches!(answer(Some(2), br#"{"ok": false, "error_type": "NoModel", "message": "quota"}"#, b""), Err(Failed::Unavailable(_))));
    assert!(matches!(answer(Some(4), b"{\"ok\": false}", b""), Err(Failed::Timeout(_))));
    assert!(matches!(answer(Some(5), b"{\"ok\": false}", b""), Err(Failed::Rejected(_))));
    assert_eq!(answer(Some(1), b"Traceback", b"a\nb\nc\nd\n"), Err(Failed::Other("no answer object (exit 1): b | c | d".into())));
}

#[cfg(unix)]
#[test]
fn the_question_goes_on_stdin_and_the_arguments_as_configured() {
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join(format!("fbt-llm-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let exe = dir.join("answer");
    std::fs::write(&exe, format!("#!/bin/sh\necho \"$@\" > '{0}/args'\ncat > '{0}/request'\necho 'INFO done' >&2\necho '{{\"ok\": true, \"data\": {{\"n\": 7}}}}'\n", dir.display())).unwrap();
    std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
    let cli = Cli { exe, args: vec!["judge".into(), "--config".into(), "my settings.json".into()] };
    let big = "x".repeat(300_000);
    let mut ask = Ask { system: "judge".into(), message: format!("the document says {big}"), schema: json!({"type": "object"}), tools: vec![] };
    assert_eq!(cli.ask_json(&ask).map(|a| a.data), Ok(json!({"n": 7})));
    let args = std::fs::read_to_string(dir.join("args")).unwrap();
    assert_eq!(args.trim(), "judge --config my settings.json");
    let request: Value = serde_json::from_str(&std::fs::read_to_string(dir.join("request")).unwrap()).unwrap();
    assert_eq!((request["system_prompt"].as_str(), request["want"].as_str()), (Some("judge"), Some("object")));
    assert_eq!(request["user_message"].as_str().map(str::len), Some(ask.message.len()));
    assert!(request.get("tools").is_none());
    ask.tools = vec![json!({"type": "sqlite_query", "path": "/m.sqlite", "name": "map_query"})];
    cli.ask_json(&ask).unwrap();
    let request: Value = serde_json::from_str(&std::fs::read_to_string(dir.join("request")).unwrap()).unwrap();
    assert_eq!(request["tools"], json!([{"type": "sqlite_query", "path": "/m.sqlite", "name": "map_query"}]));
    let missing = Cli { exe: dir.join("nothing-here"), args: vec![] };
    assert!(matches!(missing.ask_json(&ask), Err(Failed::Unavailable(_))));
    let _ = std::fs::remove_dir_all(&dir);
}
