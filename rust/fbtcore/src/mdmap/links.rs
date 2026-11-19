//! WHAT EACH DOC NAME IS IN THE MAP - one of the passes over the finished rows, since a symbol is a row another half
//! wrote. `md_links` is rewritten whole from `md_names`:
//!
//! - a PATH the half found is `bound` to its `files` row (`target_file`, empty for a file no half maps, such as a
//!   `.json`) with the line it names; one it did not find is `missing in code`, as the half said.
//! - a dotted SYMBOL a code span names (`Acme.Orders.OrderService.Submit`, `OrderKind.Beta`) is `bound` when it ends
//!   exactly one declaration - a function, a constant or member, a class or an enum - by whole segments; `ambiguous`
//!   when it ends several (`matches` says how many). One that a declared TYPE leads to and whose last segment that
//!   type does not declare (`OrderService.Gone`) is `missing in code`. A single segment is bound only to a class it
//!   alone names; anything else a span says (`true`, `json.dump`) is no row - a doc's every word is not a claim.

use rusqlite::{params, Connection};
use std::collections::{BTreeMap, HashMap};

/// What the pass wrote: rows per status.
pub struct Made {
    pub bound: usize,
    pub missing: usize,
    pub ambiguous: usize,
}

/// The tables a symbol is declared in, the first preferred when two rows declare one thing (an enum is a `classes`
/// row and an `enums` row at the same line).
const DECLARED: [&str; 4] = ["functions", "consts", "classes", "enums"];

/// One declaration: its table and row, its file, and the names it answers to, each with every generic and
/// parameter list cut (`Submit(int)` is `Submit`).
struct Declared {
    table: &'static str,
    id: String,
    file: String,
    line: i64,
    names: Vec<String>,
    symbol: String,
    /// The type it is declared in (`cls`, cut), "" at the top level.
    owner: String,
}

pub fn write(conn: &Connection) -> rusqlite::Result<Made> {
    crate::rows::cells::drop_children(conn, "md_links").map_err(failed)?;
    conn.execute_batch(
        "DROP TABLE IF EXISTS md_links;
         CREATE TABLE md_links (id, name, file, section, line, kind, text, target_path, target_file, target_line,
                                target_table, target_id, symbol, status, matches);",
    )?;
    let mut made = Made { bound: 0, missing: 0, ambiguous: 0 };
    if !has(conn, "md_names") {
        return Ok(made);
    }
    let files = files(conn)?;
    let all = declarations(conn)?;
    // A NAME WRITTEN `a::B` IS RUST'S: a python class of the same name is not what it names.
    let rust = langs(conn, "rust")?;
    let (declared, rusty): (Vec<Declared>, Vec<Declared>) = all.into_iter().partition(|d| !rust.contains(&d.file));
    let mut declared = declared;
    let from = declared.len();
    declared.extend(rusty);
    let index = |range: std::ops::Range<usize>| {
        let mut by_last: HashMap<String, Vec<usize>> = HashMap::new();
        for i in range {
            for name in &declared[i].names {
                let last = name.rsplit('.').next().unwrap_or(name).to_string();
                let held = by_last.entry(last).or_default();
                if held.last() != Some(&i) {
                    held.push(i);
                }
            }
        }
        by_last
    };
    // Every row for a dotted name; only the rust rows for one written with `::`.
    let (dotted, pathed) = (index(0..declared.len()), index(from..declared.len()));
    let mut read = conn.prepare(
        "SELECT id, file, section, line, kind, text, path, target, target_line, status, symbol FROM md_names ORDER BY file, line, id",
    )?;
    let names = read.query_map([], |r| {
        Ok((
            text(r, 0)?, text(r, 1)?, text(r, 2)?, r.get::<_, Option<i64>>(3)?.unwrap_or(0), text(r, 4)?, text(r, 5)?,
            text(r, 6)?, text(r, 7)?, r.get::<_, Option<i64>>(8)?, text(r, 9)?, text(r, 10)?,
        ))
    })?;
    let mut insert = conn.prepare("INSERT INTO md_links VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)")?;
    let mut n = 0;
    for row in names {
        let (name, file, section, line, kind, said, path, target, target_line, status, symbol) = row?;
        let mut put = |target_path: &str, target_file: &str, target_line: Option<i64>, d: Option<&Declared>, status: &str, matches: i64| {
            n += 1;
            let (table, id, symbol) = d.map_or(("", "", ""), |d| (d.table, d.id.as_str(), d.symbol.as_str()));
            let target_file = d.map_or(target_file, |d| d.file.as_str());
            let target_line = d.map(|d| d.line).or(target_line);
            insert.execute(params![format!("ml:{n}"), name, file, section, line, kind, said, target_path, target_file,
                target_line, table, id, symbol, status, matches])
        };
        match status.as_str() {
            "bound" => {
                let found = files.get(&target).map(String::as_str).unwrap_or("");
                put(&target, found, target_line, None, "bound", 1)?;
                made.bound += 1;
                continue;
            }
            "missing in code" => {
                put(&path, "", None, None, "missing in code", 0)?;
                made.missing += 1;
                continue;
            }
            _ => {}
        }
        if symbol.is_empty() {
            continue;
        }
        let by_last = if said.contains("::") { &pathed } else { &dotted };
        let hits = ends(&declared, by_last, &symbol);
        let single = !symbol.contains('.');
        match hits.as_slice() {
            [one] if !single || declared[*one].table == "classes" => {
                put("", "", None, Some(&declared[*one]), "bound", 1)?;
                made.bound += 1;
            }
            [] if !single => {
                // A TYPE THE DOC NAMES RIGHT, A MEMBER IT DOES NOT HAVE: the rename a doc was not told about. Only a
                // type whose members the map RECORDS - a half that stores no methods (rust's) would make every one missing.
                let (owner, _) = symbol.rsplit_once('.').unwrap_or_default();
                if let [t] = ends(&declared, by_last, owner).as_slice()
                    && matches!(declared[*t].table, "classes" | "enums")
                    && members(&declared, &declared[*t])
                {
                    put("", "", None, Some(&declared[*t]), "missing in code", 0)?;
                    made.missing += 1;
                }
            }
            [_, _, ..] if !single => {
                put("", "", None, None, "ambiguous", hits.len() as i64)?;
                made.ambiguous += 1;
            }
            _ => {}
        }
    }
    conn.execute_batch(
        "CREATE INDEX ix_md_links_target_file ON md_links(target_file); CREATE INDEX ix_md_links_status ON md_links(status);
         CREATE INDEX ix_md_links_file ON md_links(file);",
    )?;
    Ok(made)
}

