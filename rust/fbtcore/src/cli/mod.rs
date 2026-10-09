//! THE WHOLE COMMAND, from the arguments to the exit code: which question this run asks - a diagnostic, a
//! lens over the deep map, the map, or the gate - and every answer. The caller is `Main` and the parsers
//! only .NET has (Roslyn, ScriptDom), handed in as callbacks.
//!
//! IT PRINTS NOTHING. Every line comes back tagged, and the caller writes it: .NET encodes the console in
//! its code page and MSBuild reads it that way, so a verdict written here as raw UTF-8 would reach a build
//! log with its em dashes mangled. A diagnostic or a lens asks for UTF-8 instead (`utf8`), because what it
//! prints is a path or a translation key a person or a script reads back.

mod hook;
mod lens;
mod options;
mod probe;

use crate::gate::run::CsFile;
use crate::mapper::{CsMap, Deep, Free};
use crate::{in_string, out_string};
use options::{Options, Parsed};
use serde::Deserialize;
use serde_json::{json, Value};
use std::ffi::c_char;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;

/// What the caller hands over: the arguments, and the two things only it knows - its version and its command
/// line verbatim, which together key the gate's pass cache.
#[derive(Deserialize, Default)]
#[serde(default)]
struct Call {
    argv: Vec<String>,
    command_line: String,
    version: String,
    /// Where the exe is (.NET's `AppContext.BaseDirectory`) - a managed run's process is `dotnet`.
    base_dir: String,
    /// Which build is running: the exe (and a managed run's dll and fbtcore beside it) by size and time.
    build: String,
}

/// The lines to print, in order: `["o", line]`, `["e", line]`, `["w", text]` (no newline) and
/// `["j", value, newline]` (indented JSON).
#[derive(Default)]
pub struct Out(Vec<Value>);

impl Out {
    pub fn line(&mut self, text: String) {
        self.0.push(json!(["o", text]));
    }
    pub fn error(&mut self, text: String) {
        self.0.push(json!(["e", text]));
    }
    pub fn write(&mut self, text: String) {
        self.0.push(json!(["w", text]));
    }
    pub fn json(&mut self, value: Value, newline: bool) {
        self.0.push(json!(["j", value, newline]));
    }
    /// A gate's or a map's own tagged lines, appended.
    fn extend(&mut self, lines: &Value) {
        self.0.extend(lines.as_array().into_iter().flatten().cloned());
    }
}

/// `{out: [...], exit, utf8}`.
///
/// # Safety
/// `input_json` must be null or NUL-terminated UTF-8, and every callback must live for the whole call. The
/// result is the caller's to free with `fbt_string_free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fbt_main(input_json: *const c_char, cs_file: CsFile, cs_map: CsMap, deep: Deep, free: Free) -> *mut c_char {
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        let text = unsafe { in_string(input_json) }.unwrap_or_default();
        let call: Call = serde_json::from_str(&text).unwrap_or_default();
        main(&call, Callbacks { cs_file, cs_map, deep, free })
    }));
    out_string(match outcome {
        Ok(answer) => answer.to_string(),
        Err(_) => json!({ "out": [["e", "structuregate: the run panicked"]], "exit": 2, "utf8": false }).to_string(),
    })
}

struct Callbacks {
    cs_file: CsFile,
    cs_map: CsMap,
    deep: Deep,
    free: Free,
}

fn main(call: &Call, callbacks: Callbacks) -> Value {
    crate::embedded::sweep();
    let mut answer = asked(call, callbacks);
    // THE TRACE ENDS WITH THE EXIT CODE, and a trace that could not be written is a note - never the run's fault.
    if let Some(note) = crate::trace::finish(answer["exit"].as_i64().unwrap_or(2))
        && let Some(lines) = answer["out"].as_array_mut()
    {
        lines.push(json!(["e", note]));
    }
    answer
}

fn asked(call: &Call, callbacks: Callbacks) -> Value {
    let mut out = Out::default();
    let o = match options::parse(&call.argv) {
        Ok(Parsed::Run(o)) => o,
        Ok(Parsed::Help) => {
            out.line(options::USAGE.into());
            return answer(out, 0, false);
        }
        Err(why) => {
            out.error(format!("structuregate: {why}"));
            out.error(String::new());
            out.error(options::USAGE.into());
            return answer(out, 2, false);
        }
    };
    // A HOOK'S STDOUT IS JSON Claude Code parses, and nothing else may be written there.
    if let Some(event) = &o.claude_hook {
        let exit = hook::run(event, &mut out);
        return answer(out, exit, true);
    }
    if let Some(file) = &o.trace_report {
        let exit = crate::trace::report::run(file, &mut out);
        return answer(out, exit, true);
    }
    begin_trace(&o, call);
    if let Some(db) = &o.map_view {
        let exit = crate::view::run(db, o.map_view_out.as_deref(), o.map_view_graph.as_deref(), &mut out);
        return answer(out, exit, true);
    }
    if let Some(exit) = probe::run(&o, &mut out) {
        return answer(out, exit, true);
    }
    if o.facts_pull {
        let exit = facts_pull(&o, call, &mut out);
        return answer(out, exit, true);
    }
    if let Some(db) = &o.map_query {
        let exit = lens::run(db, &o.map_query_args, &mut out);
        return answer(out, exit, true);
    }
    // `--map-sqlite` on its own is the DEEP map without the file-level one - what a per-turn hook runs.
    let reply = if o.map || o.map_sqlite.is_some() {
        crate::mapper::run_json(map_input(&o, call), callbacks.cs_map, callbacks.deep, callbacks.free)
    } else {
        crate::gate::run::run_json(gate_input(&o, call), callbacks.cs_file, callbacks.free)
    };
    if let Some(why) = reply["error"].as_str() {
        out.error(format!("structuregate: {why}"));
        return answer(out, 2, false);
    }
    out.extend(&reply["out"]);
    // --dump's stdout is JSON another gate parses, printed indented as it always was.
    if let Some(dump) = reply.get("dump") {
        out.json(dump.clone(), false);
    }
    answer(out, reply["exit"].as_i64().unwrap_or(2), false)
}

