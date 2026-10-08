//! The SQLite end of a deep map half, run in the process that holds the rows.
//!
//! TWO CALLS, AND THE ORDER IS THE WHOLE DESIGN. `state` says what is already recorded
//! and at which sha; only then does the extractor parse the files whose sha moved. An
//! extractor that skipped the state call would have to re-parse and re-send the whole
//! tree on every run, which is the cost the incremental path exists to avoid.
//!
//! WHAT CHANGED BY MOVING IT HERE. The payload was a FILE — "tens of megabytes on a
//! real tree", read back with `json.load` — because the exe and the store were
//! different processes. They are now one, so the rows are memory the extractor already
//! owns and the batching exists only to bound peak memory, not to cross a pipe.

pub mod calls;
pub mod fts;
// THE PARTIAL RUN'S OWN MACHINERY - what is handed back, which files depend on which, what each read - in a folder
// by `#[path]`, so every `super::` in them still means `rows`.
#[path = "partial/carry.rs"]
pub mod carry;
#[path = "partial/deps.rs"]
pub mod deps;
pub mod dups;
pub mod tsapply;
pub mod half;
pub mod pyjson;
mod rawcells;
#[path = "partial/reads.rs"]
pub mod reads;
pub mod schema;
pub mod seeds;
pub mod sqlrun;
pub mod store;
pub mod ts;

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// What the extractor has to know BEFORE it parses anything.
#[derive(Debug, Serialize)]
pub struct State {
    /// The DATABASE cannot be built on — absent, or of an older schema. It is not
    /// about this language: a database that is fine but holds no rows of this half yet
    /// is NOT a rebuild, because replacing the file would take the other half's rows
    /// with it.
    pub rebuild: bool,
    pub shas: BTreeMap<String, String>,
    pub counters: BTreeMap<String, i64>,
    /// Only this half has rows here, so its ids may restart. See `half::alone`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alone: Option<bool>,
    /// What produced the rows that are in there — compiler versions and config. A
    /// different setup is a different map over the same bytes, so the recorded hashes
    /// say nothing about it and every file has to be read again.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub setup: Option<String>,
    /// What each file's rows were bound THROUGH, as the half itself recorded it - see
    /// `Batch::deps`. Left out when the half never sent one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deps: Option<String>,
}

/// What a half has to know BEFORE it parses anything.
///
/// `whole` names a half that replaces its rows whole; it gets `alone` and `setup` as
/// well, because both decide what it does next. A half that drops by file gets neither,
/// and the fields are left out of the reply rather than sent as null.
/// The dependency graph and the scope set, from one payload — for holding against the python
/// pass. Diagnostic.
pub fn deps_json(payload: &[u8]) -> Result<String> {
    let parsed: Value = serde_json::from_slice(payload)?;
    let empty = Map::new();
    let tables = parsed.get("tables").and_then(|t| t.as_object()).unwrap_or(&empty);
    let mut graph = Map::new();
    for (target, dependents) in deps::graph(tables) {
        graph.insert(target, Value::from(dependents));
    }
    Ok(serde_json::to_string(&json!({
        "graph": Value::Object(graph),
        "scope": deps::scope_files(tables),
    }))?)
}

/// WHAT THE READER HAS TO KNOW BEFORE IT PARSES, for a half that replaces its rows whole.
///
/// The C# half needs the ids already handed out and what is recorded. This one needs five things
/// more, and each is a fact only a FULL run can measure: the dependency graph and the scope set
/// decide whether a partial run is possible at all, and `json_columns`, `bool_columns`,
/// `rebuilt` and `identity` decide what the reader may do with the rows it is handed back.
///
/// THE GRAPH IS READ BACK, NEVER RECOMPUTED HERE. Deriving it costs seconds on a large map,
/// and this call is on the path of the run that has NOTHING to do — the one that costs barely more
/// than that in total. It is computed once, by the run that has already paid for the rows.
#[derive(Debug, Serialize)]
pub struct TsState {
    pub rebuild: bool,
    pub shas: BTreeMap<String, String>,
    pub setup: String,
    pub alone: bool,
    pub counters: BTreeMap<String, i64>,
    pub deps: Value,
    pub scope: Value,
    pub json_columns: Value,
    pub bool_columns: Value,
    pub rebuilt: Value,
    pub identity: Value,
    /// What each file's extraction read, surfaced, and which template goes with which component - see `reads`.
    pub readers: Value,
    pub deep_readers: Value,
    pub surfaces: Value,
    pub coupled: Value,
    /// path -> the shape of each file - see `reads::Recorded`.
    pub shapes: Value,
    /// The last full run found every file its rows name among the reads, and no run since found one missing.
    pub reads_proven: bool,
}

