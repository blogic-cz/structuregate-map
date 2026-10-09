//! The lenses --map-query answers with, one function each, called from `render`.
//! A child of `query` by `#[path]`: what it holds is `pub(super)`, for that file alone.

use super::*;

pub(super) fn lens_tables(db: &Connection, out: &mut String, limit: usize, width: usize) -> Result<()> {
    let mut rows: Vec<Vec<Value>> = Vec::new();
    let mut total = 0i64;
    for table in tables_of(db) {
        let n = count_of(db, &table)?;
        total += n;
        let columns = columns_of(db, &table)
            .into_iter()
            .filter(|c| c != "id")
            .collect::<Vec<_>>()
            .join(", ");
        rows.push(vec![
            Value::Text(table),
            Value::Integer(n),
            Value::Text(columns),
        ]);
    }
    // Biggest first, and `sort` in python is STABLE - equal counts keep the name order above.
    rows.sort_by_key(|r| match r[1] {
        Value::Integer(n) => -n,
        _ => 0,
    });
    rows.push(vec![
        Value::Text("TOTAL".to_string()),
        Value::Integer(total),
        Value::Text(String::new()),
    ]);
    show(out, &rows, &named(&["table", "rows", "columns"]), limit, width.max(90));
    let _ = writeln!(out);
    if let Ok(mut stmt) = db.prepare("SELECT key, value FROM _meta")
        && let Ok(meta) = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Value>(1)?)))
    {
        // CUT LIKE ANY CELL. `deps:python` is the whole bound-through record of a tree, and printed whole
        // it made `--tables` megabytes of output; `--width 0` still shows every value in full.
        let meta_width = if width == 0 { 0 } else { width.max(90) };
        for pair in meta.flatten() {
            let (key, value) = pair;
            let padded = format!("{key}{}", " ".repeat(8usize.saturating_sub(key.chars().count())));
            let _ = writeln!(out, "  {padded} {}", clip(&cell(&value).replace('\n', " "), meta_width));
        }
    }
    Ok(())
}

pub(super) fn lens_schema(db: &Connection, out: &mut String, table: &str, limit: usize, width: usize) -> Result<()> {
    let n = count_of(db, table)?;
    let _ = writeln!(out, "{table}  {n} rows");
    let _ = writeln!(out);
    let mut rows = Vec::new();
    for column in columns_of(db, table) {
        let sql = format!(
            "SELECT count(*) FROM \"{}\" WHERE \"{}\" IS NOT NULL AND \"{}\" <> \"\"",
            escape(table),
            escape(&column),
            escape(&column)
        );
        let filled: i64 = db.query_row(&sql, [], |r| r.get(0))?;
        let rate = if n != 0 {
            format!("{:.1}%", 100.0 * filled as f64 / n as f64)
        } else {
            "-".to_string()
        };
        rows.push(vec![Value::Text(column), Value::Integer(filled), Value::Text(rate)]);
    }
    show(out, &rows, &named(&["column", "filled", "rate"]), limit, width);
    Ok(())
}

pub(super) fn lens_find(db: &Connection, out: &mut String, needle: &str, limit: usize, width: usize) -> Result<()> {
    let known = tables_of(db);
    let like = format!("%{needle}%");
    // AN ALIAS IS FOLLOWED TO THE DEF IT BINDS - see `bound::aliased` - and that def comes FIRST: appended after
    // every call and import spelling the alias, it was the last of dozens of rows and read as not there.
    let mut rows = super::bound::aliased(db, &like)?;
    for (table, column) in NAME_COLUMNS {
        if !known.iter().any(|k| k == table) {
            continue;
        }
        // NOT EVERY TABLE ANCHORS A ROW THE SAME WAY. The Angular half's rows carry `owner_file` and a
        // `file` that is often NULL (a call hangs off a member, not a file), and five of its tables have no
        // `line` at all. Selecting `t.file, t.line` by name printed a blank path for every Angular hit and,
        // with the error swallowed, NOTHING for a table without a line - so each is asked for where present.
        let held = columns_of(db, table);
        let has = |name: &str| held.iter().any(|c| c == name);
        if !has(column) {
            continue;
        }
        let file = match (has("file"), has("owner_file")) {
            (true, true) => "COALESCE(t.file, t.owner_file)",
            (true, false) => "t.file",
            (false, true) => "t.owner_file",
            (false, false) => "NULL",
        };
        let line = if has("line") { "t.line" } else { "NULL" };
        let sql = format!(
            "SELECT ?, t.\"{col}\", f.path, {line} FROM \"{table}\" t LEFT JOIN files f ON f.id = {file}              WHERE t.\"{col}\" LIKE ?",
            col = escape(column),
            table = escape(table)
        );
        let (_, found) = query(db, &sql, &[table, &like])?;
        rows.extend(found);
    }
    show(out, &rows, &named(&["table", "name", "file", "line"]), limit, width);
    Ok(())
}

