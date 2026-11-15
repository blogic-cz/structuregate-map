//! `rsasync.rs`: each blocking call by the path its `use` lines give it, the async look-alikes that are not, a sync
//! closure inside async code, and the waiver.
use super::*;

fn lines_of(found: &[String]) -> Vec<String> {
    found.iter().map(|f| f.split(": ").next().unwrap_or("").to_string()).collect()
}

const BLOCKING: &str = r#"use std::fs;
use std::thread::sleep as nap;
use std::process::Command;
use std::sync::mpsc::*;

async fn every_kind(rx: Receiver<u8>, handle: std::thread::JoinHandle<()>) {
    let _ = fs::read_to_string("a");
    nap(std::time::Duration::from_millis(1));
    let _ = std::net::TcpStream::connect("x:1");
    let _ = Command::new("git").arg("status").output();
    let _ = rx.recv();
    let _ = handle.join();
    let _ = futures::executor::block_on(async {});
    let _ = reqwest::blocking::get("u");
}
"#;

#[test]
fn every_blocking_call_in_async_code_is_named_on_its_line() {
    let found = inspect("src/a.rs", BLOCKING);
    assert_eq!(lines_of(&found), ["src/a.rs:7", "src/a.rs:8", "src/a.rs:9", "src/a.rs:10", "src/a.rs:11", "src/a.rs:12", "src/a.rs:13", "src/a.rs:14"], "{found:#?}");
    assert!(found[0].starts_with("src/a.rs:7: std::fs inside an async body blocks on the disk — use tokio::fs (or spawn_blocking) (`let _ = fs::read_to_string(\"a\");`)"), "{}", found[0]);
}

const QUIET: &str = r#"use tokio::fs;
use std::sync::Mutex;

fn sync_is_free() {
    let _ = std::fs::read("a");
    std::thread::sleep(std::time::Duration::from_millis(1));
}

async fn async_twins(m: Mutex<u8>, mut rx: tokio::sync::mpsc::Receiver<u8>, words: Vec<String>) {
    let _ = fs::read("a").await;
    let _ = rx.recv().await;
    let _ = tokio::process::Command::new("git").output().await;
    let _ = m.lock();
    let _ = words.join(",");
    let _ = tokio::task::spawn_blocking(|| std::fs::read("a")).await;
    fn helper() { let _ = std::fs::read("a"); }
    let _ = std::fs::read("b"); // async-ok: a config read once at start, measured
    // async-ok: the same, the comment above it
    let _ = std::fs::read("c");
}
"#;

#[test]
fn async_twins_sync_code_a_sync_closure_and_a_waived_line_are_quiet() {
    assert_eq!(inspect("q.rs", QUIET), Vec::<String>::new());
}

#[test]
fn an_async_block_and_an_async_method_count_and_a_file_that_does_not_parse_has_no_finding() {
    let text = "struct S;\nimpl S {\n    async fn m(&self) { let _ = std::fs::read(\"a\"); }\n    fn n(&self) { let _ = async { std::fs::read(\"b\") }; }\n}\n";
    assert_eq!(lines_of(&inspect("m.rs", text)), ["m.rs:3", "m.rs:4"]);
    assert!(inspect("broken.rs", "async fn x( {").is_empty());
}

#[test]
fn a_bare_name_resolves_through_a_glob_unless_the_file_declares_it() {
    let glob = "use std::thread::*;\nasync fn a() { sleep(std::time::Duration::from_millis(1)); }\n";
    assert_eq!(lines_of(&inspect("g.rs", glob)), ["g.rs:2"]);
    let shadowed = "use std::fs::*;\nfn read(_: &str) {}\nasync fn b() { read(\"x\"); }\n";
    assert!(inspect("s.rs", shadowed).is_empty());
}