pub fn ts_state(db: &Path, lang: &str) -> TsState {
    let meta = |key: &str| half::meta(db, key);
    let stored = |key: &str, empty: Value| -> Value {
        let text = meta(key);
        if text.is_empty() {
            return empty;
        }
        serde_json::from_str(&text).unwrap_or(empty)
    };
    // The spec holds four of the six under one key, because one manifest is one thing to keep in
    // step. They are lifted out here so the reader does not have to know that.
    let spec = stored(&format!("spec:{lang}"), Value::Object(Map::new()));
    let from_spec = |name: &str, empty: Value| -> Value {
        spec.get(name).cloned().unwrap_or(empty)
    };

    let recorded = reads::recorded(db, lang);
    TsState {
        rebuild: !store::usable(db),
        shas: store::recorded(db, lang),
        setup: meta(&format!("setup:{lang}")),
        alone: half::alone(db, lang),
        counters: store::counters(db),
        deps: stored(&format!("deps:{lang}"), Value::Object(Map::new())),
        scope: stored(&format!("scope:{lang}"), Value::Array(Vec::new())),
        json_columns: from_spec("json_columns", Value::Object(Map::new())),
        bool_columns: from_spec("bool_columns", Value::Object(Map::new())),
        rebuilt: from_spec("rebuilt", Value::Array(Vec::new())),
        identity: from_spec("identity", Value::Object(Map::new())),
        readers: recorded.readers,
        deep_readers: recorded.deep_readers,
        surfaces: recorded.surfaces,
        coupled: recorded.coupled,
        shapes: recorded.shapes,
        reads_proven: recorded.loaded && meta(&format!("reads:{lang}")) == "proven",
    }
}

pub fn state(db: &Path, lang: &str, whole: bool) -> State {
    State {
        rebuild: !store::usable(db),
        shas: store::recorded(db, lang),
        counters: store::counters(db),
        alone: whole.then(|| half::alone(db, lang)),
        setup: whole.then(|| half::meta(db, &format!("setup:{lang}"))),
        deps: Some(half::meta(db, &format!("deps:{lang}"))).filter(|d| !d.is_empty()),
    }
}

/// One batch of rows, the shape an extractor hands over.
#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Batch {
    /// Whether this RUN sends every file of the tree or only the ones that changed. It
    /// is what lets this end refuse to rebuild a database out of a partial payload
    /// rather than write a truncated one.
    pub all: bool,
    pub first: bool,
    // "final" is a reserved word in rust, so the FIELD is renamed and the protocol is
    // not: the payload shape is shared with the python half and must not drift.
    #[serde(rename = "final")]
    pub final_: bool,
    /// The retry: every file this half records is dropped and written again. The
    /// database is NOT replaced — the rows that disagreed are this half's and the other
    /// half's are not.
    pub reset: bool,
    /// Every batch carries the WHOLE sha map, because what has LEFT the tree is decided
    /// against it; a batch naming only its own files would read as a tree that had just
    /// lost everything else.
    pub shas: Map<String, Value>,
    /// `[rel, abs]` per file re-read this run. The text is READ HERE rather than
    /// carried: the file is already on disk, and sending it would put every byte of the
    /// tree through the payload twice.
    pub read: Vec<Vec<String>>,
    pub counters: Map<String, Value>,
    pub tables: Map<String, Value>,
    pub lang: String,
    /// Replace this half's rows whole, stamping every one with this name, instead of
    /// dropping the rows of the files that moved.
    pub half: Option<String>,
    /// Stored as `setup:<lang>`, so the next run can tell that the tool which produced
    /// the rows has changed even though no file has.
    pub setup: Option<String>,
    /// Stored as `deps:<lang>` and handed back in the state: `{rel -> [what its rows were
    /// bound through]}`. A file's rows can name ANOTHER file - a python call's
    /// `target_path` - and a file whose own bytes did not move is not re-read, so without
    /// this an importer kept pointing at a module that had moved away. The half owns the
    /// meaning; this end only keeps it.
    pub deps: Option<String>,
    /// Stored as `roots:<lang>`: `[[prefix, dir], ...]`, every root the half mapped and the prefix its
    /// paths carry. `_meta.root` is the FIRST root only, and a lens handed a path as a person spells it
    /// (`py/core/config.py`) needs all of them to find `core/config.py`.
    pub roots: Option<String>,
}