pub(super) fn lens_id(db: &Connection, out: &mut String, row_id: &str, width: usize) -> Result<()> {
    let prefix = row_id.split(':').next().unwrap_or(row_id);
    for table in tables_of(db) {
        let sql = format!("SELECT * FROM \"{}\" WHERE id = ?", escape(&table));
        let Ok((columns, rows)) = query(db, &sql, &[&row_id]) else { continue };
        let Some(row) = rows.first() else { continue };
        let _ = writeln!(out, "{row_id}  in {table}");
        for (column, value) in columns.iter().zip(row.iter()) {
            let text = cell(value);
            if matches!(value, Value::Null) || text.is_empty() {
                continue;
            }
            let shown = clip(&text.replace('\n', " "), width);
            let padded =
                format!("{column}{}", " ".repeat(12usize.saturating_sub(column.chars().count())));
            let _ = writeln!(out, "  {padded} {shown}");
        }
        return Ok(());
    }
    let _ = writeln!(
        out,
        "no row {} (prefix {} is not a table in this database)",
        quoted(row_id),
        quoted(prefix)
    );
    Ok(())
}

/// Every root of the map as `(prefix, dir)`: what each half recorded as `roots:<lang>`, or the one `_meta.root`
/// with no prefix for a database that recorded none.
fn roots_of(db: &Connection) -> Vec<(String, String)> {
    let mut roots = Vec::new();
    if let Ok(mut stmt) = db.prepare("SELECT value FROM _meta WHERE key LIKE 'roots:%'")
        && let Ok(found) = stmt.query_map([], |r| r.get::<_, String>(0))
    {
        for listed in found.flatten() {
            let pairs: Vec<(String, String)> = serde_json::from_str(&listed).unwrap_or_default();
            roots.extend(pairs);
        }
    }
    if roots.is_empty()
        && let Ok(root) = db.query_row("SELECT value FROM _meta WHERE key = 'root'", [], |r| r.get::<_, String>(0))
    {
        roots.push((String::new(), root));
    }
    roots
}

/// The fragment as the map spells it, when the fragment names folders ABOVE a root: `py/core/config.py`
/// over a root `C:/repo/py/core` stored as `core/` is `core/config.py`, and over a single root
/// `C:/repo/py` it is `core/config.py` too. Only folders a root's own path ENDS with are taken off, so a
/// fragment that simply matches nothing is never shortened into one that matches something else.
fn under_root(roots: &[(String, String)], fragment: &str) -> Vec<(String, String)> {
    let parts: Vec<&str> = fragment.split(['/', '\\']).collect();
    let mut found = Vec::new();
    for (prefix, root) in roots {
        let dir = root.replace('\\', "/").trim_end_matches('/').to_lowercase();
        for k in (1..parts.len()).rev() {
            let above = parts[..k].join("/").to_lowercase();
            if !above.is_empty() && dir.ends_with(&format!("/{above}")) {
                found.push((format!("{prefix}{}", parts[k..].join("/")), root.clone()));
            }
        }
    }
    found
}

/// Rows for `sql` with `?` bound to `%fragment%`, retried under each root - see `under_root`. A note says which
/// spelling answered, and the roots are named when none did, since a path here is root-relative. The spelling
/// that answered is returned with the rows, for a caller that has to pick one of them by it.
pub(super) fn rooted(db: &Connection, out: &mut String, sql: &str, fragment: &str) -> Result<Option<(String, Vec<Vec<Value>>)>> {
    let (_, found) = query(db, sql, &[&format!("%{fragment}%")])?;
    if !found.is_empty() {
        return Ok(Some((fragment.to_string(), found)));
    }
    let roots = roots_of(db);
    let named = || roots.iter().map(|(prefix, dir)| if prefix.is_empty() { dir.clone() } else { format!("{prefix} = {dir}") });
    for (inner, root) in under_root(&roots, fragment) {
        let (_, found) = query(db, sql, &[&format!("%{inner}%")])?;
        if !found.is_empty() {
            let _ = writeln!(out, "(read as {} - a path here is relative to its root, {root})", quoted(&inner));
            return Ok(Some((inner, found)));
        }
    }
    if !roots.is_empty() {
        let named = named().collect::<Vec<_>>().join(", ");
        let _ = writeln!(out, "(a path here is relative to the root: {named})");
    }
    Ok(None)
}

