//! THE MAP ON DISK, and the ratchet when it is asked to be recorded. Written from sorted collections, so
//! two runs over one tree are byte-identical and a diff of two maps shows only what changed.

use super::join::Rows;
use super::{Computed, Graph, Input};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

/// The map, in the order a reader meets it. `inventory` goes FIRST: `--map-if-stale` reads it off the head
/// of the file without parsing the rest.
pub fn map(input: &Input, graph: &Graph, findings: &[&str]) -> Result<(), String> {
    let mut out = Map::new();
    if !input.inventory.is_empty() {
        out.insert("inventory".into(), json!(input.inventory));
    }
    let mut files = Map::new();
    for (rel, file) in &graph.files {
        let mut row = Map::new();
        row.insert("language".into(), json!(file.language));
        row.insert("lines".into(), json!(file.lines));
        if !file.summary.is_empty() {
            row.insert("summary".into(), json!(file.summary));
        }
        if file.entry {
            row.insert("entry".into(), json!(true));
        }
        if !file.registered.is_empty() {
            row.insert("registered".into(), json!(file.registered));
        }
        if !file.unmapped.is_empty() {
            row.insert("unmapped".into(), json!(file.unmapped));
        }
        // SAID OUT LOUD: a generated file contributes no duplicate group and no deep row, and a reader who
        // did not know why would read that as "nothing is written twice in here".
        if file.generated {
            row.insert("generated".into(), json!(true));
        }
        row.insert("declares".into(), json!(file.declares));
        files.insert(rel.clone(), Value::Object(row));
    }
    out.insert("files".into(), Value::Object(files));
    section(&mut out, "imports", &graph.joined.imports);
    section(&mut out, "imported_by", &graph.imported_by);
    section(&mut out, "soft_imports", &graph.joined.soft);
    section(&mut out, "imported_softly_by", &graph.imported_softly_by);
    // WHAT THE DOCS POINT AT, apart from what the code imports - never an import.
    section(&mut out, "mentions", &graph.mentions);
    section(&mut out, "mentioned_by", &graph.mentioned_by);
    section(&mut out, "external", &graph.joined.external);
    section(&mut out, "ambiguous", &graph.joined.ambiguous);
    out.insert("cycles".into(), json!(graph.cycles));
    out.insert("duplicate_bodies".into(), groups(&graph.bodies));
    out.insert("duplicate_expressions".into(), groups(&graph.expressions));
    out.insert("similar_bodies".into(), similar(&graph.similar));
    out.insert("artifacts".into(), json!(input.artifacts));
    let steps: Vec<Value> = input
        .steps
        .iter()
        .map(|(name, fields)| {
            let mut step = Map::new();
            step.insert("step".into(), json!(name));
            for (field, value) in fields {
                step.insert(field.clone(), json!(value));
            }
            Value::Object(step)
        })
        .collect();
    out.insert("steps".into(), Value::Array(steps));
    out.insert("produced_by".into(), json!(input.produced));
    out.insert("read_by".into(), json!(input.read_by));
    let mut computed: Vec<String> = input.computed.iter().map(Computed::place).collect();
    computed.sort();
    out.insert("computed_imports".into(), json!(computed));
    let halves: Vec<String> =
        input.halves.iter().cloned().chain(input.plugins.iter().map(|p| format!("plugin: {p}"))).collect();
    out.insert("halves".into(), json!(halves));
    out.insert("findings".into(), json!(findings));

    let path = std::path::Path::new(&input.options.map_path);
    let text = serde_json::to_string_pretty(&Value::Object(out)).map_err(|e| e.to_string())?;
    if let Some(folder) = path.parent().filter(|f| !f.as_os_str().is_empty()) {
        std::fs::create_dir_all(folder).map_err(|e| describe(path, &e))?;
    }
    std::fs::write(path, text).map_err(|e| describe(path, &e))
}

/// The ratchet, rewritten from the current counts: every place a name is built at run time, and every
/// file nothing imports. A count per SITE rather than a total, because a total lets one dynamic import be
/// swapped for another without the gate noticing, and gives nobody a path to zero.
pub fn baseline(path: &str, sites: &BTreeMap<String, i64>, unread: &[String]) -> Result<String, String> {
    let listed: BTreeMap<String, i64> = unread.iter().map(|rel| (rel.clone(), 1)).collect();
    let mut out = Map::new();
    out.insert("_comment".into(), json!(
        "What the map cannot resolve, as it stood when the ratchet was introduced. BOTH lists may only SHRINK. \
         `computed`: imports whose target is built at RUN TIME — resolve the name at build time rather than \
         adding an entry, because every one removed turns a blind spot into a proven edge. `unread`: files \
         nothing in the tree imports — each is either dead or reached in a way no parse tree can see (a route, \
         a shell script, a registry)."));
    out.insert("computed".into(), counts(sites));
    out.insert("unread".into(), counts(&listed));
    let text = serde_json::to_string_pretty(&Value::Object(out)).map_err(|e| e.to_string())?;
    std::fs::write(path, text).map_err(|e| describe(std::path::Path::new(path), &e))?;
    Ok(format!(
        "map baseline written: {} run-time-named import(s) in {} file(s), {} file(s) nothing imports -> {path}",
        sites.values().sum::<i64>(),
        sites.len(),
        listed.len()
    ))
}

/// Largest first, then by key - the order a reader shrinks the list in.
fn counts(counts: &BTreeMap<String, i64>) -> Value {
    let mut ordered: Vec<(&String, &i64)> = counts.iter().collect();
    ordered.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    Value::Object(ordered.into_iter().map(|(k, v)| (k.clone(), json!(v))).collect())
}

fn section(out: &mut Map<String, Value>, name: &str, rows: &Rows) {
    out.insert(name.into(), json!(rows));
}

fn groups(groups: &[super::shape::Group]) -> Value {
    Value::Array(groups.iter().map(|g| json!({ "size": g.size, "at": g.at })).collect())
}

/// A near copy is a PAIR with a score, never a group: chaining pairs into groups would put two bodies
/// that share nothing in one group through a third that resembles both.
fn similar(pairs: &[super::shape::Similar]) -> Value {
    Value::Array(
        pairs
            .iter()
            .map(|p| json!({ "score": p.score, "shared": p.shared, "total": p.total, "size": p.size, "at": p.at }))
            .collect(),
    )
}

fn describe(path: &std::path::Path, e: &std::io::Error) -> String {
    format!("{} ({e})", path.display())
}

