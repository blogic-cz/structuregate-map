# The gate: judgement, ratchet, doc rules, caches

`run.rs` runs the gate whole — pass cache, walk, counts, rule hosts, judgement, plugins, verdict — and calls
back into C# only to count a `.cs` file with Roslyn and run the async rule over the same text. **It prints
nothing**: it returns lines tagged stdout/stderr and `Program.cs` writes them, because .NET encodes in the
console code page MSBuild reads, and raw UTF-8 here mangles the em dashes in a build log.

## The limits, and the last tenth

| rule | default | flag | counted by |
|---|---|---|---|
| source lines per file | 500 | `--max-lines` | a line a TOKEN sits on: Roslyn (`.cs`), `[Parser]::ParseFile` (`.ps1`), the repo's own TypeScript compiler under `--ts-discipline`, `syn` (`.rs`); `.py` drops blank and `#` lines, keeps docstrings; other extensions a block-comment-aware scanner (`count/`) |
| source files per folder | 15 | `--max-files` | files whose extension is in `--ext` |
| non-blank lines per doc | 200 | `--max-doc-lines` | every measured `.md`, plus no dangling local `.md` link |

**Every limit stops at 90 % of itself** (`CEILING_SHARE` in `mod.rs`): 450 lines, 14 files, 180 doc lines
already fail. A `NEAR LIMIT` warning under an OK headline was a warning nobody acted on, and the edit that
finally failed was the one with least to do with the size. **There are no warnings anywhere in this tool** —
a finding fails or is not reported; the escape hatches (baseline, `// async-ok`, `# psgate-ok`,
`// tsgate-ok`) each leave the reason next to the code. A `.cs` over 4 MB is counted by the streamed `//`
scanner (`STREAM_ABOVE_BYTES`): it has failed every limit already.

## The ratchet (`ratchet.rs`)

`--baseline <file>` records debt a flat limit would turn into a red build on day one:

- a file NOT in the baseline must stay under the CEILING;
- a file IN it may not grow past its recorded count (`baseline GREW`);
- a baseline file now under the ceiling must be REMOVED — the gate says so. Without this a baseline is a
  permanent exemption list with extra steps.

`--update-baseline` freezes everything **at or over the ceiling**, not the limit: frozen from the limit, a file
in the last tenth left the tree red the day it was wired in. `--strict` ignores the baseline. A missing
baseline is an empty ratchet; one that exists and cannot be read is REPORTED (it once threw a
`JsonReaderException` with a stack trace). **Docs have no ratchet**: a doc is always splittable today.

## The doc rules (`docs.rs`)

**A context doc** (`context_doc`) is loaded without being asked for: `CLAUDE.md`, `AGENTS.md`, and anything
under `.claude/` in `agents`, `skills`, `rules` or `commands`. `--doc-scope context` limits only those (a
README costs only the reader who opened it); `--doc-scope all` measures every `.md`. The map's DOC-MISSING
uses the same test. `--doc-skip <glob>` (repeatable) marks a `.md` that is DATA — not measured, not mapped.

**Too long names its largest sections and says SPLIT, do not trim** — trimming deletes the measured reason
nobody can re-derive. The remedy depends on what the file is:

- **agent** (`.claude/agents/`): keep frontmatter and `description` (the routing surface), move a body section
  into `<agent>-<topic>.md` beside it — with NO frontmatter, or the discovery path finds a second agent;
- **skill**: move detail into `<topic>.md` beside `SKILL.md`;
- **`CLAUDE.md` / `AGENTS.md`**: move a section into `<topic>.md` in the same folder and LINK it (an `@import`
  costs the same context as leaving it); guidance about another directory goes in that directory's CLAUDE.md;
- anything else: a sibling `<topic>.md`, linked.

**Every split stays at the same folder level** — a sibling, never a new subfolder, never `docs/`. **Unless the
folder is full**: when sources and docs together already reach `--max-files`, the message sends the section
to `<stem>/<topic>.md` and says which count forced it. Being full is not itself a violation; it only decides
where the split lands, so the doc rule and the folder rule never point opposite ways.

**Links** (`broken_links`) are parsed by `pulldown-cmark`, the markdown half's parser: inline and
reference-style links alike; `topic.md#part` is checked as `topic.md`; a URL is not followed; a link inside a
code block or code span is an example, not a link. An unterminated `](` once borrowed the next link's `)` —
a parser, not a scanner, is why it no longer can.

