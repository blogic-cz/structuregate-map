//! HOW `--facts-review` ASKS (`review.rs` reads the answers):
//!
//! - IN BATCHES: a large document gives a thousand candidates, and one question of a quarter million tokens leaves
//!   a model no room to look anything up. The candidates go in batches of about `BATCH_CHARS`, every candidate of one
//!   document part in the same batch - whether one part is ambiguous is judged within it.
//! - WITH THE MAP AS A TOOL: the candidate text cannot settle everything (which class an endpoint takes, which type an
//!   inherited action returning `T` serves). Each question offers a read-only SQL tool over the map the candidates
//!   were found in, described by the tables that map holds, and the prompt says when to use it. The program that
//!   answers runs the tool (`crate::llm`); `STRUCTUREGATE_LLM_TOOLS` turns it off (`0`, `off`, `false`, `none`, for a
//!   program or backend that cannot run tools) or sets how many calls a batch may make (a number; 8 unless set).

use crate::llm::Ask;
use serde_json::{json, Value};

pub const TOOLS_ENV: &str = "STRUCTUREGATE_LLM_TOOLS";
pub const TOOL_NAME: &str = "map_query";

/// A batch closes before its listed candidates pass this many characters (one part larger than it is a batch alone).
const BATCH_CHARS: usize = 60_000;
/// The tool calls one batch may make unless `STRUCTUREGATE_LLM_TOOLS` says another number.
const CALLS: u64 = 8;
/// The tables a review question may need, described by the columns this map holds for them.
const TABLES: [&str; 19] = [
    "http_endpoints",
    "http_calls",
    "calls",
    "files",
    "file_text",
    "functions",
    "parameters",
    "members",
    "classes",
    "decorators",
    "enums",
    "consts",
    "interfaces",
    "type_members",
    "sql_seeds",
    "sql_objects",
    "sql_columns",
    "doc_facts",
    "doc_links",
];

const SYSTEM: &str = "You review candidate links between a DOCUMENT and a CODE MAP. Each candidate pairs one part of the document \
(a table column, the code spans or the routes of a section) with one set of values the code holds (a seeded table column, an enum's \
members, a class's constants, the HTTP endpoints, a type's properties, a SQL table's columns). They were paired only because they \
share values. ACCEPT a candidate when the document's part is a list of the same thing the code set holds. GAPS ARE EXPECTED: a value \
only one side has is exactly what the check that follows will report, so unmatched values are never a reason to reject - judge \
whether the two sides NAME THE SAME CONCEPT, not whether they agree. REJECT it only when the overlap is a coincidence: small numbers or common words that \
any list shares, a column of a different concept that happens to reuse some values, or a pair another candidate covers better. \
`endpoints_using` lists the HTTP endpoints that take (request) or return (response) a type, resolved by the compiler - the way to tell apart classes that share a table's names. When a candidate carries `section_endpoints`, the document's part sits under a section naming those HTTP endpoints, and `endpoint_type` says whether the code set is a type such an endpoint takes (request) or returns (response), as the compiler resolved it; a type's properties with `endpoint_type` null are another class that only shares the names - prefer the one the endpoint uses. ";

const WITHOUT_TOOL: &str = "Judge only from what you are given. Answer every candidate by its number, with a reason of one sentence.";

const WITH_TOOL: &str = "When the candidate text cannot settle it - which type an endpoint takes or returns, which class a field table \
describes, an inherited action whose type is a generic parameter, whether a value is really the one the code holds - query the code map \
with the `map_query` tool (read-only SQL) before you decide, and say in the reason what the map showed. Otherwise judge from what you are \
given. Answer every candidate by its number, with a reason of one sentence.";

/// One candidate as the question lists it: its number, what it pairs, its counts, a sample of each side, the endpoint
/// evidence when it has some.
pub fn listed(n: usize, c: &Value) -> Value {
    let mut one = json!({
        "n": n, "document": c["found"], "code": c["link"]["to"], "matched": c["matched"], "of": c["of"],
        "code_values": c["code_values"], "matched_values": c["matched_values"], "unmatched_document_values": c["unmatched"],
    });
    if let Some(users) = c.get("endpoints_using") {
        one["endpoints_using"] = users.clone();
        one["endpoints_using_count"] = c["endpoints_using_count"].clone();
    }
    if let Some(endpoints) = c.get("section_endpoints") {
        one["section_endpoints"] = endpoints.clone();
        one["endpoint_type"] = c["endpoint_type"].clone();
    }
    one
}

/// The candidates (by index) in batches: each document part's candidates together, parts in the order they first
/// appear, a batch closed before it would pass `limit` characters.
pub fn batches(candidates: &[Value], limit: usize) -> Vec<Vec<usize>> {
    let mut parts: Vec<(String, Vec<usize>)> = Vec::new();
    for (i, c) in candidates.iter().enumerate() {
        let found = c["found"].as_str().unwrap_or("").to_string();
        match parts.iter_mut().find(|(f, _)| *f == found) {
            Some((_, of)) => of.push(i),
            None => parts.push((found, vec![i])),
        }
    }
    let size = |i: &usize| listed(i + 1, &candidates[*i]).to_string().len();
    let mut out: Vec<Vec<usize>> = Vec::new();
    let mut used = 0;
    for (_, part) in parts {
        let chars: usize = part.iter().map(size).sum();
        if out.is_empty() || used + chars > limit {
            out.push(Vec::new());
            used = 0;
        }
        used += chars;
        out.last_mut().expect("pushed above").extend(part);
    }
    out
}

/// The batches of `batches` at the size a question is asked in.
pub fn in_batches(candidates: &[Value]) -> Vec<Vec<usize>> {
    batches(candidates, BATCH_CHARS)
}

