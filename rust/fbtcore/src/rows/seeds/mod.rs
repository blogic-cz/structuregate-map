//! WHAT THE DEPLOY SCRIPTS WRITE INTO A TABLE: `sql_seeds`, one row per VALUES tuple and the table it lands
//! in, with its columns and its values - `@variables` resolved where the chain sets them. Derived AFTER the
//! SQL half from its `sql_steps` and rewritten whole each run, because a value is a fact about a whole `:r`
//! chain and any file of it may have moved. It reads only rows already in the database, so it is run here,
//! against the database, with nothing crossing to the caller but a count.
//!
//! A `:r` IS A STEP TOO: it runs the other file at that line. A later `UPDATE t SET ... WHERE key IN (...)`
//! changes the rows it names, a column the INSERT leaves out takes the table's constant DEFAULT, and a row
//! under `IF '$(Mode)' = 'Full'` carries that `condition` - the deploy, not the script, decides it.
//!
//! A VALUE IS NEVER GUESSED: one the chain computes at run time (`GETDATE()`, a lookup, `$(Var)`) stays its
//! own text and the row is `partial`, with the `detail` naming it. `bound` means every value is known, every
//! column is a column of the table, and the table is in the map.

mod tables;
mod value;
mod views;
mod walk;

use rusqlite::types::Value;
use rusqlite::{params, Connection};
use serde_json::json;
use std::collections::{BTreeMap, HashMap, HashSet};
use value::Val;

/// One statement the SQL half recorded, or one `:r`.
#[derive(Clone)]
pub struct Step {
    pub id: String,
    pub file: String,
    pub line: i64,
    pub action: String,
    pub schema: String,
    pub name: String,
    pub source: String,
    pub columns: Vec<String>,
    pub targets: Vec<String>,
    pub kinds: Vec<String>,
    pub exprs: Vec<String>,
    pub text: String,
}

/// One tuple on its way to a table. `dropped` are columns a flow did not carry, `defaulted` the ones the
/// table's DEFAULT filled, `updates` the UPDATE statements that changed it after it was seeded.
#[derive(Clone)]
pub struct Pending {
    pub step: Step,
    pub root: String,
    pub database: String,
    pub condition: String,
    pub columns: Vec<String>,
    pub values: Vec<Val>,
    pub via: Vec<String>,
    pub dropped: Vec<String>,
    pub defaulted: Vec<String>,
    pub updates: Vec<String>,
}

impl Pending {
    fn new(step: &Step, root: &str, database: &str, condition: &str, columns: Vec<String>, values: Vec<Val>, via: Vec<String>) -> Self {
        Pending {
            step: step.clone(),
            root: root.into(),
            database: database.into(),
            condition: condition.into(),
            columns,
            values,
            via,
            dropped: Vec::new(),
            defaulted: Vec::new(),
            updates: Vec::new(),
        }
    }
}

pub struct Seed {
    pub row: Pending,
    pub schema: String,
    pub name: String,
    pub object: String,
    pub status: String,
    pub detail: String,
}

impl Seed {
    /// The columns whose value is not known before run time - what `detail` says in prose, as a list.
    fn unbound(&self) -> Vec<String> {
        self.row.values.iter().enumerate().filter(|(_, v)| v.0 == "expr" || v.0 == "unset")
            .map(|(i, _)| self.row.columns.get(i).cloned().unwrap_or_else(|| format!("#{}", i + 1))).collect()
    }
}

pub struct File {
    pub path: String,
    pub database: String,
    pub kind: String,
}

pub type Files = HashMap<String, File>;

