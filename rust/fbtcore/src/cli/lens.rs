//! `--map-query <db> ...` - THE LENSES OVER THE SQLITE MAP, carried by the exe so a consumer's tree keeps
//! exactly its two files. Everything after the database is read into ONE question for `query.rs`, so this
//! file never grows a lens's own logic.

use super::Out;
use serde_json::json;
use std::path::Path;

/// The lenses that take an argument. `--decorators`, `--dead`, `--defaults`, `--unused-imports` and `--magic` take one
/// OR stand alone, so what follows is only theirs when it is not the next flag.
const VALUED: [&str; 10] = ["schema", "find", "id", "file", "cat", "text", "grep", "reads", "key", "sql"];
const OPTIONAL: [&str; 5] = ["decorators", "dead", "defaults", "unused-imports", "magic"];

pub fn run(db: &str, arguments: &[String], out: &mut Out) -> i64 {
    // A DATABASE THAT IS NOT THERE IS EXIT 2, not an empty answer.
    if !Path::new(db).is_file() {
        out.line(format!("no database at {db} - build it with --map --map-sqlite"));
        return 2;
    }
    let ask = match serde_json::from_value(read(arguments)) {
        Ok(ask) => ask,
        Err(e) => {
            out.error(format!("structuregate: the query failed — the question could not be read - {e}"));
            return 2;
        }
    };
    match crate::query::render(Path::new(db), &ask) {
        Err(e) => {
            out.error(format!("structuregate: the query failed — {e:#}"));
            2
        }
        Ok(answer) => {
            out.write(answer.text);
            // A LENS THAT COULD NOT ASK ITS QUESTION IS EXIT 2, whatever it managed to print first.
            if answer.failed { 2 } else { 0 }
        }
    }
}

/// The arguments as one question.
fn read(arguments: &[String]) -> serde_json::Value {
    let (mut lens, mut arg, mut table, mut lines) = (String::new(), String::new(), String::new(), String::new());
    let (mut limit, mut width) = (50_i64, 70_i64);
    let mut i = 0;
    while i < arguments.len() {
        let Some(bare) = arguments[i].strip_prefix("--") else {
            i += 1;
            continue;
        };
        let next = arguments.get(i + 1);
        match bare {
            "table" => {
                table = next.cloned().unwrap_or_default();
                i += 1;
            }
            "lines" => {
                lines = next.cloned().unwrap_or_default();
                i += 1;
            }
            "limit" | "width" => {
                if let Some(n) = next.and_then(|n| n.trim().parse::<i32>().ok()) {
                    if bare == "limit" { limit = n.into() } else { width = n.into() }
                    i += 1;
                }
            }
            "tables" => lens = "tables".into(),
            _ if OPTIONAL.contains(&bare) => {
                lens = bare.into();
                if let Some(value) = next.filter(|n| !n.starts_with("--")) {
                    arg = value.clone();
                    i += 1;
                }
            }
            _ if VALUED.contains(&bare) => {
                lens = bare.into();
                arg = next.cloned().unwrap_or_default();
                i += 1;
            }
            _ => {}
        }
        i += 1;
    }
    json!({ "lens": lens, "arg": arg, "table": table, "lines": lines, "limit": limit, "width": width })
}
