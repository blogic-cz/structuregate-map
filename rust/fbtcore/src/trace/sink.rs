//! WHERE A FINISHED RUN GOES - the provider interface. `finish` hands the run, once, to every sink this run switched
//! on: the OTLP line appended to `STRUCTUREGATE_TRACE`, the OTLP line that REPLACES a deep map's last run, the records
//! a telemetry-agent spool takes (`telemetry.rs`). A new backend is one more `Sink` and its switch in `finish` - the
//! spans are recorded once, whatever reads them.
//!
//! A SINK NEVER FAILS THE RUN: what it could not write comes back as the `NOTE:` the run prints beside its own answer.

use super::{otlp, Span};
use serde_json::Value;
use std::cell::OnceCell;

/// One run, ended: `spans[0]` is the run itself, every other span a stage, a file or a metric under it.
pub(super) struct Finished<'a> {
    pub trace_id: &'a str,
    pub resource: &'a [(String, Value)],
    pub spans: &'a [Span],
    pub exit: i64,
    pub end_ns: u128,
    /// The OTLP line, built only when a sink asks for it and only once.
    line: OnceCell<String>,
}

impl<'a> Finished<'a> {
    pub fn new(trace_id: &'a str, resource: &'a [(String, Value)], spans: &'a [Span], exit: i64, end_ns: u128) -> Self {
        Finished { trace_id, resource, spans, exit, end_ns, line: OnceCell::new() }
    }

    /// The run as one OTLP/JSON line, newline included.
    pub fn otlp_line(&self) -> &str {
        self.line.get_or_init(|| otlp::encode(self.trace_id, self.resource, self.spans, self.exit).to_string() + "\n")
    }

    /// A fact of the run's own span (`structuregate.mode`, ..).
    pub fn fact(&self, key: &str) -> Option<&Value> {
        self.spans.first()?.attributes.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    /// A fact of what ran (`service.version`, ..).
    pub fn resource(&self, key: &str) -> Option<&Value> {
        self.resource.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    pub fn started_ns(&self) -> u128 {
        self.spans.first().map_or(self.end_ns, |run| run.start_ns)
    }
}

/// A place a finished run is written to. `Err` is the whole note the run prints.
pub(super) trait Sink {
    fn write(&self, run: &Finished) -> Result<(), String>;
}

/// The OTLP line APPENDED to a trace file (`--trace`, `STRUCTUREGATE_TRACE`), rolled over past its limit.
pub(super) struct Append(pub String);

impl Sink for Append {
    fn write(&self, run: &Finished) -> Result<(), String> {
        append(&self.0, run.otlp_line()).map_err(|why| format!("NOTE: the trace was not written to {} ({why})", self.0))
    }
}

/// The OTLP line that REPLACES a deep map's `<db>.last-run.jsonl`: it is that one run, and stays one line's size.
pub(super) struct Replace(pub String);

impl Sink for Replace {
    fn write(&self, run: &Finished) -> Result<(), String> {
        std::fs::write(&self.0, run.otlp_line()).map_err(|why| format!("NOTE: the last run's trace was not written to {} ({why})", self.0))
    }
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