/// The declarations a dotted name ends, by whole segments - one per thing declared, the preferred table's row.
fn ends(declared: &[Declared], by_last: &HashMap<String, Vec<usize>>, symbol: &str) -> Vec<usize> {
    let last = symbol.rsplit('.').next().unwrap_or(symbol);
    let tail = format!(".{symbol}");
    // ONE THING, ONE HIT: the same file, line and name from two tables is one declaration.
    let mut one: BTreeMap<(&str, i64), usize> = BTreeMap::new();
    for &i in by_last.get(last).into_iter().flatten() {
        let d = &declared[i];
        if d.names.iter().any(|n| n == symbol || n.ends_with(&tail)) {
            one.entry((d.file.as_str(), d.line)).or_insert(i);
        }
    }
    one.into_values().collect()
}

/// Whether the map records any member of the type `t`: a row in its file whose `cls` is its name.
fn members(declared: &[Declared], t: &Declared) -> bool {
    let name = t.names.first().map(String::as_str).unwrap_or("");
    let name = name.rsplit('.').next().unwrap_or(name);
    declared.iter().any(|d| d.file == t.file && d.owner.rsplit('.').next() == Some(name))
}

/// `files.path` -> its id; a doc's own row first where another half lists the same path (a `.md` the TypeScript
/// half lists too), else the first.
fn files(conn: &Connection) -> rusqlite::Result<HashMap<String, String>> {
    let mut stmt = conn.prepare("SELECT path, id FROM files ORDER BY lang <> 'markdown', rowid")?;
    let rows = stmt.query_map([], |r| Ok((text(r, 0)?, text(r, 1)?)))?;
    let mut out = HashMap::new();
    for row in rows {
        let (path, id) = row?;
        out.entry(path).or_insert(id);
    }
    Ok(out)
}

/// The ids of the files one half wrote.
fn langs(conn: &Connection, lang: &str) -> rusqlite::Result<std::collections::HashSet<String>> {
    let mut stmt = conn.prepare("SELECT id FROM files WHERE lang = ?1")?;
    let rows = stmt.query_map([lang], |r| text(r, 0))?;
    rows.collect()
}

/// Every declaration the halves stored, in the order of `DECLARED`, each with the columns its table has.
fn declarations(conn: &Connection) -> rusqlite::Result<Vec<Declared>> {
    let mut out = Vec::new();
    for table in DECLARED {
        if !has(conn, table) {
            continue;
        }
        let column = |c: &str| if crate::rows::dups::has_columns(conn, table, &[c]) { c.to_string() } else { "''".to_string() };
        let mut stmt = conn.prepare(&format!(
            "SELECT id, file, line, name, {}, {}, {} FROM {table}",
            column("cls"), column("qualname"), column("symbol"),
        ))?;
        let rows = stmt.query_map([], |r| {
            Ok((text(r, 0)?, text(r, 1)?, r.get::<_, Option<i64>>(2).unwrap_or(None).unwrap_or(0), text(r, 3)?, text(r, 4)?, text(r, 5)?, text(r, 6)?))
        })?;
        for row in rows {
            let (id, file, line, name, cls, qualname, symbol) = row?;
            let name = cut(&name);
            if name.is_empty() {
                continue;
            }
            let mut names = vec![name.clone()];
            if !cls.is_empty() {
                names.push(format!("{}.{name}", cut(&cls)));
            }
            for full in [&qualname, &symbol] {
                let full = cut(full).replace("::", ".");
                if !full.is_empty() && !names.contains(&full) {
                    names.push(full);
                }
            }
            let symbol = names.iter().max_by_key(|n| n.len()).cloned().unwrap_or_default();
            out.push(Declared { table, id, file, line, names, symbol, owner: cut(&cls) });
        }
    }
    Ok(out)
}

/// A declared name without its parameter list or generic arguments: `Submit(int)` and `List<T>` are `Submit`, `List`.
fn cut(name: &str) -> String {
    let end = name.find(['(', '<']).unwrap_or(name.len());
    name[..end].trim().to_string()
}

fn has(conn: &Connection, table: &str) -> bool {
    conn.query_row("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1", [table], |_| Ok(())).is_ok()
}

fn text(r: &rusqlite::Row, i: usize) -> rusqlite::Result<String> {
    Ok(match r.get::<_, rusqlite::types::Value>(i)? {
        rusqlite::types::Value::Text(t) => t,
        rusqlite::types::Value::Integer(n) => n.to_string(),
        rusqlite::types::Value::Real(x) => x.to_string(),
        _ => String::new(),
    })
}

fn failed(e: anyhow::Error) -> rusqlite::Error {
    rusqlite::Error::InvalidParameterName(format!("{e:#}"))
}
