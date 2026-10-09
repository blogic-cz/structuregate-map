//! `--claude-hook <event>`: what the Claude Code plugin's hooks run, so a session in a mapped tree ASKS THE MAP
//! instead of grepping for a symbol. In the exe because a `PreToolUse` hook runs before every search: a script host
//! costs half a second each time, this costs none.
//!
//!   `session-start`  the tree has `buildmap.sqlite`: say so, how old it is, and the three queries that answer most
//!                    questions - injected into the session as context.
//!   `pre-tool-use`   a `Grep`, or a `grep`/`rg` in `Bash`, for a word spelled like a code symbol: the search RUNS,
//!                    and the model is told which `--map-query` answers the same question from the compiler-bound
//!                    map. A NUDGE, never a permission decision - `allow` would approve the call past the user's own
//!                    rules. `STRUCTUREGATE_HOOK=deny` refuses it instead, for a team that wants it enforced.
//!
//! SILENT WHERE IT CANNOT HELP: no map in the project, a pattern that is prose or a regex, a malformed input. A hook
//! that talks when it has nothing to say gets switched off. At most `NUDGES` per session, counted in a temp file.

use super::Out;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

const NUDGES: u32 = 3;
const SEARCHERS: [&str; 5] = ["grep", "egrep", "rg", "ag", "ack"];

pub fn run(event: &str, out: &mut Out) -> i64 {
    let input: Value = std::io::read_to_string(std::io::stdin()).ok()
        .and_then(|text| serde_json::from_str(&text).ok()).unwrap_or(Value::Null);
    let root = std::env::var("CLAUDE_PROJECT_DIR").ok().filter(|r| !r.is_empty())
        .or_else(|| input["cwd"].as_str().map(str::to_string)).unwrap_or_else(|| ".".into());
    let db = Path::new(&root).join("buildmap.sqlite");
    if !db.is_file() {
        return 0;
    }
    let reply = match event {
        "session-start" => Some(session_start(&db)),
        "pre-tool-use" => pre_tool_use(&input),
        _ => None,
    };
    if let Some(reply) = reply {
        out.line(reply.to_string());
    }
    0
}

fn session_start(db: &Path) -> Value {
    let age = std::fs::metadata(db).and_then(|m| m.modified()).ok()
        .and_then(|t| t.elapsed().ok()).map(|d| d.as_secs() / 60);
    let built = match age {
        Some(m) if m < 120 => format!("{m} minute(s) ago"),
        Some(m) => format!("{} hour(s) ago", m / 60),
        None => "at an unknown time".into(),
    };
    let text = format!(
        "This repository has a structuregate code map: `buildmap.sqlite` (every declaration, call, read and expression, \
         bound by each language's own compiler; built {built}) and `buildmap.json` (what imports what). Ask it BEFORE \
         grepping for a symbol - grep cannot tell a call from a comment or a same-named symbol elsewhere:\n\
         - `buildtools/structuregate --map-query buildmap.sqlite --find <Name>`: who declares, calls and reads it\n\
         - `buildtools/structuregate --map-query buildmap.sqlite --text \"<phrase>\"`: full-text over the code\n\
         - `buildtools/structuregate --map-query buildmap.sqlite --sql \"SELECT ...\"`: exact counts and joins\n\
         The `map-sqlite` and `buildmap` skills describe the tables. Grep stays right for text the map does not hold: \
         configs, logs, prose."
    );
    json!({"hookSpecificOutput": {"hookEventName": "SessionStart", "additionalContext": text}})
}

fn pre_tool_use(input: &Value) -> Option<Value> {
    let pattern = match input["tool_name"].as_str()? {
        "Grep" => input["tool_input"]["pattern"].as_str().map(str::to_string),
        "Bash" => searched(input["tool_input"]["command"].as_str()?),
        _ => None,
    }?;
    let symbol = pattern.trim_matches(|c| c == '"' || c == '\'');
    if !looks_like_symbol(symbol) || !counted(input["session_id"].as_str().unwrap_or("")) {
        return None;
    }
    let advice = format!(
        "`{symbol}` looks like a code symbol. `buildtools/structuregate --map-query buildmap.sqlite --find {symbol}` \
         answers who declares, calls and reads it from the compiler-bound map, which a text search cannot tell apart \
         from a comment or a same-named symbol (see the `map-sqlite` skill)."
    );
    let deny = std::env::var("STRUCTUREGATE_HOOK").is_ok_and(|v| v == "deny");
    Some(if deny {
        json!({"hookSpecificOutput": {"hookEventName": "PreToolUse", "permissionDecision": "deny", "permissionDecisionReason": advice}})
    } else {
        json!({"hookSpecificOutput": {"hookEventName": "PreToolUse", "additionalContext": advice}})
    })
}

