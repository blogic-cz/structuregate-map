//! `--facts-suggest`: WHICH PART OF A DOCUMENT LISTS WHAT THE CODE HOLDS, found by EXACT values, never by names. Every
//! column of every table, every section's code spans and the routes its text names are compared with every value set
//! the map holds - a seeded table's column, an enum's members, a class's constants, the endpoints, a type's
//! properties, a SQL table's columns - and a pair that shares enough values is written as a candidate fact and link
//! with its counts. Nothing is accepted here: a person or an agent reads the candidates and copies the right ones into
//! `structuregate.facts.json`, so the map run itself stays what the config says.

use super::config::{Config, Link};
use super::links::{self, Code};
use super::{apis, reader, words};
use crate::rows::dups::{has_columns, items_sql, text};
use rusqlite::Connection;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};

pub const FILE_NAME: &str = "structuregate.facts.suggest.json";

/// A pair is a candidate when this many document values are code values, and at least a fifth of the group.
const AT_LEAST: usize = 2;

/// One group of document values: what it is (as a fact entry of the config) and its distinct values in order.
struct Group {
    source: String,
    shown: String,
    fact: Value,
    values: Vec<String>,
}

/// One set of code values: the link that names it, and its values.
struct Set {
    link: Link,
    codes: Vec<Code>,
}