/// `{counts: [[status, n], ...], total}`, or `{skipped: true}` when there is no `sql_steps` to walk.
pub(crate) fn run(db: &str) -> Result<serde_json::Value, String> {
    let mut conn = Connection::open(db).map_err(|e| format!("the database could not be opened - {e}"))?;
    let Some(raw) = query(&conn, "SELECT id, file, line, action, schema, name, source, columns, targets, kinds, exprs, text \
                                   FROM sql_steps ORDER BY file, line, seq") else {
        return Ok(json!({ "skipped": true }));
    };
    let mut files: Files = HashMap::new();
    let mut by_path: HashMap<String, String> = HashMap::new();
    for row in query(&conn, "SELECT id, path, database, kind FROM files WHERE lang = 'sql' ORDER BY path").unwrap_or_default() {
        by_path.insert(text(&row[1]).to_lowercase(), text(&row[0]));
        files.insert(text(&row[0]), File { path: text(&row[1]), database: text(&row[2]), kind: text(&row[3]) });
    }
    let mut steps: HashMap<String, Vec<Step>> = HashMap::new();
    for row in &raw {
        let step = Step {
            id: text(&row[0]), file: text(&row[1]), line: num(&row[2]), action: text(&row[3]), schema: text(&row[4]),
            name: text(&row[5]), source: text(&row[6]), columns: list(&row[7]), targets: list(&row[8]), kinds: list(&row[9]),
            exprs: list(&row[10]), text: text(&row[11]),
        };
        steps.entry(step.file.clone()).or_default().push(step);
    }
    let mut included = HashSet::new();
    for row in query(&conn, "SELECT id, file, line, name FROM sql_refs WHERE lang = 'sql' AND action = 'include'").unwrap_or_default() {
        let from = text(&row[1]);
        let beside = files.get(&from).map(|f| f.path.clone()).unwrap_or_default();
        let Some(target) = by_path.get(&normal(&joined(&beside, &text(&row[3]))).to_lowercase()).cloned() else { continue };
        included.insert(target.clone());
        steps.entry(from.clone()).or_default().push(Step {
            id: text(&row[0]), file: from, line: num(&row[2]), action: "include".into(), schema: String::new(), name: target,
            source: String::new(), columns: Vec::new(), targets: Vec::new(), kinds: Vec::new(), exprs: Vec::new(), text: String::new(),
        });
    }
    // STABLE, by line: steps on one line (a DECLARE of two variables, a row of VALUES) keep their order.
    for list in steps.values_mut() {
        list.sort_by_key(|s| s.line);
    }

    let tables = tables::Tables::load(&conn);
    let mut walker = walk::Walker::new(&steps, &files, &tables);
    let mut roots: Vec<&String> = files.keys().filter(|f| !included.contains(*f) && steps.contains_key(*f)).collect();
    roots.sort_by(|a, b| files[*a].path.cmp(&files[*b].path));
    for root in roots {
        walker.root(root);
    }
    // ROWS THAT NEVER LEFT THEIR TEMP are still what the script wrote - named by the temp, never a table.
    let stranded: Vec<(Pending, String)> = walker.stranded.iter()
        .filter(|(id, _)| !walker.flushed.contains(*id)).map(|(_, v)| v.clone()).collect();
    let mut seeds = std::mem::take(&mut walker.seeds);
    for (row, temp) in stranded {
        seeds.push(Seed { row, schema: String::new(), name: temp, object: String::new(), status: "temp".into(),
            detail: "no MERGE or INSERT moves these rows into a table".into() });
    }

    write(&mut conn, &seeds, &files).map_err(|e| format!("{e}"))?;
    let views = views::write(&conn).map_err(|e| format!("the typed seed views were not made - {e}"))?;
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for seed in &seeds {
        *counts.entry(seed.status.clone()).or_insert(0) += 1;
    }
    Ok(json!({ "counts": counts.into_iter().collect::<Vec<_>>(), "total": seeds.len(), "views": views }))
}