/// The map as a read-only SQL tool, or None: switched off by `setting`, or no map at `map` to open.
pub fn map_tool(map: &str, setting: &str) -> Option<Value> {
    let setting = setting.trim().to_lowercase();
    if matches!(setting.as_str(), "0" | "off" | "false" | "none") {
        return None;
    }
    let calls = setting.parse::<u64>().unwrap_or(CALLS);
    // ABSOLUTE, NOT CANONICAL: on Windows a canonical path is `\\?\C:\..`, which a program may not open.
    let path = std::path::absolute(map).ok().filter(|p| p.is_file())?;
    let conn = rusqlite::Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).ok()?;
    let tables: Vec<String> = TABLES
        .iter()
        .filter_map(|t| {
            let columns = crate::rows::schema::existing_columns(&conn, t).unwrap_or_default();
            (!columns.is_empty()).then(|| format!("{t}({})", columns.join(", ")))
        })
        .collect();
    let description = format!(
        "Read-only SQLite over the code map the candidates were found in. Tables: {}. A list or object cell is a child table \
         <table>__<column> of nodes: owner (the row's id), n, parent (0 = the root), ord, role, value. Joins that answer most questions: \
         an endpoint's return type - http_endpoints e JOIN functions f ON f.id = e.func (f.type); its parameters - parameters p ON \
         p.file = f.file AND p.qualname = f.qualname; a class's properties - members WHERE kind = 'property' AND class_symbol = ?; \
         a url the code calls - http_calls h JOIN calls c ON c.id = h.call (c.symbol: the method bound, with its type arguments, \
         `Api.PostAsync<ReqDto, RespDto>`; h.file is a path, c.file a files.id - files.path names it); a text in any file - \
         SELECT path FROM file_text WHERE file_text MATCH '\"words\"'.",
        tables.join("; ")
    );
    Some(json!({ "type": "sqlite_query", "path": path.to_string_lossy(), "name": TOOL_NAME, "description": description, "max_calls": calls }))
}

/// The question for one batch.
pub fn ask(candidates: &[Value], batch: &[usize], tool: Option<&Value>) -> Ask {
    let listed: Vec<Value> = batch.iter().map(|&i| listed(i + 1, &candidates[i])).collect();
    let schema = json!({
        "type": "object", "required": ["verdicts"],
        "properties": { "verdicts": { "type": "array", "items": {
            "type": "object", "required": ["n", "accept", "reason"],
            "properties": { "n": { "type": "integer" }, "accept": { "type": "boolean" }, "reason": { "type": "string" } }
        } } }
    });
    Ask {
        system: format!("{SYSTEM}{}", if tool.is_some() { WITH_TOOL } else { WITHOUT_TOOL }),
        message: serde_json::to_string_pretty(&json!({ "candidates": listed })).unwrap_or_default(),
        schema,
        tools: tool.cloned().into_iter().collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(found: &str, pad: usize) -> Value {
        json!({"found": found, "matched": 1, "of": 1, "code_values": 1, "matched_values": ["x".repeat(pad)], "unmatched": [], "link": {"to": {}}})
    }

    #[test]
    fn a_documents_parts_stay_whole_and_a_batch_closes_before_its_limit() {
        let all = [candidate("a", 100), candidate("b", 100), candidate("a", 100), candidate("c", 100), candidate("d", 1000)];
        let one = listed(1, &all[0]).to_string().len();
        assert_eq!(batches(&all, 3 * one), [vec![0, 2, 1], vec![3], vec![4]]);
        assert_eq!(batches(&all, 100_000), [vec![0, 2, 1, 3, 4]]);
        assert_eq!(batches(&[], 10), Vec::<Vec<usize>>::new());
    }

    #[test]
    fn the_map_is_a_tool_described_by_its_tables_unless_switched_off_and_the_prompt_says_when_to_use_it() {
        let dir = std::env::temp_dir().join(format!("fbt-asking-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let map = dir.join("map.sqlite");
        rusqlite::Connection::open(&map).unwrap().execute_batch("CREATE TABLE http_endpoints (id, verb, template, func);").unwrap();
        let tool = map_tool(map.to_str().unwrap(), "").unwrap();
        assert_eq!((tool["type"].as_str(), tool["name"].as_str(), tool["max_calls"].as_u64()), (Some("sqlite_query"), Some(TOOL_NAME), Some(8)));
        assert_eq!(tool["path"].as_str(), map.to_str());
        let said = tool["description"].as_str().unwrap();
        assert!(said.contains("Tables: http_endpoints(id, verb, template, func)."), "{said}");
        assert!(!said.contains("functions("), "{said}");
        assert!(said.contains("http_calls h JOIN calls c ON c.id = h.call") && said.contains("file_text MATCH"), "{said}");
        assert_eq!(map_tool(map.to_str().unwrap(), "3").unwrap()["max_calls"], 3);
        assert!(map_tool(map.to_str().unwrap(), " Off ").is_none());
        assert!(map_tool(dir.join("none.sqlite").to_str().unwrap(), "").is_none());
        let all = [candidate("a", 1), candidate("b", 1)];
        let with = ask(&all, &[1], Some(&tool));
        assert!(with.system.contains("`map_query` tool") && !with.system.contains("Judge only"), "{}", with.system);
        assert_eq!(with.tools, [tool.clone()]);
        let message: Value = serde_json::from_str(&with.message).unwrap();
        assert_eq!(message["candidates"][0]["n"], 2);
        let without = ask(&all, &[0, 1], None);
        assert!(without.system.ends_with(WITHOUT_TOOL) && without.tools.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