/// The run's root span: what ran, over which tree, asked what.
fn begin_trace(o: &Options, call: &Call) {
    let mode = if o.map_query.is_some() {
        "query"
    } else if o.map {
        "map"
    } else if o.map_sqlite.is_some() {
        "deep"
    } else {
        "gate"
    };
    // `HOSTNAME` is a shell variable, rarely exported: off Windows the name is read where the system keeps it.
    let host = if cfg!(windows) { std::env::var("COMPUTERNAME").unwrap_or_default() } else { std::fs::read_to_string("/etc/hostname").unwrap_or_default() };
    let host = host.trim();
    // A DEEP MAP KEEPS ITS LAST RUN'S TRACE beside its database, asked for or not - a query only reads one.
    let last = o.map_sqlite.as_deref().filter(|_| o.map_query.is_none()).map(crate::trace::last_run);
    crate::trace::begin(
        o.trace.as_deref(),
        last,
        o.trace_detail,
        vec![("service.version", json!(call.version)), ("structuregate.build", json!(call.build)), ("host.name", json!(host)),
            ("os.type", json!(std::env::consts::OS)), ("process.pid", json!(std::process::id()))],
        vec![("structuregate.root", json!(o.roots[0])), ("structuregate.roots", json!(o.roots)), ("structuregate.mode", json!(mode)),
            ("process.command_args", json!(call.argv))],
    );
}

/// `--facts-pull`: 0 when every document is saved and current, 1 when one failed, 2 when the config is wrong.
fn facts_pull(o: &Options, call: &Call, out: &mut Out) -> i64 {
    let config = match crate::facts::config::read(o.facts_config.as_deref(), &call.base_dir) {
        Ok(Some(config)) => config,
        Ok(None) => {
            out.error(format!("structuregate: --facts-pull needs a facts config: {} beside the exe, or --facts-config <file>", crate::facts::config::FILE_NAME));
            return 2;
        }
        Err(why) => {
            out.error(format!("structuregate: the facts config is wrong - {why}"));
            return 2;
        }
    };
    out.line(format!("facts pull: {} source(s) in {}", config.sources.len(), config.path.display()));
    let mut lines = Vec::new();
    let exit = crate::facts::pull::run(&config, &mut lines);
    for line in lines {
        out.line(line);
    }
    exit
}

fn answer(out: Out, exit: i64, utf8: bool) -> Value {
    json!({ "out": out.0, "exit": exit, "utf8": utf8 })
}

fn gate_input(o: &Options, call: &Call) -> Value {
    json!({
        "roots": o.roots, "extensions": o.extensions, "skip": o.skip, "skip_files": o.skip_files, "tracked": o.tracked,
        "include_untracked": o.include_untracked, "context_docs_only": o.context_docs_only, "doc_skip": o.doc_skip,
        "ps_discipline": o.ps_discipline, "ts_discipline": o.ts_discipline, "async_discipline": o.async_discipline,
        "ps_host": o.ps_host, "ts_host": o.ts_host,
        "max_source_lines": o.max_source_lines, "max_files_per_dir": o.max_files_per_dir, "max_doc_lines": o.max_doc_lines,
        "baseline_path": o.baseline, "strict": o.strict, "update_baseline": o.update_baseline,
        "dump": o.dump, "worst": o.worst, "plugins": o.plugins, "no_gate_cache": o.no_gate_cache,
        "version": call.version, "build": call.build, "arguments": call.command_line,
    })
}

fn map_input(o: &Options, call: &Call) -> Value {
    // Beside the tree it describes, under the name the tool it came from is called by.
    let map_out = o.map_out.clone().unwrap_or_else(|| Path::new(&o.roots[0]).join("buildmap.json").to_string_lossy().into_owned());
    json!({
        "roots": o.roots, "extensions": o.extensions, "skip": o.skip, "tracked": o.tracked,
        "include_untracked": o.include_untracked, "context_docs_only": o.context_docs_only, "doc_skip": o.doc_skip,
        "doc_root": o.doc_root,
        "map": o.map, "map_sqlite": o.map_sqlite, "map_out": map_out, "map_if_stale": o.map_if_stale,
        "map_check": o.map_check, "baseline_path": o.map_baseline, "update_baseline": o.update_map_baseline,
        "map_plugins": o.map_plugins, "ts_host": o.ts_host, "py_host": o.py_host, "ps_host": o.ps_host,
        "ts_node_modules": o.ts_node_modules, "ts_config": o.ts_config, "ts_html": o.ts_html,
        "map_row_fts": o.map_row_fts, "map_atlas_dir": o.map_atlas_dir, "map_exclude": o.map_exclude, "map_reread": o.map_reread, "sql_config": o.sql_config, "facts_config": o.facts_config, "base_dir": call.base_dir, "build": call.build,
    })
}
