//! THE COMMAND LINE: every flag, its default, and what is refused. Defaults are the ones the tools are held
//! to. A flag that cannot do anything on this run is REFUSED, never ignored: a hook whose `--map-check` was
//! silently dropped reports OK on a check it never made.

use std::path::Path;

pub const USAGE: &str = include_str!("usage.txt");

/// What `--ps-discipline` and `--ts-discipline` MEASURE, added to `--ext` by the flag itself: a rule
/// switched on over a file set that holds none of its files is the one failure this gate refuses to have.
const POWERSHELL: [&str; 3] = [".ps1", ".psm1", ".psd1"];
const TYPESCRIPT: [&str; 4] = [".ts", ".tsx", ".mts", ".cts"];

/// Every flag, parsed. The field names are the ones the gate and the map read their input by.
#[derive(Default)]
pub struct Options {
    pub roots: Vec<String>,
    pub max_source_lines: i64,
    pub max_files_per_dir: i64,
    pub max_doc_lines: i64,
    pub extensions: Vec<String>,
    pub skip: Vec<String>,
    /// `--skip-file`, repeatable: globs over a file's path from its root that the GATE does not measure.
    pub skip_files: Vec<String>,
    /// `--doc-skip <glob>`, repeatable: a `.md` that is DATA, not a doc - the gate does not measure it and the map
    /// does not map it.
    pub doc_skip: Vec<String>,
    /// `--doc-root <dir>`: the map's docs come from here, not from the code roots.
    pub doc_root: Option<String>,
    pub context_docs_only: bool,
    pub async_discipline: bool,
    pub ps_discipline: bool,
    pub ps_host: String,
    pub ts_discipline: bool,
    pub ts_host: String,
    pub ts_node_modules: Vec<String>,
    pub ts_config: Option<String>,
    pub sql_config: Option<String>,
    /// `--facts-config`: `structuregate.facts.json`, the documents whose facts the deep map checks the code against.
    pub facts_config: Option<String>,
    /// `--facts-pull`: save each document of the facts config as its snapshot, and do nothing else.
    pub facts_pull: bool,
    pub ts_html: Option<String>,
    pub map_row_fts: bool,
    pub map_atlas_dir: Option<String>,
    /// `--map-exclude`, repeatable: C# files the deep map lists and never walks.
    pub map_exclude: Vec<String>,
    /// `--map-reread <halves>`: the deep halves that read every file again this run, whatever they recorded.
    pub map_reread: Vec<String>,
    pub map: bool,
    pub map_out: Option<String>,
    pub map_check: bool,
    pub map_if_stale: bool,
    pub py_host: String,
    pub map_plugins: Vec<String>,
    pub map_sqlite: Option<String>,
    pub map_query: Option<String>,
    pub map_query_args: Vec<String>,
    /// `--trace <file>`: what `STRUCTUREGATE_TRACE` names, on the command line; `--trace-detail` asks for every span.
    pub trace: Option<String>,
    pub trace_detail: bool,
    /// `--trace-report`: the trace file `STRUCTUREGATE_TRACE` wrote, read back.
    pub trace_report: Option<String>,
    /// `--claude-hook <event>`: answer a Claude Code hook (`session-start`, `pre-tool-use`) from stdin.
    pub claude_hook: Option<String>,
    /// `--map-view`: the deep map's database, drawn as one HTML page; `--map-view-out` names the page.
    pub map_view: Option<String>,
    pub map_view_out: Option<String>,
    pub map_view_graph: Option<String>,
    pub map_baseline: Option<String>,
    pub update_map_baseline: bool,
    pub tracked: bool,
    pub include_untracked: bool,
    pub baseline: Option<String>,
    pub update_baseline: bool,
    pub strict: bool,
    pub worst: bool,
    pub dump: bool,
    pub no_gate_cache: bool,
    pub plugins: Vec<String>,
    pub probe: Probe,
}

