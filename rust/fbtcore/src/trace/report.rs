//! `--trace-report <file>`: the trace lines read back - per tree, which stage the time went to across every run
//! the file holds, and which single files were the slowest. The file is the one `STRUCTUREGATE_TRACE` writes;
//! a line that is not a run (a torn write, another tool's line) is counted and passed over, never fatal.
//!
//! NO TIME GOES UNNAMED: a span's children are added up - a `breakdown` explains its parent and is left out - and
//! what they do not cover is the span's own `(untraced)` row. The last run of each tree is then printed as a tree,
//! same-named siblings (a batch per C# batch) folded into one line, so a gap shows where it sits.

use crate::cli::Out;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

/// How many single files are listed - the slowest across every run.
const FILES_LISTED: usize = 15;

#[derive(Default)]
struct Stage {
    took_ms: Vec<f64>,
}

pub fn run(path: &str, out: &mut Out) -> i64 {
    let text = match std::fs::read_to_string(path) {
        // AND THE FILE IT ROLLED OVER INTO, older runs first: a report just after a roll would otherwise start empty.
        Ok(text) => std::fs::read_to_string(super::rolled(path)).unwrap_or_default() + &text,
        Err(why) => {
            out.error(format!("structuregate: the trace {path} could not be read ({why})"));
            return 2;
        }
    };
    // tree -> stage -> every time it took. A BTreeMap so the report reads the same on every run.
    let mut trees: BTreeMap<String, BTreeMap<String, Stage>> = BTreeMap::new();
    let mut files: Vec<(f64, String, String, String)> = Vec::new();
    let mut last: BTreeMap<String, Vec<Node>> = BTreeMap::new();
    let (mut runs, mut skipped) = (0, 0);
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let Ok(request) = serde_json::from_str::<Value>(line) else {
            skipped += 1;
            continue;
        };
        let spans: Vec<&Value> = request["resourceSpans"].as_array().into_iter().flatten()
            .flat_map(|r| r["scopeSpans"].as_array().into_iter().flatten())
            .flat_map(|s| s["spans"].as_array().into_iter().flatten())
            .collect();
        let Some(root) = spans.iter().find(|s| s.get("parentSpanId").is_none()) else {
            skipped += 1;
            continue;
        };
        runs += 1;
        let tree = attribute(root, "structuregate.root").unwrap_or_else(|| "(no root)".into());
        let mode = attribute(root, "structuregate.mode").unwrap_or_default();
        let stages = trees.entry(tree.clone()).or_default();
        let nodes = nodes(&spans);
        for (name, ms) in untraced(&nodes) {
            stages.entry(format!("{name} (untraced)")).or_default().took_ms.push(ms);
        }
        // A BREAKDOWN NAMED LIKE A STAGE OF ITS OWN RUN is that stage's time told again: a run's `csharp: rows` was dozens of
        // batch spans and one breakdown of their sum, and the table added them to twice the time the work took.
        let timed: BTreeSet<&str> = nodes.iter().filter(|n| !n.breakdown).map(|n| n.name.as_str()).collect();
        let retold: BTreeSet<String> = nodes.iter().filter(|n| n.breakdown && timed.contains(n.name.as_str())).map(|n| n.id.clone()).collect();
        last.insert(tree.clone(), nodes);
        for span in &spans {
            let name = span["name"].as_str().unwrap_or("");
            let took = millis(span);
            if let Some(file) = attribute(span, "code.filepath") {
                files.push((took, attribute(span, "structuregate.lang").unwrap_or_default(), file, tree.clone()));
                continue;
            }
            let name = if span.get("parentSpanId").is_none() {
                format!("whole run ({mode})")
            } else if span["spanId"].as_str().is_some_and(|id| retold.contains(id)) {
                format!("{name} (breakdown)")
            } else {
                name.to_string()
            };
            stages.entry(name).or_default().took_ms.push(took);
        }
    }
    out.line(format!("structuregate trace: {runs} run(s) in {path}{}", if skipped > 0 { format!(", {skipped} line(s) that are not a run passed over") } else { String::new() }));
    for (tree, stages) in &trees {
        out.line(String::new());
        out.line(tree.clone());
        out.line(format!("  {:<44} {:>5} {:>9} {:>9} {:>9}", "stage", "runs", "median", "max", "total"));
        let mut ordered: Vec<(&String, &Stage)> = stages.iter().collect();
        ordered.sort_by(|a, b| total(b.1).total_cmp(&total(a.1)));
        for (name, stage) in ordered {
            let mut sorted = stage.took_ms.clone();
            sorted.sort_by(f64::total_cmp);
            let median = sorted[sorted.len() / 2];
            let max = sorted.last().copied().unwrap_or(0.0);
            out.line(format!("  {:<44} {:>5} {:>9} {:>9} {:>9}", name, sorted.len(), shown(median), shown(max), shown(total(stage))));
        }
    }
    for (tree, nodes) in &last {
        out.line(String::new());
        out.line(format!("the last run over {tree}, as a tree (total, and what no child covers):"));
        let roots: Vec<&Node> = nodes.iter().filter(|n| n.parent.is_none()).collect();
        print(nodes, &roots, 1, out);
    }
    if !files.is_empty() {
        files.sort_by(|a, b| b.0.total_cmp(&a.0));
        out.line(String::new());
        out.line(format!("slowest files (any run, the {FILES_LISTED} slowest):"));
        for (took, lang, file, tree) in files.iter().take(FILES_LISTED) {
            out.line(format!("  {:>9}  {lang:<8} {file}  ({tree})", shown(*took)));
        }
    }
    0
}

