//! THE RULE HALVES' ANSWER - PowerShell (`PSGATE`) and TypeScript (`TSGATE`), each counting lines and
//! checking rules with its own language's parser in one host launch. One protocol, two prefixes:
//!
//!   <P>-LINES|rel|n                     the token line count of one file
//!   <P>|<severity>|rel|line|message     one finding - the severity is always `error`: a finding that does
//!                                       not fail the build scrolls past in a log that ends with OK
//!   <P>-FATAL|message                   the half cannot run at all (no compiler in the tree)
//!   <P>-DONE|n                          LAST line; its absence means the host died half way

use super::{run, tail, Launch};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct Input {
    pub launch: Launch,
    /// `PSGATE` or `TSGATE`.
    pub prefix: String,
    /// How a problem names this half: `powershell rules`, `typescript rules`.
    pub label: String,
    /// How a count names a file of it: `PowerShell`, `TypeScript`.
    pub noun: String,
    /// What to do when the host will not start.
    pub remedy: String,
}

pub fn inspect(input: &Input) -> Value {
    // IN THE ORDER THE HOST SAID THEM, a repeat updating in place: the caller files these counts in that
    // order, and every "largest first" tie keeps it.
    let mut lines: Vec<(String, i64)> = Vec::new();
    let mut at: HashMap<String, usize> = HashMap::new();
    let mut problems: Vec<String> = Vec::new();
    // EACH FINDING WITH THE FILE IT NAMES, so the caller can keep a file's answer; `clean` says the host ran
    // to its DONE line, the only run whose answers are worth keeping.
    let mut findings: Vec<(String, String)> = Vec::new();
    let answer = |lines: &Vec<(String, i64)>, problems: &Vec<String>, findings: &Vec<(String, String)>, clean: bool| {
        json!({ "lines": lines, "problems": problems, "findings": findings, "clean": clean })
    };
    if input.launch.files.is_empty() {
        return answer(&lines, &problems, &findings, true);
    }
    let host = &input.launch.host;
    let ran = match run(&input.launch) {
        Ok(ran) => ran,
        // NOT a skip: a host that will not launch leaves every file unmeasured, and a gate that passes in
        // that state is reporting on checks it never ran.
        Err(why) => {
            problems.push(format!("{}: `{host}` did not run ({why}) — {}", input.label, input.remedy));
            return answer(&lines, &problems, &findings, false);
        }
    };
    let fatal = format!("{}-FATAL|", input.prefix);
    if let Some(reason) = ran.stdout.split('\n').map(|l| l.trim_end_matches('\r')).find_map(|l| l.strip_prefix(&fatal)) {
        problems.push(format!("{}: {reason}", input.label));
        return answer(&lines, &problems, &findings, false);
    }
    let counted = format!("{}-LINES|", input.prefix);
    let finding = format!("{}|", input.prefix);
    for raw in ran.stdout.split('\n') {
        let line = raw.trim_end_matches('\r');
        if line.starts_with(&counted) {
            let parts: Vec<&str> = line.split('|').collect();
            if parts.len() >= 3
                && let Ok(n) = parts[2].parse::<i64>()
            {
                match at.get(parts[1]) {
                    Some(&index) => lines[index].1 = n,
                    None => {
                        at.insert(parts[1].to_string(), lines.len());
                        lines.push((parts[1].to_string(), n));
                    }
                }
            }
            continue;
        }
        if !line.starts_with(&finding) {
            continue;
        }
        // <P>|<severity>|<rel>|<line>|<message>; the message itself may hold `|`, so the split is bounded.
        let fields: Vec<&str> = line.splitn(5, '|').collect();
        if fields.len() == 5 {
            let problem = format!("{}:{}: {}", fields[2], fields[3], fields[4]);
            findings.push((fields[2].to_string(), problem.clone()));
            problems.push(problem);
        }
    }
    // The DONE line is the receipt. Without it the host died half way, and a truncated parse would look
    // exactly like a clean file set.
    if !ran.stdout.contains(&format!("{}-DONE|", input.prefix)) {
        problems.push(format!(
            "{}: `{host}` did not finish (exit {}) — {}",
            input.label,
            ran.exit,
            tail(&(ran.stderr.clone() + &ran.stdout))
        ));
        return answer(&lines, &problems, &findings, false);
    }
    for (rel, _) in &input.launch.files {
        if !at.contains_key(rel) {
            problems.push(format!("{rel}: {} file was NOT measured — the checker returned no count for it", input.noun));
        }
    }
    answer(&lines, &problems, &findings, true)
}
