//! THE HOSTS - node, python and PowerShell, each started to run the parser that is authoritative for its
//! language, and each answering on stdout in a line protocol. Starting one, handing it the file set and
//! reading its answer are the same work for every half; only the script and the command-line shape differ,
//! so the shape is DATA (`before`, `after`, `list_flag`, `root_flag`) and there is one launcher.
//!
//! A HOST THAT WILL NOT LAUNCH IS A FINDING, NEVER A SKIP. A run missing every python file because `python`
//! was not on PATH looks exactly like a run over a tree with no python in it.

pub(crate) mod plugins;
pub(crate) mod rules;

use serde::Deserialize;

/// One host launch. The script is already staged on disk by the caller - it holds the embedded copy.
#[derive(Deserialize, Default)]
#[serde(default)]
#[derive(Clone)]
pub struct Launch {
    pub host: String,
    pub before: Vec<String>,
    pub script: String,
    pub after: Vec<String>,
    pub list_flag: String,
    pub root_flag: String,
    pub cwd: String,
    /// `[[rel, abs], ...]` - written to a list FILE, never onto the command line: 300 modules would blow its
    /// length limit, and a path with a space would need quoting rules on both sides.
    pub files: Vec<(String, String)>,
    /// Names the list file, so two halves in one process do not share one.
    pub tag: String,
}

/// What a host said. `why` instead of the rest when it never started.
#[derive(Clone)]
pub struct Ran {
    pub stdout: String,
    pub stderr: String,
    pub exit: i32,
}

/// Start the host on the script with the file list, and wait for it. `Err` says why it never ran.
pub fn run(launch: &Launch) -> Result<Ran, String> {
    let list = crate::hosts::temp_dir().join(format!("structuregate-{}list-{}.txt", launch.tag, std::process::id()));
    let newline = if cfg!(windows) { "\r\n" } else { "\n" };
    let body: String = launch.files.iter().map(|(rel, abs)| format!("{rel}\t{abs}{newline}")).collect();
    std::fs::write(&list, body).map_err(|e| format!("the file list could not be written ({e})"))?;
    let mut args: Vec<String> = launch.before.clone();
    args.push(launch.script.clone());
    args.extend(launch.after.iter().cloned());
    args.extend([launch.list_flag.clone(), list.to_string_lossy().into_owned(), launch.root_flag.clone(), launch.cwd.clone()]);
    let ran = process(&launch.host, &args, &launch.cwd, &[]);
    // THE LIST IS THIS RUN'S ALONE: left behind, every launch of every run added one to the temp folder.
    let _ = std::fs::remove_file(&list);
    ran
}

const NODE_COMPILE_CACHE: &str = "NODE_COMPILE_CACHE";

/// THE TEMP FOLDER, never "" and never relative. An EMPTY `TMPDIR` makes `crate::hosts::temp_dir()` answer "", so the
/// embedded scripts were staged into whatever folder the run started in - beside the user's files - and a host started
/// in the tree's root could not find them. Every temp path of a run comes from here.
pub(crate) fn temp_dir() -> std::path::PathBuf {
    let told = std::env::temp_dir();
    if told.as_os_str().is_empty() || told.is_relative() {
        return std::path::PathBuf::from(if cfg!(windows) { r"C:\Windows\Temp" } else { "/tmp" });
    }
    told
}

/// Start `host` with `args` in `cwd`, `env` added, and wait for it - both streams read to the end
/// together, so a child filling one pipe never blocks on the other. `Err` says why it never started.
pub fn process(host: &str, args: &[String], cwd: &str, env: &[(String, String)]) -> Result<Ran, String> {
    let mut command = std::process::Command::new(host);
    command.current_dir(cwd).args(args);
    // NODE KEEPS WHAT IT COMPILED, across runs: every launch compiled the 9 MB `typescript.js` again, a third of a
    // second of each node start on every turn of every consumer. Node 22.1+ reads this; older nodes, python and
    // PowerShell ignore it, and a cache the environment already names is the one used.
    if std::env::var_os(NODE_COMPILE_CACHE).is_none() && !env.iter().any(|(name, _)| name == NODE_COMPILE_CACHE) {
        command.env(NODE_COMPILE_CACHE, crate::hosts::temp_dir().join("structuregate-node-cache"));
    }
    for (name, value) in env {
        command.env(name, value);
    }
    let out = command.output().map_err(|e| e.to_string())?;
    Ok(Ran {
        // Every half writes UTF-8. Read any other way, a summary quoted out of a file with a non-ASCII
        // identifier comes back mangled - and in the deep map that literal is a row.
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        exit: out.status.code().unwrap_or(-1),
    })
}

/// The last three non-blank lines of what a host said - enough to see why it stopped.
pub fn tail(text: &str) -> String {
    let lines: Vec<&str> = text.split('\n').map(str::trim).filter(|l| !l.is_empty()).collect();
    if lines.is_empty() { "no output".into() } else { lines[lines.len().saturating_sub(3)..].join(" / ") }
}