impl Default for Batch {
    fn default() -> Self {
        Batch {
            all: false,
            first: false,
            final_: false,
            reset: false,
            shas: Map::new(),
            read: Vec::new(),
            counters: Map::new(),
            tables: Map::new(),
            lang: "csharp".to_string(),
            half: None,
            setup: None,
            deps: None,
            roots: None,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct Receipt {
    /// Rows now in each table, once the last batch has counted them.
    pub written: BTreeMap<String, i64>,
    pub read: usize,
    pub files: usize,
    /// Files that disagree with the tree after the LAST batch. Anything but zero and
    /// the extractor must send everything again with `reset`.
    pub retry: i64,
    /// Read back OUT OF THE DATABASE, not counted off what was sent. A half that says
    /// "I was given 101 files" proves nothing about what landed.
    pub stored: usize,
    pub fts: bool,
}

pub fn apply(db: &Path, root: &str, batch: &Batch) -> Result<Receipt> {
    let before = store::recorded(db, &batch.lang);

    // ONLY THE FIRST BATCH MAY MEET AN UNUSABLE DATABASE. A later one means the file
    // was replaced or removed underneath this run, and rebuilding it out of one batch's
    // rows would write a database holding the last few files of the tree and nothing
    // else.
    if !batch.first && !store::usable(db) {
        bail!("the database went away between two batches of the same run");
    }

    let full = !store::usable(db);
    if full && !batch.all {
        bail!("the database has to be rebuilt but only the changed files were sent");
    }

    let mut gone: BTreeSet<String> = before
        .keys()
        .filter(|rel| !batch.shas.contains_key(*rel))
        .cloned()
        .collect();
    if batch.reset {
        gone.extend(before.keys().cloned());
    }

    let texts = texts_of(&batch.read);
    let extra: BTreeMap<String, String> = batch
        .deps
        .iter()
        .map(|deps| (format!("deps:{}", batch.lang), deps.clone()))
        .chain(batch.roots.iter().map(|roots| (format!("roots:{}", batch.lang), roots.clone())))
        .collect();

    let request = store::WriteRequest {
        tables: &batch.tables,
        texts: &store::Texts::Held(&texts),
        root,
        shas: &batch.shas,
        gone: &gone,
        full,
        counters: &batch.counters,
        lang: &batch.lang,
        half: batch.half.as_deref(),
        setup: batch.setup.as_deref(),
        extra: &extra,
        keep_existing: false,
        counts: batch.final_,
    };
    let applied = store::write(db, &request)?;

    let stored = if batch.final_ {
        store::recorded(db, &batch.lang).len()
    } else {
        batch.shas.len()
    };

    Ok(Receipt {
        written: applied.written,
        read: batch.read.len(),
        files: batch.shas.len(),
        // ONLY AFTER THE LAST BATCH. Until then the tree holds files this database has
        // not been given yet, and every one counts as a disagreement: checked per batch,
        // a large tree would ask for a full rewrite on every single run.
        retry: if batch.final_ { applied.wrong } else { 0 },
        stored,
        fts: applied.fts,
    })
}

/// The source of every file that was re-read, for `file_text`.
///
/// A file that cannot be read is skipped rather than fatal: the file-level pass over
/// the same tree already names it, in the same run.
fn texts_of(read: &[Vec<String>]) -> BTreeMap<String, String> {
    let mut texts = BTreeMap::new();
    for entry in read {
        let (Some(rel), Some(abs)) = (entry.first(), entry.get(1)) else {
            continue;
        };
        if let Some(text) = pyjson::read_text(abs) {
            texts.insert(rel.clone(), text);
        }
    }
    texts
}

/// A TEST'S DATABASE, gone when the test ends: the database, its `-wal`/`-shm` and the `.carry.json` a partial run
/// writes beside it. The helpers only deleted BEFORE a test, by the process id, so every `cargo test` left its own
/// set behind - 2 500 files in a RAM-backed /tmp.
#[cfg(test)]
pub(crate) struct TempDb(std::path::PathBuf);

#[cfg(test)]
impl TempDb {
    pub(crate) fn new(prefix: &str, name: &str) -> TempDb {
        let db = TempDb(std::env::temp_dir().join(format!("fbt-{prefix}-{name}-{}.db", std::process::id())));
        db.remove();
        db
    }

    fn remove(&self) {
        for suffix in ["", "-wal", "-shm", "-journal"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", self.0.display()));
        }
        let _ = std::fs::remove_file(self.0.with_extension("carry.json"));
    }
}

#[cfg(test)]
impl std::ops::Deref for TempDb {
    type Target = std::path::Path;
    fn deref(&self) -> &std::path::Path {
        &self.0
    }
}

#[cfg(test)]
impl AsRef<std::path::Path> for TempDb {
    fn as_ref(&self) -> &std::path::Path {
        &self.0
    }
}

#[cfg(test)]
impl Drop for TempDb {
    fn drop(&mut self) {
        self.remove();
    }
}
