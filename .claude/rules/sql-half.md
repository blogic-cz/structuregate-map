---
paths:
  - "src/Map/Sql/**"
  - "rust/fbtcore/src/rows/sqlrun.rs"
  - "rust/fbtcore/src/rows/sqlshape/**"
  - "rust/fbtcore/src/cli/sqlparse.rs"
  - "rust/fbtcore/src/sqlpg/**"
  - "rust/fbtcore/src/rows/seeds/**"
  - "rust/fbtcore/src/rows/dups.rs"
  - "rust/fbtcore/src/mapper/deep/sqlconfig.rs"
  - "tests/deep/SqlMap.Tests.ps1"
  - "tests/deep/sql/**"
---

# The SQL half of the deep map, and the links to C#

What a consumer queries (every table and column, the worked queries) is `skills/map-sqlite/csharp-sql.md`; keep
it in step. This file is how the half is built and the rules that keep it honest.

`src/Map/Sql/` parses every `.sql` (`--ext .sql`) IN PROCESS with Microsoft's own T-SQL parser
(`Microsoft.SqlServer.TransactSql.ScriptDom`, +5.3 MB of exe, no trim warning), or with PostgreSQL's own parser where the
config says PostgreSQL (below). Rows are stored through the
same store as C#, scoped by `lang = 'sql'`: `sql_objects`, `sql_columns`, `sql_keys`, `sql_refs`, `sql_steps`,
`sql_dynamic`, parse errors in `diagnostics`. A file belongs to its nearest `.sqlproj`, which names its database
(`SqlConfig.DatabaseOf`, the config read by `rust/fbtcore/src/mapper/deep/sqlconfig.rs`) and what it is to the
build (`build`, `predeploy`, `postdeploy`, `script`).
**BUMP `SQL_ROWS_VERSION` (`rust/fbtcore/src/mapper/deep/driven.rs`) WITH EVERY CHANGE TO WHAT IT WRITES**
(`sql_steps` included) - an unbumped visitor fix re-read 0 files.

SQLCMD is not T-SQL: `:r`/`:setvar` lines are blanked and `$(Var)` becomes an identifier of the SAME LENGTH
(`SqlVisitor.Prepared`), so lines and offsets still point into the file; the `:r` targets become
`include` refs. `inserted`/`deleted`, `#temp`, `@table` and CTE names are never touches - a CTE's names are known to
its WHOLE statement before any of it is visited (`SqlVisitor.Scoped`: ScriptDom visits a SELECT's query before its
WITH) and forgotten after it.

## PostgreSQL - `rust/fbtcore/src/sqlpg/`, one producer still

A file the config's `dialects` (`sqlconfig.rs::dialect_of`, a glob over the map path) names `postgres` is told to C# in
the `sql-batch` question (`dialects`), its dialect folded into its sha. C# writes its `files` row (`dialect`) and calls
`fbt_sql_parse` (`FbtCore.SqlParse`), then writes the rows rust answers through the SAME `CsTables` and counters
(`SqlPostgres.cs`) - two producers of `lang = 'sql'` would each delete the other's rows.

