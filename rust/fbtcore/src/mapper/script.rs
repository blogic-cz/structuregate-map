//! ONE SCRIPT HALF, run in the host that owns its language: what it is asked (only what moved, `kept.rs`), its
//! launch, and every check its answer goes through. A launch is `begin`, the host, then `end` - split, so a half
//! whose host is slow to start (PowerShell) runs on beside the rest of the run (`background`, `finish`).

use super::kept::Kept;
use super::protocol::{first, read, Collector};
use crate::hosts::{self, Launch};
use std::collections::{HashMap, HashSet};

/// One script half's launch: its language, host, staged entry script and command-line shape.
pub struct Script<'a> {
    pub language: &'a str,
    pub host: &'a str,
    pub script: Result<&'a str, &'a str>,
    pub flag: &'a str,
    pub before: Vec<String>,
    pub after: Vec<String>,
    pub list_flag: &'a str,
    pub root_flag: &'a str,
    /// A half that answers per FILE is held to a record for every file it was given; one that answers in
    /// TABLES (a deep half) is held to the count on its DONE line instead.
    pub per_file: bool,
    /// The flag that hands a half EVERY file while its list holds only the ones to parse - so a file's lines can be
    /// kept between runs (`kept.rs`). Empty for a half whose lines about a file read other files: it parses all.
    pub known_flag: &'a str,
    /// For a half whose lines cannot be kept per file: what it reads beyond its own files, so its WHOLE answer is
    /// kept and replayed while none of it moved (`Kept::whole`). None: the half always runs.
    pub whole: Option<fn(&str) -> bool>,
    /// For a half kept PER FILE whose answer about a file also reads the files it imports (python binds a name by
    /// reading its module): what it reads beyond its files (`pyproject.toml`). A file is read again when it or
    /// anything it imports, through any chain, moved; its entry points, a line about the RUN, are kept apart.
    pub dependents: Option<fn(&str) -> bool>,
    /// WHAT THE HOST ALREADY SAID, when it was started before this half's turn (`payload::Early`): the run is not
    /// launched again, and its answer goes through every check a launch does.
    pub early: Option<Result<hosts::Ran, String>>,
}

/// Run one script half over one root's files and return WHY it did not map them - empty when it did. A half
/// that never ran is a missing tool; one that ran and died half way is broken, and that stays an error.
pub fn script(into: &mut Collector, half: &Script, files: &[(String, String)], root: &str, prefix: &str, kept: &mut Option<Kept>) -> String {
    match begin(into, half, files, root, prefix, kept) {
        Begin::Done(reason) => reason,
        Begin::Run(begun) => {
            let ran = launched(half, &begun).unwrap_or_else(|| hosts::run(&begun.launch));
            end(into, half, files, prefix, kept, begun, ran)
        }
    }
}

/// A half whose host RUNS ON while the rest of the run goes on - the PowerShell file map beside the deep map, since
/// `pwsh` alone is seconds on a cold run and nothing after it needs its answer. `finish` waits for it and reads it.
pub enum Pending {
    Done(String),
    Running(Box<Begun>, std::thread::JoinHandle<Result<hosts::Ran, String>>),
}

pub fn background(into: &mut Collector, half: &Script, files: &[(String, String)], root: &str, prefix: &str, kept: &mut Option<Kept>) -> Pending {
    match begin(into, half, files, root, prefix, kept) {
        Begin::Done(reason) => Pending::Done(reason),
        Begin::Run(begun) => match launched(half, &begun) {
            Some(ran) => Pending::Done(end(into, half, files, prefix, kept, begun, ran)),
            None => {
                // TIMED UNTIL ITS HOST ANSWERS, but no parent of the stages that open meanwhile.
                begun.stage.detach();
                let launch = begun.launch.clone();
                Pending::Running(Box::new(begun), std::thread::spawn(move || hosts::run(&launch)))
            }
        },
    }
}

