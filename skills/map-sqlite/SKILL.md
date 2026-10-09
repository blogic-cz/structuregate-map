---
name: map-sqlite
description: "Query the deep source map as SQL, through one SQLite file built by `structuregate --map-sqlite` and read with `--map-query`. Every deep half lands in it - python, C# (bound by Roslyn), TypeScript with or without Angular, T-SQL - and `files.lang` says which half wrote a row. Use it when a question is about EXPRESSIONS rather than files: which functions read a constant, who calls a method BY ITS RESOLVED SYMBOL, what calls pass a given argument, where a phrase appears in the code, which component a route loads, which C# entity maps a table, how many branches a module carries. The JSON map beside it (buildmap.json) answers what imports what and deliberately carries NO code, so anything needing the source text or an aggregate has nowhere else to go — this database holds every call, branch, assignment, decorator and expression as a row WITH its own source, plus the whole tree FTS5-indexed, so `--text` searches the code and `--cat` prints it. ALSO use it for any COUNT you are about to report: `--sql` gives an exact GROUP BY over all the rows instead of a lens output that looks like a total. It is a CACHE of the tree, rebuilt incrementally on each run; `_meta` carries the root and file count that prove which tree it describes."
allowed-tools: Bash(structuregate.exe:*), Bash(dotnet:*), Read, Grep, Glob
argument-hint: "[a name, a symbol, a file fragment, a search string, or SQL]"
---

One SQLite file holding every row the tree's deep halves produce - on a small python tree that
is **tens of thousands of rows in 15 tables**; on a large multi-language tree (C# bound by Roslyn, Angular
and SQL) it is 70 tables and millions of rows - every join column indexed, plus
`file_text` with the source itself FTS5-indexed.

**The origin of this skill is `skills\map-sqlite` in the `structuregate-map` repository**; the copy here is a
junction to it. Each half's own tables and columns are a page beside this one - read the one for the
language you are asking about: [python.md](python.md), [csharp-sql.md](csharp-sql.md) (C#, T-SQL and the
links between them), [typescript.md](typescript.md) (plain TypeScript and Angular). When a
`<consumer>.md` beside this skill names the tree, read it first — a tree may keep its database elsewhere.

**This is a cache of the tree, not a second source.** `buildmap.json` beside it is the resolved GRAPH — what
imports what, what is unread, what is written twice — and is what the Stop hook gates on. This database is
the microscope: it holds no resolved edges, only the rows and the code. Reach for it when the shape of the
question is SQL, or when the answer needs the source.

## Which half wrote a row — `files.lang`, always in the WHERE

Every half writes into ONE database and the halves SHARE table names (`calls`, `functions`, `imports`,
`classes`, ...) wherever the meaning is the same. Every row carries `file` (joins `files.id`), and
`files.lang` says who wrote it:

| `files.lang` | half | shape of its rows |
|---|---|---|
| `python` | python's `ast`, run in the tree's python | syntactic, per file; calls bound through imports and module-level instances |
| `csharp` | Roslyn, one compilation per project | SEMANTIC where a project compiles: `calls.symbol` is the method it runs |
| `ts` | TypeScript without an Angular workspace | syntactic, per file, shaped like python's |
| `typescript` | the Angular half (`angular.json`/`nx.json` present) | semantic, replaced WHOLE each run; every row also carries `half = 'typescript'` and most of its tables have no `file` |
| `sql` | T-SQL (ScriptDom), files under a `.sqlproj` | its own `sql_*` tables, joined to the C# rows by `sql_links` |
| `rust` | `syn`, in the exe | `files`, `consts`, both literal tables, and `handlers`: every `?`, `Err` arm, `unwrap`, `.ok()`, `let _ =`… with its `shape`, `panics`, `test` |

**Not here:** `.ps1` and `.md` are in the JSON map (`buildmap.json`) and write no rows in this
database. A count over a shared table without `f.lang = ...` adds two languages together.
`--tables` prints the languages present (`files:csharp 36`) beside the root and file count, from `_meta`.

