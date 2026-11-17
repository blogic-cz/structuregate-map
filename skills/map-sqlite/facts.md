# Facts from documents outside the tree

A reference document can list codes the code also seeds (here into `Demo.Items`). When an item is added,
nothing says the document is now behind. The deep map reads such a document and checks it against the code,
both ways. Query side: [SKILL.md](SKILL.md).

**The tool provides the means; the consumer writes the configuration.** structuregate provides a puller that
saves each document as a snapshot, a reader for its tables, and a link engine that checks each table against
the code. The consumer says which documents, which tables, which code - and which code rows MUST be
documented, which is a query over `doc_links`.

## `structuregate.facts.json`

Beside the exe, or at `--facts-config <file>`. Paths are relative to the file; `//` comments and trailing
commas are allowed, as in `structuregate.sql.json`.

```json
{
  "auth": { "token_command": "gcloud auth application-default print-access-token" },
  "sources": [
    { "name": "ref", "kind": "google-doc", "id": "<google-doc-id>", "save": "ref.json" },
    { "name": "local", "kind": "file", "path": "codes.md" },
    { "name": "spec", "kind": "drive-file", "id": "<drive-file-id>", "save": "spec.docx" }
  ],
  "facts": [
    { "name": "codes", "source": "ref", "section": "Item codes", "table": "all", "key": "Item code", "label": 2 },
    { "name": "routes", "source": "spec", "from": "routes", "prefix": "/api/" }
  ],
  "links": [
    { "facts": "codes", "to": { "kind": "seed", "object": "Demo.Items", "column": "ItemID" } },
    { "facts": "types", "to": { "kind": "enum", "symbol": "Demo.Domain.Models.ItemKind" } },
    { "facts": "routes", "to": { "kind": "endpoint" } }
  ]
}
```

