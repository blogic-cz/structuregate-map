//! THE DIAGNOSTICS (`--fbt-*`): each asks one piece of the store directly, prints what it says and changes
//! nothing else. The suites are black box over this CLI, so the thing that decides which files a half
//! re-parses - the scan - and every pass over the rows have to be answerable from the command line alone.

use super::options::{Options, Probe};
use super::Out;
use crate::rows::{self, calls};
use crate::session::{self, Request, Session};
use serde_json::{json, Map, Value};
use std::path::Path;

/// The probe the options ask for, in the order they have always been tried - or None.
pub fn run(o: &Options, out: &mut Out) -> Option<i64> {
    let p = &o.probe;
    let root = o.roots.first().cloned().unwrap_or_default();
    let feature = |db: &str, what: fn(&Path, &str, &[String], &str) -> anyhow::Result<String>| {
        what(Path::new(db), "typescript", &checks(&p.ts_checks), &p.ts_enum).map_err(|e| format!("{e:#}"))
    };
    let answer: Result<String, String> = if let Some(db) = &p.scan {
        return Some(scan(db, &root, p, out));
    } else if let Some(db) = &p.rows {
        rows_probe(db, &root, p)
    } else if let Some(db) = &p.ts_routes {
        feature(db, rows::ts::routes_json)
    } else if let Some(db) = &p.ts_litgates {
        feature(db, rows::ts::literal_gates_json)
    } else if let Some(db) = &p.ts_gates {
        feature(db, rows::ts::gate_features_json)
    } else if let Some(db) = &p.ts_values {
        rows::ts::gate_values_json(Path::new(db), "typescript").map_err(|e| format!("{e:#}"))
    } else if let Some(db) = &p.ts_paths {
        feature(db, rows::ts::render_paths_json)
    } else if let Some(db) = &p.ts_reach {
        rows::ts::key_reach_json(Path::new(db), "typescript", &checks(&p.ts_checks), &p.ts_enum, Path::new(&p.ts_root))
            .map_err(|e| format!("{e:#}"))
    } else if let Some(db) = &p.ts_state {
        calls::ts_state(Path::new(db), "typescript")
    } else if let Some(db) = &p.rows_fts {
        calls::search(Path::new(db), p.rows_fts_json)
    } else if let Some(db) = &p.map_atlas {
        match &p.map_atlas_dir {
            None => Err("--fbt-map-atlas needs --fbt-map-atlas-dir".into()),
            Some(dir) => calls::atlas(Path::new(db), dir),
        }
    } else if let Some(db) = &p.ts_apply {
        match &p.ts_payload {
            None => Err("--fbt-ts-apply needs --fbt-ts-payload".into()),
            Some(payload) => calls::ts_apply(Path::new(db), &root, payload),
        }
    } else {
        return None;
    };
    Some(print(answer, out))
}

fn print(answer: Result<String, String>, out: &mut Out) -> i64 {
    match answer {
        Ok(text) => {
            out.line(text);
            0
        }
        Err(why) => {
            out.error(format!("structuregate: {why}"));
            1
        }
    }
}

/// `--fbt-rows`: the state with no payload, one batch applied with one, the dependency graph a payload
/// implies, or the rows a partial run carries - as the protocol line the python end printed.
fn rows_probe(db: &str, root: &str, p: &Probe) -> Result<String, String> {
    let read = |path: &str| std::fs::read(path).map_err(|e| format!("{path} could not be read - {e}"));
    if let Some(deps) = &p.rows_deps {
        return rows::deps_json(&read(deps)?).map_err(|e| format!("{e:#}"));
    }
    if let Some(carry) = &p.rows_carry {
        let receipt: Value = serde_json::from_str(&calls::carry(Path::new(db), &p.rows_lang,
            p.rows_affected.as_deref().unwrap_or(""), carry)?).unwrap_or_default();
        let count = |name: &str| receipt[name].as_i64().unwrap_or(0);
        return Ok(format!("MAP-CARRY|{}|{}", count("rows"), count("tables")));
    }
    match &p.rows_file {
        None => calls::state(Path::new(db), &p.rows_lang, p.rows_whole),
        Some(file) => calls::apply(Path::new(db), root, &read(file)?),
    }
}