/// The pattern a `grep`/`rg` in a shell command searches for: the word after the searcher that is not a flag, or
/// the one after `-e`. Only the first searcher of the command; a pipeline's later `grep` filters output, not code.
fn searched(command: &str) -> Option<String> {
    let words: Vec<&str> = command.split_whitespace().collect();
    let at = words.iter().position(|w| SEARCHERS.contains(&w.rsplit('/').next().unwrap_or(w)))
        .or_else(|| words.windows(2).position(|p| p[0] == "git" && p[1] == "grep").map(|i| i + 1))?;
    let mut rest = words[at + 1..].iter();
    while let Some(w) = rest.next() {
        match *w {
            "-e" | "--regexp" => return rest.next().map(|p| p.to_string()),
            "|" | "&&" | ";" => return None,
            flag if flag.starts_with('-') => {}
            word => return Some(word.to_string()),
        }
    }
    None
}

/// `OrderService`, `get_user`, `isVisible`: letters, digits and `_`, a lowercase letter AND an uppercase one or an
/// underscore - so prose (`error`), a shouted word (`TODO`) and every regex stay grep's.
fn looks_like_symbol(s: &str) -> bool {
    let mut chars = s.chars();
    let first_ok = chars.next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_');
    let word = s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    let lower = s.chars().any(|c| c.is_ascii_lowercase());
    let marked = s.chars().any(|c| c.is_ascii_uppercase() || c == '_');
    s.len() >= 4 && first_ok && word && lower && marked
}

/// Whether this session still has a nudge left, counting this one.
fn counted(session: &str) -> bool {
    if session.is_empty() {
        return true;
    }
    let name: String = session.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').collect();
    let file: PathBuf = std::env::temp_dir().join(format!("structuregate-hook-{name}"));
    let used: u32 = std::fs::read_to_string(&file).ok().and_then(|t| t.trim().parse().ok()).unwrap_or(0);
    if used >= NUDGES {
        return false;
    }
    std::fs::write(&file, (used + 1).to_string()).is_ok() || used == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_shell_search_names_its_pattern_and_a_later_filter_is_not_one() {
        assert_eq!(searched("rg -n OrderService src").as_deref(), Some("OrderService"));
        assert_eq!(searched("grep -rn -e get_user .").as_deref(), Some("get_user"));
        assert_eq!(searched("git grep isVisible").as_deref(), Some("isVisible"));
        assert_eq!(searched("/usr/bin/grep -i OrderService x").as_deref(), Some("OrderService"));
        assert_eq!(searched("ls -la | head"), None);
        assert_eq!(searched("rg -n | head"), None, "the next command is not the pattern");
    }

    #[test]
    fn only_a_word_spelled_like_code_is_a_symbol() {
        for yes in ["OrderService", "get_user", "isVisible", "Send"] {
            assert!(looks_like_symbol(yes), "{yes}");
        }
        for no in ["error", "TODO", "Order.*Service", "a b", "Id", "\\bfoo\\b"] {
            assert!(!looks_like_symbol(no), "{no}");
        }
    }

    #[test]
    fn a_grep_for_a_symbol_is_a_nudge_never_a_permission() {
        let input = json!({"tool_name": "Grep", "tool_input": {"pattern": "OrderService"}});
        let reply = pre_tool_use(&input).expect("nudged");
        assert!(reply["hookSpecificOutput"].get("permissionDecision").is_none(), "allow would bypass the user's rules");
        assert!(reply["hookSpecificOutput"]["additionalContext"].as_str().unwrap().contains("--find OrderService"));
        let prose = json!({"tool_name": "Grep", "tool_input": {"pattern": "connection refused"}});
        assert!(pre_tool_use(&prose).is_none());
    }
}
