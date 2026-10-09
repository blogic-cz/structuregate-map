//! WHAT THE RUN CONCLUDED, each line carrying its own severity, and THE RATCHET over what the map cannot
//! resolve. Only a state that cannot be legitimate is an error; everything that can be (a file nothing
//! imports, a cycle, an ambiguous name) is a note, because a gate that fails on those gets switched off.

use super::{Graph, Input};
use std::collections::BTreeMap;

/// Findings `--map-check` may fail a build over. A file nothing imports CAN be legitimate (a public API,
/// an entry point this tool did not recognise) and a cycle in C# is ordinary, so neither is here.
const ERRORS: [&str; 6] = ["UNPARSED", "HALF", "BROKEN", "PLUGIN", "DYNAMIC", "UNREAD"];

/// Whether a finding's text is one `--map-check` fails over - read off its prefix, so a finding kept in a written map
/// is judged exactly as the run that wrote it judged it.
pub fn is_error(text: &str) -> bool {
    ERRORS.iter().any(|p| text.starts_with(p))
}

/// A NAME THE FILE CALLS ITSELF IS NOT UNREAD, and listing it reads as dead code that is not: a script run by
/// path from outside the tree uses its own functions. So only what nothing calls is listed, and a file whose
/// every name it calls itself is said to be what it is - a file nothing in the tree RUNS.
fn no_reader(rel: &str, file: &super::File, computed: usize) -> String {
    let idle: Vec<&str> = file.declares.iter().filter(|d| *d != rel && !file.uses.contains(*d)).map(String::as_str).collect();
    let caveat = format!(
        "Before calling it dead: a caller OUTSIDE this tree is invisible here, and {computed} import(s) in this tree \
         name their target at run time (see computed_imports)"
    );
    if idle.is_empty() && file.declares.iter().any(|d| d != rel) {
        return format!("NO READER {rel}: nothing in the mapped tree runs or imports this file, and it uses every \
                        function it declares itself. {caveat}");
    }
    let listed: Vec<&str> = if idle.is_empty() { vec![rel] } else { idle.into_iter().take(4).collect() };
    format!("NO READER {rel}: declares {} and nothing in the mapped tree names any of them. {caveat}", listed.join(", "))
}

pub struct Finding {
    pub text: String,
    pub error: bool,
}

pub fn all(input: &Input, graph: &Graph) -> Vec<Finding> {
    let mut found: Vec<String> = input.errors.clone();
    found.extend(input.notes.iter().map(|note| format!("note      {note}")));
    found.extend(input.plugin_findings.iter().cloned());
    found.extend(ratchet(input, graph));
    found.extend(graph.joined.broken.iter().map(|b| format!("BROKEN    {b}")));
    for (rel, file) in &graph.files {
        if !file.unmapped.is_empty() {
            found.push(format!("UNMAPPED  {rel}: {}", file.unmapped));
        }
    }
    // A CONTEXT DOC NAMING A FILE THAT IS NOT THERE sends every session that loads it looking. Only the
    // context docs, and only as a note: a README or a `docs/` page is opened by choice, and "is this a
    // claim about THIS tree" is a judgement - a doc may describe another tree on purpose.
    for (rel, file) in graph.files.iter().filter(|(rel, _)| crate::gate::docs::context_doc(rel)) {
        for (line, text) in &file.missing {
            found.push(format!(
                "DOC-MISSING {rel}:{line}: `{text}` — named as a path, and not beside the doc, above it or under \
                 it. Gone, moved, or a claim about another tree"
            ));
        }
    }
    // A DOC NO SESSION IS LED TO: only a note - a doc can be meant for a person who opens it by choice - but one a
    // CLAUDE.md never leads to is one an agent never reads.
    for rel in super::reach::orphan_docs(&graph.files) {
        found.push(format!(
            "DOC-ORPHAN {rel}: no CLAUDE.md, AGENTS.md or .claude/ doc leads to it through the paths the docs name \
             (a link, a path in code, its folder) - a session never learns it is there. Name it from a doc that is \
             reached, or delete it"
        ));
    }
    // QUALIFIED, because an unqualified finding here reads as dead code and gets acted on. Two readers are
    // invisible to this graph: something outside the mapped tree, and something that names its target at
    // run time.
    for rel in &graph.unread {
        found.push(no_reader(rel, &graph.files[rel], input.computed.len()));
    }
    if !input.computed.is_empty() {
        let first: Vec<String> = input.computed.iter().take(3).map(|c| c.place()).collect();
        found.push(format!(
            "COMPUTED  {} import(s) name their target at run time, so they are in no edge here: {}{}",
            input.computed.len(),
            first.join(", "),
            if input.computed.len() > 3 { " …" } else { "" }
        ));
    }
    for (name, files) in &graph.joined.ambiguous {
        let first: Vec<&str> = files.iter().take(3).map(String::as_str).collect();
        found.push(format!(
            "AMBIGUOUS {name}: declared in {} files ({}) — no edge was drawn to any of them, because which one a \
             use binds to is not a question this tool can answer without a compilation",
            files.len(),
            first.join(", ")
        ));
    }
    for cycle in &graph.cycles {
        found.push(format!("CYCLE     {} -> {}", cycle.join(" -> "), cycle[0]));
    }
    for group in &graph.bodies {
        found.push(format!(
            "DUPLICATE {} function bodies are identical ({} chars): {}",
            group.at.len(),
            group.size,
            group.at.join(", ")
        ));
    }
    // A NEAR COPY IS NAMED AS ONE, with how near: a reader told "duplicate" checks the diff and finds the
    // one changed literal, so the line says what is shared and what is not.
    for pair in &graph.similar {
        found.push(format!(
            "SIMILAR   2 function bodies share {} of {} statement shape(s) ({}%) and differ in the rest - a literal, a line: {}",
            pair.shared,
            pair.total,
            pair.score,
            pair.at.join(", ")
        ));
    }
    for group in &graph.expressions {
        found.push(format!(
            "COPIED    the same {}-character expression is in {} places: {}",
            group.size,
            group.at.len(),
            group.at.join(", ")
        ));
    }
    found
        .into_iter()
        .map(|text| Finding { error: is_error(&text), text })
        .collect()
}