/// The table, rewritten whole, in one transaction.
fn write(conn: &mut Connection, seeds: &[Seed], files: &Files) -> rusqlite::Result<()> {
    let tx = conn.transaction()?;
    tx.execute_batch("DROP TABLE IF EXISTS sql_seeds; CREATE TABLE sql_seeds (id, step, file, line, root, deployed, \
        condition, database, schema, name, object, via, columns, row_values, defaults, updates, unbound, status, detail, text);")?;
    {
        let mut insert = tx.prepare("INSERT INTO sql_seeds VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)")?;
        for (i, s) in seeds.iter().enumerate() {
            // A chain whose root no `.sqlproj` deploys is a script someone may run by hand - or nobody.
            let root = files.get(&s.row.root);
            let deployed = root.is_some_and(|r| r.kind == "postdeploy" || r.kind == "predeploy") as i64;
            insert.execute(params![
                format!("sd:{}", i + 1), s.row.step.id, s.row.step.file, s.row.step.line,
                root.map(|r| r.path.clone()).unwrap_or_default(), deployed, s.row.condition, s.row.database, s.schema, s.name,
                s.object, s.row.via.join(" > "), value::columns(&s.row.columns), value::values(&s.row.values),
                value::columns(&s.row.defaulted), value::columns(&s.row.updates), value::columns(&s.unbound()), s.status, s.detail,
                s.row.step.text,
            ])?;
        }
    }
    tx.execute_batch("CREATE INDEX ix_sql_seeds_name ON sql_seeds(name); CREATE INDEX ix_sql_seeds_object ON sql_seeds(object); \
        CREATE INDEX ix_sql_seeds_file ON sql_seeds(file); CREATE INDEX ix_sql_seeds_step ON sql_seeds(step);")?;
    tx.commit()
}

/// Every row of a query, or `None` when it fails - a table no half wrote.
pub fn query(conn: &Connection, sql: &str) -> Option<Vec<Vec<Value>>> {
    let mut stmt = conn.prepare(sql).ok()?;
    let width = stmt.column_count();
    let rows = stmt.query_map([], |row| (0..width).map(|i| row.get::<_, Value>(i)).collect::<rusqlite::Result<Vec<_>>>()).ok()?;
    rows.collect::<rusqlite::Result<Vec<_>>>().ok()
}

/// A cell as text: a string as it is, a number as it prints, anything else empty.
pub fn text(cell: &Value) -> String {
    match cell {
        Value::Text(t) => t.clone(),
        Value::Integer(n) => n.to_string(),
        Value::Real(f) => serde_json::Number::from_f64(*f).map_or_else(|| f.to_string(), |n| n.to_string()),
        _ => String::new(),
    }
}

pub fn num(cell: &Value) -> i64 {
    match cell {
        Value::Integer(n) => *n,
        other => text(other).parse().unwrap_or(0),
    }
}

/// A JSON array of strings held in a text cell.
fn list(cell: &Value) -> Vec<String> {
    let Value::Text(t) = cell else { return Vec::new() };
    match serde_json::from_str::<serde_json::Value>(t) {
        Ok(serde_json::Value::Array(items)) => items.iter().map(|i| i.as_str().unwrap_or("").to_string()).collect(),
        _ => Vec::new(),
    }
}

/// The default schema of a name that gave none.
pub fn schema(schema: &str) -> String {
    if schema.is_empty() { "dbo".into() } else { schema.into() }
}

/// A `:r` target beside the file that runs it - or as it stands when it is rooted.
fn joined(beside: &str, target: &str) -> String {
    let rooted = target.starts_with('/') || target.starts_with('\\') || target.as_bytes().get(1) == Some(&b':');
    if rooted {
        return target.into();
    }
    match beside.rfind(['/', '\\']) {
        Some(at) => format!("{}/{target}", &beside[..at]),
        None => target.into(),
    }
}

/// `a\b/../c` as `a/c`, the way the SQL half names its files.
fn normal(path: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split(['/', '\\']) {
        match part {
            "" | "." => {}
            ".." if !parts.is_empty() => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    parts.join("/")
}