- **`auth`**: `token_env` names an environment variable holding a Drive access token; `token_command` prints
  one. structuregate never stores a token - see [the token](#the-token).
- **`sources`**: `google-doc` is pulled into `save` - a `.json` save gets the document as Google keeps it (the
  Docs API's `documents.get`, every tab), the form to prefer; any other save gets Google's Markdown export.
  `drive-file` is an UPLOADED file (a `.docx`, which the Docs API and the Markdown export refuse), saved as it
  is into `save` (which must end in `.docx`). `file` is a snapshot the consumer keeps itself (`.json` in the Docs
  API's shape, a `.docx`, or Markdown); nothing is pulled.
- **`facts`**: what lists facts under the heading named by `section`, up to the next heading of the same level or
  higher - the heading equal to `section` ignoring case, else the only one containing it. No `section` is the
  whole document (then `name` is required).
  - `from`: `table` (the default) reads a table, below; `code` takes every code span (each once); `routes` takes
    every route the text names - in running text, code spans, headings, list items, table cells and code blocks:
    a word starting with `/` (a full url keeps its path; a query is cut), with its verb when the word before it is
    `GET`/`POST`/`PUT`/`DELETE`/`PATCH`/`HEAD`/`OPTIONS`. `prefix` keeps only the routes starting with it.
  - `table`: `"all"` reads every table of the section as ONE list; a number reads that table only (default 1).
  - `key` (required) and `label`: a header cell's text, or a column number from 1. A named column is found
    in the first row of a table holding a cell with that text (GFM header row or body row); the rows after
    it are data. A table with no such row continues the one before it, same columns.
  - `name` defaults to the section.
- **`links`** (`to.kind`):

| kind | the code values | matched by |
|---|---|---|
| `seed` | the `column` of every `sql_seeds` row of `object` (`Schema.Table`) | value |
| `enum` | the members of the enum with that `symbol` (C#), or that `name` when exactly one enum has it (TypeScript) | value, or `"by": "name"` |
| `consts` | the constants the class `symbol` declares (a static class of codes) | value, or `"by": "name"` |
| `class` | the classes that list `symbol` among their bases | class name |
| `endpoint` | every `http_endpoints` row, `VERB /template` - of one controller with `"class"` | segments and verb |
| `property` | the properties of `symbol`: a C# class with what its bases declare (named by `[JsonPropertyName]`, `[JsonProperty]` or `[DataMember(Name)]` where it has one, `"by": "name"` for the C# name), a TypeScript interface, type alias or class | name, ignoring case; a child row (`-> Name`, `->`/`–>`/`—>`/`→`, once per level) in its parent row's property type first, linked or not (an array's or a list's element) |
| `sql-column` | the columns of the SQL table `object` (`Schema.Table`, `dbo` by default) | name, ignoring case |

Two values match when their text is equal, or both are numbers with the same value (`007` and `7`). A route matches an
endpoint by its segments, ignoring case: a parameter (`{id}`, `{id:int}`, `:id`) there takes any one segment here, an
optional one (`{id?}`) may be left out, and the verbs must agree when both name one (`[Route]` alone answers every
verb). A list linked to several types (a response and those nested in it) is one check: `missing in code` when none has it.

## `--facts-suggest`

```
structuregate --facts-suggest <map.sqlite> [--facts-config <file>]
```

Writing the `facts` and `links` by hand means knowing which part of a document lists what. This finds them by
EXACT values, never by names: every column of every table (under its nearest heading), every section's code
spans and routes, and the whole document's, against every value set the map holds - each seeded table's column,
each enum's members (by value and by name), each class's constants (both), the endpoints, each type's
properties, each SQL table's columns. A pair is a candidate when at least 2 values and a fifth of the document's
group are code values. A count alone misleads, so pairs are RANKED by:

1. **rows that agree** - for a table column: the rows whose key matches a code row AND whose other cell names that
   same row's other value (a seed row's other column, an enum member's or a constant's name). The true link is the
   one where the label column agrees too; a pair with agreeing rows outranks every pair without. The agreeing
   column becomes the candidate's `label`.
2. **the section's endpoint** - a routed section (a route in its heading or one above it, or in the text right under
   its own heading - not an overview above it; `/api/x POST` too) binds the endpoints it SERVES (resolved return, parameter, `[ProducesResponseType]` and
   property types; a generic base's `T` bound by the serving controller) and the routes it CALLS (`http_calls`, the
   call's bound type and parameters, matched by the path's end - a base url is ignored). A type of one outranks a class only sharing
   the names: `, the (outbound) response type of POST /x` / `, no type of ...`; every property candidate lists `endpoints_using`.
3. **hits beyond chance** - the document's integers times how densely the code set covers THEIR range (a key
   column `1..50` matches any small-int enum, and any `1..N` ordinal column whatever else it holds): that many hits
   are expected and taken off; then the hits.

Each group's best set and each set's best group are kept, best first:

```
facts suggest: 3 candidate(s) -> structuregate.facts.suggest.json
  spec: table 2 under "Items", column 1 "ItemNo": 45/50 values = seed dbo.Items.ItemNo (200 code values, 40 rows agree: column 2 = Name)
  spec: routes the whole document: 38/41 values = endpoint every endpoint (52 code values)
  spec: table 2 under "Items", column 1 "ItemNo": 30/50 values = enum Demo.Region (30 code values, ~30 by chance)
```

`structuregate.facts.suggest.json` (beside the config) holds each candidate as a ready `facts` entry and `link`
entry, its counts and up to 10 of the document's values that matched nothing. Nothing is accepted: a person or an
agent copies the right ones into the config. Only `sources` are needed. Exit 0 (none is an answer too), 2 no config/map.

### `--facts-review`: a model reads the candidates

Sharing values is not meaning the same thing (`1, 2, 3` are in every enum). `--facts-suggest <map> --facts-review`
asks a model, in batches of ~60k characters (a document part's candidates together), whether each part lists the same
thing as its code set, and writes its verdict and reason BESIDE each candidate (`review` in the suggest json); the
accepted ones become a `{"facts", "links"}` fragment in `structuregate.facts.reviewed.json`, a part accepted against
several sets `ACCEPTED, AMBIGUOUS` under `ambiguous`. A failed batch is `NOT REVIEWED`, the rest still count. Each
question offers the MAP AS A TOOL - `{"type": "sqlite_query", "path", "name": "map_query", "description", "max_calls": 8}`
in `tools`, described by the map's own tables - and asks the model to query it when the text cannot settle a candidate;
`STRUCTUREGATE_LLM_TOOLS` = `0`/`off` drops it (a program that cannot run tools), a number sets `max_calls`.

`STRUCTUREGATE_LLM` is a program (a path, or a name on PATH), `STRUCTUREGATE_LLM_ARGS` its arguments split at
whitespace; off unless named. The contract: `{system_prompt, user_message, schema, want, tools?}` as ONE JSON object
on stdin, ONE back on stdout - `{"ok": true, "data", "tool_calls"?}` or `{"ok": false, "error_type", "message"}` - exit
0, or 2 unavailable, 3 invalid reply, 4 timeout, 5 refused. The first line counts the tool calls when it reports them.
**The candidates' values, and whatever the model reads from the map, are sent to that model.**

| exit | meaning |
|---|---|
| 0 | candidates written and reviewed |
| 1 | the model gave no answer (the candidates are written, unreviewed) |
| 2 | no model configured (checked before any work) |

## How a snapshot is read - nothing is matched as text

- **JSON** is walked as the Docs API structures it: a heading is a `HEADING_n` (or `TITLE`) paragraph style;
  a paragraph is bold when every visible character is in a bold run; a table is its rows and cells.
- **A `.docx`** is a zip read by the gate itself: `word/document.xml` is the body, a paragraph is a heading when its
  style's NAME is `heading N` or `Title` (`word/styles.xml`; the id may be localized) or it has an outline level, a
  run is bold by `w:b` and a code span when set in a monospace font. A row's `line` is the body element's number.
- **Every form**: whitespace is squeezed to single spaces, and the zero-width characters a pasted text carries
  (U+200B, U+200C, U+200D, U+2060, U+FEFF) are dropped - invisible in the document, they would split a route or a
  code into words nobody wrote.
- **Markdown** is parsed by `pulldown-cmark` with GFM tables. Each cell, heading and paragraph is parsed once
  more as inline Markdown, because an export that escapes its own emphasis (`\*\*7\*\*`) leaves the markup
  in the text; the second parse removes it, so the key is `7`.

## `--facts-pull`

```
structuregate --facts-pull [--facts-config <file>]
```

For each `google-doc` and `drive-file` source: ask Drive for its `version`; skip it when the snapshot exists and
`<save>.meta.json` records that version; otherwise download it (a drive-file as it is, `alt=media`; for a
google-doc the Docs API document for a `.json` save, else the Markdown export) and write the file and `<save>.meta.json` (`id`, `name`, `version`,
`modifiedTime`, `pulled`), each beside its target and renamed over it, so a failed pull leaves the last
snapshot whole.

Every wait is bounded: 30 s to connect, send and start answering, 300 s for a body - a stalled network fails the
document instead of hanging the run. A busy API (429, 5xx) is asked 3 times, 2 s then 4 s apart. A `.json` save
of a file that is no Google Doc (an uploaded `.docx` or PDF) says so instead of a bare `HTTP 400`. The token is
sent only in headers and the `tokeninfo` POST body, never in a URL.

| exit | meaning |
|---|---|
| 0 | every document is saved and current |
| 1 | a document failed; its line says why (token refused, 404, not a Google Doc, busy API, stalled network) |
| 2 | the config is wrong |

The map run never fetches anything. Committing the snapshots is the consumer's choice.

## The token

A pull only reads, so the token needs only `https://www.googleapis.com/auth/drive.readonly`. With gcloud:

- **`gcloud auth login --enable-gdrive-access`** works with gcloud's own OAuth client but grants the full
  `drive` scope - read AND write to every file of the account. If you use it, `gcloud auth revoke` after the
  pull; `--facts-pull` prints a note when a token has that scope.
- **A read-only token needs your own OAuth client.** Google blocks Drive scopes for gcloud's built-in client
  in an `application-default` login ("This app is blocked"). Create an OAuth client of type *Desktop app*,
  download its `client_secret.json`, and log in with it:

  ```powershell
  gcloud auth application-default login --client-id-file=client_secret.json --scopes="https://www.googleapis.com/auth/drive.readonly,https://www.googleapis.com/auth/cloud-platform"
  ```

  In PowerShell `--scopes` must be quoted, or the comma splits it. Then set
  `"token_command": "gcloud auth application-default print-access-token"`.

When Drive refuses the token (401/403), the pull asks Google which account it belongs to and which scopes it
carries, and the error line names the cause: no Drive scope, or that account cannot read the document. A 403 that
says the API itself is switched off in the token's Cloud project (`SERVICE_DISABLED`) names that API and where
to enable it instead.

## What the map writes

| table | one row per |
|---|---|
| `doc_facts` | a row of a configured table, or a code span or route (`key` only, `cells` empty): `facts`, `source`, `section`, `subsection` (the bold paragraph or lower heading above its table), `key`, `label`, `cells` (node rows `doc_facts__cells`, header -> cell; a column with no header is its number), `file` (the snapshot), `line` (a Markdown line, a JSON `startIndex`), `version` |
| `doc_links` | a fact against a code value; `status` is `bound`, `bound, nesting fixed` (a child row nested wrong, read in another open row's type - `nesting` the depth its arrows should show), `missing in code` (in the document, not the code) or `missing in doc` (in the code, not the document); `target` is the code row (`sd:`, `e:`, `k:`, `c:`, `he:`, `m:`/`tm:`, a `sql_columns` id) with its `file`, `line`, `value`, `name` and a seed's `condition`; a fact matching several code rows gets a `bound` row for each |
| `doc_sections`, `doc_tables` | EVERY section of every source, configured facts or not: `doc_sections` (`id` `ds:`, `source`, `file`, `n`, `level` - 0 before the first heading -, `heading`, `path` the headings down to it joined ` > `, `line`, `text` every text it holds, a line each); `doc_tables` a cell (`section` its `ds:` id, `tbl` from 1, `row` - 0 a Markdown header row; a Docs or Word table, which marks none, from 1 -, `col` from 1, `value`, `line`) |

A section missing from the snapshot, a missing snapshot, or a link whose code is not in the map is a NOTE,
and the other lists are still checked (in the last case every fact of that link is `missing in code`). A
config that cannot be read is an ERROR, checked before any half runs, and fails `--map-sqlite`.

The check is one of the passes over the finished rows. Its key holds the config and every snapshot, so an
unchanged refresh replays it, and a newly pulled document is checked again on a tree that did not move
(the note reads `the facts config or its snapshots moved`).

```sql
-- items the document must list and does not: the CONSUMER's rule
SELECT v.ItemID, v.Name FROM doc_links l JOIN seed_Demo_Items v ON v.id = l.target
WHERE l.facts = 'codes' AND l.status = 'missing in doc' AND v.IsInternal = 0
```