pub(super) fn lens_file(db: &Connection, out: &mut String, fragment: &str, limit: usize, width: usize) -> Result<()> {
    // NOT EVERY HALF WRITES EVERY COLUMN. `entry` and `doc` are the python and C# halves' - the
    // TypeScript half writes neither, and selecting them by name failed the whole lens with
    // `no such column: entry` over a map that holds thousands of files. Asked for where present and
    // substituted with NULL where not, so one lens reads every half's `files`.
    let held = columns_of(db, "files");
    let optional = |name: &str| {
        if held.iter().any(|c| c == name) { name.to_string() } else { format!("NULL AS {name}") }
    };
    let sql = format!(
        "SELECT id, path, module, lines, {}, {} FROM files WHERE path LIKE ?",
        optional("entry"),
        optional("doc")
    );
    let Some((_, found)) = rooted(db, out, &sql, fragment)? else {
        let _ = writeln!(out, "no file matches {}", quoted(fragment));
        return Ok(());
    };
    for row in found.iter().take(limit) {
        let (id, path, module) = (cell(&row[0]), cell(&row[1]), cell(&row[2]));
        let lines = cell(&row[3]);
        let entry = !matches!(row[4], Value::Null) && cell(&row[4]) != "0" && !cell(&row[4]).is_empty();
        let _ = writeln!(
            out,
            "{id}  {path}  ({module}, {lines} lines{})",
            if entry { ", entry" } else { "" }
        );
        let doc = cell(&row[5]);
        if !doc.is_empty() {
            let first = clip(doc.split('\n').next().unwrap_or(""), width);
            let _ = writeln!(out, "   {first}");
        }
        let mut counts = Vec::new();
        for table in tables_of(db) {
            if table == "files" || table == "file_text" {
                continue;
            }
            let sql = format!("SELECT count(*) FROM \"{}\" WHERE file = ?", escape(&table));
            let Ok(n) = db.query_row(&sql, [&id], |r| r.get::<_, i64>(0)) else { continue };
            if n != 0 {
                counts.push(format!("{table} {n}"));
            }
        }
        let _ = writeln!(out, "   {}", counts.join(", "));
        let _ = writeln!(out);
    }
    Ok(())
}

pub(super) fn lens_text(db: &Connection, out: &mut String, needle: &str, limit: usize, width: usize) -> Result<()> {
    let sql = "SELECT path, snippet(file_text, 1, '>>', '<<', '...', 12) \
               FROM file_text WHERE file_text MATCH ? LIMIT ?";
    // THIS ONE CATCHES ITS OWN, as the python did: a map built without FTS5 has no `file_text` to match against,
    // and that is worth saying plainly rather than as a failed query.
    let held = db.query_row("SELECT 1 FROM sqlite_master WHERE name = 'file_text'", [], |_| Ok(())).is_ok();
    if !held {
        let _ = writeln!(out, "the source index is not in this database (built without FTS5) - see --tables");
        return Ok(());
    }
    match query(db, sql, &[&needle, &(limit as i64)]) {
        Ok((_, rows)) => show(out, &rows, &named(&["file", "match"]), limit, width),
        // A TERM FTS5 CANNOT PARSE BARE - `settings.app`, `a-b` - is searched as the phrase it spells. It was
        // reported as an index missing from a database that had one.
        Err(first) => {
            let phrase = format!("\"{}\"", needle.replace('"', "\"\""));
            match query(db, sql, &[&phrase, &(limit as i64)]) {
                Ok((_, rows)) => {
                    let _ = writeln!(out, "(searched as the phrase {phrase}: {} is no FTS5 query bare)", quoted(needle));
                    show(out, &rows, &named(&["file", "match"]), limit, width);
                }
                Err(_) => {
                    let _ = writeln!(out, "FTS5 cannot read {} as a query ({}) - quote the term", quoted(needle), sqlite_message(&first));
                }
            }
        }
    }
    Ok(())
}

pub(super) fn lens_grep(db: &Connection, out: &mut String, needle: &str, table: &str, limit: usize, width: usize) -> Result<()> {
    let like = format!("%{needle}%");
    let names: Vec<String> =
        if table.is_empty() { tables_of(db) } else { vec![table.to_string()] };
    let mut rows = Vec::new();
    for name in names {
        if name == "file_text" {
            continue;
        }
        let columns = columns_of(db, &name);
        for column in GREP_COLUMNS {
            if !columns.iter().any(|c| c == column) {
                continue;
            }
            let sql = format!(
                "SELECT ?, f.path, t.line, t.\"{}\" FROM \"{}\" t LEFT JOIN files f ON f.id = t.file \
                 WHERE t.\"{}\" LIKE ?",
                escape(column),
                escape(&name),
                escape(column)
            );
            if let Ok((_, found)) = query(db, &sql, &[&name, &like]) {
                rows.extend(found);
            }
        }
    }
    show(out, &rows, &named(&["table", "file", "line", "text"]), limit, width);
    Ok(())
}