pub fn finish(into: &mut Collector, half: &Script, files: &[(String, String)], prefix: &str, kept: &mut Option<Kept>, pending: Pending) -> String {
    match pending {
        Pending::Done(reason) => reason,
        Pending::Running(begun, host) => {
            let ran = host.join().unwrap_or_else(|_| Err("its thread stopped".into()));
            end(into, half, files, prefix, kept, *begun, ran)
        }
    }
}

/// What a half's answer needs once its host has spoken: what was answered from the cache, how each asked file is
/// keyed, the launch - and the stage, timed until the answer is read.
pub struct Begun {
    stage: crate::trace::Stage,
    answered: String,
    keys: HashMap<String, String>,
    globals: Option<String>,
    whole: Option<String>,
    failed: Option<String>,
    replayed: Option<String>,
    known: Option<std::path::PathBuf>,
    launch: Launch,
}

enum Begin {
    Done(String),
    Run(Begun),
}

/// What the host said without being launched now: a replayed answer, or one it gave when started early.
fn launched(half: &Script, begun: &Begun) -> Option<Result<hosts::Ran, String>> {
    match (&begun.replayed, &half.early) {
        (Some(stdout), _) => Some(Ok(hosts::Ran { stdout: stdout.clone(), stderr: String::new(), exit: 0 })),
        (None, Some(early)) => Some(early.clone()),
        (None, None) => None,
    }
}

fn begin(into: &mut Collector, half: &Script, files: &[(String, String)], root: &str, prefix: &str, kept: &mut Option<Kept>) -> Begin {
    if files.is_empty() {
        return Begin::Done(String::new());
    }
    let stage = crate::trace::stage(&format!("map: {}", half.language));
    stage.set("structuregate.files", files.len() as i64);
    stage.set("structuregate.host", half.host);
    let script = match half.script {
        Ok(path) => path,
        Err(why) => return Begin::Done(format!("the {} half could not be staged ({why})", half.language)),
    };
    // WHAT DID NOT MOVE IS ANSWERED FROM THE LAST RUN, and only the rest is handed to the host (`kept.rs`).
    let keeping = kept.as_mut().filter(|_| !half.known_flag.is_empty());
    let (mut answered, mut asked, mut keys) = (String::new(), Vec::new(), HashMap::new());
    let mut globals: Option<String> = None;
    match keeping {
        Some(kept) => {
            let project = if half.language == "typescript" { kept.project() } else { String::new() };
            let reads = half.dependents.and_then(|also| kept.whole("reads", &[], also)).unwrap_or_default();
            let stamp = format!("{}|{}|{script}|{project}|{reads}", half.language, half.host);
            let keyed: Vec<Option<String>> = files.iter().map(|(rel, _)| kept.key(&stamp, rel)).collect();
            let mut ask: HashSet<usize> = (0..files.len()).filter(|&i| keyed[i].as_deref().is_none_or(|k| !kept.has(k))).collect();
            if half.dependents.is_some() {
                globals = kept.whole(&format!("{stamp}|globals"), &[], |_| false);
                if !globals.as_deref().is_some_and(|g| kept.has(g)) {
                    ask = (0..files.len()).collect();
                } else if !ask.is_empty() {
                    ask = with_importers(kept, files, ask);
                }
            }
            for (i, (rel, abs)) in files.iter().enumerate() {
                if ask.contains(&i) {
                    if let Some(key) = &keyed[i] {
                        keys.insert(rel.clone(), key.clone());
                    }
                    asked.push((rel.clone(), abs.clone()));
                } else if let Some(lines) = keyed[i].as_deref().and_then(|k| kept.get(k)) {
                    answered.push_str(&lines);
                }
            }
            // NOTHING TO PARSE: the run's own lines (entry points, notes) are replayed beside the files' lines.
            if asked.is_empty()
                && let Some(lines) = globals.as_deref().and_then(|g| kept.get(g))
            {
                answered.push_str(&lines);
            }
        }
        None => asked = files.to_vec(),
    }
    // A HALF KEPT WHOLE: replayed while nothing it reads moved, through the same checks as a run.
    let whole = half.whole.filter(|_| half.known_flag.is_empty())
        .and_then(|also| kept.as_ref().and_then(|k| k.whole(&format!("{}|{}|{script}", half.language, half.host), files, also)));
    // A TYPESCRIPT HALF THAT FAILED (no compiler) fails the same way until a file or a compiler moves: its answer is
    // kept too, so a tree with no `typescript` does not start node on every turn to hear it again (`Kept::compilers`).
    let failed = (half.language == "typescript" && !asked.is_empty()).then(|| kept.as_ref().and_then(|k| {
        let stamp = format!("{}|{}|{script}|fatal|{}|{}", half.language, half.host, k.project(), Kept::compilers(root, files));
        k.whole(&stamp, files, |_| false)
    })).flatten();
    // A REPLAY IS ONE OF THE TWO, NEVER A MIX: only a finished run is kept whole, only a failed one under `failed`.
    let replay = whole.clone().or(failed.clone());
    let replayed = replay.as_deref().and_then(|key| kept.as_mut().and_then(|k| k.get(key)));
    if replayed.is_some() {
        asked.clear();
    }
    stage.set("structuregate.parsed", asked.len() as i64);
    if asked.is_empty() && replayed.is_none() {
        into.halves.insert(format!("{} via {}", half.language, half.host));
        read(&answered, into, half.language, prefix);
        return Begin::Done(String::new());
    }
    let mut after = half.after.clone();
    let known = (asked.len() < files.len()).then(|| known_list(half.language, files)).flatten();
    if let Some(list) = &known {
        after.extend([half.known_flag.to_string(), list.to_string_lossy().into_owned()]);
    }
    let launch = Launch {
        host: half.host.into(),
        before: half.before.clone(),
        script: script.into(),
        after,
        list_flag: half.list_flag.into(),
        root_flag: half.root_flag.into(),
        cwd: root.into(),
        files: asked.clone(),
        tag: half.language.into(),
    };
    Begin::Run(Begun { stage, answered, keys, globals, whole, failed, replayed, known, launch })
}

