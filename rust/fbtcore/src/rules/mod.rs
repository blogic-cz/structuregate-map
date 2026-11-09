//! RULES OVER THE DEEP MAP - questions a parse tree alone cannot answer, asked of the BOUND rows after a run:
//! a call's resolved return type, a method's resolved signature, which interface member it implements. Each rule
//! is one SQL query over the map (`csharp.rs`), and each is switched by the consumer, never on by itself.
//!
//! `structuregate.rules.json` (beside the exe, or `--rules-config`): `{"rules": {"cs-task-dropped": "error", ..}}`,
//! each rule `off` (the default), `warn` (listed by every `--map-sqlite` run) or `error` (listed, and the run exits 1);
//! `true` is `error`, `false` is `off`. A rule the file names that does not exist is an error, not a silent no-op.
//! `--map-query <db> --rules [FRAGMENT]` lists every rule's findings whatever the file says - what turning one on
//! would report.
//!
//! OPT OUT WITH A REASON: a line carrying `// async-ok` (or the line above it) is skipped, as in the gate's own
//! `--async-discipline`. TEST CODE IS LEFT OUT, by path (`NOT_TEST_PATH`).

pub(crate) mod csharp;

use anyhow::Result;
use rusqlite::Connection;
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

pub const FILE_NAME: &str = "structuregate.rules.json";
const WAIVER: &str = "async-ok";

/// `f.path` is not test code: a `test`/`tests` folder, a `*.Tests` project, or a file named like a test.
pub(crate) const NOT_TEST_PATH: &str = "NOT (('/' || lower(f.path)) GLOB '*/test/*' OR ('/' || lower(f.path)) GLOB '*/tests/*' \
    OR lower(f.path) GLOB '*.tests/*' OR lower(f.path) GLOB '*_test.*' OR ('/' || lower(f.path)) GLOB '*/test_*' \
    OR lower(f.path) GLOB '*.spec.*' OR lower(f.path) GLOB '*.test.*' OR lower(f.path) GLOB '*tests.cs')";

/// One rule: what it finds, what to do instead, the columns it reads, and its query - `path, line, name, type`
/// rows, the file aliased `f`.
pub struct Rule {
    pub id: &'static str,
    pub says: &'static str,
    pub fix: &'static str,
    pub needs: &'static [(&'static str, &'static str)],
    pub sql: fn() -> String,
}

pub fn all() -> &'static [Rule] {
    csharp::RULES
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Level {
    Off,
    Warn,
    Error,
}

/// What the consumer switched on, and what was wrong with the file.
pub struct Config {
    pub levels: Vec<(&'static Rule, Level)>,
    pub problems: Vec<String>,
}

/// One place a rule found.
pub struct Finding {
    pub path: String,
    pub line: i64,
    pub name: String,
    pub kind: String,
}

/// The config at `given`, or `structuregate.rules.json` in `base_dir`; every rule `off` without one.
pub fn read(given: Option<&str>, base_dir: &str) -> Config {
    let mut config = Config { levels: all().iter().map(|rule| (rule, Level::Off)).collect(), problems: Vec::new() };
    let path = given.map(String::from).unwrap_or_else(|| Path::new(base_dir).join(FILE_NAME).to_string_lossy().into_owned());
    if !Path::new(&path).is_file() {
        if given.is_some() {
            config.problems.push(format!("{path} does not exist"));
        }
        return config;
    }
    let text = match std::fs::read(&path) {
        Ok(bytes) => String::from_utf8_lossy(bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&bytes)).into_owned(),
        Err(e) => {
            config.problems.push(format!("{path} could not be read - {e}"));
            return config;
        }
    };
    let root: Value = match serde_json::from_str(&crate::mapper::deep::sqlconfig::lenient(&text)) {
        Ok(root) => root,
        Err(e) => {
            config.problems.push(format!("{path} is not JSON - {e}"));
            return config;
        }
    };
    let Some(rules) = root["rules"].as_object() else {
        config.problems.push(format!("{path}: \"rules\" must be an object of rule id to off, warn or error"));
        return config;
    };
    for (id, value) in rules {
        let level = match value {
            Value::Bool(true) => Some(Level::Error),
            Value::Bool(false) => Some(Level::Off),
            Value::String(s) if s == "off" => Some(Level::Off),
            Value::String(s) if s == "warn" => Some(Level::Warn),
            Value::String(s) if s == "error" => Some(Level::Error),
            _ => None,
        };
        match (config.levels.iter_mut().find(|(rule, _)| rule.id == id), level) {
            (Some(slot), Some(level)) => slot.1 = level,
            (None, _) => config.problems.push(format!("{path}: no rule is called \"{id}\" - the rules are {}", ids())),
            (_, None) => config.problems.push(format!("{path}: rule \"{id}\" is off, warn or error, not {value}")),
        }
    }
    config
}

