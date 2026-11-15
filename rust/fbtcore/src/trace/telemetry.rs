//! A RUN AS TELEMETRY-AGENT RECORDS - the optional sink `STRUCTUREGATE_TELEMETRY` switches on. At exit the run writes
//! ONE NDJSON file into `<root>/spool/` (session start, machine, the run's event, its stage timings as one metric
//! record, a crash when it panicked or exited 2, session end) and starts `telemetry-agent` with `TELEMETRY_ROOT=<root>`,
//! which sweeps the spool and ships it to the HTTP endpoint or Drive folder configured in that root. This process opens
//! no socket: it writes a file and starts a process, as it always did for its hosts.
//!
//! The record shapes are telemetry-agent's (`crates/proto`): a metric's `d` is `{stats: [{name, kind, n, sum, min, max,
//! mean}]}`, a crash's `{message, location, thread, backtrace, uptime_s}`.
//!
//! WHAT LEAVES THE MACHINE IS THE RUN, NEVER THE TREE: the gate runs inside private trees, so no record carries a path,
//! a file name, the command line, the host name or a project's name - stage names (`public_name` folds the few that
//! name something of the tree), times, counts, the mode, the exit code. A panic's message keeps its words with every
//! path in it replaced (`scrub`); its backtrace is this exe's own frames, paths of the machine that BUILT it.
//!
//! THE SWEEP EATS EVERY `*.ndjson` AT ONCE, with no lock: the file is written as `<name>.ndjson.part` and renamed, and
//! named `<epoch ms, 13 digits>-<pid>-<seq>` so name order is time order. One file at EXIT: a killed run leaves none,
//! and never shows as a death the agent would reap.

use super::sink::{Finished, Sink};
use super::Span;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Mutex, Once};

/// Switches the sink on: `1`/`on`/`true` = the default root (`default_root`), any other value = the root itself.
/// Unset, empty, `0`, `off` or `false`: nothing is written, nothing is started.
pub const ENV: &str = "STRUCTUREGATE_TELEMETRY";
/// The agent to start: a path, or `0`/`off`/`none` for none (an agent already running as a service). Unset:
/// `telemetry-agent` on PATH, and none found = none started.
pub const AGENT_ENV: &str = "STRUCTUREGATE_TELEMETRY_AGENT";
/// Frame names that are this sink catching a panic: cut off the top of the backtrace, or every panic groups as one.
const OWN_FRAMES: &str = "trace::telemetry";
/// Spans whose name carries a name of the TREE after this prefix: counted under the prefix alone.
const NAMED_AFTER_THE_TREE: [&str; 2] = ["csharp project: ", "deep: hash "];

static SEQ: AtomicU32 = AtomicU32::new(0);
static PANIC: Mutex<Option<Value>> = Mutex::new(None);
/// The run ENDED in a panic (`trace::finish_panicked`): only then is the kept panic its crash - a parser's panic
/// caught and answered as a failure is not the run's.
static ESCAPED: AtomicBool = AtomicBool::new(false);
static HOOKED: Once = Once::new();

/// Where this run's telemetry goes, if anywhere.
pub(super) fn root() -> Option<PathBuf> {
    let asked = std::env::var(ENV).unwrap_or_default();
    match asked.trim() {
        "" | "0" | "off" | "false" => None,
        "1" | "on" | "true" => default_root(),
        root => Some(PathBuf::from(root)),
    }
}

/// telemetry-agent's own convention for an app's root: `%LOCALAPPDATA%\structuregate\telemetry` on Windows,
/// `$XDG_DATA_HOME/structuregate/telemetry` (`~/.local/share/..` without it) elsewhere.
fn default_root() -> Option<PathBuf> {
    let set = |name: &str| std::env::var(name).ok().filter(|v| !v.trim().is_empty()).map(PathBuf::from);
    let data = if cfg!(windows) { set("LOCALAPPDATA") } else { set("XDG_DATA_HOME").or_else(|| set("HOME").map(|h| h.join(".local").join("share"))) };
    data.map(|d| d.join("structuregate").join("telemetry"))
}

/// A panic is caught where the run ends (`fbt_main`), with nothing of where it happened: the hook keeps its message,
/// place and backtrace for the crash record, and hands on to the hook before it.
pub(super) fn catch_panics() {
    HOOKED.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            keep_panic(info);
            previous(info);
        }));
    });
}

/// The run ended in the panic the hook kept last.
pub(super) fn escaped() {
    ESCAPED.store(true, Ordering::Relaxed);
}

