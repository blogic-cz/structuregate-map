//! WHERE A RUN SPENT ITS TIME, written down - and only when asked: `STRUCTUREGATE_TRACE=<file>` appends ONE LINE
//! per run to that file, nothing is written without it, and a run never fails because the line could not be.
//!
//! THE LINE IS AN OTLP/JSON `ExportTraceServiceRequest` (`otlp.rs`): the run is the root span, each stage a child,
//! and the slowest files the stages parsed one by one are spans of their own. Nothing here opens a socket - every
//! consumer's build runs this exe, and paths are not sent anywhere a person did not point them. A machine that
//! WANTS them remote points an OpenTelemetry Collector's `otlpjsonfile` receiver at the file and exports from
//! there; the file is already what that receiver reads. `--trace-report <file>` reads the same lines back here.
//!
//! A DEEP MAP RUN (`--map-sqlite <db>`) IS ALWAYS TRACED, into `<db>.last-run.jsonl` beside its database, REPLACED by
//! the next run: a refresh that took minutes after a pull could not be analysed afterwards, since nobody had asked for
//! a trace. One line of a few KB, the detail left off - the run's spans cost it microseconds.
//!
//! `--trace <file>` names the file on the command line instead, and `--trace-detail` (`STRUCTUREGATE_TRACE_DETAIL=1`)
//! asks for MORE: a span per C# batch with what the host spent in it, every project rather than the 20 slowest, and
//! the 1000 slowest files rather than 20. Its line is larger; a run nobody is investigating does not want it.
//!
//! A span that only EXPLAINS another (a phase of `csharp: compile`, a project) is `structuregate.timing =
//! breakdown`: `--trace-report` leaves it out when it adds up a span's children, so whatever the children do not
//! cover is printed as the span's own `(untraced)` time - a gap is named, never hidden (minutes of `deep:
//! csharp` were in no span).
//!
//! ONE RUN AT A TIME, ONE THREAD OF STAGES: the stages run in order, so the open span is a stack. Work fanned out
//! inside a stage is timed as the stage, never span by span from the workers.

mod otlp;
pub mod report;

use serde_json::Value;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// The file the run's line is appended to. Unset or empty: nothing is traced, and nothing is paid for it.
pub const ENV: &str = "STRUCTUREGATE_TRACE";
/// Asks for the detailed trace without the flag - for a hook or a build that cannot change its arguments.
pub const DETAIL_ENV: &str = "STRUCTUREGATE_TRACE_DETAIL";
/// What a deep map's LAST RUN is traced into, beside its database. Every walk that hashes a tree prunes it with the
/// database's other siblings, or the run that writes it would read as a changed tree on the next.
pub const LAST_RUN: &str = ".last-run.jsonl";
/// How many single files are kept per run - the slowest. Every file of a big tree would be a line of megabytes.
const SLOWEST_FILES: usize = 20;
const SLOWEST_FILES_DETAIL: usize = 1000;

static ON: AtomicBool = AtomicBool::new(false);
static DETAIL: AtomicBool = AtomicBool::new(false);
static RUN: Mutex<Option<Run>> = Mutex::new(None);

struct Run {
    /// The file the run's line is APPENDED to, when one was asked for.
    path: Option<String>,
    /// The file it REPLACES the last run's line in - a deep map's `<db>.last-run.jsonl`.
    last: Option<String>,
    trace_id: String,
    started: Instant,
    started_ns: u128,
    resource: Vec<(String, Value)>,
    spans: Vec<Span>,
    /// Indices into `spans`, innermost last.
    open: Vec<usize>,
    /// The slowest single files, kept apart so the cap does not drop a stage.
    files: Vec<Span>,
    /// Spans made so far, dropped ones included: an id is never handed out twice.
    made: u64,
}

struct Span {
    id: String,
    parent: Option<String>,
    name: String,
    start_ns: u128,
    end_ns: u128,
    attributes: Vec<(String, Value)>,
}

impl Run {
    fn now_ns(&self) -> u128 {
        self.started_ns + self.started.elapsed().as_nanos()
    }
    fn span_id(&mut self) -> String {
        self.made += 1;
        blake3::hash(format!("{}|{}", self.trace_id, self.made).as_bytes()).to_hex()[..16].to_string()
    }
    fn parent(&self) -> Option<String> {
        self.open.last().map(|&at| self.spans[at].id.clone())
    }
}