/// The diagnostics: each asks one piece of the store directly and prints what it says. They exist because
/// the suites are black box over this CLI, and a pass nobody can call on its own cannot be tested on its own.
#[derive(Default)]
pub struct Probe {
    pub scan: Option<String>,
    pub dir_hashes: bool,
    pub no_store: bool,
    pub rows: Option<String>,
    pub rows_file: Option<String>,
    pub rows_lang: String,
    pub rows_whole: bool,
    pub rows_carry: Option<String>,
    pub rows_deps: Option<String>,
    pub rows_affected: Option<String>,
    pub rows_fts: Option<String>,
    pub rows_fts_json: bool,
    pub map_atlas: Option<String>,
    pub map_atlas_dir: Option<String>,
    pub ts_apply: Option<String>,
    pub ts_payload: Option<String>,
    pub ts_state: Option<String>,
    pub ts_routes: Option<String>,
    pub ts_litgates: Option<String>,
    pub ts_gates: Option<String>,
    pub ts_values: Option<String>,
    pub ts_paths: Option<String>,
    pub ts_reach: Option<String>,
    pub ts_root: String,
    pub ts_checks: String,
    pub ts_enum: String,
}

pub enum Parsed {
    Run(Box<Options>),
    Help,
}

/// The arguments, parsed - or the one line that says what was wrong with them.
pub fn parse(args: &[String]) -> Result<Parsed, String> {
    let mut o = Options {
        max_source_lines: 500,
        max_files_per_dir: 15,
        max_doc_lines: 200,
        extensions: [".cs", ".mjs", ".js", ".ts"].map(String::from).to_vec(),
        skip: ["out", "bin", "obj", "dist", "node_modules", ".git", ".vs", ".venv", "__pycache__"]
            .map(String::from)
            .to_vec(),
        // Windows PowerShell and `python` on Windows; elsewhere PowerShell is `pwsh` and python is `python3`.
        ps_host: if cfg!(windows) { "powershell.exe" } else { "pwsh" }.into(),
        ts_host: "node".into(),
        py_host: if cfg!(windows) { "python" } else { "python3" }.into(),
        probe: Probe { rows_lang: "csharp".into(), ..Default::default() },
        ..Default::default()
    };
    let mut i = 0;
    while i < args.len() {
        let flag = args[i].as_str();
        let mut next = || -> Result<String, String> {
            i += 1;
            args.get(i).cloned().ok_or_else(|| format!("{flag} needs a value"))
        };
        match flag {
            "--root" => o.roots.push(full(&next()?)),
            "--max-lines" => o.max_source_lines = number(flag, &next()?)?,
            "--max-files" => o.max_files_per_dir = number(flag, &next()?)?,
            "--max-doc-lines" => o.max_doc_lines = number(flag, &next()?)?,
            "--ext" => o.extensions = list(&next()?),
            "--skip" => o.skip = list(&next()?),
            "--skip-file" => o.skip_files.push(next()?),
            "--doc-skip" => o.doc_skip.push(next()?),
            "--doc-root" => o.doc_root = Some(full(&next()?)),
            "--doc-scope" => {
                o.context_docs_only = match next()?.as_str() {
                    "context" => true,
                    "all" => false,
                    other => return Err(format!("--doc-scope takes `context` or `all`, not `{other}`")),
                }
            }
            "--async-discipline" => o.async_discipline = true,
            "--ps-discipline" => o.ps_discipline = true,
            "--ps-host" => o.ps_host = next()?,
            "--ts-discipline" => o.ts_discipline = true,
            "--ts-host" => o.ts_host = next()?,
            "--ts-node-modules" => o.ts_node_modules.push(full(&next()?)),
            "--ts-config" => o.ts_config = Some(full(&next()?)),
            "--sql-config" => o.sql_config = Some(full(&next()?)),
            "--facts-config" => o.facts_config = Some(full(&next()?)),
            "--facts-pull" => o.facts_pull = true,
            "--ts-html" => o.ts_html = Some(full(&next()?)),
            "--map-row-fts" => o.map_row_fts = true,
            "--map-atlas" => o.map_atlas_dir = Some(full(&next()?)),
            "--map-exclude" => o.map_exclude.push(next()?),
            "--map-reread" => o.map_reread.extend(list(&next()?).into_iter().map(|h| h.to_lowercase())),
            "--tracked" => o.tracked = true,
            "--include-untracked" => {
                o.tracked = true;
                o.include_untracked = true;
            }
            "--baseline" => o.baseline = Some(full(&next()?)),
            "--update-baseline" => o.update_baseline = true,
            "--strict" => o.strict = true,
            "--worst" => o.worst = true,
            "--dump" => o.dump = true,
            "--map" => o.map = true,
            "--map-out" => o.map_out = Some(full(&next()?)),
            "--map-check" => o.map_check = true,
            "--map-if-stale" => o.map_if_stale = true,
            "--map-plugin" => o.map_plugins.push(next()?),
            "--map-sqlite" => o.map_sqlite = Some(full(&next()?)),
            "--fbt-scan" => o.probe.scan = Some(full(&next()?)),
            "--fbt-dir-hashes" => o.probe.dir_hashes = true,
            "--fbt-no-store" => o.probe.no_store = true,
            "--fbt-rows" => o.probe.rows = Some(full(&next()?)),
            "--fbt-rows-file" => o.probe.rows_file = Some(full(&next()?)),
            "--fbt-rows-lang" => o.probe.rows_lang = next()?,
            "--fbt-rows-whole" => o.probe.rows_whole = true,
            "--fbt-rows-carry" => o.probe.rows_carry = Some(full(&next()?)),
            "--fbt-rows-deps" => o.probe.rows_deps = Some(full(&next()?)),
            "--fbt-rows-fts" => o.probe.rows_fts = Some(full(&next()?)),
            "--no-gate-cache" => o.no_gate_cache = true,
            "--fbt-map-atlas" => o.probe.map_atlas = Some(full(&next()?)),
            "--fbt-map-atlas-dir" => o.probe.map_atlas_dir = Some(full(&next()?)),
            "--fbt-rows-fts-json" => o.probe.rows_fts_json = true,
            "--fbt-ts-apply" => o.probe.ts_apply = Some(full(&next()?)),
            "--fbt-ts-payload" => o.probe.ts_payload = Some(full(&next()?)),
            "--fbt-ts-state" => o.probe.ts_state = Some(full(&next()?)),
            "--fbt-rows-affected" => o.probe.rows_affected = Some(full(&next()?)),
            "--fbt-ts-routes" => o.probe.ts_routes = Some(full(&next()?)),
            "--fbt-ts-litgates" => o.probe.ts_litgates = Some(full(&next()?)),
            "--fbt-ts-gates" => o.probe.ts_gates = Some(full(&next()?)),
            "--fbt-ts-values" => o.probe.ts_values = Some(full(&next()?)),
            "--fbt-ts-paths" => o.probe.ts_paths = Some(full(&next()?)),
            "--fbt-ts-reach" => o.probe.ts_reach = Some(full(&next()?)),
            "--fbt-ts-root" => o.probe.ts_root = full(&next()?),
            "--fbt-ts-checks" => o.probe.ts_checks = next()?,
            "--fbt-ts-enum" => o.probe.ts_enum = next()?,
            "--map-query" => {
                o.map_query = Some(full(&next()?));
                // EVERYTHING AFTER IT BELONGS TO THE LENS: a lens flag this tool does not know is not an
                // error, it is the lens's business.
                o.map_query_args = args[i + 1..].to_vec();
                i = args.len();
            }
            "--trace-report" => o.trace_report = Some(full(&next()?)),
            "--claude-hook" => o.claude_hook = Some(next()?),
            "--trace" => o.trace = Some(full(&next()?)),
            "--trace-detail" => o.trace_detail = true,
            "--map-view" => o.map_view = Some(full(&next()?)),
            "--map-view-out" => o.map_view_out = Some(full(&next()?)),
            "--map-view-graph" => o.map_view_graph = Some(full(&next()?)),
            "--map-baseline" => o.map_baseline = Some(full(&next()?)),
            "--update-map-baseline" => o.update_map_baseline = true,
            "--py-host" => o.py_host = next()?,
            "--plugin" => o.plugins.push(next()?),
            "--help" | "-h" => return Ok(Parsed::Help),
            other => return Err(format!("unknown argument: {other}")),
        }
        i += 1;
    }
    if o.ps_discipline {
        add(&mut o.extensions, &POWERSHELL);
    }
    if o.ts_discipline {
        add(&mut o.extensions, &TYPESCRIPT);
    }
    if o.roots.is_empty() {
        o.roots.push(std::env::current_dir().map(|d| d.to_string_lossy().into_owned()).unwrap_or_default());
    }
    if let Some(root) = o.roots.iter().find(|r| !Path::new(r).is_dir()) {
        return Err(format!("--root does not exist: {root}"));
    }
    if let Some(root) = &o.doc_root {
        if !o.map {
            return Err("--doc-root belongs to --map: it says where the file map's docs are; the gate measures docs inside its --root".into());
        }
        if !Path::new(root).is_dir() {
            return Err(format!("--doc-root does not exist: {root}"));
        }
    }
    if o.update_baseline && o.baseline.is_none() {
        return Err("--update-baseline needs --baseline <file>".into());
    }
    // `--map-sqlite` is the exception: it builds a map of its own and needs nothing else.
    let map_only = o.map_out.is_some() || o.map_check || o.map_if_stale || !o.map_plugins.is_empty()
        || o.map_baseline.is_some() || o.update_map_baseline;
    if !o.map && map_only && o.map_query.is_none() {
        return Err("--map-out, --map-check, --map-if-stale, --map-plugin and the map baseline flags need --map — but --map-sqlite alone builds the deep map on its own".into());
    }
    // THE MAP KEEPS A SKIPPED FILE - other files import it - so on a map run the flag would do nothing.
    if !o.skip_files.is_empty() && (o.map || o.map_sqlite.is_some()) {
        return Err("--skip-file leaves a file out of the GATE only; a map keeps it, since other files import it".into());
    }
    // A HALF THAT CAN BE TOLD TO READ EVERYTHING AGAIN, and only with a deep map to read it into.
    if let Some(other) = o.map_reread.iter().find(|h| !["csharp", "sql", "typescript"].contains(&h.as_str())) {
        return Err(format!("--map-reread takes csharp, sql or typescript, not `{other}`"));
    }
    if !o.map_reread.is_empty() && o.map_sqlite.is_none() {
        return Err("--map-reread belongs to --map-sqlite: it re-reads a deep half".into());
    }
    // A DETAILED TRACE NEEDS A TRACE: without a file it would be dropped in silence.
    if o.trace_detail && o.trace.is_none() && std::env::var(crate::trace::ENV).unwrap_or_default().trim().is_empty() {
        return Err(format!("--trace-detail needs --trace <file> or {}", crate::trace::ENV));
    }
    if (o.map_view_out.is_some() || o.map_view_graph.is_some()) && o.map_view.is_none() {
        return Err("--map-view-out and --map-view-graph belong to --map-view <db>".into());
    }
    if o.update_map_baseline && o.map_baseline.is_none() {
        return Err("--update-map-baseline needs --map-baseline <file>".into());
    }
    Ok(Parsed::Run(Box::new(o)))
}

/// A path made absolute against the working directory, as the tree is walked by it.
fn full(path: &str) -> String {
    std::path::absolute(path).map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|_| path.to_string())
}

/// A limit, as a POSITIVE number: a limit of 0 is not a stricter gate, it is one that fails on every file.
fn number(flag: &str, value: &str) -> Result<i64, String> {
    let n: i32 = value.trim().parse().map_err(|_| format!("{flag} takes a number, not `{value}`"))?;
    if n < 1 {
        return Err(format!("{flag} must be at least 1, not {n}"));
    }
    Ok(i64::from(n))
}

/// A comma-separated set, each item once whatever its case - the first spelling kept.
fn list(text: &str) -> Vec<String> {
    let mut items = Vec::new();
    add(&mut items, &text.split(',').map(str::trim).filter(|s| !s.is_empty()).collect::<Vec<_>>());
    items
}

fn add(set: &mut Vec<String>, items: &[&str]) {
    for item in items {
        if !set.iter().any(|s| s.eq_ignore_ascii_case(item)) {
            set.push((*item).to_string());
        }
    }
}
