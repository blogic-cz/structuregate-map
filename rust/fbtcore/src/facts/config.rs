//! `structuregate.facts.json`, beside the exe or at `--facts-config`: WHAT THE CONSUMER SAYS about its documents -
//! which to pull and where to save them, which table of which section is a list of facts, and which code each
//! list is checked against. Every path in it is relative to the file itself. The tool knows no document and no
//! table of its own: the means are the tool's, the configuration the consumer's.

use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

pub const FILE_NAME: &str = "structuregate.facts.json";

/// One document: where it comes from and the file its snapshot is.
#[derive(Clone, Debug)]
pub struct Source {
    pub name: String,
    /// `google-doc` (pulled by `--facts-pull`) or `file` (a snapshot the consumer keeps by hand).
    pub kind: String,
    pub id: String,
    /// The snapshot, absolute.
    pub path: PathBuf,
    /// The snapshot as the config spells it, for the rows.
    pub shown: String,
}

/// A column of a table: by the text of a header cell, or by its position from 1.
#[derive(Clone, Debug, PartialEq)]
pub enum Column {
    Name(String),
    At(usize),
}

/// One list of facts: the tables under a heading of a source.
#[derive(Clone, Debug)]
pub struct Facts {
    pub name: String,
    pub source: String,
    pub section: String,
    /// Which table under the heading, from 1; 0 is every one of them, as one list.
    pub table: usize,
    pub key: Column,
    pub label: Option<Column>,
}

/// One declared join from a list of facts to code.
#[derive(Clone, Debug)]
pub struct Link {
    pub facts: String,
    /// `seed`, `enum`, `consts` or `class`.
    pub kind: String,
    /// `Schema.Table` for a seed, a type's symbol (or name) otherwise.
    pub target: String,
    pub column: String,
    /// `value` (the default) or `name`: what an enum member or a constant is matched by.
    pub by: String,
}

#[derive(Clone, Debug, Default)]
pub struct Config {
    pub path: PathBuf,
    pub token_env: String,
    pub token_command: String,
    pub sources: Vec<Source>,
    pub facts: Vec<Facts>,
    pub links: Vec<Link>,
    /// The config's text and every snapshot's, hashed: what the passes over the rows are keyed by.
    pub stamp: String,
}

/// The config, None when there is none to read, or why it cannot be used. A file NAMED by `--facts-config` that is
/// not there is an error; the default one beside the exe is simply optional.
pub fn read(given: Option<&str>, base_dir: &str) -> Result<Option<Config>, String> {
    let path = given.map(PathBuf::from).unwrap_or_else(|| Path::new(base_dir).join(FILE_NAME));
    if !path.is_file() {
        return match given {
            Some(_) => Err(format!("{} does not exist", path.display())),
            None => Ok(None),
        };
    }
    let bytes = std::fs::read(&path).map_err(|e| format!("{} could not be read - {e}", path.display()))?;
    let text = String::from_utf8_lossy(bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&bytes)).into_owned();
    let root: Value = serde_json::from_str(&crate::mapper::deep::sqlconfig::lenient(&text))
        .map_err(|e| format!("{} could not be read - {e}", path.display()))?;
    let folder = std::path::absolute(&path).ok().and_then(|p| p.parent().map(Path::to_path_buf)).unwrap_or_default();
    let wrong = |what: String| format!("{}: {what}", path.display());
    let mut config = Config {
        path: path.clone(),
        token_env: string(&root["auth"]["token_env"]),
        token_command: string(&root["auth"]["token_command"]),
        ..Default::default()
    };
    for (i, entry) in list(&root["sources"]).iter().enumerate() {
        let name = string(&entry["name"]);
        let kind = if string(&entry["kind"]).is_empty() { "file".to_string() } else { string(&entry["kind"]) };
        let shown = match kind.as_str() {
            "google-doc" => string(&entry["save"]),
            "file" => string(&entry["path"]),
            other => return Err(wrong(format!("source {} has kind `{other}` - `google-doc` or `file`", i + 1))),
        };
        if name.is_empty() || shown.is_empty() {
            return Err(wrong(format!("source {} needs a `name`, and a `save` (google-doc) or a `path` (file)", i + 1)));
        }
        let id = string(&entry["id"]);
        if kind == "google-doc" && id.is_empty() {
            return Err(wrong(format!("source `{name}` is a google-doc with no `id`")));
        }
        config.sources.push(Source { name, kind, id, path: folder.join(&shown), shown });
    }
    for (i, entry) in list(&root["facts"]).iter().enumerate() {
        let section = string(&entry["section"]);
        let source = string(&entry["source"]);
        let key = column(&entry["key"]);
        if section.is_empty() || key.is_none() || !config.sources.iter().any(|s| s.name == source) {
            return Err(wrong(format!(
                "facts {} needs a `section`, a `key` (a header cell's text, or a column number from 1) and a `source` the config declares",
                i + 1
            )));
        }
        let name = if string(&entry["name"]).is_empty() { section.clone() } else { string(&entry["name"]) };
        let table = match &entry["table"] {
            Value::Null => 1,
            Value::String(s) if s.eq_ignore_ascii_case("all") => 0,
            Value::Number(n) if n.as_u64().is_some_and(|n| n >= 1) => n.as_u64().unwrap_or(1) as usize,
            other => return Err(wrong(format!("facts {}: `table` is a number from 1 or \"all\", not {other}", i + 1))),
        };
        config.facts.push(Facts { name, source, section, table, key: key.unwrap_or(Column::At(1)), label: column(&entry["label"]) });
    }
    for (i, entry) in list(&root["links"]).iter().enumerate() {
        let facts = string(&entry["facts"]);
        let to = &entry["to"];
        let kind = string(&to["kind"]);
        let target = match kind.as_str() {
            "seed" => string(&to["object"]),
            "enum" | "consts" | "class" => {
                if string(&to["symbol"]).is_empty() { string(&to["name"]) } else { string(&to["symbol"]) }
            }
            other => return Err(wrong(format!("link {} goes to `{other}` - `seed`, `enum`, `consts` or `class`", i + 1))),
        };
        let column = string(&to["column"]);
        if !config.facts.iter().any(|f| f.name == facts) || target.is_empty() || (kind == "seed" && column.is_empty()) {
            return Err(wrong(format!(
                "link {} needs `facts` naming a list the config declares, and its target (a seed's `object` and `column`, a type's `symbol`)",
                i + 1
            )));
        }
        let by = if string(&to["by"]).is_empty() { "value".to_string() } else { string(&to["by"]) };
        config.links.push(Link { facts, kind, target, column, by });
    }
    let mut hash = Sha256::new();
    hash.update(text.as_bytes());
    for source in &config.sources {
        hash.update(source.shown.as_bytes());
        hash.update(std::fs::read(&source.path).unwrap_or_default());
    }
    config.stamp = hash.finalize().iter().take(8).map(|b| format!("{b:02X}")).collect();
    Ok(Some(config))
}

/// A header cell's text, or a position from 1; None for nothing, an empty text or 0.
fn column(value: &Value) -> Option<Column> {
    match value {
        Value::Number(n) => n.as_u64().filter(|n| *n >= 1).map(|n| Column::At(n as usize)),
        Value::String(s) if !s.trim().is_empty() => Some(Column::Name(s.trim().to_string())),
        _ => None,
    }
}

fn string(value: &Value) -> String {
    value.as_str().unwrap_or("").trim().to_string()
}

fn list(value: &Value) -> Vec<Value> {
    value.as_array().cloned().unwrap_or_default()
}