/// Starts the run's root span - when `--trace` or `STRUCTUREGATE_TRACE` names a file, or `last` does (a deep map's
/// own). `resource` says WHAT ran (version, host), `attributes` what it was asked (root, mode, arguments).
pub fn begin(given: Option<&str>, last: Option<String>, detail: bool, resource: Vec<(&str, Value)>, attributes: Vec<(&str, Value)>) {
    let path = Some(given.map(String::from).unwrap_or_else(|| std::env::var(ENV).unwrap_or_default())).filter(|p| !p.trim().is_empty());
    if path.is_none() && last.is_none() {
        return;
    }
    let detail = detail || std::env::var(DETAIL_ENV).is_ok_and(|v| !v.is_empty() && v != "0");
    DETAIL.store(detail, Ordering::Relaxed);
    let mut attributes = attributes;
    attributes.push(("structuregate.trace.detail", Value::from(detail)));
    let started_ns = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let seed = format!("{started_ns}|{}|{path:?}|{last:?}", std::process::id());
    let mut run = Run {
        path,
        last,
        trace_id: blake3::hash(seed.as_bytes()).to_hex()[..32].to_string(),
        started: Instant::now(),
        started_ns,
        resource: owned(resource),
        spans: Vec::new(),
        open: Vec::new(),
        files: Vec::new(),
        made: 0,
    };
    // THE ENVIRONMENT WINS: a machine that names itself in `OTEL_RESOURCE_ATTRIBUTES` meant it.
    for (key, value) in otlp::from_environment() {
        run.resource.retain(|(k, _)| *k != key);
        run.resource.push((key, value));
    }
    let id = run.span_id();
    run.spans.push(Span { id, parent: None, name: "structuregate".into(), start_ns: started_ns, end_ns: 0, attributes: owned(attributes) });
    run.open.push(0);
    if let Ok(mut slot) = RUN.lock() {
        *slot = Some(run);
        ON.store(true, Ordering::Relaxed);
    }
}

/// A stage, timed from here until the guard drops. Free when nothing is traced.
#[must_use = "the stage ends when the guard drops"]
pub struct Stage(Option<usize>);

pub fn stage(name: &str) -> Stage {
    if !ON.load(Ordering::Relaxed) {
        return Stage(None);
    }
    with(|run| {
        let span = Span { id: run.span_id(), parent: run.parent(), name: name.into(), start_ns: run.now_ns(), end_ns: 0, attributes: Vec::new() };
        run.spans.push(span);
        run.open.push(run.spans.len() - 1);
        run.spans.len() - 1
    })
    .map_or(Stage(None), |at| Stage(Some(at)))
}

impl Stage {
    /// Still timed until it drops, but NO LONGER THE PARENT of what opens next: a stage whose host runs on beside the
    /// rest of the run (the PowerShell file map, through the deep map) would otherwise adopt every later stage.
    pub fn detach(&self) {
        if let Some(at) = self.0 {
            with(|run| run.open.retain(|&open| open != at));
        }
    }

    /// A fact about this stage - how many files it was handed, which host ran it.
    pub fn set(&self, key: &str, value: impl Into<Value>) {
        if let Some(at) = self.0 {
            let value = value.into();
            with(|run| run.spans[at].attributes.push((key.into(), value)));
        }
    }
}

impl Drop for Stage {
    fn drop(&mut self) {
        if let Some(at) = self.0 {
            with(|run| {
                run.spans[at].end_ns = run.now_ns();
                run.open.retain(|&open| open != at);
            });
        }
    }
}

/// A fact about the innermost open stage - the run itself when no stage is open.
pub fn set(key: &str, value: impl Into<Value>) {
    if !ON.load(Ordering::Relaxed) {
        return;
    }
    let value = value.into();
    with(|run| {
        if let Some(&at) = run.open.last() {
            run.spans[at].attributes.push((key.into(), value));
        }
    });
}

/// Whether this run asked for the detailed trace - and is traced at all.
pub fn detail() -> bool {
    ON.load(Ordering::Relaxed) && DETAIL.load(Ordering::Relaxed)
}

/// A total that EXPLAINS the open stage rather than adding to it - a phase of the compile, one project: laid back
/// like `reported`, and left out when the report adds up the stage's children.
pub fn breakdown_with(name: &str, ms: u64, facts: Vec<(String, Value)>) {
    let mut facts = facts;
    facts.insert(0, ("structuregate.timing".to_string(), Value::from("breakdown")));
    reported_with(name, ms, facts);
}

/// A part of the open stage that a HOST timed and reported as a total (the C# half's compile, the TypeScript
/// half's phases): it ends now and is laid back from here, and `structuregate.timing = reported` says so.
pub fn reported(name: &str, ms: u64) {
    reported_with(name, ms, Vec::new());
}

/// `reported`, with what the host said about it beside the time - a project's phases, its file count.
pub fn reported_with(name: &str, ms: u64, facts: Vec<(String, Value)>) {
    if !ON.load(Ordering::Relaxed) {
        return;
    }
    with(|run| {
        let end_ns = run.now_ns();
        let start_ns = end_ns.saturating_sub(u128::from(ms) * 1_000_000);
        // A BREAKDOWN says so in its own first fact; anything else a host reported is `reported`.
        let mut attributes = if facts.first().is_some_and(|(k, _)| k == "structuregate.timing") {
            Vec::new()
        } else {
            vec![("structuregate.timing".to_string(), Value::from("reported"))]
        };
        attributes.extend(facts);
        let span = Span { id: run.span_id(), parent: run.parent(), name: name.into(), start_ns, end_ns, attributes };
        run.spans.push(span);
    });
}

