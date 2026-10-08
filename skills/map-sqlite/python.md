# The python half (`files.lang = 'python'`)

Everything in [SKILL.md](SKILL.md) holds; this is what the python rows add. Rows are SYNTACTIC, stored and
replaced per FILE, from python's own `ast` run in the tree's interpreter.

**One `--root` per import root.** A tree that puts several folders on `sys.path` resolves `from core
import x` inside each; a single root above them finds nothing, and the database answers wrongly about
every module under them. Pass the roots the entry point passes (`_meta` `roots:python` lists them).

## Its own tables

`withs`: one row per context manager (`position`, `source`, `target` the `as` part, `is_async`).
`globals`: `global`/`nonlocal` declarations (`kind`, `name`). `deletes`: one row per `del` target.

`branches.kind` is `if` `while` `for` `try` `assert` `match` `case`. A `try` row's `test` lists every type
its handlers catch, dotted ones included; empty means `try/finally` or a bare `except:` only.

`regexes` here is one row per call that builds a regex (`compile`, `match`, `search`, `sub`, ... of `re` or
`regex`, aliases resolved): `api` (`re.compile`), `pattern` + `pattern_kind`, `flags`, `used_by`.

## Two columns that cost real bugs by going unread

* **`decorators.target` + `target_kind`** name WHAT a decorator decorates. Removing a dead function
  under a decorator without asking left the `@property` on the NEXT function (a method became a
  property: `TypeError: 'dict' object is not callable`) and stacked `@lru_cache` twice elsewhere. The
  check is `SELECT target FROM decorators WHERE file = ?`, and the audit for a whole tree is a
  decorator whose target matches no `functions`/`classes` row. It is also how you tell a route handler
  from dead code — `@app.get`/`@router.get` is entered by HTTP, never by name.
* **`decorators.args`** is what a decorator is CALLED WITH, one source text per argument - the route of
  `@app.post("/items")` - so `--decorators app.py` lists the routes a file serves without a `--cat`.
* **`functions.func` + `qualname`** already carry the PARENT of a nested function (`outer.inner`), so
  "dead closure or dead module function" is `WHERE func <> ''`, not a guess from indentation.

## A name is read in ten places, and `--reads` looks at all of them

`expressions`, `assignments`, `consts`, `returns`, `branches`, `raises`, `parameters`, `withs`, `deletes`
and `handlers` each carry a resolved `reads`, because a query over one of them answers with a fraction.
The two that a hand-written join always misses:

* **`parameters`** — one row per parameter with `annotation`, `default_expr` and the default's own `reads`.
  A tree whose defaults are constants (`def load(path=_SETTINGS_PATH)`) reports every one of those
  constants as unread without this table; a dead-code pass built on that offered dozens for deletion.
  `default_expr` and not `default`: the second is a SQL keyword and every query naming it needs quoting.
* **`returns.reads` and `branches.reads`** — `return DATA_PATH` and `if DATA_PATH:` produce NO expression
  row (a bare name is not one), so before these columns a constant used that way answered "nothing reads
  this" — which is exactly the shape a dead-code pass then offers up.

## A call is bound, not matched by name

`calls.target_path` + `target_name` are the file and `qualname` it runs, through the file's own imports and
scopes and through package re-exports (`self.help()` is `Thing.help`), and through the class that made a
module-level instance (`STORE.get()` is `Store.get`). EMPTY when the tree cannot say: a
parameter, an inherited method. **Every `reads` has a `binds` beside it**, position for position: the
`file::qualname` each read names, or "" - so config.py's `ROOT` is told from every other `ROOT` by
`json_each(binds)`, never by the name. `imports.bind` and `classes.bases_bind` are the same for an imported
name and a base class; `--dead` walks the latter to keep an override, and a method of a class whose base is
outside the map, off its list. `getattr(mod, name)` gives `target_name = '*'` - any def in that file.
`functions.launched = 1` is a def a launcher calls. `--find` follows `imports.bind` (an alias, or a
re-import of one, to the def), and `--reads X` matches `binds` as well as `reads`, so a read through
`from m import X as Y` is a read of X.

**A settings key carries its path.** `string_literals.key` is the dotted key a lookup string reads
(`_S["server"]["host"]` is `server.host`; `"url"` in `_svc.get("url")` after `_svc = _S.get("service") or {}`
is `service.url`), and `assignments.keys` / `consts.keys` list the keys a value reads - what `--key sec.key`
joins on.

**`--dead` is the dead-def question - never hand-write it.** The pasted SQL it replaced took most of a minute and
listed dozens of live defs (route handlers, dunders, names in a dispatch table) as dead.

## Worked queries

```sql
-- who calls into the store, and how often
SELECT callee, count(*) n FROM calls WHERE callee LIKE 'STORE.%' GROUP BY callee ORDER BY n DESC;

-- which functions read a given constant (`--reads DATA_DIR` is this query over all ten tables)
SELECT f.path, x.func, count(*) n FROM expressions x JOIN files f ON f.id = x.file
WHERE x.reads LIKE '%DATA_DIR%' AND x.func <> '' GROUP BY f.path, x.func ORDER BY n DESC;

-- a constant reached through a parameter default, which the query above cannot see
SELECT f.path, p.qualname, p.name, p.default_expr FROM parameters p JOIN files f ON f.id = p.file
WHERE p.reads LIKE '%DATA_DIR%';

-- one decorator applied twice to one def: silent at run time, invisible in a diff
SELECT file, target, name, count(*) n FROM decorators GROUP BY file, target, name HAVING n > 1;

-- every optional import, and where
SELECT f.path, i.line, i.module FROM imports i JOIN files f ON f.id = i.file WHERE i.guarded = 1;

-- decorators in use, which is how the routes and the caches show themselves
SELECT name, count(*) n FROM decorators GROUP BY name ORDER BY n DESC;
```
