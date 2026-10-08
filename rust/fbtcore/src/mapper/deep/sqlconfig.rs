//! WHAT THE CODE CANNOT SAY ABOUT ITS DATABASES - `structuregate.sql.json`, beside the exe or at
//! `--sql-config`, every path in it relative to the file itself. The same table can live in two database
//! projects, and which one a `DbContext` or a Dapper call reaches is decided at RUN time from a connection
//! string: no parse tree holds that, so the consumer states it. WITHOUT the file the map still parses every
//! `.sqlproj` and links a name only where it is unique - fewer links, none of them guessed.
//!
//! Read here, handed to the SQL half and its links (ScriptDom, in the caller) already resolved.

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::Path;

/// The methods whose SQL argument is parsed even without a file: Dapper, and EF's raw SQL. NOT `Dapper.*` -
/// `DynamicParameters.Add("Offset", ...)` is Dapper too, and its parameter NAME read as a statement.
const BUILT_IN_SQL_TEXT: [&str; 5] = [
    "Dapper.SqlMapper.*",
    "Dapper.CommandDefinition.CommandDefinition",
    "Microsoft.EntityFrameworkCore.RelationalQueryableExtensions.FromSqlRaw*",
    "Microsoft.EntityFrameworkCore.RelationalDatabaseFacadeExtensions.ExecuteSqlRaw*",
    "Microsoft.EntityFrameworkCore.RelationalDatabaseFacadeExtensions.SqlQueryRaw*",
];

/// `{databases: [{name, project, contexts, connections}], sql_text, default, unattributed_contexts, stamp, problem}`. The stamp is
/// folded into every SQL file's sha, because a database's NAME is in its rows and the file says what it is.
pub fn read(given: Option<&str>, base_dir: &str) -> Value {
    let path = given.map(String::from).unwrap_or_else(|| Path::new(base_dir).join("structuregate.sql.json").to_string_lossy().into_owned());
    let mut config = json!({ "databases": [], "sql_text": BUILT_IN_SQL_TEXT, "default": "", "unattributed_contexts": [], "stamp": "none", "problem": "" });
    if !Path::new(&path).is_file() {
        if given.is_some() {
            config["problem"] = json!(format!("{path} does not exist"));
        }
        return config;
    }
    let text = match std::fs::read(&path) {
        Ok(bytes) => String::from_utf8_lossy(bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&bytes)).into_owned(),
        Err(e) => {
            config["problem"] = json!(format!("{path} could not be read - {e}"));
            return config;
        }
    };
    config["stamp"] = json!(Sha256::digest(text.as_bytes()).iter().take(8).map(|b| format!("{b:02X}")).collect::<String>());
    let root: Value = match serde_json::from_str(&lenient(&text)) {
        Ok(root) => root,
        Err(e) => {
            config["problem"] = json!(format!("{path} could not be read - {e}"));
            return config;
        }
    };
    let folder = std::path::absolute(&path).ok().and_then(|p| p.parent().map(Path::to_path_buf)).unwrap_or_default();
    let databases: Vec<Value> = root["databases"].as_array().into_iter().flatten()
        .filter_map(|entry| {
            let name = entry["name"].as_str().unwrap_or("");
            if name.is_empty() {
                return None;
            }
            let project = entry["project"].as_str().unwrap_or("");
            let project = if project.is_empty() {
                String::new()
            } else {
                std::path::absolute(folder.join(project)).map(|p| p.to_string_lossy().into_owned()).unwrap_or_default()
            };
            Some(json!({ "name": name, "project": project, "contexts": strings(&entry["contexts"]),
                "connections": strings(&entry["connections"]) }))
        })
        .collect();
    config["databases"] = Value::Array(databases);
    let mut sql_text: Vec<String> = BUILT_IN_SQL_TEXT.map(String::from).to_vec();
    sql_text.extend(strings(&root["sqlText"]));
    config["sql_text"] = json!(sql_text);
    config["default"] = json!(root["default"].as_str().unwrap_or(""));
    config["unattributed_contexts"] = json!(strings(&root["unattributedContexts"]));
    config
}

fn strings(value: &Value) -> Vec<String> {
    value.as_array().into_iter().flatten().filter_map(|v| v.as_str()).filter(|s| !s.is_empty()).map(String::from).collect()
}

/// The config as strict JSON: `//` and `/* */` comments dropped and a comma before `}` or `]` removed,
/// the two liberties the file has always been allowed. Strings are copied untouched.
pub(crate) fn lenient(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '"' {
            out.push(c);
            i += 1;
            while i < chars.len() {
                out.push(chars[i]);
                if chars[i] == '\\' && i + 1 < chars.len() {
                    out.push(chars[i + 1]);
                    i += 2;
                    continue;
                }
                i += 1;
                if chars[i - 1] == '"' {
                    break;
                }
            }
            continue;
        }
        if c == '/' && chars.get(i + 1) == Some(&'/') {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }
        if c == '/' && chars.get(i + 1) == Some(&'*') {
            i += 2;
            while i < chars.len() && !(chars[i] == '*' && chars.get(i + 1) == Some(&'/')) {
                i += 1;
            }
            i += 2;
            continue;
        }
        if c == ']' || c == '}' {
            let trimmed = out.trim_end().len();
            if out[..trimmed].ends_with(',') {
                out.truncate(trimmed - 1);
            }
        }
        out.push(c);
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comments_and_trailing_commas_are_the_files_two_liberties() {
        let text = "{ // a note\n \"a\": [1, 2,], /* b */ \"s\": \"// not a comment,]\", }";
        let parsed: Value = serde_json::from_str(&lenient(text)).unwrap();
        assert_eq!(parsed, json!({ "a": [1, 2], "s": "// not a comment,]" }));
    }
}
