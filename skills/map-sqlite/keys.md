# Entity keys - joining your own data to the map across builds (#130)

A row id (`fn:17`, `so:3`) is local to ONE build: a fresh map numbers rows again in path order, so one new file moves
every id after it. To keep your own data about an entity - a verdict per method, a note per SQL column - store its
KEY from `entity_keys`, never its id.

`entity_keys` has one row per (table, row id): `tbl`, `id`, `lang`, `key`. The key is built only from what the source
declares, so the same declaration has the same key in every build on every machine:

| entity | `key` |
|---|---|
| a file | `file:<path from its root>` |
| C# type, property, field, enum | `cs:<symbol>` |
| C# method, constructor, local function | `cs:<symbol>(<parameter types>)` - overloads differ: `cs:Demo.OrderService.Run(int)` |
| python, plain TypeScript class or function | `py:<path>#<qualname>`, `ts:<path>#<qualname>` |
| Angular-half class, enum, interface, type alias, const, named function | `ts:<path>#<Name>` |
| Angular-half member | `ts:<path>#<Class>.<member>` |
| component / directive / pipe / injectable / NgModule row | `ng:<path>#<Class>` |
| enum member | its enum's key `.<Member>`; `id` is `<enum id>#<ord>` (`enums__members` item at `parent = 0`, that `ord`) |
| SQL object, column | `sql:<schema>.<name>`, `sql:<schema>.<table>.<column>`, then `@<database>` when it has one |
| HTTP endpoint, call | `http:<VERB> <route>`, `httpcall:<VERB> <url>` |

- A key is unique IN ITS TABLE. One entity two tables hold has the SAME key in both (a C# enum is a `classes` and an
  `enums` row; a property a `functions` and a `members` row) - add `tbl` to pick one.
- Two rows of one table that would share a key (a partial type, a name declared twice) take `@<path>` (when the key has
  no path in it), then `~<n>` in (path, line) order.
- A row with no name of its own - an anonymous callback, an expression, a call - has no key.

```sql
-- every link by row id is one join away: a column's table, by key
SELECT k.key FROM sql_columns c JOIN entity_keys k ON k.id = c.object WHERE c.name = 'Code';
-- a stored key, back to this build's row
SELECT id FROM entity_keys WHERE key = 'cs:Demo.OrderService.Run(int)' AND tbl = 'functions';
-- ATTACH your own database and join on the key
ATTACH 'reviews.sqlite' AS r; SELECT k.key, v.verdict FROM r.verdicts v JOIN entity_keys k ON k.key = v.key;
```

## Which source a map describes - `_meta`

| key | value |
|---|---|
| `source_commit:<root>`, `source_branch:<root>` | the git commit and branch of each root (absent outside a git repository) |
| `source_dirty:<root>` | `1` when tracked files under the root differ from that commit, else `0` |
| `gate_version` | the release tag the gate was built from (`dev` for a local build) |
| `gate_build` | the digest of what produces the rows - tells two local builds apart |
| `built_at` | UTC time (ISO 8601) of the last run that derived the tables again; a run that changed nothing leaves it |

Every key is written only when its value changes, so a run that changed nothing leaves the database untouched.
