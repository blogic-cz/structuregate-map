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
    { "name": "local", "kind": "file", "path": "codes.md" }
  ],
  "facts": [
    { "name": "codes", "source": "ref", "section": "Item codes", "table": "all", "key": "Item code", "label": 2 }
  ],
  "links": [
    { "facts": "codes", "to": { "kind": "seed", "object": "Demo.Items", "column": "ItemID" } },
    { "facts": "types", "to": { "kind": "enum", "symbol": "Demo.Domain.Models.ItemKind" } }
  ]
}
```

- **`auth`**: `token_env` names an environment variable holding a Drive access token; `token_command` prints
  one. structuregate never stores a token - see [the token](#the-token).
- **`sources`**: `google-doc` is pulled into `save` - a `.json` save gets the document as Google keeps it (the
  Docs API's `documents.get`, every tab), the form to prefer; any other save gets Google's Markdown export.
  `file` is a snapshot the consumer keeps itself (`.json` in the Docs API's shape, or Markdown); nothing is pulled.
- **`facts`**: the tables under the heading named by `section`, up to the next heading of the same level or
  higher - the heading equal to `section` ignoring case, else the only one containing it.
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

Two values match when their text is equal, or both are numbers with the same value (`007` and `7`).

## How a snapshot is read - nothing is matched as text

- **JSON** is walked as the Docs API structures it: a heading is a `HEADING_n` (or `TITLE`) paragraph style;
  a paragraph is bold when every visible character is in a bold run; a table is its rows and cells.
- **Markdown** is parsed by `pulldown-cmark` with GFM tables. Each cell, heading and paragraph is parsed once
  more as inline Markdown, because an export that escapes its own emphasis (`\*\*7\*\*`) leaves the markup
  in the text; the second parse removes it, so the key is `7`.

## `--facts-pull`

```
structuregate --facts-pull [--facts-config <file>]
```

For each `google-doc` source: ask Drive for its `version`; skip it when the snapshot exists and
`<save>.meta.json` records that version; otherwise download it (the Docs API document for a `.json` save,
else the Markdown export) and write the file and `<save>.meta.json` (`id`, `name`, `version`,
`modifiedTime`, `pulled`), each beside its target and renamed over it, so a failed pull leaves the last
snapshot whole.

| exit | meaning |
|---|---|
| 0 | every document is saved and current |
| 1 | a document failed; its line says why (token refused, 404, network) |
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
carries, and the error line names the cause: no Drive scope, or that account cannot read the document.

## What the map writes

| table | one row per |
|---|---|
| `doc_facts` | a row of a configured table: `facts`, `source`, `section`, `subsection` (the bold paragraph or lower heading above its table), `key`, `label`, `cells` (JSON, header -> cell; a column with no header is its number), `file` (the snapshot), `line` (a Markdown line, a JSON `startIndex`), `version` |
| `doc_links` | a fact against a code value; `status` is `bound`, `missing in code` (in the document, not the code) or `missing in doc` (in the code, not the document); `target` is the code row (`sd:`, `e:`, `k:`, `c:`) with its `file`, `line`, `value`, `name` and a seed's `condition` |

A fact matching several code rows (a code seeded twice) gets a `bound` row for each.

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