pub(super) fn lens_reads(db: &Connection, out: &mut String, name: &str, limit: usize, width: usize) -> Result<()> {
    let like = format!("%{name}%");
    let mut rows = Vec::new();
    for (table, what) in READ_SOURCES {
        // A READ THROUGH AN ALIAS IS A READ OF WHAT IT BINDS: `binds` beside `reads` - see `bound::reads_bound`.
        let sql = format!(
            "SELECT ?1, f.path, t.line, t.func, t.\"{}\" FROM \"{}\" t \
             LEFT JOIN files f ON f.id = t.file WHERE t.reads LIKE ?2 OR {}",
            escape(what),
            escape(table),
            super::bound::reads_bound(db, table)
        );
        if let Ok((_, found)) = query(db, &sql, &[table, &like]) {
            rows.extend(found);
        }
    }
    if rows.is_empty() {
        let _ = writeln!(
            out,
            "nothing reads {} - check the spelling with --find {name}",
            quoted(name)
        );
        return Ok(());
    }
    show(out, &rows, &named(&["table", "file", "line", "func", "what"]), limit, width);
    Ok(())
}

pub(super) fn lens_key(db: &Connection, out: &mut String, key: &str, limit: usize, width: usize) -> Result<()> {
    let _ = writeln!(out, "1. where {} is spelled", quoted(key));
    // A KEY READ FROM A SECTION A LOCAL HOLDS is spelled by its last part only - see `bound::key_spelled`.
    let (_, spelled) = query(
        db,
        &format!(
            "SELECT f.path, s.line, s.func, s.value FROM string_literals s \
             JOIN files f ON f.id = s.file WHERE s.value = ?1 OR {} ORDER BY f.path, s.line",
            super::bound::key_spelled(db)
        ),
        &[&key],
    )?;
    show(out, &spelled, &named(&["file", "line", "func", "value"]), limit, width);
    let _ = writeln!(out);

    let _ = writeln!(out, "2. names built from it");
    let like = format!("%{key}%");
    let (_, built) = query(
        db,
        &format!(
            "SELECT f.path, k.line, k.name, k.source FROM consts k JOIN files f ON f.id = k.file \
             WHERE k.source LIKE ?1 OR {} UNION \
             SELECT f.path, a.line, a.target, a.source FROM assignments a \
             JOIN files f ON f.id = a.file WHERE (a.source LIKE ?1 OR {}) AND a.func = ''",
            super::bound::key_built(db, "consts", "k"),
            super::bound::key_built(db, "assignments", "a")
        ),
        &[&like, &key],
    )?;
    show(out, &built, &named(&["file", "line", "name", "source"]), limit, width);
    let _ = writeln!(out);

    let _ = writeln!(out, "3. who reads those names");
    let mut names: Vec<String> = built.iter().map(|r| cell(&r[2])).collect();
    names.sort();
    names.dedup();
    for name in names {
        let _ = writeln!(out, "   {name}");
        lens_reads(db, out, &name, limit, width)?;
        let _ = writeln!(out);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A member whose `file` is NULL but whose `owner_file` is set, and a route with no `line` column: the
    /// two Angular shapes `--find` used to answer with a blank path and with nothing.
    #[test]
    fn find_reaches_a_member_by_its_owner_file_and_a_table_without_a_line() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE files (id TEXT, path TEXT);
             INSERT INTO files VALUES ('f:1', 'web/src/shop.component.ts');
             CREATE TABLE members (id TEXT, name TEXT, file TEXT, owner_file TEXT, line INTEGER);
             INSERT INTO members VALUES ('m:1', 'loadCart', NULL, 'f:1', 41);
             CREATE TABLE routes (id TEXT, path TEXT, owner_file TEXT);
             INSERT INTO routes VALUES ('r:1', 'cart', 'f:1');",
        )
        .unwrap();
        let mut out = String::new();
        lens_find(&db, &mut out, "cart", 50, 0).unwrap();
        let members = out.lines().find(|l| l.starts_with("members")).expect("the member row");
        assert!(members.contains("loadCart") && members.contains("shop.component.ts") && members.contains("41"), "{out}");
        let routes = out.lines().find(|l| l.starts_with("routes")).expect("the route row");
        assert!(routes.contains("shop.component.ts"), "{out}");
    }
}