fn end(into: &mut Collector, half: &Script, files: &[(String, String)], prefix: &str, kept: &mut Option<Kept>, begun: Begun, ran: Result<hosts::Ran, String>) -> String {
    let Begun { stage: _stage, mut answered, keys, globals, whole, failed, replayed, known, .. } = begun;
    if let Some(list) = known {
        let _ = std::fs::remove_file(list);
    }
    let ran = match ran {
        Ok(ran) => ran,
        Err(why) => {
            return format!("`{}` did not run ({why}) — pass {} with a host that exists, or drop {} from --ext", half.host, half.flag, half.language);
        }
    };
    into.halves.insert(format!("{} via {}", half.language, half.host));
    // ONLY A RUN THAT FINISHED IS KEPT: a host that died half way said nothing true about the files it never reached.
    if let Some(kept) = kept.as_mut().filter(|_| ran.stdout.contains("MAP-DONE|") && !ran.stdout.contains("MAP-FATAL|")) {
        // THE RUN'S OWN LINES - a note, an entry a launcher names (`MAP-ENTRY|rel|launched`) - are kept apart from
        // what each file says of itself, so a file answered from the cache never carries a launcher's stale verdict.
        let run_wide = |line: &str| half.dependents.is_some()
            && (line.starts_with("MAP-NOTE|") || (line.starts_with("MAP-ENTRY|") && line.ends_with("|launched")));
        for (rel, lines) in by_file(&ran.stdout, &keys, &run_wide) {
            kept.put(keys[&rel].clone(), lines);
        }
        if let Some(key) = &globals {
            let lines: String = ran.stdout.lines().filter(|l| run_wide(l)).map(|l| format!("{l}\n")).collect();
            kept.put(key.clone(), lines);
        }
        if let (Some(key), None) = (&whole, &replayed) {
            kept.put(key.clone(), ran.stdout.clone());
        }
    }
    if let (Some(kept), Some(key), None) = (kept.as_mut(), &failed, &replayed)
        && ran.stdout.contains("MAP-FATAL|")
    {
        kept.put(key.clone(), ran.stdout.clone());
    }
    answered.push_str(&ran.stdout);
    let mapped = read(&answered, into, half.language, prefix);
    if let Some(fatal) = first(&ran.stdout, "MAP-FATAL|") {
        return fatal;
    }
    // The DONE line is the receipt. STARTED AND DIED is not never-started: the map it produced is TRUNCATED,
    // which looks exactly like a small tree.
    if !ran.stdout.contains("MAP-DONE|") {
        into.errors.push(format!(
            "HALF      {}: `{}` did not finish (exit {}) — {}",
            half.language,
            half.host,
            ran.exit,
            hosts::tail(&(ran.stderr.clone() + &ran.stdout))
        ));
        return format!("`{}` stopped part way through, so this file may be missing its edges", half.host);
    }
    if half.per_file {
        for (rel, _) in files {
            if !mapped.contains(&format!("{prefix}{rel}")) {
                into.errors.push(format!("UNPARSED  {prefix}{rel}: the {} half returned nothing for this file", half.language));
            }
        }
        return String::new();
    }
    // A HALF THAT ANSWERS IN TABLES still accounts for every file: a short DONE count means files were
    // dropped without a word.
    let done = first(&ran.stdout, "MAP-DONE|").unwrap_or_default();
    if let Ok(read) = done.split('|').next().unwrap_or("").trim().parse::<usize>()
        && read < files.len()
    {
        into.errors.push(format!("UNPARSED  the {} half read {read} of {} file(s) and said nothing about the rest", half.language, files.len()));
    }
    String::new()
}

