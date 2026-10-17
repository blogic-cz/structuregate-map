# SQL statements: their trees, fingerprints, columns and shape

Everything in [SKILL.md](SKILL.md) and [csharp-sql.md](csharp-sql.md) holds; this is what the per-statement
tables of the SQL half (`files.lang = 'sql'`, T-SQL and PostgreSQL alike) say, and how to ask them.

**A statement's tree (`sql_nodes`) is in kinds no dialect owns**, so one query reads every SQL the half parses: the
root is the statement's `kind`; below it `query` (a SELECT's body), `union`/`except`/`intersect` (`value` `all`),
`item` (a select-list entry), `from`, `table` (`value` the name, children `schema`, `database`, `alias`),
`subquery`, `join` (`value` `inner`, `left`, `right`, `full`, `cross`, `cross apply`, `outer apply`; children: the two
sides in order, then `on`), `where`, `groupby`, `having`, `orderby` + `sort` (`asc`/`desc`), `top`, `offset`,
`distinct`, `cte` (`value` its name), `column` (`o.Amount`, `*`), `func` (lowercased: `sum`, `row_number`, `cast`),
`over` + `partition`, `literal`, `null`, `param` (`@id`), `op` (`=`, `<>`, `and`, `or`, `not`, `+`, `like`, `is null`,
`between`), `in` (`value` `not` for NOT IN), `exists`, `case`/`when`, `values`/`row`, `set` (one assignment),
`target`/`using`/`into`, `output`, `action` (a MERGE's `matched`/`not matched`), `var`, `proc`, `name`, `columndef`,
`type`. Anything else is transparent: its children hang off the nearest node above. `fingerprint` is the same for two
statements that differ only in literals or in how many literals an IN list holds; one more operator, column or
table is another fingerprint.

A column is attributed the way SQL resolves it: its qualifier is an alias or table name of its own query, then of
the queries around it (a correlated subquery reads its outer query), then of the statement's UPDATE/DELETE/INSERT/MERGE
target; the left side of a SET, an INSERT's column list and a MERGE action's columns are the target's. A select-list
alias named again in ORDER BY is no column. **The shape of a statement is four VIEWS over `sql_nodes`** - they hold
nothing, so they cannot disagree with it:

* `sql_clauses` (`file`, `statement`, `kind`, `line`): `cte`, `where`, `groupby`, `having`, `orderby` (the query's,
  not an OVER's), `join`/`leftjoin`/`rightjoin`/`fulljoin`/`crossjoin`/`crossapply`/`outerapply`, `top`, `offset`,
  `distinct`, `union`/`unionall`, `except`, `intersect`, `subquery`, `values`, `output`, `window`.
* `sql_joins` (`file`, `statement`, `type`, `joined` - `table` or `subquery`, `schema`, `name`, `alias`, `on_line`,
  `line`): the side each join brings in; its ON columns are `sql_column_refs` rows with `role = 'join'`.
* `sql_functions` (`file`, `statement`, `name`, `windowed`, `line`) and `sql_ctes` (`file`, `statement`, `name`, `line`).

**ONE SQL TEXT, NO TREE**: `structuregate --sql-parse <file|->` (`-` reads stdin; `--dialect tsql`, the default, or `postgres`)
prints what this half would hold for that text as JSON on stdout - `statements`, `objects`, `refs`, `column_refs`,
`clauses`, `joins`, `functions`, `ctes`, `nodes`, `diagnostics`, each row with the columns of its table above and
`statement` the handle that joins them - and exits 1 when the text does not parse. It is derived by the same code
as the map, so a statement's `fingerprint` is the one a map of it holds.

```sql
-- which queries LEFT JOIN Customers, and on what
SELECT f.path, s.line, c.alias || '.' || c.column on_column FROM sql_joins j JOIN sql_statements s ON s.id = j.statement
JOIN files f ON f.id = s.file LEFT JOIN sql_column_refs c ON c.statement = j.statement AND c.role = 'join'
WHERE j.type = 'left' AND j.name = 'Customers';

-- who filters on a column
SELECT f.path, c.line FROM sql_column_refs c JOIN files f ON f.id = c.file
WHERE c.name = 'Orders' AND c.column = 'Status' AND c.role = 'where';

-- the same query, written in several places
SELECT s.fingerprint, count(*) n, min(f.path || ':' || s.line) first FROM sql_statements s JOIN files f ON f.id = s.file
WHERE s.kind = 'select' GROUP BY s.fingerprint HAVING n > 1 ORDER BY n DESC;
```