THE PARSER IS POSTGRESQL'S OWN, as every half's is its language's own: libpg_query (PostgreSQL 17's grammar and
scanner), VENDORED as C source in `rust/fbtcore/vendor/libpg_query/` (its README: the tag, the commit, what was kept)
and compiled by `rust/fbtcore/build.rs` with `cc` into a static archive inside fbtcore - no bindgen, the few doors are
`ffi.rs`. NOT the `pg_query` crate: it needs bindgen and libclang, writes into its own source when `protoc` is on PATH,
and its protobuf decode stops at 100 levels. The vendored folder is outside `src/`, so the regex ban's rust search and
the gate's walk (`.c`/`.h` are no measured extension) never reach it; LeakGuard skips it (upstream text, no consumer's).
Three things this build changes and why (`build.rs`): `YYMAXDEPTH` lifted in both grammars (bison's 10 000 levels was a
depth cap; PostgreSQL's 1 GB allocation limit is the bound now), `DEBUG` defined (without it every call `dup2`s the
process's stderr, which two threads race), and mimalloc's `local_dynamic_tls` (its `initial-exec` made the test build's
`.so` STATIC_TLS, and libpg_query's thread-locals no longer fit glibc's spare static TLS).

ONE THREAD PARSES for the life of the process (`mod.rs`, a 512 MB stack for the library's recursive JSON printer): the
library registers a `pthread` key per thread it first sees, and keys run out. The JSON is read without serde_json's
recursion limit and walked with an explicit stack. The whole file is parsed at once; where the parser stops, the
statement holding its position (between top-level `;` of PostgreSQL's own scanner, `pg_query_scan`) is ONE diagnostic
at that position and the text on each side is parsed again; a quote never closed stops the scanner, and what comes
before that statement is still read. Nodes are numbered in SOURCE order like ScriptDom's (a node's place is the least
of its own location and its children's, `tree.rs`), AND/OR rebuilt as T-SQL's left-deep pairs, and a name keeps the
source's SPELLING (`scan.rs`: PostgreSQL folds `Orders` to `orders`, ScriptDom does not) - so a query written in both
dialects has one tree and one fingerprint (`tests/deep/sql/SqlPostgres.Tests.ps1`). Refs are read off the tree by the
wrapper a `table` sits in (`rows.rs`). A view's query, a PL/pgSQL function's and DO block's statements
(`pg_query_parse_plpgsql`, each `PLpgSQL_expr` parsed again in its own parse mode), a `LANGUAGE sql` body's and a
`BEGIN ATOMIC` body's are statements inside the CREATE/DO (`bodies.rs`); a PL/pgSQL name is a `param`. The
fingerprint, column refs and views are the shared pass's.

## `sql_statements` and `sql_nodes` - per file, the fingerprint filled in rust

`SqlStatements.cs` lists every fragment in visit order and rebuilds the nesting from TOKEN RANGES with an explicit
stack (ScriptDom's visitor has no exit hook): a `TSqlStatement` is a row and the root of its own tree, a statement
inside it a row with `parent`. `SqlTree.cs` maps ~70 fragment types onto the dialect-free node kinds the skill lists;
an unmapped fragment is transparent, and a node no type is (`on`, `target`, `using`, `partition`, `alias`, `schema`,
`distinct`) is made from where a fragment sits or from a property. Nodes are numbered in SOURCE preorder (siblings by
first token): visit order is not source order. A `sql_refs` row's `statement` is the innermost statement holding its
token. `sql_nodes` rows carry NO id - its key is `(statement, n)` - through `CsTables.Plain`.

The fingerprint is RUST'S (`rust/fbtcore/src/rows/sqlshape/`), run by `mapper/deep/mod.rs` right after the half
stores: the half writes `fingerprint = ''`, and the pass fills only those - the statements written this run - so a
partial run is exact by construction (the store drops a re-read file's rows by `file` first). It also replaces the
store's useless `parent` index on `sql_nodes` with `(statement, parent)`. One implementation over the dialect-free
kinds, so every dialect's statements hash alike.

The same pass writes `sql_column_refs` for those statements (`sqlshape/columns.rs`: scopes are `query` nodes and the
root, a `target` wrapper marks the written table, an UPDATE's alias target is its FROM table, nothing ambiguous is
attributed) - a table of its own, NOT a widened `sql_refs`, because every `sql_refs` row becomes a `sql_ref` link
(`SqlLinksEf.cs`). It has a `file` column, so the store's per-file drop covers it; the tally recounts it. The shape
VIEWS (`sqlshape/views.rs`: `sql_clauses`, `sql_joins`, `sql_functions`, `sql_ctes`) are rewritten only when their
text moved. **A test compares a partial run with a fresh map of the edited tree, row by row, ids replaced by what
they name** (`tests/deep/sql/SqlStatements.Tests.ps1`) - a new per-statement table goes into its content queries.

`--sql-parse` (`rust/fbtcore/src/cli/sqlparse.rs`) asks C# the callback mode `sql-parse` (`SqlDeep.Parse`: the same
`Rows` a file of no project gets, as the payload's JSON), inserts the tables into an IN-MEMORY SQLite through
`store::insert_rows`, runs `sqlshape::run_on` and reads the tables and views back - never a second derivation in rust
memory, which would be a second implementation to disagree with the map.

## `sql_links` - derived after both halves, rewritten whole each run

`SqlLinks.cs`, through `fbt_sql_run` (SQL over the map, `rust/fbtcore/src/rows/sqlrun.rs`). The C# side needs no
change: an EF `ToTable` call already carries table, schema and entity type, and a Dapper call's SQL is already
folded into `arguments.const`. Kinds: `ef_table`, `ef_column` (each column to the entity property of the same
name, bases included, or `no property`; a fluent `Property(..)` call names its line, `HasColumnName` its column, and its
chain the facets - `SqlLinksEfFacets.cs`, matched by bound symbol and by the chain's source prefix), `ef_trigger`, `sql_text`, `sql_ref` (a view/proc/trigger/script
statement to its object in ITS OWN database), `sql_include`.

**A NAME IS RESOLVED UNDER `structuregate.sql.json`, NEVER GUESSED** (`SqlResolver.cs`): the databases a site may
reach (a context in `typeof(...)` on the mapping class UNION the contexts holding a `DbSet` of the entity; a
connection name a literal or folded argument spells in the same method), then the one that has the object, then
`default` (`bound`, `detail` = `default of A, B` - the consumer's decision, findable), else `ambiguous` with the
candidates and `database` empty. A `''` database (a script outside every `.sqlproj`) is no candidate beside a
named one, and a script binds its own objects first. An EF mapping applied to several contexts
(`unattributedContexts`) is one `ef_table` per database that has the table. `sys`/`INFORMATION_SCHEMA`/`sp_*` are
`system`. **Without the config file every `.sqlproj` is still parsed and a name links only where exactly one
database has it: fewer links, none guessed.**

The config (beside the exe, or `--sql-config <file>`; paths relative to it) says what code cannot - which
database a context or connection reaches is decided at RUN time: `databases[]` (`name`, `project`, `contexts`,
`connections`), `sqlText` (more methods whose SQL argument is parsed; `*` is the only wildcard),
`unattributedContexts` (where a mapping class with no context in `typeof(...)` is applied), `default`.

**SQL BUILT AROUND HOLES.** A string that did not fold is read from `arguments.template`
(`src/Map/CsRows/CsBody/CsTemplate.cs`: `nameof`, a `const`, a never-written `readonly`, `string.Format`'s `{n}`
and a `foreach` over literal strings are folded or made holes there). `SqlHoles.cs` fills each hole with `0` and
asks the TOKEN it lands in: a number, string or variable is a VALUE (tables stay `bound`, `detail` names the
holes); a hole in a name is `unknown`, or one `partial` row spelled with its hole (`dbo.{name}_Rows`) beside the
known tables. A string that is not T-SQL is `unparsed`, the parser's error in `detail`.

The built-in SQL-text methods are `Dapper.SqlMapper.*` and the `CommandDefinition` constructor (plus EF's
`FromSqlRaw`/`ExecuteSqlRaw`/`SqlQueryRaw`), NOT `Dapper.*`: `DynamicParameters.Add("Size", ..)` read its
NAME as a statement. Dapper's `IN @ids` is rewritten to `IN (@ids)` before parsing (`SqlLinks.Lists`).

**A TRIGGER BUILT BY DYNAMIC SQL**: a `CREATE TRIGGER` in a string is a `sql_dynamic` row (`open` = 1 when the name
runs past the literal, so `name` is its prefix). `HasTrigger("T_Items")` with no `CREATE TRIGGER` for it is
`dynamic` when one of the building script's `@table`/`#temp` VALUES rows says `Items`, `missing` when none does
(a script listing no rows matches on the prefix alone) - which is why the seed pass runs BEFORE the links.

## `sql_seeds` - derived in rust (`rust/fbtcore/src/rows/seeds/`), run by `mapper/deep/` after the SQL half

Input is `sql_steps` (`SqlSeedVisitor.cs`: values/set/temp/drop/flow per file, NOTHING resolved there). Variables
and temps cross `:r` files, so the pass walks each ROOT (a file no `:r` points at) in SQLCMD order with every
`:r` spliced in at its line: `@id` declared in a root is known in its child, a `SET` holds until the next. A sum
of whole numbers or a join of strings is computed; anything the deploy computes (`GETDATE()`, a subquery,
`$(Var)`) stays its text and the column is `unbound`. Steps are ordered by a STABLE sort on line - two on one line
keep their parse order.

- A temp's rows are held until a `MERGE ... USING #t`, `USING (SELECT ... FROM #t)` or `INSERT ... SELECT ...
  FROM #t` moves them through that statement's column mapping (`status = 'temp'` if nothing does); `THEN INSERT
  VALUES (...)` with no column list fills the table's columns in order. `MERGE ... USING (VALUES ...)` and
  `INSERT ... SELECT ... FROM (VALUES ...) AS v(...)` write directly. An INSERT inside a procedure, function,
  trigger or view body is no seed.
- A later `UPDATE t SET ... WHERE` over keys and literals (`=`, `IN`, joined by AND) changes the rows it names
  (`updates` lists each `file:line`); an UPDATE with a join or any other WHERE is not applied, a DELETE never is.
  A left-out column takes the table's DEFAULT only when that is a constant (`defaults`).
- A non-`N` string holding a non-ASCII character is varchar: the row keeps the script's spelling and `detail` says
  the server keeps only what the code page has.
- An `IF` whose condition spells a SQLCMD variable is the DEPLOY's choice: rows under it carry `condition`
  (`'$(Mode)' = 'A'`). Compare a deployed database only with the rows its value allows.
- The values column is `row_values`, node rows of `sql_seeds__row_values` (one item per column, in order) - the text
  as the script spells it, a no-break space included: `values` is SQL-reserved, and PowerShell 5 strips the quotes a
  consumer would need around it.
- **Every seeded table is also a TYPED VIEW `seed_<schema>_<table>`** (`rust/fbtcore/src/rows/seeds/views.rs`): a
  column per column any seed row sets, read by NAME whatever order the INSERT listed it, beside `id, file, line,
  database, condition, status` (a clash is `<table>_<column>`). Views hold nothing, so they cannot disagree with
  `sql_seeds`; a `#temp` gets none.

`duplicate_types` (`rust/fbtcore/src/rows/dups.rs`) is derived beside them: a type name declared in more than one
PLACE (a symbol in a project - a `partial` class is one), and for an enum whether the copies' members differ. It
merges nothing; which copy is the truth is the consumer's rule.
