# A slim map: the tables a config picks, out of a full map

`--map-slim <base> --slim-config <json> --map-sqlite <out>` copies part of a full deep map into a new database - a review map
a CI job uploads, a database an agent opens - without extracting anything again. A slice (`--map-slice`) cuts by FILES; a slim
map keeps every file and cuts by TABLE and ROW.

```json
{
  "tables": ["files", "file_text", "classes", "members", "calls", "expressions"],
  "where": { "expressions": "length(source) >= 40", "calls": "target_id IS NOT NULL" },
  "derived": ["ts_reads"]
}
```

* `tables`: each is kept with its node tables (`<table>__<column>`) and its indexes; `file_text` with its full-text shadow
  tables, so `MATCH` and `--text` answer. `_meta` is always kept. A view of the base is kept when every table it reads was.
* `where`: a SQL condition per kept table, read against the base's columns; only those rows are copied, and their node rows.
* `derived`: `ts_reads` - one row per TypeScript read site that names its declaration, out of the `target` nodes of the
  expression trees (code and template): `expression`, `kind` (`Read`, `SafeRead`, `Write`), `name`, `decl_file` (a `files`
  id, NULL for a library's), `decl_path`, `decl_line`, `file`, `line`, `col` - the read's own place (a template's in its
  `.html`). On a base before rows 134 the place is the expression's line (a template's binding line) and `col` is NULL, so two
  reads of one declaration on a line are one row. Indexed on `(decl_file, decl_line)` and `(file, line)`.

Who reads a member: `SELECT f.path, r.line, r.col FROM ts_reads r JOIN files f ON f.id = r.file JOIN members m ON m.file = r.decl_file AND m.line = r.decl_line WHERE m.name = ?`.

Every name is checked against the base: a table it does not hold, a `where` on a table not kept, or an unknown derived table
fails (exit 2) - a list kept by hand cannot drift from the schema. `_meta` says what it is: `partial = 1` (no map run
writes into it), `slim:of` (the base), `slim:at` (when, UTC), `slim:tables`; `options:text = 0` without `file_text`.
`--map-query` notes on every question that a table or row missing here was left out, not absent from the tree.
