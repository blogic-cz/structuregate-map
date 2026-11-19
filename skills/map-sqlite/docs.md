# The docs in the deep map - `md_sections`, `md_names`, `md_links`

Every `.md` the gate measures (the `--doc-scope` set, not `--ext`) under a `--root` is a `files` row - and with
`--doc-root <dir>` every one under that folder instead, keyed from it (a tree mapped from `--root src --root tests`
whose `CLAUDE.md` and `docs/` sit above them passes `--doc-root .`). Each is a `files` row with `lang = 'markdown'`, its
first heading - or a skill's or agent's frontmatter `name` - in `files.doc`. Parsed by `pulldown-cmark` in the exe.

| table | one row per | columns |
|---|---|---|
| `md_sections` | heading | `file`, `n` (from 1), `level`, `heading`, `path` (`Orders > Submitting`), `line` |
| `md_names` | name a reader would follow | `file`, `section` (an `md_sections.id`, '' before the first heading), `line`, `kind` (`link`, `code`, `command`), `text` as written, `path` it claims, `target` it resolved to, `target_line`, `status`, `symbol` |
| `md_links` | name that is bound or missing | `md_names`' columns plus `target_path`, `target_file` (a `files.id`), `target_table` + `target_id` (the declaration row), `symbol`, `status`, `matches` |

WHAT IS A NAME: a link target, a code span, or a word of a SHELL fence - the file map's own rules, so a path is
`bound` here exactly when `buildmap.json` says the doc `mentions` it. A path resolves beside the doc, in each
folder above it, under it, then from each code root and the repository top (`tests/x.cs` in `src/A/CLAUDE.md`); a
`\` path is read as `/`. `target_file` is empty for a file no half maps (a `.json`); `target_line` is the line a location names (`x.cs:3`).

WHAT A SYMBOL BINDS TO: a dotted code span (`Acme.Orders.OrderService.Submit`, `OrderKind.Beta`, `Submit()`) is
`bound` when it ends exactly ONE declaration - `functions`, `consts` (an enum member too), `classes`, `enums` - by
whole segments; `ambiguous` when it ends several (`matches`). A span written `a::B` binds rust rows only. A single
word binds only to a class it alone names. A span ending in a lower-case file extension (`OrderService.cs`) is a
file, never a symbol. Anything else a span says (`true`, `json.dump`) is no row.

`status = 'missing in code'`: a link target, a shell word with a folder, or a code-span path ending in a file
name that is not there - or a member of a declared TYPE that the type does not declare (`OrderService.Gone`,
said only for a type whose members the map records). It is said for every doc; the gate's `DOC-MISSING` note
only for a context doc.

```sql
-- which docs describe this file, before changing it
SELECT d.path, l.line, l.text FROM md_links l JOIN files d ON d.id = l.file
 JOIN files t ON t.id = l.target_file WHERE t.path = 'src/Acme/Orders/OrderService.cs';
-- every stale doc link: a file or member that is gone
SELECT d.path, l.line, l.text FROM md_links l JOIN files d ON d.id = l.file WHERE l.status = 'missing in code';
-- what a section of a doc names
SELECT s.path, n.text, n.status FROM md_names n JOIN md_sections s ON s.id = n.section WHERE s.heading = 'Submitting';
```