## The entry point builds it — do not build it by hand

A tree wired by `Connect-Gate.ps1` writes it as `<tree>\buildmap.sqlite`, beside `buildmap.json`, from the
SAME parse — every run of the npm launcher, the MSBuild target or the Stop hook
(`buildtools\StructureGate.Hook.ps1`), wherever a half can write rows. Measured on a few hundred python files: an
unchanged run takes a fraction of a second, one touched file a few seconds (the JSON re-parse, already being paid), a cold build a little more.
A C# file is read again when it, its project file or its project's references changed; the Angular half
runs whole when any `.ts`/`.html` under its workspace moved and starts nothing when none did.

**A SLOW REFRESH EXPLAINS ITSELF AFTERWARDS.** `_meta` key `last_refresh` is the last WORKING run's account (a run
that changed nothing keeps it), as JSON:
`when`, `took_ms`, `steps_ms` per half, `passes` (`ran`/`replayed`), `reread`, and `csharp.causes` (re-reads by
cause: `content`, `project`, `new`, …) with `csharp.projects` - the projects whose inputs forced the most, and which
input moved - plus `typescript` (files moved by extension, `setup_moved`). Its full trace is `<db>.last-run.jsonl`
beside the database: `structuregate --trace-report <db>.last-run.jsonl`.

**BUILDING IT BY HAND IS WHAT MAKES IT LIE.** `--map-if-stale` compares the source, the file set and the
gate's own mtime against `--map-out` alone, so a hand-built database stays behind while the JSON beside it
is rewritten every run, and nothing says so. Measured before it rode the hook: the JSON most of an hour fresher than the database over a
day of edits, with `--tables` still reporting a confident file count. Run the entry point, not a second
spelling of its command - and pass the roots it passes (python: one `--root` per import root, see
[python.md](python.md)).

A file whose SHA has not moved is not re-read. That is safe because the invariant is checked, not
assumed: any row whose file disagrees with the tree throws the cache away and rebuilds once.

## The lenses

```bash
buildtools/structuregate.exe --map-query buildmap.sqlite --tables
```