/// One span of a run, as the report needs it.
struct Node {
    id: String,
    parent: Option<String>,
    name: String,
    start: u128,
    ms: f64,
    breakdown: bool,
}

/// The spans that are not single files, in the order they started.
fn nodes(spans: &[&Value]) -> Vec<Node> {
    let mut out: Vec<Node> = spans
        .iter()
        .filter(|s| attribute(s, "code.filepath").is_none())
        .map(|s| Node {
            id: s["spanId"].as_str().unwrap_or("").to_string(),
            parent: s["parentSpanId"].as_str().map(String::from),
            name: s["name"].as_str().unwrap_or("").to_string(),
            start: s["startTimeUnixNano"].as_str().and_then(|t| t.parse().ok()).unwrap_or(0),
            ms: millis(s),
            breakdown: attribute(s, "structuregate.timing").as_deref() == Some("breakdown") || explains(s["name"].as_str().unwrap_or("")),
        })
        .collect();
    out.sort_by_key(|n| n.start);
    out
}

/// A TRACE WRITTEN BEFORE `breakdown` EXISTED (v1.5.8 and older) reported the compile's phases and projects as
/// plain children, and added up they counted the compile twice: known by their names.
fn explains(name: &str) -> bool {
    name.starts_with("csharp project: ")
        || ["csharp: load references", "csharp: parse sources", "csharp: razor", "csharp: declarations"].contains(&name)
}

/// What a span's children add up to, leaving out the ones that only explain it.
fn covered(nodes: &[Node], id: &str) -> Option<f64> {
    let children: Vec<&Node> = nodes.iter().filter(|n| n.parent.as_deref() == Some(id) && !n.breakdown).collect();
    (!children.is_empty()).then(|| children.iter().map(|n| n.ms).sum())
}

/// Each span with children, and the time none of them covers.
fn untraced(nodes: &[Node]) -> Vec<(String, f64)> {
    nodes.iter().filter_map(|n| covered(nodes, &n.id).map(|c| (n.name.clone(), (n.ms - c).max(0.0)))).collect()
}

/// Siblings printed as a tree: the same name folded into one line, the 25 largest shown and the rest summed.
fn print(nodes: &[Node], siblings: &[&Node], depth: usize, out: &mut Out) {
    let mut groups: Vec<(String, Vec<&Node>)> = Vec::new();
    for node in siblings {
        match groups.iter_mut().find(|(name, _)| *name == node.name) {
            Some((_, members)) => members.push(node),
            None => groups.push((node.name.clone(), vec![node])),
        }
    }
    let sum = |members: &Vec<&Node>| members.iter().map(|n| n.ms).sum::<f64>();
    groups.sort_by(|a, b| sum(&b.1).total_cmp(&sum(&a.1)));
    const SHOWN: usize = 25;
    for (name, members) in groups.iter().take(SHOWN) {
        let total = sum(members);
        let count = if members.len() > 1 { format!(" x{}", members.len()) } else { String::new() };
        let gap: f64 = members.iter().filter_map(|n| covered(nodes, &n.id).map(|c| (n.ms - c).max(0.0))).sum();
        let has_children = members.iter().any(|n| covered(nodes, &n.id).is_some());
        let mark = if members[0].breakdown { "  (breakdown)" } else { "" };
        let gap = if has_children { format!("  untraced {}", shown(gap)) } else { String::new() };
        out.line(format!("{}{name}{count}  {}{gap}{mark}", "  ".repeat(depth), shown(total)));
        let children: Vec<&Node> = nodes.iter().filter(|n| members.iter().any(|m| n.parent.as_deref() == Some(m.id.as_str()))).collect();
        if !children.is_empty() {
            print(nodes, &children, depth + 1, out);
        }
    }
    if groups.len() > SHOWN {
        let rest: f64 = groups[SHOWN..].iter().map(|(_, m)| sum(m)).sum();
        out.line(format!("{}... {} more, {}", "  ".repeat(depth), groups.len() - SHOWN, shown(rest)));
    }
}

fn total(stage: &Stage) -> f64 {
    stage.took_ms.iter().sum()
}

fn millis(span: &Value) -> f64 {
    let ns = |key: &str| span[key].as_str().and_then(|s| s.parse::<u128>().ok()).unwrap_or(0);
    ns("endTimeUnixNano").saturating_sub(ns("startTimeUnixNano")) as f64 / 1_000_000.0
}

fn shown(ms: f64) -> String {
    if ms >= 1000.0 { format!("{:.1} s", ms / 1000.0) } else { format!("{ms:.0} ms") }
}

/// An attribute's value as text, whichever `AnyValue` it was written as.
fn attribute(span: &Value, key: &str) -> Option<String> {
    let found = span["attributes"].as_array()?.iter().find(|a| a["key"] == key)?;
    let value = &found["value"];
    value["stringValue"].as_str().or(value["intValue"].as_str()).map(String::from)
        .or_else(|| value["boolValue"].as_bool().map(|b| b.to_string()))
}