/// `--fbt-scan`: what moved, in the shape the halves receive, printed indented.
fn scan(db: &str, root: &str, p: &Probe, out: &mut Out) -> i64 {
    let request = Request { skip_paths: siblings(db, root), dir_hashes: p.dir_hashes, store: !p.no_store, ..Default::default() };
    let reply = Session::open(Path::new(db))
        .and_then(|mut session| session.scan(&session::absolute(root), &request))
        .map_err(|e| format!("{e:#}"))
        .and_then(|reply| serde_json::to_value(&reply).map_err(|e| e.to_string()));
    let reply = match reply {
        Ok(reply) => reply,
        Err(why) => {
            out.error(format!("structuregate: the scan failed — {why}"));
            return 1;
        }
    };
    let strings = |name: &str| -> Vec<Value> {
        reply[name].as_array().into_iter().flatten().filter(|v| v.is_string()).cloned().collect()
    };
    let mut shaped = Map::new();
    shaped.insert("unchanged".into(), json!(reply["unchanged"] == Value::Bool(true)));
    shaped.insert("first_scan".into(), json!(reply["first_scan"] == Value::Bool(true)));
    shaped.insert("root_hash".into(), text(&reply["root_hash"]));
    shaped.insert("previous_root_hash".into(), text(&reply["previous_root_hash"]));
    for name in ["entries", "read_from_disk", "elapsed_ms"] {
        shaped.insert(name.into(), json!(reply[name].as_i64().unwrap_or(0)));
    }
    let (stale, gone) = (strings("stale"), strings("gone"));
    shaped.insert("stale".into(), json!(stale.len()));
    shaped.insert("gone".into(), json!(gone.len()));
    shaped.insert("stale_paths".into(), Value::Array(stale));
    shaped.insert("gone_paths".into(), Value::Array(gone));
    let renamed: Vec<Value> = reply["renamed"].as_array().into_iter().flatten()
        .filter(|r| r["from"].is_string() && r["to"].is_string())
        .map(|r| json!({ "from": r["from"], "to": r["to"] }))
        .collect();
    shaped.insert("renamed".into(), Value::Array(renamed));
    let mut hashes: Vec<(&String, &Value)> = reply["dir_hashes"].as_object().into_iter().flatten().filter(|(_, v)| v.is_string()).collect();
    if !hashes.is_empty() {
        hashes.sort_by(|a, b| a.0.cmp(b.0));
        shaped.insert("dir_hashes".into(), Value::Object(hashes.into_iter().map(|(k, v)| (k.clone(), v.clone())).collect()));
    }
    out.json(Value::Object(shaped), true);
    0
}

fn text(value: &Value) -> Value {
    if value.is_string() { value.clone() } else { Value::Null }
}

/// The database's path under the root with the files SQLite writes beside it: they change BECAUSE of the
/// run, so a tree holding them could never read as unchanged. Empty when the database is outside the tree.
fn siblings(db: &str, root: &str) -> Vec<String> {
    let prefix = format!("{root}{}", std::path::MAIN_SEPARATOR);
    let inside = db.get(..prefix.len()).is_some_and(|head| head.eq_ignore_ascii_case(&prefix));
    let Some(rel) = db.get(prefix.len()..).filter(|_| inside) else {
        return Vec::new();
    };
    let rel = rel.replace('\\', "/");
    ["", "-wal", "-shm", "-journal"].iter().map(|suffix| format!("{rel}{suffix}")).collect()
}

fn checks(csv: &str) -> Vec<String> {
    csv.split(',').map(str::trim).filter(|s| !s.is_empty()).map(String::from).collect()
}