fn keep_panic(info: &std::panic::PanicHookInfo) {
    let payload = info.payload();
    let message = payload.downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| payload.downcast_ref::<String>().cloned()).unwrap_or_default();
    let location = info.location().map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column())).unwrap_or_default();
    let thread = std::thread::current().name().unwrap_or("unnamed").to_string();
    let backtrace = without_own_frames(&std::backtrace::Backtrace::force_capture().to_string());
    if let Ok(mut slot) = PANIC.lock() {
        *slot = Some(json!({ "message": scrub(&message), "location": location, "thread": thread, "backtrace": backtrace }));
    }
}

/// The backtrace from the first frame AFTER the last one of this sink's hook.
fn without_own_frames(backtrace: &str) -> String {
    let lines: Vec<&str> = backtrace.lines().collect();
    let frame = |line: &str| line.trim_start().split_once(": ").is_some_and(|(n, _)| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()));
    let Some(own) = lines.iter().rposition(|l| frame(l) && l.contains(OWN_FRAMES)) else { return backtrace.to_string() };
    let from = lines.iter().skip(own + 1).position(|l| frame(l)).map_or(lines.len(), |at| own + 1 + at);
    lines[from..].join("\n")
}

/// A message's words, every one that holds a path separator replaced by `<path>`.
pub(super) fn scrub(message: &str) -> String {
    message.split_whitespace().map(|word| if word.contains('/') || word.contains('\\') { "<path>" } else { word }).collect::<Vec<_>>().join(" ")
}

/// A span's name as telemetry counts it, or `None` for a span that is not a stage: the run, a single file, a metric.
pub(super) fn public_name(span: &Span) -> Option<String> {
    if span.parent.is_none() || span.name.starts_with("metric") || span.attributes.iter().any(|(k, _)| k == "code.filepath") {
        return None;
    }
    let named = NAMED_AFTER_THE_TREE.iter().find(|prefix| span.name.starts_with(**prefix));
    Some(named.map_or_else(|| span.name.clone(), |prefix| prefix.trim_end_matches([' ', ':']).to_string()))
}

/// The sink: the spool file, then the agent.
pub(super) struct Agent(pub PathBuf);

impl Sink for Agent {
    fn write(&self, run: &Finished) -> Result<(), String> {
        let spool = self.0.join("spool");
        let written = std::fs::create_dir_all(&spool)
            .and_then(|_| install_id(&self.0))
            .and_then(|install| spool_file(&spool, run, &records(run, &install)));
        if let Err(why) = written {
            return Err(format!("NOTE: telemetry was not written to {} ({why})", spool.display()));
        }
        start_agent(&self.0).map_err(|why| format!("NOTE: telemetry was written, but telemetry-agent did not start ({why})"))
    }
}

/// This root's install id - a random UUID made once and kept in `<root>/install_id`, never derived from the host or
/// the user. Two runs making it at once: the rename is atomic, and the loser reads the winner's.
fn install_id(root: &Path) -> std::io::Result<String> {
    let file = root.join("install_id");
    if let Ok(kept) = std::fs::read_to_string(&file)
        && !kept.trim().is_empty()
    {
        return Ok(kept.trim().to_string());
    }
    let id = random_uuid();
    let part = root.join(format!("install_id.{}.part", std::process::id()));
    std::fs::write(&part, &id)?;
    std::fs::rename(&part, &file)?;
    Ok(std::fs::read_to_string(&file).map(|kept| kept.trim().to_string()).unwrap_or(id))
}

/// A version 4 UUID from the OS-seeded keys of two `RandomState`s - no crate for 16 random bytes.
fn random_uuid() -> String {
    use std::hash::{BuildHasher, Hasher};
    let half = || {
        let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
        hasher.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos()));
        hasher.finish()
    };
    let bits = (u128::from(half()) << 64 | u128::from(half())) & !(0xf000 << 64) & !(0xc000_0000_0000_0000) | (0x4000 << 64) | 0x8000_0000_0000_0000;
    let hex = format!("{bits:032x}");
    format!("{}-{}-{}-{}-{}", &hex[..8], &hex[8..12], &hex[12..16], &hex[16..20], &hex[20..])
}