/// One file, parsed on its own since `started` - kept when it is among the run's slowest.
pub fn file(lang: &str, rel: &str, started: Instant) {
    if !ON.load(Ordering::Relaxed) {
        return;
    }
    let took = started.elapsed().as_nanos();
    let kept = if DETAIL.load(Ordering::Relaxed) { SLOWEST_FILES_DETAIL } else { SLOWEST_FILES };
    with(|run| {
        if run.files.len() >= kept && run.files.iter().all(|f| f.end_ns - f.start_ns >= took) {
            return;
        }
        let end_ns = run.now_ns();
        let attributes = vec![("code.filepath".to_string(), Value::from(rel)), ("structuregate.lang".to_string(), Value::from(lang))];
        let span = Span { id: run.span_id(), parent: run.parent(), name: format!("file: {lang}"), start_ns: end_ns - took, end_ns, attributes };
        run.files.push(span);
        if run.files.len() > kept {
            run.files.sort_by(|a, b| (b.end_ns - b.start_ns).cmp(&(a.end_ns - a.start_ns)));
            run.files.truncate(kept);
        }
    });
}

/// Ends the run with its exit code and appends its line. `Some(note)` when the line could not be written: the
/// run's own answer stands, and the note says where the trace did not go.
pub fn finish(exit: i64) -> Option<String> {
    if !ON.swap(false, Ordering::Relaxed) {
        return None;
    }
    let mut run = RUN.lock().ok()?.take()?;
    let end_ns = run.now_ns();
    for span in run.spans.iter_mut().filter(|s| s.end_ns == 0) {
        span.end_ns = end_ns;
    }
    run.spans[0].attributes.push(("process.exit.code".into(), Value::from(exit)));
    let mut spans = std::mem::take(&mut run.spans);
    spans.append(&mut run.files);
    let mut line = otlp::encode(&run.trace_id, &run.resource, &spans, exit).to_string();
    line.push('\n');
    let mut failed = Vec::new();
    if let Some(path) = &run.path
        && let Err(why) = append(path, &line)
    {
        failed.push(format!("NOTE: the trace was not written to {path} ({why})"));
    }
    // THE LAST RUN'S FILE IS REPLACED, never appended to: it is that one run, and stays one line's size.
    if let Some(last) = &run.last
        && let Err(why) = std::fs::write(last, &line)
    {
        failed.push(format!("NOTE: the last run's trace was not written to {last} ({why})"));
    }
    (!failed.is_empty()).then(|| failed.join("\n"))
}

/// THIS RUN DID NOTHING WORTH KEEPING: the last run's trace beside the database stays as it is. Replacing it on every
/// run that changed nothing would rewrite a file inside the tree each turn - and lose the slow run it was kept for.
pub fn keep_previous_run() {
    if ON.load(Ordering::Relaxed) {
        with(|run| run.last = None);
    }
}

/// Where a deep map's last run is traced: `<db>.last-run.jsonl`.
pub fn last_run(db: &str) -> String {
    format!("{db}{LAST_RUN}")
}

/// ONE WRITE of a whole line, in append mode: builds running side by side append to the same file, and a line
/// written in pieces could interleave with theirs.
fn append(path: &str, line: &str) -> std::io::Result<()> {
    use std::io::Write;
    if let Some(folder) = std::path::Path::new(path).parent().filter(|f| !f.as_os_str().is_empty()) {
        std::fs::create_dir_all(folder)?;
    }
    roll(path);
    std::fs::OpenOptions::new().create(true).append(true).open(path)?.write_all(line.as_bytes())
}

/// How big a trace grows before it ROLLS OVER: ~5 KB a run and two runs a turn is megabytes a day, and the file
/// a machine-wide `STRUCTUREGATE_TRACE` names (`Update-Gate.ps1 -Trace`) was never cut. `STRUCTUREGATE_TRACE_MAX_BYTES`
/// moves the limit.
const ROLL_AT: u64 = 20 * 1024 * 1024;

/// A trace past its limit becomes `<name>.1<ext>` - replacing the one before - and the run starts the file again, so
/// at most twice the limit is ever on disk. Two runs rolling at once: the second finds nothing to move, and writes on.
fn roll(path: &str) {
    let limit = std::env::var("STRUCTUREGATE_TRACE_MAX_BYTES").ok().and_then(|n| n.parse::<u64>().ok()).unwrap_or(ROLL_AT);
    if std::fs::metadata(path).map_or(0, |m| m.len()) < limit {
        return;
    }
    let _ = std::fs::rename(path, rolled(path));
}

/// `trace.jsonl` -> `trace.1.jsonl`; a name with no extension gets `.1`.
pub(crate) fn rolled(path: &str) -> std::path::PathBuf {
    let at = std::path::Path::new(path);
    let stem = at.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let name = match at.extension() {
        Some(ext) => format!("{stem}.1.{}", ext.to_string_lossy()),
        None => format!("{stem}.1"),
    };
    at.with_file_name(name)
}

fn with<T>(work: impl FnOnce(&mut Run) -> T) -> Option<T> {
    RUN.lock().ok()?.as_mut().map(work)
}

fn owned(pairs: Vec<(&str, Value)>) -> Vec<(String, Value)> {
    pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect()
}