/// The candidates as lines to print, after writing them whole to `structuregate.facts.suggest.json` beside the config.
pub fn run(db: &str, config: &Config) -> Result<Vec<String>, String> {
    let conn = Connection::open_with_flags(db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(|e| format!("{db} could not be opened - {e}"))?;
    let mut lines = Vec::new();
    let mut groups = Vec::new();
    for source in &config.sources {
        let Ok(bytes) = std::fs::read(&source.path) else {
            lines.push(format!("  {}: no snapshot at {} - run --facts-pull", source.name, source.shown));
            continue;
        };
        match reader::blocks(&source.path, &bytes) {
            Ok(blocks) => groups.extend(groups_of(&source.name, &blocks)),
            Err(why) => lines.push(format!("  {}: not read - {why}", source.name)),
        }
    }
    let sets = sets(&conn).map_err(|e| format!("the map could not be read - {e}"))?;
    let found = pairs(&groups, &sets);
    let mut entries = Vec::new();
    for (n, (g, s, hit)) in found.iter().enumerate() {
        let (group, set) = (&groups[*g], &sets[*s]);
        let name = format!("{}-{}", group.source, n + 1);
        let mut fact = group.fact.clone();
        fact["name"] = json!(name);
        let link = link_entry(&name, &set.link);
        let unmatched: Vec<&String> = group.values.iter().filter(|v| !set.codes.iter().any(|c| links::matches(&set.link.kind, &c.value, v))).take(10).collect();
        let target = if set.link.target.is_empty() { "every endpoint".to_string() } else { set.link.target.clone() };
        let column = if set.link.column.is_empty() { String::new() } else { format!(".{}", set.link.column) };
        let by = if set.link.by == "name" { " (by name)" } else { "" };
        lines.push(format!(
            "  {}: {}/{} values = {} {target}{column}{by} ({} code values)",
            group.shown,
            hit,
            group.values.len(),
            set.link.kind,
            set.codes.len()
        ));
        entries.push(json!({
            "found": group.shown, "matched": hit, "of": group.values.len(), "code_values": set.codes.len(),
            "unmatched": unmatched, "facts": fact, "link": link,
        }));
    }
    let out = config.path.parent().unwrap_or(std::path::Path::new(".")).join(FILE_NAME);
    let body = json!({ "map": db, "candidates": entries });
    std::fs::write(&out, serde_json::to_string_pretty(&body).unwrap_or_default()).map_err(|e| format!("{} could not be written - {e}", out.display()))?;
    lines.insert(0, format!("facts suggest: {} candidate(s) -> {}", found.len(), out.display()));
    Ok(lines)
}

/// The config entry of a link, as `config.rs` reads it back.
fn link_entry(facts: &str, link: &Link) -> Value {
    let mut to = json!({ "kind": link.kind });
    match link.kind.as_str() {
        "seed" => {
            to["object"] = json!(link.target);
            to["column"] = json!(link.column);
        }
        "sql-column" => to["object"] = json!(link.target),
        "endpoint" if link.target.is_empty() => {}
        "endpoint" => to["class"] = json!(link.target),
        _ => to["symbol"] = json!(link.target),
    }
    if link.by == "name" {
        to["by"] = json!("name");
    }
    json!({ "facts": facts, "to": to })
}

/// Every group of a document: each column of each table (under its nearest heading), each section's code spans and
/// routes, and the whole document's.
fn groups_of(source: &str, blocks: &[reader::Block]) -> Vec<Group> {
    let mut out = Vec::new();
    let mut heading = String::new();
    for block in blocks {
        match block {
            reader::Block::Heading { text, .. } => heading = text.clone(),
            reader::Block::Table { rows, head } => out.extend(columns_of(source, blocks, &heading, rows, *head)),
            _ => {}
        }
    }
    let mut sections: Vec<String> = vec![String::new()];
    for block in blocks {
        if let reader::Block::Heading { text, .. } = block
            && !sections.contains(text)
        {
            sections.push(text.clone());
        }
    }
    for section in sections {
        let Ok(texts) = reader::texts(blocks, &section) else { continue };
        let place = if section.is_empty() { "the whole document".to_string() } else { format!("under \"{section}\"") };
        for (from, found) in [("code", words::code_spans(&texts)), ("routes", words::routes(&texts, ""))] {
            let values = distinct(found.into_iter().map(|w| w.key));
            if values.len() >= AT_LEAST {
                let what = if from == "code" { "code spans" } else { "routes" };
                out.push(Group { source: source.into(), shown: format!("{source}: {what} {place}"), fact: json!({"source": source, "section": section, "from": from}), values });
            }
        }
    }
    out
}

/// The columns of one table as groups: a column is named by its first row's cell when that names it alone.
fn columns_of(source: &str, blocks: &[reader::Block], heading: &str, rows: &[(usize, Vec<String>)], head: bool) -> Vec<Group> {
    // WHICH TABLE OF ITS SECTION it is, as `reader::section` counts: the heading's, or the document's when the
    // heading's text names more than one heading.
    let place = |section: &str| reader::section(blocks, section, 0).ok().and_then(|found| found.iter().position(|f| std::ptr::eq(f.rows, rows)));
    let (section, n) = match place(heading) {
        Some(n) => (heading.to_string(), n + 1),
        None => (String::new(), place("").map_or(1, |n| n + 1)),
    };
    let Some((_, first)) = rows.first() else { return vec![] };
    let width = rows.iter().map(|(_, r)| r.len()).max().unwrap_or(0);
    let mut out = Vec::new();
    for c in 0..width {
        let header = first.get(c).cloned().unwrap_or_default();
        let named = !header.is_empty() && first.iter().filter(|h| h.trim().eq_ignore_ascii_case(header.trim())).count() == 1;
        let data = if named || head { &rows[1..] } else { rows };
        let values = distinct(data.iter().filter_map(|(_, r)| r.get(c).cloned()));
        if values.len() < AT_LEAST {
            continue;
        }
        let key = if named { json!(header) } else { json!(c + 1) };
        let under = if section.is_empty() { "of the document".to_string() } else { format!("under \"{section}\"") };
        let label = if header.is_empty() { String::new() } else { format!(" \"{header}\"") };
        out.push(Group {
            source: source.into(),
            shown: format!("{source}: table {n} {under}, column {}{label}", c + 1),
            fact: json!({"source": source, "section": section, "table": n, "key": key}),
            values,
        });
    }
    out
}

fn distinct(values: impl Iterator<Item = String>) -> Vec<String> {
    let mut seen = HashSet::new();
    values.map(|v| v.trim().to_string()).filter(|v| !v.is_empty() && seen.insert(v.clone())).collect()
}

/// Which group shares enough values with which set: each set's best group, and each group's best set - once each,
/// the most values first.
fn pairs(groups: &[Group], sets: &[Set]) -> Vec<(usize, usize, usize)> {
    // EVERY VALUE ONCE, as its kind compares it, so a group is counted against every set in one pass of its values.
    let mut index: HashMap<String, Vec<usize>> = HashMap::new();
    let mut routed: Vec<usize> = Vec::new();
    for (s, set) in sets.iter().enumerate() {
        if set.link.kind == "endpoint" {
            routed.push(s);
            continue;
        }
        let keys: HashSet<String> = set.codes.iter().map(|c| key(&set.link.kind, &c.value)).collect();
        for k in keys {
            index.entry(k).or_default().push(s);
        }
    }
    let mut hits: Vec<(usize, usize, usize)> = Vec::new();
    for (g, group) in groups.iter().enumerate() {
        let mut counted: HashMap<usize, usize> = HashMap::new();
        for value in &group.values {
            let mut once: HashSet<usize> = HashSet::new();
            // A SET IS LOOKED UP BY THE KEY ITS OWN KIND MAKES: exact for a seed, ignoring case for a property.
            for (k, folded) in [(key("seed", value), false), (key("property", value), true)] {
                once.extend(index.get(&k).into_iter().flatten().copied().filter(|s| folds(&sets[*s].link.kind) == folded));
            }
            for s in &routed {
                if sets[*s].codes.iter().any(|c| apis::same_endpoint(&c.value, value)) {
                    once.insert(*s);
                }
            }
            once.into_iter().for_each(|s| *counted.entry(s).or_default() += 1);
        }
        hits.extend(counted.into_iter().filter(|(_, n)| *n >= AT_LEAST && n * 5 >= group.values.len()).map(|(s, n)| (g, s, n)));
    }
    let best = |of: &dyn Fn(&(usize, usize, usize)) -> usize| {
        let mut top: HashMap<usize, (usize, usize, usize)> = HashMap::new();
        for h in &hits {
            let slot = top.entry(of(h)).or_insert(*h);
            if h.2 > slot.2 || (h.2 == slot.2 && groups[h.0].values.len() < groups[slot.0].values.len()) {
                *slot = *h;
            }
        }
        top.into_values().collect::<Vec<_>>()
    };
    let mut kept: Vec<(usize, usize, usize)> = best(&|h| h.0);
    kept.extend(best(&|h| h.1));
    kept.sort_by(|a, b| b.2.cmp(&a.2).then(a.0.cmp(&b.0)).then(a.1.cmp(&b.1)));
    kept.dedup();
    kept
}

/// Whether a kind compares its values ignoring case.
fn folds(kind: &str) -> bool {
    matches!(kind, "property" | "sql-column")
}

/// A value as its kind compares it: a number by its value, a property or column name ignoring case, text as it is.
fn key(kind: &str, value: &str) -> String {
    let value = value.trim();
    match folds(kind) {
        true => value.to_lowercase(),
        false => value.parse::<f64>().map(|n| n.to_string()).unwrap_or_else(|_| value.to_string()),
    }
}

/// Every value set the map holds, by link kind.
fn sets(conn: &Connection) -> rusqlite::Result<Vec<Set>> {
    let mut out = seeds(conn)?;
    let link = |kind: &str, target: String, by: &str| Link { facts: String::new(), kind: kind.into(), target, column: String::new(), by: by.into() };
    let mut asked: Vec<Link> = vec![link("endpoint", String::new(), "value")];
    let enum_name = if has_columns(conn, "enums", &["symbol"]) { "CASE coalesce(symbol, '') WHEN '' THEN name ELSE symbol END" } else { "name" };
    for target in list(conn, &format!("SELECT DISTINCT {enum_name} FROM enums"), &["enums"])? {
        asked.push(link("enum", target.clone(), "value"));
        asked.push(link("enum", target, "name"));
    }
    let class_name = if has_columns(conn, "classes", &["symbol"]) { "CASE coalesce(c.symbol, '') WHEN '' THEN c.name ELSE c.symbol END" } else { "c.name" };
    let sql = format!("SELECT DISTINCT {class_name} FROM classes c JOIN consts k ON k.file = c.file AND k.cls = c.name");
    for target in list(conn, &sql, &["classes", "consts"])? {
        asked.push(link("consts", target.clone(), "value"));
        asked.push(link("consts", target, "name"));
    }
    if has_columns(conn, "members", &["class_symbol", "kind"]) {
        for target in list(conn, "SELECT DISTINCT class_symbol FROM members WHERE kind = 'property' AND coalesce(class_symbol, '') <> ''", &["members"])? {
            asked.push(link("property", target, "value"));
        }
    }
    for table in ["interfaces", "type_aliases"] {
        for target in list(conn, &format!("SELECT DISTINCT name FROM {table}"), &[table])? {
            asked.push(link("property", target, "value"));
        }
    }
    let sql = "SELECT DISTINCT CASE coalesce(o.schema, '') WHEN '' THEN 'dbo' ELSE o.schema END || '.' || o.name FROM sql_objects o \
               WHERE EXISTS (SELECT 1 FROM sql_columns c WHERE c.object = o.id)";
    for target in list(conn, sql, &["sql_objects", "sql_columns"])? {
        asked.push(link("sql-column", target, "value"));
    }
    // A SET THAT CANNOT BE READ (a name two enums share) is no candidate: `links::codes` says why, and here that is no news.
    out.extend(asked.into_iter().filter_map(|l| links::codes(conn, &l).ok().map(|codes| Set { link: l, codes })));
    Ok(out)
}

/// The first column of a query's rows, or none when a table it reads is not in this map.
fn list(conn: &Connection, sql: &str, tables: &[&str]) -> rusqlite::Result<Vec<String>> {
    if tables.iter().any(|t| crate::rows::schema::existing_columns(conn, t).unwrap_or_default().is_empty()) {
        return Ok(vec![]);
    }
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map([], |r| text(r, 0))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?.into_iter().filter(|t| !t.is_empty()).collect())
}

