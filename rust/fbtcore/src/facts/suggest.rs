//! `--facts-suggest`: WHICH PART OF A DOCUMENT LISTS WHAT THE CODE HOLDS, found by EXACT values, never by names. Every
//! column of every table, every section's code spans and the routes its text names are compared with every value set
//! the map holds - a seeded table's column, an enum's members, a class's constants, the endpoints, a type's
//! properties, a SQL table's columns - and a pair that shares enough values is written as a candidate fact and link
//! with its counts. Nothing is accepted here: a person or an agent reads the candidates and copies the right ones into
//! `structuregate.facts.json`, so the map run itself stays what the config says.

use super::config::{Config, Link};
use super::links::{self, Code};
use super::rank::{self, Score, Table};
use super::uses::{self, Bound, Role};
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
    /// A table column's rows, so the rest of a row can be asked whether it agrees (`rank.rs`).
    table: Option<Table>,
    /// The routes its place is under (`routes_at`), and the endpoints they name with the classes those use.
    routes: Vec<String>,
    bound: Bound,
}

/// One set of code values: the link that names it, and its values.
struct Set {
    link: Link,
    codes: Vec<Code>,
    /// A seed row's every column, by its id: what a document row's other cells may agree with.
    others: HashMap<String, Vec<(String, String)>>,
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
    let used = uses::load(&conn).map_err(|e| format!("the map's endpoints could not be read - {e}"))?;
    for group in &mut groups {
        group.bound = used.of(&group.routes);
    }
    let users = used.users();
    let found = pairs(&groups, &sets);
    let mut entries = Vec::new();
    for (n, (g, s, score)) in found.iter().enumerate() {
        let (group, set) = (&groups[*g], &sets[*s]);
        let hit = score.hits;
        let name = format!("{}-{}", group.source, n + 1);
        let mut fact = group.fact.clone();
        fact["name"] = json!(name);
        // THE AGREEING COLUMN IS THE LABEL: named as the key is, by its header when that names it alone.
        if score.agree >= AT_LEAST {
            fact["label"] = group.fact.get("headers").and_then(|h| h.get(score.doc_column - 1)).filter(|h| h.as_str().is_some_and(|t| !t.is_empty())).cloned().unwrap_or(json!(score.doc_column));
        }
        if let Some(object) = fact.as_object_mut() {
            object.remove("headers");
        }
        let link = link_entry(&name, &set.link);
        let (matched, unmatched): (Vec<&String>, Vec<&String>) = group.values.iter().partition(|v| set.codes.iter().any(|c| links::matches(&set.link.kind, &c.value, v)));
        let (matched, unmatched): (Vec<&String>, Vec<&String>) = (matched.into_iter().take(10).collect(), unmatched.into_iter().take(10).collect());
        let target = if set.link.target.is_empty() { "every endpoint".to_string() } else { set.link.target.clone() };
        let column = if set.link.column.is_empty() { String::new() } else { format!(".{}", set.link.column) };
        let by = if set.link.by == "name" { " (by name)" } else { "" };
        let agree = if score.agree >= AT_LEAST { format!(", {} rows agree: column {} = {}", score.agree, score.doc_column, score.code_column) } else { String::new() };
        let chance = if score.chance >= 1.0 { format!(", ~{:.0} by chance", score.chance) } else { String::new() };
        let endpoints = group.bound.endpoints.join(", ");
        let typed = typed(group, set);
        let used = match typed {
            Some(role) => format!(", the {} type of {}", role.word(), group.bound.used_by.get(&set.link.target).map(|e| e.join(", ")).unwrap_or_default()),
            None if set.link.kind == "property" && !endpoints.is_empty() => format!(", no type of {endpoints}"),
            None => String::new(),
        };
        lines.push(format!(
            "  {}: {}/{} values = {} {target}{column}{by} ({} code values{agree}{used}{chance})",
            group.shown,
            hit,
            group.values.len(),
            set.link.kind,
            set.codes.len()
        ));
        entries.push(json!({
            "found": group.shown, "matched": hit, "of": group.values.len(), "code_values": set.codes.len(),
            "by_chance": (score.chance * 10.0).round() / 10.0, "rows_agree": score.agree,
            "agree_on": if score.agree >= AT_LEAST { json!({"document_column": score.doc_column, "code": score.code_column}) } else { Value::Null },
            "matched_values": matched, "unmatched": unmatched, "facts": fact, "link": link,
        }));
        // EVERY ENDPOINT USING A PROPERTY SET'S CLASS, wherever the document names it: when several classes share a table's
        // names, which endpoint takes or returns each is what tells them apart.
        if set.link.kind == "property"
            && let Some(by) = users.get(&set.link.target)
        {
            let entry = entries.last_mut().expect("just pushed");
            entry["endpoints_using"] = json!(by.iter().take(10).collect::<Vec<_>>());
            entry["endpoints_using_count"] = json!(by.len());
        }
        // THE ENDPOINT THE SECTION NAMES, and whether this type is one it takes or returns - resolved symbols, not names.
        if !group.bound.endpoints.is_empty() {
            let entry = entries.last_mut().expect("just pushed");
            entry["section_endpoints"] = json!(group.bound.endpoints);
            entry["endpoint_type"] = json!(typed.map(Role::word));
        }
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
    for (i, block) in blocks.iter().enumerate() {
        match block {
            reader::Block::Heading { text, .. } => heading = text.clone(),
            reader::Block::Table { rows, head } => {
                let routes = routes_at(blocks, i);
                out.extend(columns_of(source, blocks, &heading, rows, *head).into_iter().map(|g| Group { routes: routes.clone(), ..g }));
            }
            _ => {}
        }
    }
    let mut sections: Vec<(String, Option<usize>)> = vec![(String::new(), None)];
    for (i, block) in blocks.iter().enumerate() {
        if let reader::Block::Heading { text, .. } = block
            && !sections.iter().any(|(s, _)| s == text)
        {
            sections.push((text.clone(), Some(i)));
        }
    }
    for (section, at) in sections {
        let Ok(texts) = reader::texts(blocks, &section) else { continue };
        let place = if section.is_empty() { "the whole document".to_string() } else { format!("under \"{section}\"") };
        for (from, found) in [("code", words::code_spans(&texts)), ("routes", words::routes(&texts, ""))] {
            let values = distinct(found.into_iter().map(|w| w.key));
            if values.len() >= AT_LEAST {
                let what = if from == "code" { "code spans" } else { "routes" };
                out.push(Group {
                    source: source.into(),
                    shown: format!("{source}: {what} {place}"),
                    fact: json!({"source": source, "section": section, "from": from}),
                    values,
                    table: None,
                    routes: at.map(|i| routes_at(blocks, i)).unwrap_or_default(),
                    bound: Bound::default(),
                });
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
        // THE HEADERS RIDE THE FACT until it is written, so an agreeing column can be named as the key is.
        let headers: Vec<Value> = first
            .iter()
            .map(|h| if !h.is_empty() && first.iter().filter(|x| x.trim().eq_ignore_ascii_case(h.trim())).count() == 1 { json!(h) } else { json!("") })
            .collect();
        out.push(Group {
            source: source.into(),
            shown: format!("{source}: table {n} {under}, column {}{label}", c + 1),
            fact: json!({"source": source, "section": section, "table": n, "key": key, "headers": if named { headers } else { vec![] }}),
            values,
            table: Some(Table { rows: data.iter().map(|(_, r)| r.clone()).collect(), column: c }),
            routes: Vec::new(),
            bound: Bound::default(),
        });
    }
    out
}

/// The routes a place in a document is under: those its own heading and each heading above it name, and those the
/// text right under its own heading names (up to the next heading). A field table under `3.4.2 POST /api/items`, or
/// under `Response` below it, describes that endpoint's types; an overview above it that lists every route, or a
/// sibling section's route, is no part of it.
fn routes_at(blocks: &[reader::Block], at: usize) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut level = usize::MAX;
    for h in (0..=at.min(blocks.len().saturating_sub(1))).rev() {
        let reader::Block::Heading { level: l, text } = &blocks[h] else { continue };
        if *l >= level {
            continue;
        }
        let own = level == usize::MAX;
        level = *l;
        out.extend(words::routes_in(text));
        for block in blocks[h + 1..].iter().take_while(|b| own && !matches!(b, reader::Block::Heading { .. })) {
            if let reader::Block::Text { text, .. } = block {
                out.extend(words::routes_in(text));
            }
        }
    }
    distinct(out.into_iter())
}

/// What a property set's class is to the endpoints its group's section names - None for any other set.
fn typed(group: &Group, set: &Set) -> Option<Role> {
    (set.link.kind == "property").then(|| group.bound.classes.get(&set.link.target).copied()).flatten()
}

fn distinct(values: impl Iterator<Item = String>) -> Vec<String> {
    let mut seen = HashSet::new();
    values.map(|v| v.trim().to_string()).filter(|v| !v.is_empty() && seen.insert(v.clone())).collect()
}

/// Which group shares enough values with which set: each set's best group, and each group's best set - once each,
/// the most values first.
fn pairs(groups: &[Group], sets: &[Set]) -> Vec<(usize, usize, Score)> {
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
    let mut hits: Vec<(usize, usize, Score)> = Vec::new();
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
        for (s, n) in counted.into_iter().filter(|(_, n)| *n >= AT_LEAST && n * 5 >= group.values.len()) {
            let set = &sets[s];
            let (agree, doc_column, code_column) = match &group.table {
                Some(table) => rank::agreement(table, &set.link.kind, &set.link.column, &set.codes, &set.others),
                None => Default::default(),
            };
            let chance = rank::chance(&group.values, &set.codes);
            let used = usize::from(typed(group, set).is_some());
            hits.push((g, s, Score { hits: n, chance, agree, doc_column, code_column, used }));
        }
    }
    // A TIE IS BROKEN THE SAME WAY EVERY RUN: the higher rank, then the smaller document group, then the smaller code
    // set (the more specific one), then the link by its names. `counted` is a hash map, so its order is no order.
    let order = |a: &(usize, usize, Score), b: &(usize, usize, Score)| {
        let link = |h: &(usize, usize, Score)| {
            let l = &sets[h.1].link;
            (sets[h.1].codes.len(), l.kind.clone(), l.target.clone(), l.column.clone(), l.by.clone())
        };
        b.2.rank().cmp(&a.2.rank()).then(groups[a.0].values.len().cmp(&groups[b.0].values.len())).then_with(|| link(a).cmp(&link(b))).then(a.0.cmp(&b.0))
    };
    let best = |of: &dyn Fn(&(usize, usize, Score)) -> usize| {
        let mut top: HashMap<usize, &(usize, usize, Score)> = HashMap::new();
        for h in &hits {
            let slot = top.entry(of(h)).or_insert(h);
            if order(h, slot).is_lt() {
                *slot = h;
            }
        }
        top.into_values().cloned().collect::<Vec<_>>()
    };
    let mut kept: Vec<(usize, usize, Score)> = best(&|h| h.0);
    kept.extend(best(&|h| h.1));
    kept.sort_by(|a, b| b.2.rank().cmp(&a.2.rank()).then(a.0.cmp(&b.0)).then_with(|| order(a, b)));
    kept.dedup_by(|a, b| a.0 == b.0 && a.1 == b.1);
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
    // EACH KIND READ ONCE for every target: asked per target, thousands of classes scanned `members` thousands of times.
    let (enums, consts, properties, columns) = (links::Enums::read(conn)?, links::Consts::read(conn)?, apis::Properties::read(conn)?, apis::Columns::read(conn)?);
    let codes = |l: &Link| -> Option<Vec<Code>> {
        match l.kind.as_str() {
            // A SET THAT CANNOT BE READ (a name two enums share) is no candidate, and here that is no news.
            "enum" => enums.codes(conn, &l.target, &l.by).ok(),
            "consts" => Some(consts.codes(&l.target, &l.by)),
            "property" => Some(properties.codes(&l.target, &l.by)),
            "sql-column" => Some(columns.codes(&l.target)),
            _ => links::codes(conn, l).ok(),
        }
        .filter(|c| !c.is_empty())
    };
    out.extend(asked.into_iter().filter_map(|l| codes(&l).map(|codes| Set { link: l, codes, others: HashMap::new() })));
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
    // EVERY COLUMN OF EVERY SEED ROW, by its id: what a document row's other cells are asked to agree with.
    let mut rows_of: HashMap<String, Vec<(String, String)>> = HashMap::new();
    let rows = stmt.query_map([], |r| Ok((text(r, 0)?, text(r, 1)?, r.get::<_, Option<i64>>(2)?.unwrap_or(0), text(r, 3)?, text(r, 4)?, text(r, 5)?, text(r, 6)?)))?;
    for row in rows {
        let (id, file, line, condition, table, column, value) = row?;
        if value.is_empty() {
            continue;
        }
        rows_of.entry(id.clone()).or_default().push((column.clone(), value.clone()));
        let slot = *at.entry((table.to_lowercase(), column.to_lowercase())).or_insert_with(|| {
            by.push(Set {
                link: Link { facts: String::new(), kind: "seed".into(), target: table.clone(), column: column.clone(), by: "value".into() },
                codes: Vec::new(),
                others: HashMap::new(),
            });
            by.len() - 1
        });
        by[slot].codes.push(Code { target: id, file, line, value, name: String::new(), condition });
    }
    // Each table's sets share its rows: a set asks only by the ids its own codes carry.
    for set in &mut by {
        set.others = set.codes.iter().filter_map(|c| rows_of.get(&c.target).map(|r| (c.target.clone(), r.clone()))).collect();
    }
    Ok(by)
}

#[cfg(test)]
#[path = "suggest_tests.rs"]
mod tests;