/// The run's records, one per line.
pub(super) fn records(run: &Finished, install: &str) -> String {
    let ms = |ns: u128| (ns / 1_000_000) as u64;
    let version = run.resource("service.version").cloned().unwrap_or(Value::from(""));
    let mode = run.fact("structuregate.mode").and_then(Value::as_str).unwrap_or("").to_string();
    let uptime_s = (run.end_ns.saturating_sub(run.started_ns())) as f64 / 1e9;
    let record = |t: u128, k: &str, n: &str, d: Value| json!({ "t": ms(t), "s": run.trace_id, "i": install, "v": version, "k": k, "n": n, "d": d }).to_string();
    let mut lines = vec![
        record(run.started_ns(), "session", "start", json!({ "pid": std::process::id() })),
        record(run.started_ns(), "machine", "machine", json!({ "os": std::env::consts::OS, "arch": std::env::consts::ARCH,
            "cpus": std::thread::available_parallelism().map_or(0, |n| n.get()) })),
        record(run.end_ns, "event", "run", json!({ "mode": mode, "exit_code": run.exit,
            "build": run.resource("structuregate.build").cloned().unwrap_or(Value::Null),
            "detail": run.fact("structuregate.trace.detail").cloned().unwrap_or(Value::Bool(false)) })),
        record(run.end_ns, "metric", "run", json!({ "stats": stats(run) })),
    ];
    let panicked = ESCAPED.swap(false, Ordering::Relaxed).then(|| PANIC.lock().ok().and_then(|mut slot| slot.take())).flatten();
    if let Some(mut crash) = panicked {
        crash["uptime_s"] = json!(uptime_s);
        lines.push(record(run.end_ns, "crash", "panic", crash));
    } else if run.exit == 2 {
        let crash = json!({ "message": format!("a {mode} run exited 2"), "location": "", "thread": "main", "backtrace": "", "uptime_s": uptime_s });
        lines.push(record(run.end_ns, "crash", "exit_2", crash));
    }
    lines.push(record(run.end_ns, "session", "end", json!({ "reason": "exit", "exit_code": run.exit, "uptime_s": uptime_s })));
    lines.join("\n") + "\n"
}

/// The run's time and every stage's, in milliseconds: one series per stage NAME, a name met twice counted twice.
fn stats(run: &Finished) -> Vec<Value> {
    let ms = |span: &Span| (span.end_ns.max(span.start_ns) - span.start_ns) as f64 / 1e6;
    let mut series: BTreeMap<String, (u64, f64, f64, f64)> = BTreeMap::new();
    for span in run.spans {
        if let Some(name) = public_name(span) {
            let took = ms(span);
            let entry = series.entry(format!("stage.{name}")).or_insert((0, 0.0, f64::MAX, 0.0));
            *entry = (entry.0 + 1, entry.1 + took, entry.2.min(took), entry.3.max(took));
        }
    }
    let one = |name: &str, (n, sum, min, max): (u64, f64, f64, f64)| {
        json!({ "name": name, "kind": "timing", "n": n, "sum": sum, "min": min, "max": max, "mean": sum / n as f64 })
    };
    let took = run.spans.first().map_or(0.0, ms);
    let mut out = vec![one("run", (1, took, took, took))];
    out.extend(series.into_iter().map(|(name, s)| one(&name, s)));
    out
}

/// Written as `.part`, renamed into the spool whole.
fn spool_file(spool: &Path, run: &Finished, text: &str) -> std::io::Result<()> {
    let name = format!("{:013}-{}-{}.ndjson", run.end_ns / 1_000_000, std::process::id(), SEQ.fetch_add(1, Ordering::Relaxed));
    let part = spool.join(format!("{name}.part"));
    std::fs::write(&part, text)?;
    std::fs::rename(&part, spool.join(name))
}

/// Starts the agent DETACHED and does not wait: one holding `<root>/agent.lock` already makes the new one exit at
/// once, so a burst of runs costs one agent. Never `--once`, which skips the lock and would ship a batch twice.
fn start_agent(root: &Path) -> Result<(), String> {
    let asked = std::env::var(AGENT_ENV).unwrap_or_default();
    let exe = match asked.trim() {
        "0" | "off" | "none" => return Ok(()),
        "" => match on_path() {
            Some(found) => found,
            None => return Ok(()),
        },
        given => PathBuf::from(given),
    };
    let mut command = std::process::Command::new(&exe);
    command.env("TELEMETRY_ROOT", root).current_dir(root);
    command.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW: MSBuild must not wait on it, nor Ctrl+C reach it.
        command.creation_flags(0x0000_0008 | 0x0000_0200 | 0x0800_0000);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    command.spawn().map(drop).map_err(|why| format!("{}: {why}", exe.display()))
}

fn on_path() -> Option<PathBuf> {
    let name = if cfg!(windows) { "telemetry-agent.exe" } else { "telemetry-agent" };
    std::env::split_paths(&std::env::var_os("PATH")?).map(|dir| dir.join(name)).find(|exe| exe.is_file())
}

#[cfg(test)]
#[path = "telemetry_tests.rs"]
mod tests;