## The caches (`cache.rs`, `counts.rs`)

Both live in `.fbt/gate.sqlite` in the gated tree; `.fbt/` writes its own `.gitignore` of `*` (`tree.rs`).

- **The pass cache**: a tree byte-identical to one that passed still passes. Keyed by the tree's merkle root
  (hashed from disk by `tree.rs`, never read out of the store), the exe's version and the ARGUMENTS VERBATIM —
  every verdict-changing flag is in them by construction, where a list of "flags that matter" goes stale.
  A miss is bounded: one build skips its gate, the next change is caught. A tree that cannot be hashed is never
  a failure; the gate just walks.
- **The count cache**: on a miss, a file that did not move is not parsed or counted again. An answer is keyed
  by content hash (the tree map's, never re-hashed here), path, rules asked, the tool's build and the host's
  path; the TypeScript key also holds every `tsconfig*.json`, `package.json` and lock file, since a TS 7
  compiler answers per project. A file the snapshot does not name, or a run with `--no-gate-cache` or several
  roots, is counted as before. Only what this run counted is kept.

## The walk refuses to die

A gate that throws instead of reporting gets switched off (`sources/`).

- **Directory links are not followed** — a symlink or junction (`lib64 -> lib` in every python venv). One
  pointing at an ancestor recursed until the path length killed the process; and the target is either walked
  under its real name already (counting it twice inflates a folder) or outside the root.
- **An unreadable path is NAMED, never swallowed** — deny ACE, lock, broken reparse point, dead share. Both the
  listing and the open are guarded, for I/O and access errors alike (catching only the first once let one
  ACL'd file end the run). `NOTE: n path(s) could not be read and were NOT measured` prints before any mode
  returns — on a pass, a violation, and under `--dump` on stderr so the JSON stays parseable.
- `--tracked` runs git with `core.quotepath=off`, or a non-ASCII path arrives as octal escapes.

## The modes

| flag | what it buys |
|---|---|
| `--tracked` / `--include-untracked` | measure what `git ls-files` reports — what a COMMIT contains. Untracked mode catches a violation when authored, not at `git add`. A tracked file deleted on disk is named |
| `--skip <names>` | folder NAMES dropped at every depth; REPLACES the default `out,bin,obj,dist,node_modules,.git,.vs,.venv,__pycache__` |
| `--skip-file <glob>` | one file by its path from the root (`**/schema.d.ts`). Repeatable; each dropped file is named, a pattern matching nothing is named stale, and a map run refuses it (the map keeps the file: others import it) |
| `--async-discipline` | C# async rules (`src/AsyncRules.cs`): nothing blocks inside `async` (`.Result`, `.Wait()`, `.GetAwaiter().GetResult()`, `Task.WaitAll/WaitAny`, `Thread.Sleep`), no sync API with an awaitable twin, `async void` only for an `(object, EventArgs)` handler. Syntax-only; silent where only a symbol table could decide. **Opt-in** because a codebase not written async-first would get a wall of errors on its first build |
| `--ps-discipline` / `--ps-host` | PowerShell, parsed by the host that will run it — see [../../../../src/PsGate/CLAUDE.md](../../../../src/PsGate/CLAUDE.md) |
| `--ts-discipline` / `--ts-host` | TypeScript, by the compiler resolved from the nearest folder at or under the root holding a `package.json` (`src/TsGate/TsGate.Groups.mjs`) — twelve rules plus the token count |
| `--plugin "<cmd>"` | a repo's own rule run where its parser is authoritative (`hosts/plugins.rs`): quote-aware, repeatable, run in the first `--root`, folded into one verdict and exit code. **A plugin that cannot launch is a violation, not a skip** |
| `--worst` | the 15 entries closest to their limit; never fails |
| `--dump` | every measured count as JSON, for comparing two gates file for file |

`--root` repeats. A numeric flag that is not a number, or below 1, exits 2 with one line — it once crashed
with a `FormatException`.

**Equivalence before replacing another gate**: compare `--dump` file for file, both directions, zero
disagreements. A shared gate that silently disagrees with the one it replaces is worse than the duplication.