/// {file and shape -> how many}. The unit the ratchet counts in, and deliberately NOT the line: a dynamic
/// call does not become a different one because something above it was edited.
pub fn sites(input: &Input) -> BTreeMap<String, i64> {
    let mut sites = BTreeMap::new();
    for computed in &input.computed {
        *sites.entry(computed.site()).or_insert(0) += 1;
    }
    sites
}

/// THE RATCHET OVER WHAT THE MAP CANNOT RESOLVE, with the size baseline's three rules: a site NOT recorded
/// is rejected outright; a recorded site may not GROW; a recorded site that is GONE must leave the list, so
/// it can only shrink. Every entry removed turns a blind spot into a proven edge.
fn ratchet(input: &Input, graph: &Graph) -> Vec<String> {
    let mut found = Vec::new();
    let Some(path) = &input.options.baseline_path else { return found };
    if input.options.update_baseline {
        return found;
    }
    let (computed, unread) = match load(path) {
        Ok(recorded) => recorded,
        Err(problem) => {
            found.push(format!("DYNAMIC   {problem}"));
            (BTreeMap::new(), BTreeMap::new())
        }
    };
    let name = std::path::Path::new(path).file_name().map_or(path.clone(), |n| n.to_string_lossy().into_owned());
    let now_sites: Vec<(String, i64)> = sites(input).into_iter().collect();
    judge(&mut found, "DYNAMIC ", &now_sites, &computed, &name,
        "a name built at RUN TIME that nothing records. Resolve it at build time, or record it with \
         --update-map-baseline and shrink the list later",
        "recorded dynamic code GREW; the list may only shrink",
        "none left here");
    // A FILE NOTHING IMPORTS is recorded rather than resolved - neither dead nor route-reached is fixed by
    // parsing harder; what is worth refusing is the NEXT one arriving unnoticed.
    let now_unread: Vec<(String, i64)> = graph.unread.iter().map(|rel| (rel.clone(), 1)).collect();
    judge(&mut found, "UNREAD  ", &now_unread, &unread, &name,
        "nothing in the tree imports it and nothing records that. Give it a reader, mark it an entry point, or \
         record it with --update-map-baseline",
        "this should not happen — an unread file is counted once",
        "something imports it now");
    found
}

/// A missing baseline is an EMPTY ratchet - nothing recorded, nothing exempt. One that exists and cannot be
/// READ is the opposite, the ratchet not applied at all, so it is reported rather than thrown.
fn load(path: &str) -> Result<(BTreeMap<String, i64>, BTreeMap<String, i64>), String> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return if std::path::Path::new(path).exists() {
            Err(format!("baseline `{path}` could not be read (IOException), so the ratchet is NOT being applied: \
                         the file could not be opened — fix the file, or rewrite it with --update-baseline"))
        } else {
            Ok((BTreeMap::new(), BTreeMap::new()))
        };
    };
    let root: serde_json::Value = serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(|e| {
        format!("baseline `{path}` could not be read (JsonException), so the ratchet is NOT being applied: {e} \
                 — fix the file, or rewrite it with --update-baseline")
    })?;
    let section = |name: &str| -> BTreeMap<String, i64> {
        root.get(name)
            .and_then(|s| s.as_object())
            .map(|s| s.iter().filter_map(|(k, v)| v.as_i64().map(|n| (k.clone(), n))).collect())
            .unwrap_or_default()
    };
    Ok((section("computed"), section("unread")))
}

