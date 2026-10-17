//! A STATEMENT'S SHAPE AS VIEWS over `sql_nodes`: which clauses it has, what it joins, which functions it calls, which
//! CTEs it names. Views hold nothing, so they cannot disagree with the nodes - exact on every run, partial or full.
//! Each is written only when it is missing or its text moved: a run that changed nothing writes nothing.

use rusqlite::{Connection, OptionalExtension};

/// The schema an unqualified table name is in, as SQL Server resolves it.
const DEFAULT_SCHEMA: &str = "'dbo'";

fn views() -> [(&'static str, String); 4] {
    [
        // ONE ROW PER CLAUSE: a join by its type (`leftjoin`), a UNION ALL as `unionall`, an OVER as `window`. An ORDER BY
        // inside an OVER is the window's, not the query's.
        ("sql_clauses", "SELECT n.file, n.statement, CASE n.kind \
             WHEN 'join' THEN CASE n.value WHEN 'inner' THEN 'join' WHEN 'cross apply' THEN 'crossapply' \
                 WHEN 'outer apply' THEN 'outerapply' ELSE n.value || 'join' END \
             WHEN 'union' THEN 'union' || n.value WHEN 'over' THEN 'window' ELSE n.kind END AS kind, n.line \
           FROM sql_nodes n \
           WHERE n.kind IN ('cte', 'where', 'groupby', 'having', 'orderby', 'join', 'top', 'offset', 'distinct', 'union', \
                 'except', 'intersect', 'subquery', 'values', 'output', 'over') \
             AND NOT (n.kind = 'orderby' AND EXISTS (SELECT 1 FROM sql_nodes p WHERE p.statement = n.statement AND p.n = n.parent AND p.kind = 'over'))".to_string()),
        // WHAT A JOIN BRINGS IN: its second side. A CTE or a #temp is in no schema; `joined` says a table from a subquery.
        ("sql_joins", format!("SELECT j.file, j.statement, j.value AS type, t.kind AS joined, \
             CASE WHEN t.kind <> 'table' OR substr(t.value, 1, 1) IN ('#', '@') \
                  OR EXISTS (SELECT 1 FROM sql_nodes c WHERE c.statement = t.statement AND c.kind = 'cte' AND c.value = t.value COLLATE NOCASE) THEN '' \
                  ELSE coalesce((SELECT s.value FROM sql_nodes s WHERE s.statement = t.statement AND s.parent = t.n AND s.kind = 'schema'), {DEFAULT_SCHEMA}) END AS schema, \
             CASE WHEN t.kind = 'table' THEN t.value ELSE '' END AS name, \
             coalesce((SELECT a.value FROM sql_nodes a WHERE a.statement = t.statement AND a.parent = t.n AND a.kind = 'alias'), '') AS alias, \
             (SELECT o.line FROM sql_nodes o WHERE o.statement = j.statement AND o.parent = j.n AND o.kind = 'on') AS on_line, j.line \
           FROM sql_nodes j JOIN sql_nodes t ON t.statement = j.statement AND t.parent = j.n AND t.ord = 1 \
           WHERE j.kind = 'join'")),
        ("sql_functions", "SELECT f.file, f.statement, f.value AS name, \
             EXISTS (SELECT 1 FROM sql_nodes o WHERE o.statement = f.statement AND o.parent = f.n AND o.kind = 'over') AS windowed, f.line \
           FROM sql_nodes f WHERE f.kind = 'func'".to_string()),
        ("sql_ctes", "SELECT c.file, c.statement, c.value AS name, c.line FROM sql_nodes c WHERE c.kind = 'cte'".to_string()),
    ]
}

/// Each view as it should read, written where it does not.
pub fn write(conn: &Connection) -> rusqlite::Result<()> {
    for (name, body) in views() {
        let wanted = format!("CREATE VIEW {name} AS {body}");
        let held: Option<String> = conn
            .query_row("SELECT sql FROM sqlite_master WHERE type = 'view' AND name = ?1", [name], |r| r.get(0))
            .optional()?;
        if held.as_deref() != Some(wanted.as_str()) {
            conn.execute_batch(&format!("DROP VIEW IF EXISTS {name}; {wanted}"))?;
        }
    }
    Ok(())
}