/// Every seeded table's every column, in one read: `(table, column) -> values`.
fn seeds(conn: &Connection) -> rusqlite::Result<Vec<Set>> {
    if !has_columns(conn, "sql_seeds", &["schema", "name", "status", "columns", "row_values"]) {
        return Ok(vec![]);
    }
    let (names, values) = (items_sql(conn, "sql_seeds", "columns", &["value"]), items_sql(conn, "sql_seeds", "row_values", &["value"]));
    let mut stmt = conn.prepare(&format!(
        "SELECT s.id, s.file, s.line, s.condition, CASE coalesce(s.schema, '') WHEN '' THEN 'dbo' ELSE s.schema END || '.' || s.name, c.value, v.value \
         FROM sql_seeds s JOIN {names} c ON c.owner = s.id JOIN {values} v ON v.owner = c.owner AND v.ord = c.ord \
         WHERE s.status <> 'temp' ORDER BY s.id, c.ord"
    ))?;
    let mut by: Vec<Set> = Vec::new();
    let mut at: HashMap<(String, String), usize> = HashMap::new();
    let rows = stmt.query_map([], |r| Ok((text(r, 0)?, text(r, 1)?, r.get::<_, Option<i64>>(2)?.unwrap_or(0), text(r, 3)?, text(r, 4)?, text(r, 5)?, text(r, 6)?)))?;
    for row in rows {
        let (id, file, line, condition, table, column, value) = row?;
        if value.is_empty() {
            continue;
        }
        let slot = *at.entry((table.to_lowercase(), column.to_lowercase())).or_insert_with(|| {
            by.push(Set { link: Link { facts: String::new(), kind: "seed".into(), target: table.clone(), column: column.clone(), by: "value".into() }, codes: Vec::new() });
            by.len() - 1
        });
        by[slot].codes.push(Code { target: id, file, line, value, name: String::new(), condition });
    }
    Ok(by)
}

#[cfg(test)]
#[path = "suggest_tests.rs"]
mod tests;
