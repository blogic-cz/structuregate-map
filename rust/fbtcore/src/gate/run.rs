//! THE GATE, RUN WHOLE: the pass cache, the walk, every count, the rule hosts, the judgement, the plugins
//! and the verdict - in this library, calling back into the caller for the ONE thing only it can do: count
//! a C# file with Roslyn and check it with the async rule, which read the same text.
//!
//! IT PRINTS NOTHING. It answers with the lines to print, each tagged for stdout or stderr, and the caller
//! writes them: .NET encodes console output in the console's code page and MSBuild reads it that way, so a
//! verdict written here as raw UTF-8 would reach a build log with its em dashes mangled.

use super::docs::context_doc;
use super::{cache, counts::Counts, folder, judge, Doc, Input as Judged};
use crate::hosts::{plugins, rules, Launch};
use crate::{count, sources};
use indexmap::IndexMap;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::ffi::{c_char, CStr, CString};
use std::path::Path;

/// The caller's C# counter: `(abs, rel, async_rules)` in, `{lines, problems}` or `{error: <exception>}` out,
/// allocated by the caller and handed back to `Free`.
pub type CsFile = extern "C" fn(abs: *const c_char, rel: *const c_char, async_rules: i32) -> *mut c_char;
pub type Free = extern "C" fn(text: *mut c_char);

/// Above this a C# file is counted by the `//` scanner, streamed: it has failed every limit already.
const STREAM_ABOVE_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Deserialize, Default)]
#[serde(default)]
struct Run {
    roots: Vec<String>,
    extensions: Vec<String>,
    skip: Vec<String>,
    skip_files: Vec<String>,
    /// `--doc-skip`: docs that are DATA, never measured as docs.
    doc_skip: Vec<String>,
    tracked: bool,
    include_untracked: bool,
    context_docs_only: bool,
    ps_discipline: bool,
    ts_discipline: bool,
    async_discipline: bool,
    ps_host: String,
    ts_host: String,
    max_source_lines: i64,
    max_files_per_dir: i64,
    max_doc_lines: i64,
    baseline_path: Option<String>,
    strict: bool,
    update_baseline: bool,
    dump: bool,
    worst: bool,
    plugins: Vec<String>,
    no_gate_cache: bool,
    version: String,
    build: String,
    arguments: String,
}

/// `{out: [["o"|"e", line], ...], exit, dump?}` - or `{error}` when the input is not the gate's.
pub(crate) fn run_json(input: Value, cs: CsFile, free: Free) -> Value {
    match serde_json::from_value::<Run>(input) {
        Ok(input) => run(&input, cs, free),
        Err(e) => json!({ "error": format!("the gate input is not JSON: {e}") }),
    }
}