fn ids() -> String {
    all().iter().map(|rule| rule.id).collect::<Vec<_>>().join(", ")
}

/// Every place `rule` finds under `fragment` (a path's part, "" for all), waived lines left out. A map without the
/// columns the rule reads - no C# in the tree - finds nothing.
pub fn findings(db: &Connection, rule: &Rule, fragment: &str, texts: &mut HashMap<String, Option<Vec<String>>>) -> Result<Vec<Finding>> {
    if !rule.needs.iter().all(|(table, column)| has(db, table, column)) {
        return Ok(Vec::new());
    }
    let sql = format!("SELECT * FROM ({}) WHERE path LIKE ?1 ORDER BY path, line", (rule.sql)());
    let mut statement = db.prepare(&sql)?;
    let rows = statement.query_map([format!("%{fragment}%")], |r| {
        Ok(Finding { path: r.get(0)?, line: r.get(1)?, name: r.get::<_, Option<String>>(2)?.unwrap_or_default(), kind: r.get::<_, Option<String>>(3)?.unwrap_or_default() })
    })?;
    let mut out = Vec::new();
    for finding in rows {
        let finding = finding?;
        let lines = texts.entry(finding.path.clone()).or_insert_with(|| {
            crate::query::cat::on_disk(db, &finding.path).map(|text| text.lines().map(str::to_string).collect())
        });
        if !waived(lines.as_deref(), finding.line) {
            out.push(finding);
        }
    }
    Ok(out)
}

/// The line, or the one above it, carries the waiver.
fn waived(lines: Option<&[String]>, line: i64) -> bool {
    let Some(lines) = lines else { return false };
    let at = |n: i64| usize::try_from(n - 1).ok().and_then(|i| lines.get(i)).is_some_and(|text| text.contains(WAIVER));
    at(line) || at(line - 1)
}

/// Asked of the table's columns: `SELECT "x"` of a column that is not there reads as the STRING 'x' and prepares fine.
fn has(db: &Connection, table: &str, column: &str) -> bool {
    db.query_row("SELECT 1 FROM pragma_table_info(?1) WHERE name = ?2", [table, column], |_| Ok(())).is_ok()
}

/// THE RULES A `--map-sqlite` RUN ENFORCES: each rule switched on lists its findings; 1 when a rule at `error`
/// found one, or the config is wrong.
pub fn check(db_path: &str, config: &Config, out: &mut Vec<(&str, String)>) -> i64 {
    for problem in &config.problems {
        out.push(("e", format!("  error: rules config: {problem}")));
    }
    let mut exit = i64::from(!config.problems.is_empty());
    if config.levels.iter().all(|(_, level)| *level == Level::Off) {
        return exit;
    }
    let db = match Connection::open_with_flags(db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY) {
        Ok(db) => db,
        Err(e) => {
            out.push(("e", format!("  error: rules: cannot open {db_path} - {e}")));
            return 1;
        }
    };
    let mut texts = HashMap::new();
    for (rule, level) in &config.levels {
        if *level == Level::Off {
            continue;
        }
        let found = match findings(&db, rule, "", &mut texts) {
            Ok(found) => found,
            Err(e) => {
                out.push(("e", format!("  error: rule {} could not run - {e:#}", rule.id)));
                exit = 1;
                continue;
            }
        };
        let (word, stream) = if *level == Level::Error { ("error", "e") } else { ("warn", "o") };
        for f in &found {
            out.push((stream, format!("  rule {word} {}  {}:{}  {}: {} - {}", rule.id, f.path, f.line, f.name, rule.says, rule.fix)));
        }
        if *level == Level::Error && !found.is_empty() {
            exit = 1;
        }
    }
    exit
}

#[cfg(test)]
#[path = "rules_tests.rs"]
mod tests;