/// The files to read again, and every file that imports one of them through any chain - by the last map's edges.
/// Without a last map, every file.
fn with_importers(kept: &mut Kept, files: &[(String, String)], ask: HashSet<usize>) -> HashSet<usize> {
    let Some(importers) = kept.importers() else { return (0..files.len()).collect() };
    let at: HashMap<&str, usize> = files.iter().enumerate().map(|(i, (rel, _))| (rel.as_str(), i)).collect();
    let mut out = ask.clone();
    let mut wave: Vec<usize> = ask.into_iter().collect();
    while let Some(i) = wave.pop() {
        for from in importers.get(&files[i].0).into_iter().flatten() {
            if let Some(&j) = at.get(from.as_str())
                && out.insert(j)
            {
                wave.push(j);
            }
        }
    }
    out
}

/// Every file of the map, for a half that is handed only the ones to parse - in the list file's own format.
fn known_list(tag: &str, files: &[(String, String)]) -> Option<std::path::PathBuf> {
    let path = crate::hosts::temp_dir().join(format!("structuregate-{tag}known-{}.txt", std::process::id()));
    let newline = if cfg!(windows) { "\r\n" } else { "\n" };
    let body: String = files.iter().map(|(rel, abs)| format!("{rel}\t{abs}{newline}")).collect();
    std::fs::write(&path, body).ok().map(|()| path)
}

/// A host's lines grouped by the file they are about - only the files it was asked to parse, and only lines that
/// name one in their second field. The rest (DONE, a NOTE) belong to the run, not to a file.
fn by_file(stdout: &str, asked: &HashMap<String, String>, run_wide: &dyn Fn(&str) -> bool) -> Vec<(String, String)> {
    let mut grouped: Vec<(String, String)> = Vec::new();
    let mut at: HashMap<String, usize> = HashMap::new();
    for line in stdout.lines().filter(|l| !run_wide(l)) {
        let Some(rel) = line.split('|').nth(1).filter(|rel| asked.contains_key(*rel)) else { continue };
        let slot = *at.entry(rel.to_string()).or_insert_with(|| {
            grouped.push((rel.to_string(), String::new()));
            grouped.len() - 1
        });
        grouped[slot].1.push_str(line);
        grouped[slot].1.push('\n');
    }
    grouped
}