struct Said(Vec<(&'static str, String)>);

impl Said {
    fn out(&mut self, line: String) {
        self.0.push(("o", line));
    }
    fn err(&mut self, line: String) {
        self.0.push(("e", line));
    }
    fn done(self, exit: i32) -> Value {
        json!({ "out": self.0, "exit": exit })
    }
}

fn run(input: &Run, cs: CsFile, free: Free) -> Value {
    let mut said = Said(Vec::new());
    let roots = input.roots.join(", ");
    // HAS THIS EXACT TREE ALREADY PASSED? The rules are a pure function of the tree and the run, and this
    // gate runs before `CoreCompile` on every build of every consumer - a whole-tree walk each time.
    let cached = if input.no_gate_cache || input.roots.len() != 1 {
        None
    } else {
        cache::find(&input.roots[0], &input.skip, input.tracked, &input.version, &input.arguments)
    };
    if let Some(found) = cached.as_ref().filter(|f| f.passed) {
        crate::trace::set("structuregate.gate.cached", true);
        said.out(format!("structuregate: nothing changed under {roots} since it last passed ({})", found.method));
        return said.done(0);
    }

    // A MISS STILL KNOWS every C# file the snapshot just refreshed says did not move.
    let mut known = cached.as_ref().and_then(|_| Counts::open(&input.roots[0], &input.skip, input.tracked, &format!("{}|{}", input.build, input.version), input.async_discipline));
    let extensions: HashSet<String> = input.extensions.iter().map(|e| e.to_lowercase()).collect();
    let in_scope = |rel: &str| {
        let extension = extension(rel);
        !extensions.contains(&extension) && extension == ".md" && (!input.context_docs_only || context_doc(rel))
            && !crate::sources::glob::excluded(&input.doc_skip, rel)
    };
    let mut problems: Vec<String> = Vec::new();
    let mut deleted: Vec<String> = Vec::new();
    let mut unreadable: Vec<String> = Vec::new();
    // IN THE ORDER THE WALK MET THEM - every "largest first" tie keeps that order.
    let mut counts: IndexMap<String, i64> = IndexMap::new();
    let mut per_folder: IndexMap<String, i64> = IndexMap::new();
    let mut doc_counts: IndexMap<String, i64> = IndexMap::new();
    let mut doc_paths: BTreeMap<String, String> = BTreeMap::new();
    // Docs per folder: never failed, only consulted so the split advice knows whether a sibling fits.
    let mut docs_per_folder: BTreeMap<String, i64> = BTreeMap::new();
    let mut ps_files: Vec<(String, String)> = Vec::new();
    let mut ts_files: Vec<(String, String)> = Vec::new();
    // `--skip-file`: what each pattern left out, so a run says so and a pattern that matches nothing is named.
    let mut left_out: Vec<Vec<String>> = vec![Vec::new(); input.skip_files.len()];

    let walk = crate::trace::stage("walk and count");
    for root in &input.roots {
        let walked = sources::walked(&sources::Input {
            root: root.clone(),
            skip: input.skip.clone(),
            tracked: input.tracked,
            include_untracked: input.include_untracked,
            generated_cs: false,
        });
        // A tracked file deleted in the working tree is named - but only one this gate would have MEASURED.
        deleted.extend(walked.deleted.into_iter().filter(|rel| extensions.contains(&extension(rel)) || in_scope(rel)));
        unreadable.extend(walked.unreadable);
        // With several roots a bare relative path is ambiguous - two roots can both hold `CLAUDE.md`.
        let prefix = if input.roots.len() > 1 && !input.tracked {
            let trimmed = root.trim_end_matches(std::path::MAIN_SEPARATOR);
            Path::new(trimmed).file_name().map(|n| format!("{}/", n.to_string_lossy())).unwrap_or_default()
        } else {
            String::new()
        };
        for (abs, found) in walked.found {
            let rel = format!("{prefix}{found}");
            // BY THE PATH FROM ITS OWN ROOT, so `web/src/api/schema.d.ts` reads the same with one root or several.
            if let Some(at) = input.skip_files.iter().position(|p| crate::sources::glob::excluded(std::slice::from_ref(p), &found)) {
                left_out[at].push(rel);
                continue;
            }
            let extension = extension(&abs);
            let source = extensions.contains(&extension);
            // READ NOTHING NO RULE MEASURES: a large test binary, a data JSON and a .docx are walked past.
            if !source && !in_scope(&rel) {
                continue;
            }
            let bump = |map: &mut IndexMap<String, i64>| *map.entry(folder(&rel)).or_insert(0) += 1;
            // PowerShell and TypeScript are counted where their parsers are - the folder count here, the line
            // count from the host below.
            if source && input.ps_discipline && [".ps1", ".psm1", ".psd1"].contains(&extension.as_str()) {
                ps_files.push((rel.clone(), abs));
                bump(&mut per_folder);
                continue;
            }
            if source && input.ts_discipline && [".ts", ".tsx", ".mts", ".cts"].contains(&extension.as_str()) {
                ts_files.push((rel.clone(), abs));
                bump(&mut per_folder);
                continue;
            }
            // C# IS COUNTED BY ROSLYN, on the caller's side, where the async rule reads the same text - up to
            // 4 MB; a bigger file is counted here, streamed.
            // The size is asked of a C# file only: every other file is counted here whatever its size.
            if source && extension == ".cs" && std::fs::metadata(&abs).map_or(0, |m| m.len()) <= STREAM_ABOVE_BYTES {
                let count = || {
                    let started = std::time::Instant::now();
                    let counted = csharp(cs, free, &abs, &rel, input.async_discipline);
                    crate::trace::file("csharp", &rel, started);
                    counted
                };
                let answer = match known.as_mut() {
                    Some(known) => known.csharp(&rel, count),
                    None => count(),
                };
                match answer {
                    Err(error) => unreadable.push(format!("{rel} ({error})")),
                    Ok((lines, found)) => {
                        problems.extend(found);
                        counts.insert(rel.clone(), lines);
                        bump(&mut per_folder);
                    }
                }
                continue;
            }
            let measure = || count::count(Path::new(&abs), !source, false).map(|n| n as i64).map_err(|e| e.to_string());
            let measured = match known.as_mut() {
                Some(known) => known.lines(&rel, if source { "lines" } else { "doc" }, measure),
                None => measure(),
            };
            match measured {
                Err(error) => unreadable.push(format!("{rel} ({error})")),
                Ok(lines) if source => {
                    counts.insert(rel.clone(), lines);
                    bump(&mut per_folder);
                }
                Ok(lines) => {
                    doc_counts.insert(rel.clone(), lines);
                    doc_paths.insert(rel.clone(), abs);
                    *docs_per_folder.entry(folder(&rel)).or_insert(0) += 1;
                }
            }
        }
    }

    // NEVER SILENT: a file the gate does not measure is named, and a pattern left matching nothing is a stale
    // exemption. Under --dump stdout is JSON another gate parses, so these go to stderr there.
    for (pattern, files) in input.skip_files.iter().zip(&left_out) {
        let note = if files.is_empty() {
            format!("NOTE: --skip-file `{pattern}` matched no file — remove it")
        } else {
            format!("NOTE: --skip-file `{pattern}` left {} file(s) unmeasured: {}{}", files.len(),
                files.iter().take(5).cloned().collect::<Vec<_>>().join(", "), if files.len() > 5 { " …" } else { "" })
        };
        if input.dump { said.err(note) } else { said.out(note) }
    }
    walk.set("structuregate.files", (counts.len() + doc_counts.len() + ps_files.len() + ts_files.len()) as i64);
    drop(walk);

    // BEFORE any mode returns, because --dump and --worst need these counts too.
    let hosts = [
        (&ps_files, &crate::embedded::PSGATE, "PSGATE", "powershell rules", "PowerShell",
            "pass --ps-host with a host that exists, or drop --ps-discipline", &input.ps_host, "ps"),
        (&ts_files, &crate::embedded::TSGATE, "TSGATE", "typescript rules", "TypeScript",
            "pass --ts-host with a node that exists, or drop --ts-discipline", &input.ts_host, "ts"),
    ];
    for (files, set, prefix, label, noun, remedy, host, tag) in hosts {
        // A FILE WHOSE ANSWER IS KEPT IS NOT HANDED TO THE HOST, and a host with nothing left to ask is not
        // started: PowerShell's start alone is most of a second.
        let rules = known.as_ref().map(|k| k.host(tag, host)).unwrap_or_default();
        let mut answered: HashMap<String, (i64, Vec<String>)> = HashMap::new();
        let mut asked: Vec<(String, String)> = Vec::new();
        for (rel, abs) in files.iter() {
            match known.as_mut().and_then(|k| k.get(rel, &rules)) {
                Some(found) => {
                    answered.insert(rel.clone(), found);
                }
                None => asked.push((rel.clone(), abs.clone())),
            }
        }
        for (rel, _) in files.iter() {
            if let Some((lines, found)) = answered.get(rel) {
                counts.insert(rel.clone(), *lines);
                problems.extend(found.iter().cloned());
            }
        }
        if asked.is_empty() {
            continue;
        }
        let stage = crate::trace::stage(label);
        stage.set("structuregate.files", asked.len() as i64);
        stage.set("structuregate.host", host.as_str());
        let script = match crate::embedded::stage(set) {
            Ok(script) => script,
            Err(why) => {
                problems.push(format!("{label}: could not stage the checker ({why})"));
                continue;
            }
        };
        let (before, list_flag, root_flag): (Vec<String>, &str, &str) = if tag == "ps" {
            (["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"].map(String::from).to_vec(), "-ListFile", "-Root")
        } else {
            (Vec::new(), "--list-file", "--root")
        };
        let answer = rules::inspect(&rules::Input {
            launch: Launch {
                host: host.clone(),
                before,
                script: script.clone(),
                after: Vec::new(),
                list_flag: list_flag.into(),
                root_flag: root_flag.into(),
                cwd: input.roots[0].clone(),
                files: asked.clone(),
                tag: tag.into(),
            },
            prefix: prefix.into(),
            label: label.into(),
            noun: noun.into(),
            remedy: remedy.into(),
        });
        let mut fresh: HashMap<String, (i64, Vec<String>)> = HashMap::new();
        for pair in answer["lines"].as_array().into_iter().flatten() {
            let rel = pair[0].as_str().unwrap_or("").to_string();
            counts.insert(rel.clone(), pair[1].as_i64().unwrap_or(0));
            fresh.insert(rel, (pair[1].as_i64().unwrap_or(0), Vec::new()));
        }
        problems.extend(strings(&answer["problems"]));
        // KEPT ONLY FROM A HOST THAT RAN TO ITS END, and only for a file it counted.
        if let Some(known) = known.as_mut().filter(|_| answer["clean"] == Value::Bool(true)) {
            for finding in answer["findings"].as_array().into_iter().flatten() {
                if let Some(entry) = fresh.get_mut(finding[0].as_str().unwrap_or("")) {
                    entry.1.push(finding[1].as_str().unwrap_or("").to_string());
                }
            }
            for (rel, _) in &asked {
                if let Some(found) = fresh.get(rel) {
                    known.put(rel, &rules, found);
                }
            }
        }
    }
    if let Some(known) = known {
        known.keep();
    }

    // Said HERE, before any mode can return: a path the gate could not open is missing from every count that
    // follows. Under --dump the note goes to stderr, since stdout is JSON another gate parses.
    if !unreadable.is_empty() {
        let first: Vec<&str> = unreadable.iter().take(5).map(String::as_str).collect();
        let note = format!(
            "NOTE: {} path(s) could not be read and were NOT measured — skip them or fix access: {}{}",
            unreadable.len(),
            first.join(", "),
            if unreadable.len() > 5 { " …" } else { "" }
        );
        if input.dump { said.err(note) } else { said.out(note) }
    }

    let judging = crate::trace::stage("judge");
    let verdict = judge(&Judged {
        files: counts.into_iter().collect(),
        folders: per_folder.into_iter().collect(),
        docs: doc_counts.into_iter().map(|(rel, lines)| Doc { abs: doc_paths.get(&rel).cloned().unwrap_or_default(), rel, lines }).collect(),
        doc_folders: docs_per_folder,
        max_source_lines: input.max_source_lines,
        max_files_per_dir: input.max_files_per_dir,
        max_doc_lines: input.max_doc_lines,
        baseline_path: input.baseline_path.clone(),
        strict: input.strict,
        update_baseline: input.update_baseline,
        dump: input.dump,
        worst: input.worst,
    });
    drop(judging);
    if let Some(dump) = verdict.get("dump") {
        let mut answer = said.done(0);
        answer["dump"] = dump.clone();
        return answer;
    }
    if let Some(error) = verdict["error"].as_str() {
        said.err(format!("structuregate: {error}"));
        return said.done(2);
    }
    if let Some(written) = verdict["baseline_written"].as_str() {
        said.out(written.to_string());
        return said.done(0);
    }
    if let Some(lines) = verdict.get("worst") {
        for line in strings(lines) {
            said.out(line);
        }
        return said.done(0);
    }
    problems.extend(strings(&verdict["problems"]));

    // A project's own rules run LAST and inside the same verdict: one command, one exit code.
    if !input.plugins.is_empty() {
        let _plugins = crate::trace::stage("plugins");
        let ran = plugins::gate(&plugins::Input { commands: input.plugins.clone(), cwd: input.roots[0].clone(), files: Vec::new() });
        for line in strings(&ran["said"]) {
            said.out(line);
        }
        problems.extend(strings(&ran["problems"]));
    }

    if !problems.is_empty() {
        // stderr, NOT stdout: a Claude Code hook that fails non-blocking shows stderr and DISCARDS stdout.
        said.err(format!("structuregate: {} structure violation(s) under {roots}", problems.len()));
        for problem in problems {
            said.err(format!("  error: {problem}"));
        }
        return said.done(1);
    }
    // IT PASSED, so the tree it passed over is worth remembering - only on a clean run.
    if let Some(found) = &cached {
        cache::remember(&input.roots[0], &found.key);
    }
    if !deleted.is_empty() {
        let first: Vec<&str> = deleted.iter().take(5).map(String::as_str).collect();
        said.out(format!(
            "NOTE: {} tracked file(s) are deleted in the working tree and were not checked — stage the deletion: {}{}",
            deleted.len(),
            first.join(", "),
            if deleted.len() > 5 { " …" } else { "" }
        ));
    }
    said.out(verdict["ok"].as_str().unwrap_or("").to_string());
    said.done(0)
}

/// One C# file through the caller: `(lines, async problems)`, or the exception that stopped the read.
fn csharp(cs: CsFile, free: Free, abs: &str, rel: &str, async_rules: bool) -> Result<(i64, Vec<String>), String> {
    let (Ok(abs), Ok(rel)) = (CString::new(abs), CString::new(rel)) else { return Err("IOException".into()) };
    let reply = cs(abs.as_ptr(), rel.as_ptr(), async_rules as i32);
    if reply.is_null() {
        return Err("IOException".into());
    }
    let text = unsafe { CStr::from_ptr(reply) }.to_string_lossy().into_owned();
    free(reply);
    let answer: Value = serde_json::from_str(&text).map_err(|_| "IOException".to_string())?;
    if let Some(error) = answer["error"].as_str() {
        return Err(error.to_string());
    }
    Ok((answer["lines"].as_i64().unwrap_or(0), strings(&answer["problems"])))
}

fn strings(value: &Value) -> Vec<String> {
    value.as_array().into_iter().flatten().map(|v| v.as_str().unwrap_or("").to_string()).collect()
}

/// `.ext` in lowercase, as `Path.GetExtension` reads it off the last segment - "" when there is none.
fn extension(path: &str) -> String {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    name.rfind('.').map(|at| name[at..].to_lowercase()).unwrap_or_default()
}
