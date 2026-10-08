//! THE MAP AFTER THE PARSE - the join, what it adds up to, the findings, the ratchet and the JSON. Every
//! half hands back the same vocabulary (`MapFile` on the C# side); this is where one file's USES meet
//! another file's DECLARES, written once for every language, and where the answer is judged and written.
//!
//! IT PARSES NOTHING. It takes the rows every half already produced and answers questions about the
//! TREE, which is why it lives here and not beside any parser: a join written in the host of one
//! language would be one more copy for the next language to disagree with.
//!
//! A NAME MATCHING SEVERAL FILES YIELDS NO EDGE, only a report. Which file such a use resolves to is a
//! question about scope and compilation order this tool does not ask; attributing it to all the
//! candidates would invent edges, and to one would be a coin toss presented as a fact.

mod findings;
pub(crate) use findings::is_error;
mod join;
mod reach;
mod shape;
mod write;

use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

/// One mapped file, as every half describes it. Absent fields are empty: a half that has no notion of
/// a soft import or a literal simply never sends one.
#[derive(Deserialize, Default)]
#[serde(default)]
pub struct File {
    pub rel: String,
    pub language: String,
    pub lines: i64,
    pub summary: String,
    pub declares: BTreeSet<String>,
    pub uses: BTreeSet<String>,
    pub uses_path: BTreeSet<String>,
    pub uses_suffix: BTreeSet<String>,
    pub soft_uses: BTreeSet<String>,
    pub soft_uses_path: BTreeSet<String>,
    pub soft_uses_suffix: BTreeSet<String>,
    pub literals: BTreeSet<String>,
    /// What each `from X import a, b` takes from X, by X.
    pub names: BTreeMap<String, BTreeSet<String>>,
    /// What an import can take FROM this file.
    pub binds: BTreeSet<String>,
    pub reports_external: bool,
    pub entry: bool,
    pub unmapped: String,
    pub generated: bool,
    /// Decorators that hand a def to an object - the file is entered through that object.
    /// THE FILES THIS ONE'S BOUND SYMBOLS ARE DECLARED IN, as the deep C# map's compiler said (`file_refs`). Used only
    /// to choose among a name's CURRENT owners: an unchanged file keeps its deep rows when the file it binds to moves,
    /// so a target is never an edge on its own word.
    pub bound: BTreeSet<String>,
    pub registered: BTreeSet<String>,
    pub mentions: BTreeSet<String>,
    /// What a doc names that is nowhere a reader would look: `(line, text)`.
    pub missing: Vec<(i64, String)>,
}

/// One place a fingerprint was seen: `path:line[:name]` and its size in source characters.
#[derive(Deserialize)]
pub struct Place(pub String, pub usize);

/// One function body as the SET of its statement shapes (names and literals blanked, one digest per
/// top-level statement, unique and sorted) beside its whole-body digest and its size. What
/// `shape::similar` compares; only a half that sends shingles fills it - python today.
#[derive(Deserialize, Default)]
#[serde(default)]
pub struct Shingled {
    pub place: String,
    pub size: usize,
    pub digest: String,
    pub shingles: Vec<String>,
}

/// An import whose target is built at RUN TIME.
#[derive(Deserialize)]
pub struct Computed {
    pub rel: String,
    pub line: String,
    pub what: String,
}

impl Computed {
    /// How it reads in the map and in a finding.
    pub fn place(&self) -> String {
        format!("{}:{}: {}", self.rel, self.line, self.what)
    }

    /// How the ratchet counts it - no line number, so only a real change moves it.
    pub fn site(&self) -> String {
        format!("{}: {}", self.rel, self.what)
    }
}

/// What the run was asked to do with the answer.
#[derive(Deserialize, Default)]
#[serde(default)]
pub struct Options {
    pub map_path: String,
    pub baseline_path: Option<String>,
    pub update_baseline: bool,
}

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct Input {
    pub files: Vec<File>,
    /// Each entry is ONE fingerprint's places - the fingerprint itself says nothing a reader needs.
    pub bodies: Vec<Vec<Place>>,
    pub expressions: Vec<Vec<Place>>,
    pub shingled: Vec<Shingled>,
    pub computed: Vec<Computed>,
    pub errors: Vec<String>,
    pub notes: Vec<String>,
    pub plugin_findings: Vec<String>,
    pub halves: Vec<String>,
    pub plugins: Vec<String>,
    pub inventory: String,
    // WHAT ONLY THE PROJECT CAN STATE, from a `--map-plugin`: carried into the JSON unchanged.
    pub artifacts: BTreeMap<String, BTreeMap<String, String>>,
    /// IN THE ORDER THEY ARRIVED - the order is the dependency statement.
    pub steps: Vec<(String, BTreeMap<String, String>)>,
    pub produced: BTreeMap<String, BTreeSet<String>>,
    pub read_by: BTreeMap<String, BTreeSet<String>>,
    /// Rows the deep map wrote per table, and how many files it re-read (-1: it did not run).
    pub database: BTreeMap<String, i64>,
    pub reread: i64,
    pub options: Options,
}


/// What the join worked out, handed to the findings, the writer and the report alike.
pub struct Graph {
    pub files: BTreeMap<String, File>,
    pub joined: join::Joined,
    pub imported_by: join::Rows,
    pub imported_softly_by: join::Rows,
    pub mentions: join::Rows,
    pub mentioned_by: join::Rows,
    pub cycles: Vec<Vec<String>>,
    pub bodies: Vec<shape::Group>,
    pub expressions: Vec<shape::Group>,
    /// The near copies: pairs of bodies sharing most statement shapes but not all.
    pub similar: Vec<shape::Similar>,
    /// Worked out ONCE and handed to both readers: the note that names an unread file and the ratchet that
    /// refuses a new one have to agree on what "unread" means.
    pub unread: Vec<String>,
}

pub(crate) fn finish(mut input: Input) -> Value {
    let files: BTreeMap<String, File> = std::mem::take(&mut input.files).into_iter().map(|f| (f.rel.clone(), f)).collect();
    let joined = join::resolve(&files);
    let unread = shape::unread(&files, &joined);
    let mentions: join::Rows = files
        .values()
        .filter(|f| !f.mentions.is_empty())
        .map(|f| (f.rel.clone(), f.mentions.iter().cloned().collect()))
        .collect();
    let graph = Graph {
        imported_by: join::reverse(&joined.imports),
        imported_softly_by: join::reverse(&joined.soft),
        mentioned_by: join::reverse(&mentions),
        mentions,
        cycles: shape::cycles(&joined.imports),
        bodies: shape::bodies(&input.bodies),
        expressions: shape::expressions(&input.expressions),
        similar: shape::similar(&input.shingled),
        unread,
        joined,
        files,
    };
    let found = findings::all(&input, &graph);
    let texts: Vec<&str> = found.iter().map(|f| f.text.as_str()).collect();
    if let Err(why) = write::map(&input, &graph, &texts) {
        return json!({ "write_error": why });
    }
    let mut answer = json!({
        "findings": found.iter().map(|f| json!({ "text": f.text, "error": f.error })).collect::<Vec<_>>(),
        "report": findings::report(&input, &graph),
    });
    if input.options.update_baseline
        && let Some(path) = &input.options.baseline_path
    {
        match write::baseline(path, &findings::sites(&input), &graph.unread) {
            Ok(said) => answer["baseline_written"] = json!(said),
            Err(why) => answer["baseline_error"] = json!(why),
        }
    }
    answer
}