/// The three rules over one list, written once - they differ only in the words, and the rule that a GONE
/// entry must LEAVE the list is the one that stops a baseline being a permanent exemption.
#[allow(clippy::too_many_arguments)]
fn judge(found: &mut Vec<String>, prefix: &str, now: &[(String, i64)], recorded: &BTreeMap<String, i64>,
    baseline: &str, fresh_why: &str, grown_why: &str, gone_why: &str) {
    let mut by_count: Vec<&(String, i64)> = now.iter().collect();
    by_count.sort_by(|a, b| b.1.cmp(&a.1));
    for (key, n) in by_count {
        match recorded.get(key) {
            None => found.push(format!("{prefix}  {n}  {key} — {fresh_why}")),
            Some(was) if n > was => found.push(format!("{prefix}  {n} (baseline {was})  {key}   +{} — {grown_why}", n - was)),
            Some(_) => {}
        }
    }
    let counts: BTreeMap<&str, i64> = now.iter().map(|(k, n)| (k.as_str(), *n)).collect();
    for key in recorded.keys() {
        let n = counts.get(key.as_str()).copied().unwrap_or(0);
        if n <= 0 {
            found.push(format!(
                "{prefix}  {n}  {key}   (entry now within 0) — {gone_why}: remove it from {baseline} so the list \
                 keeps shrinking"
            ));
        }
    }
}

/// The lines a run prints about what it mapped - said so a map that mapped nothing can be told from an
/// empty tree.
pub fn report(input: &Input, graph: &Graph) -> Vec<String> {
    let total = graph.files.len();
    let mapped = graph.files.values().filter(|f| f.unmapped.is_empty()).count();
    let mut lines = vec![
        format!("  files        {total} ({mapped} mapped, {} with no parser here)", total - mapped),
        format!("  halves       {}", input.halves.iter().chain(&input.plugins).cloned().collect::<Vec<_>>().join("; ")),
    ];
    if !input.artifacts.is_empty() || !input.steps.is_empty() {
        lines.push(format!(
            "  declared     {} artifact(s), {} step(s), {} produced_by, {} read_by",
            input.artifacts.len(),
            input.steps.len(),
            input.produced.len(),
            input.read_by.len()
        ));
    }
    lines.push(format!(
        "  import edges into {} file(s) from {}; {} name(s) too ambiguous to resolve to one file",
        graph.imported_by.len(),
        graph.joined.imports.len(),
        graph.joined.ambiguous.len()
    ));
    let docs: Vec<_> = graph.files.values().filter(|f| f.language == "markdown").collect();
    if !docs.is_empty() {
        lines.push(format!(
            "  docs         {} doc(s) mention {} path(s); {} named path(s) are nowhere; {} doc(s) no CLAUDE.md leads to",
            docs.len(),
            docs.iter().map(|d| d.mentions.len()).sum::<usize>(),
            docs.iter().map(|d| d.missing.len()).sum::<usize>(),
            super::reach::orphan_docs(&graph.files).len()
        ));
    }
    lines.push(format!(
        "  cycles       {}; {} import(s) named at run time and therefore not edges",
        graph.cycles.len(),
        input.computed.len()
    ));
    lines.push(format!(
        "  duplicates   {} body group(s) over {} function(s); {} near-copy pair(s); {} expression shape(s) across files",
        graph.bodies.len(),
        graph.bodies.iter().map(|g| g.at.len()).sum::<usize>(),
        graph.similar.len(),
        graph.expressions.len()
    ));
    lines.push(format!("  wrote {}", input.options.map_path));
    if !input.database.is_empty() {
        let mut tables: Vec<(&String, &i64)> = input.database.iter().collect();
        tables.sort_by(|a, b| b.1.cmp(a.1));
        let top: Vec<String> = tables.iter().take(6).map(|(k, v)| format!("{k} {v}")).collect();
        lines.push(format!(
            "  database     {} row(s) in {} table(s){}: {}",
            input.database.values().filter(|n| **n > 0).sum::<i64>(),
            input.database.len(),
            if input.reread >= 0 { format!(", {} file(s) re-read", input.reread) } else { String::new() },
            top.join(", ")
        ));
    }
    // A HALF GIVEN FILES THAT STORED NONE SAYS WHY, here and not only among the findings: the halves line names it as
    // run, and a database with none of its rows reads as a half that found nothing to say.
    for note in input.notes.iter().filter(|n| crate::mapper::stored_nothing(n)) {
        lines.push(format!("  not stored   {note}"));
    }
    for note in input.notes.iter().filter(|n| n.contains(crate::mapper::UNRESTORED)) {
        lines.push(format!("  WARNING      {note}"));
    }
    lines
}