| question | command |
|---|---|
| what is in here, and from which tree | `--tables` |
| what does a table record, and how filled | `--schema expressions` |
| what is this thing CALLED that (an import alias also lists the def it binds, FIRST: `both (as _both)`) | `--find STORE.get` |
| what is this row, and what joins to it | `--id x:41` |
| what does the map say about this file (a path is ROOT-RELATIVE) | `--file core/config.py` |
| **what does the file actually SAY** | `--cat core/config.py --lines 40-60` |
| where does this text appear in the ROWS | `--grep scope --table calls` |
| **which functions READ this name** (and every local import alias of it, by `binds`) | `--reads DATA_DIR` |
| **a settings key, end to end** (`section.key` follows a section held by a local: `x = S.get('sec') or {}`, then `x.get('key')`) | `--key REGISTRY_PATH`, `--key service.url` |
| what decorates what, and what is decorated twice; the fragment matches the FILE too, and `args` is what each is called with (a route's path) | `--decorators lru_cache`, `--decorators app.py` |
| **which defs and constants could be DELETED** (candidates, never a verdict; the `unused imports` column names the lines that go with each) | `--dead [path fragment]` |
| **is this default ever changed by a caller** (`passed` / `never passed` / `cannot tell` + why; `bound` is the same verdict over the bound calls alone) | `--defaults [path fragment]` |
| **which import lines does nothing use** (re-exports followed to their takers; a C# `using` as the compiler judges it, none in a file with errors) | `--unused-imports [path fragment]` |
| **which numbers, strings and split/join separators does nothing NAME** (every half; tests left out) | `--magic [path fragment]` |
| **where does it appear in the SOURCE** | `--text "cache AND key"` |
| anything SQL can express | `--sql "SELECT ..."` |

`--limit N` caps rows (default 50, `0` = every row), `--width N` widens cut cells - **a cell is shown cut at
70 characters and ends in `...[+N chars]`; pass `--width 0` before quoting a `source`, a `symbol` or a
`const`** (but not with `--tables` on a big database: it prints megabytes of `_meta`). `--dead` sees only
what the map holds: a folder the hook skips (`tests/`) is invisible, so check it before deleting a candidate.
A path in `--file`/`--cat` is the map's own: relative to the root, or `<root folder>/<path>` with several
roots (`_meta` `roots:python` lists them). A path spelled from a folder above a root
(`src/core/config.py`) is read as the map spells it, and a note says so. `--cat` takes the file the
fragment NAMES - the whole path, else the one path ending on it at a folder - and with several candidates it
lists them and exits 2 rather than guess; a `--lines` range past the file's end exits 2 too. An Angular
file's path is relative to its WORKSPACE folder (`src/app/...`), not the root.

## The shared tables

`files` `imports` `classes` `functions` `parameters` `calls` `assignments` `returns` `raises` `branches`
`decorators` `consts` `exports` `string_literals` `number_literals` `expressions` `arguments` `handlers` `comments`
`regexes` `diagnostics` `file_text` - and C#'s `file_refs` (file -> the mapped `.cs` files its bound symbols are declared in)

A literal's `use` is what it does there (`compare`, `index`, `argument`, `declared`…), with its `callee` and `target`.
Every row carries `file` (joins to `files.id`), `line`, and the scope it sits in — `cls` and `func`, so
"which calls happen inside `build_index`" is a WHERE clause rather than a line-number reconstruction.
`functions.func` + `qualname` carry the PARENT of a nested function (`outer.inner`).

**TWO HALVES DO NOT CARRY IDENTICAL COLUMNS** in a shared table - a C# `using` has an `imports.kind` a
python import does not, a C# call a `symbol` a python call does not - and the store widens the table with
whichever columns arrive. So `--schema <table>` reports a column's FILL RATE: a column at 40 % is usually
one half's, not a half-empty fact. **RUN `--schema <table>` BEFORE REASONING AROUND A MISSING FIELD.**

`regexes`: one row per regex the code builds, every half - `kind` (`literal`, `call`, `attribute`), `api`
(the real function or type: `re.compile`, `System.Text.RegularExpressions.Regex`, `RegExp`), `pattern` +
`pattern_kind` (`literal`, or empty when only the run knows it), `flags`, `used_by`, `source`.

`handlers`: one row per `except`/`catch` clause, every half but SQL - `types` (a list), `bare`, `name` (the
`as`/`catch (e)` name) and `name_read`, `try_line`, and what the body does: `passes` (only `pass`/`...`/a
string, or an empty block), `raises`, `reraises` (bare `raise`/`throw;` or rethrowing the caught name),
`calls`, and the `comment` on the clause's line. python adds `star` (`except*`) and `exc_info`; C# adds
`guard` (its `when` filter), `finally` and `symbol` (the bound exception type). TypeScript names no type a
clause catches, so every TypeScript row is `bare = 1`, and its `try` row in `branches` has an empty `test`:
the caught name is `handlers.name`.

`comments`: every comment - `line`, `col`, `context` (the enclosing def or class, null at module level),
`text`, `inline`, `noqa`, `codes` (python and TypeScript). `diagnostics`: what did not parse or bind (`kind`,
`name` the code, `source` the message), with `files.errors` counting them per file - not `kind = 'generator'` (C#'s CS8795/CS0762: a source generator's missing body).

`arguments`: one row per argument (`call` joins `calls.id`; `position` -1 for a keyword; `star`; `value` a
literal's repr) with **`param`, the parameter the CALLEE declares** - bound calls only, `self` handled.

**The facts are extracted, the text is carried.** An `expressions` row holds `source` *and* what was read
off it:

```
source   (STORE.get(name) or {}).items()
reads    ["STORE", "STORE.get", "name"]
calls    ["STORE.get"]
```

**So do not pattern-match over `source`.** `reads`, `calls` and `strings` are JSON arrays already resolved
from the tree; a regex over the text re-implements them badly and misses everything written across two
lines. Use `LIKE` on those columns, or `json_each`.

**Derived across halves, rewritten each run:** `seed_<schema>_<table>` is a VIEW per seeded table, one column
per seed column, so join on `ProductID` instead of parsing `sql_seeds.row_values`. `duplicate_types` lists a type
name declared in more than one place, and for an enum whether the copies' members differ. With a
`structuregate.facts.json`, `doc_facts` holds the tables of a document outside the tree, and `doc_links` holds
each fact against the code: `bound`, `missing in code` or `missing in doc` - the config, `--facts-pull` and
the token are in [facts.md](facts.md).

## Worked queries, across halves

```sql
-- the densest branching in the tree, per language
SELECT f.lang, f.path, count(*) n FROM branches b JOIN files f ON f.id = b.file GROUP BY f.lang, f.path ORDER BY n DESC;

-- a catch that swallows, in any language
SELECT f.lang, f.path, h.line FROM handlers h JOIN files f ON f.id = h.file WHERE h.passes = 1 AND h.calls = '[]';

-- every regex the tree builds at run time (no literal pattern), and who uses it
SELECT f.lang, f.path, r.line, r.api, r.used_by FROM regexes r JOIN files f ON f.id = r.file WHERE r.pattern_kind = '';
```

The per-language pages carry the rest: dead defs, constants and decorators ([python.md](python.md)); who
calls a method by its bound symbol, how well a tree binds, which entity maps a table
([csharp-sql.md](csharp-sql.md)); which route loads which component ([typescript.md](typescript.md)). To SEE it - folders, a file's neighbourhood, a cycle matrix, a treemap - `--map-view <db>` writes one offline HTML page ([../buildmap/view.md](../buildmap/view.md)). How the
database is built and refreshed: [../buildmap/outputs.md](../buildmap/outputs.md).

## Traps

* **`file_text` is a virtual FTS5 table.** Its content column is `content`, not `text`, and `--schema
  file_text` may answer oddly because the schema lens reads `PRAGMA table_info`. That does not mean the
  source is unreachable — `--text` and `--cat` both work, and so does `SELECT content FROM file_text`.
* **A row id means nothing outside one build.** `x:41` is the 41st expression THIS extraction emitted. Join
  with it inside a session; never store it. Nine prefixes (`f`, `c`, `x`, `fn`, `br`, `p`, `k`, `e`, `i`)
  are shared between the C# and Angular halves.
* **Join on `files.id`, never on `files.path`.** Every `file` column is a row id (`f:12`).
* **`--tables` prints the root and file count from `_meta`.** Compare them against the tree before trusting
  a number from a database you did not just build.
* **A count from a lens is not a total** — lenses stop at `--limit`. Use `--sql` with `count(*)` when you
  are about to report a number.
* **IT IS THE MAPPED ROOTS, NOT EVERY FILE IN THE FOLDER — so an empty result is not an absence.** A
  folder the entry point `--skip`s (usually `tests/`, deliberately: a fixture importing a module is not a
  consumer of it), a file outside every `--root`, a C# file `--map-exclude` matched, and a language the
  entry point's `--ext` does not name are not here. So "no function reads this constant" means no MAPPED
  function does. Read the entry point's command line, and check what it leaves out with `grep` — the one
  question this database cannot answer.
* **Every half lands in ONE database.** Filter a shared table by `files.lang`, or two languages are
  summed; `.ps1` and `.md` are in the JSON map but write no rows here, and `.rs` writes only the few tables above.
