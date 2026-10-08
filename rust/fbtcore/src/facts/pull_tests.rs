//! `pull.rs` against a fake Drive API on a local port: one thread, one response per request, every request kept.
use super::*;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

/// A server answering `(status, body)` per request in order, and the request lines it was sent.
fn serve(answers: Vec<(u16, &'static str)>) -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let asked = Arc::new(Mutex::new(Vec::new()));
    let kept = Arc::clone(&asked);
    std::thread::spawn(move || {
        for (status, body) in answers {
            let Ok((mut stream, _)) = listener.accept() else { return };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut first = String::new();
            reader.read_line(&mut first).unwrap();
            let mut auth = String::new();
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line.to_lowercase().starts_with("authorization:") {
                    auth = line.trim().to_string();
                }
                if line.trim().is_empty() {
                    break;
                }
            }
            kept.lock().unwrap().push(format!("{} {auth}", first.trim()));
            let reply = format!("HTTP/1.1 {status} X\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
            stream.write_all(reply.as_bytes()).unwrap();
        }
    });
    (url, asked)
}

fn source(dir: &Path) -> Source {
    let _ = std::fs::remove_dir_all(dir);
    Source { name: "guide".into(), kind: "google-doc".into(), id: "DOC".into(), path: dir.join("docs").join("guide.md"), shown: "docs/guide.md".into() }
}

/// A folder of the test's own, removed when the test ends - every `cargo test` left three behind.
struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn scratch(name: &str) -> Scratch {
    let dir = std::env::temp_dir().join(format!("fbt-pull-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    Scratch(dir)
}

#[test]
fn a_new_version_is_saved_and_an_unchanged_one_is_not_downloaded() {
    let dir = scratch("versions");
    let s = source(&dir.0);
    let (url, asked) = serve(vec![(200, r#"{"version": "17", "name": "Guide 1"}"#), (200, "# Codes\n"), (200, r#"{"version": "17"}"#)]);
    assert!(pull(&s, "T0K", &url, &url).unwrap().starts_with("saved version 17"));
    assert_eq!(std::fs::read_to_string(&s.path).unwrap(), "# Codes\n");
    let meta: Value = serde_json::from_str(&std::fs::read_to_string(meta_path(&s.path)).unwrap()).unwrap();
    assert_eq!(meta["version"], "17");
    assert!(pull(&s, "T0K", &url, &url).unwrap().starts_with("unchanged (version 17)"));
    let asked = asked.lock().unwrap().clone();
    assert_eq!(asked.len(), 3, "{asked:?}");
    assert!(asked[0].starts_with("GET /drive/v3/files/DOC?fields=version") && asked[0].ends_with("Bearer T0K"), "{asked:?}");
    assert!(asked[1].starts_with("GET /drive/v3/files/DOC/export?mimeType=text/markdown"), "{asked:?}");
}

#[test]
fn a_failed_download_keeps_the_last_snapshot_whole() {
    let dir = scratch("failed");
    let s = source(&dir.0);
    std::fs::create_dir_all(s.path.parent().unwrap()).unwrap();
    std::fs::write(&s.path, "the old one").unwrap();
    std::fs::write(meta_path(&s.path), r#"{"version": "3"}"#).unwrap();
    let (url, _) = serve(vec![(200, r#"{"version": "4"}"#), (500, "boom")]);
    assert_eq!(pull(&s, "T", &url, &url).unwrap_err(), "HTTP 500");
    assert_eq!(std::fs::read_to_string(&s.path).unwrap(), "the old one");
    let (url, _) = serve(vec![(403, "")]);
    assert!(pull(&s, "T", &url, &url).unwrap_err().starts_with(REFUSED));
}

#[test]
fn a_json_snapshot_is_the_document_as_the_docs_api_keeps_it() {
    let dir = scratch("json");
    let mut s = source(&dir.0);
    s.path = s.path.with_extension("json");
    let (url, asked) = serve(vec![(200, r#"{"version": "5"}"#), (200, r#"{"body": {"content": []}}"#)]);
    assert!(pull(&s, "T", &url, &url).unwrap().starts_with("saved version 5"));
    assert_eq!(std::fs::read_to_string(&s.path).unwrap(), r#"{"body": {"content": []}}"#);
    let asked = asked.lock().unwrap().clone();
    assert!(asked[1].starts_with("GET /v1/documents/DOC?includeTabsContent=true"), "{asked:?}");
}

#[test]
fn a_refused_token_says_which_cause_its_account_and_scopes_point_at() {
    let readonly = Holder { email: "a@example.com".into(), scopes: vec!["https://www.googleapis.com/auth/drive.readonly".into()] };
    assert!(refused(REFUSED, Some(&readonly)).contains("its account a@example.com may not read the document"));
    let none = Holder { email: "a@example.com".into(), scopes: vec!["openid".into()] };
    assert!(refused(REFUSED, Some(&none)).contains("it carries no Drive scope (it has: openid)"));
    assert!(refused(REFUSED, None).contains("either it carries no Drive scope, or its account may not read"));
    let (url, asked) = serve(vec![(200, r#"{"email": "a@example.com", "scope": "openid https://www.googleapis.com/auth/drive"}"#)]);
    let who = holder_of(&url, "T0K").unwrap();
    assert_eq!((who.email.as_str(), who.scopes.len()), ("a@example.com", 2));
    assert!(asked.lock().unwrap()[0].starts_with("GET /oauth2/v3/tokeninfo?access_token=T0K"));
}
