//! A PROJECT'S OWN SCRIPTS, run as part of the gate (`--plugin`) and of the map (`--map-plugin`). The rule
//! lives where its parser is authoritative, and this tool adds the thing it is good at: one entry point,
//! one verdict, one exit code.
//!
//! A PLUGIN THAT WILL NOT LAUNCH FAILS. It was NAMED on the command line, so a missing one is a
//! misconfiguration, and a gate that passes without it is reporting on a check it never ran.

use super::{process, tail, Ran};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct Input {
    pub commands: Vec<String>,
    pub cwd: String,
    /// The map's files, `[[rel, abs], ...]` - handed to a map plugin as a list file, see `map`.
    pub files: Vec<(String, String)>,
}

/// A command line into its parts, honouring double quotes so a path with a space survives. Scanned
/// character by character, as every other parse in this tool is.
pub fn split(command: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    for c in command.chars() {
        match c {
            '"' => quoted = !quoted,
            ' ' if !quoted => {
                if !current.is_empty() {
                    parts.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(c),
        }
    }
    if !current.is_empty() {
        parts.push(current);
    }
    parts
}

fn start(parts: &[String], cwd: &str, env: &[(&str, &str)]) -> Result<Ran, String> {
    let env: Vec<(String, String)> = env.iter().map(|(n, v)| (n.to_string(), v.to_string())).collect();
    process(&parts[0], &parts[1..], cwd, &env)
}

fn lines(text: &str) -> Vec<&str> {
    text.split('\n').map(|l| l.trim_end_matches('\r')).filter(|l| !l.trim().is_empty()).collect()
}

/// The gate's plugins, judged by EXIT CODE: `{problems: [..], said: [..]}` - `said` being what a plugin
/// that passed printed, for the caller to show.
pub fn gate(input: &Input) -> Value {
    let mut problems = Vec::new();
    let mut said = Vec::new();
    for command in &input.commands {
        let parts = split(command);
        if parts.is_empty() {
            problems.push(format!("plugin: empty command in --plugin \"{command}\""));
            continue;
        }
        let out = match start(&parts, &input.cwd, &[]) {
            Ok(out) => out,
            Err(why) => {
                problems.push(format!("plugin `{command}`: {why} (in {})", input.cwd));
                continue;
            }
        };
        let text = out.stdout + &out.stderr;
        if out.exit == 0 {
            said.extend(lines(&text).into_iter().map(|l| format!("  {l}")));
            continue;
        }
        let indented: Vec<String> = lines(&text).into_iter().map(|l| format!("        {l}")).collect();
        problems.push(format!("plugin `{command}` failed (exit {}):\n{}", out.exit, indented.join("\n")));
    }
    json!({ "problems": problems, "said": said })
}

/// The map's plugins. A map plugin is not read for its exit code: it speaks the map protocol and states its
/// own findings, so each one comes back as `{command, stdout, error?}` - the rows are read on the caller's
/// side, where they land - with `error` set when it did not run, said it cannot, or did not finish.
///
/// What it is handed, as ENVIRONMENT, so the command line stays exactly what the caller wrote:
/// `STRUCTUREGATE_MAP_ROOT` (the first root, also its working directory) and `STRUCTUREGATE_MAP_LIST`
/// (every file of the map, so a plugin scanning the tree agrees with this one about what is in it).
pub fn map(input: &Input) -> Value {
    if input.commands.is_empty() {
        return json!({ "runs": [] });
    }
    let list = crate::hosts::temp_dir().join(format!("structuregate-maplist-{}.txt", std::process::id()));
    let newline = if cfg!(windows) { "\r\n" } else { "\n" };
    let body: String = input.files.iter().map(|(rel, abs)| format!("{rel}\t{abs}{newline}")).collect();
    if let Err(e) = std::fs::write(&list, body) {
        return json!({ "error": format!("PLUGIN    the map file list could not be written (IOException: {e})") });
    }
    let list = list.to_string_lossy().into_owned();
    let mut runs = Vec::new();
    for command in &input.commands {
        let parts = split(command);
        if parts.is_empty() {
            runs.push(json!({ "command": command, "stdout": "", "error": format!("PLUGIN    empty command in --map-plugin \"{command}\"") }));
            continue;
        }
        let env = [("STRUCTUREGATE_MAP_ROOT", input.cwd.as_str()), ("STRUCTUREGATE_MAP_LIST", list.as_str())];
        let out = match start(&parts, &input.cwd, &env) {
            Ok(out) => out,
            Err(why) => {
                runs.push(json!({ "command": command, "stdout": "", "error": format!("PLUGIN    `{command}` did not run ({why}) in {}", input.cwd) }));
                continue;
            }
        };
        let first = |prefix: &str| out.stdout.split('\n').map(|l| l.trim_end_matches('\r')).find_map(|l| l.strip_prefix(prefix).map(str::to_string));
        let error = if let Some(fatal) = first("MAP-FATAL|") {
            Some(format!("PLUGIN    `{command}`: {fatal}"))
        } else if !out.stdout.contains("MAP-DONE|") {
            // A non-zero exit means it fell over, and then its rows are partial whether or not it managed to
            // print some - so the receipt, not the exit code, is what is checked.
            Some(format!("PLUGIN    `{command}` did not finish (exit {}) — {}", out.exit, tail(&(out.stderr.clone() + &out.stdout))))
        } else {
            None
        };
        runs.push(json!({ "command": command, "stdout": out.stdout, "error": error }));
    }
    let _ = std::fs::remove_file(&list);
    json!({ "runs": runs })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_quoted_path_with_a_space_is_one_part() {
        assert_eq!(split(r#"python "C:\My Tools\check.py" --strict"#), ["python", r"C:\My Tools\check.py", "--strict"]);
        assert!(split("   ").is_empty());
    }
}
