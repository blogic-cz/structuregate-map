---
paths:
  - "src/TsRows/**"
  - "rust/fbtcore/src/rows/**"
  - "rust/fbtcore/src/cssmap/**"
  - "rust/fbtcore/src/atlas.rs"
  - "rust/fbtcore/src/mapper/deep/**"
  - "tests/deep/TsRows*.ps1"
  - "tests/deep/TsAngular/**"
  - "tests/deep/TsKeys/**"
---

# The TypeScript (Angular) half of the deep map

How the half RUNS. The row contract node writes is [src/TsRows/CLAUDE.md](../../src/TsRows/CLAUDE.md); what the
closure derives and which restrictions a gate is read for is
[rust/fbtcore/src/rows/ts/CLAUDE.md](../../rust/fbtcore/src/rows/ts/CLAUDE.md).

A tree with NO Angular workspace is not this half's: the Angular half (`rust/fbtcore/src/mapper/deep/ts.rs`) says so
and the plain half (`src/TsRows/TsPlain/`, `lang = 'ts'`) maps it with syntactic, per-file rows - its columns are the
`map-sqlite` skill's `skills/map-sqlite/typescript.md`. It imports `TsGate.Map.mjs` for the fingerprint, so a change
there changes both maps' duplicate groups; it bumps its own `ROWS_VERSION` (`TsPlain/TsPlain.mjs`).

`src/TsRows/` sits at its folder limit, so a new file goes in a subfolder: `TsDecls/` (declarations), `TsTpl/`
(the template pass), `TsDerive/` (derived passes, ORDER-SENSITIVE), `TsSetup/`, `TsPlain/`. Each subfolder needs
its own line in the `BanRegex` globs. **A module in a subfolder still imports FLAT** (`./TsNodes.mjs`): every
embedded script is staged into ONE temporary folder, so `../` resolves to nothing at run time. Adding a script
means one line in the `TSROWS` set of `rust/fbtcore/src/embedded/mod.rs`, which fails the build on a missing name.

node parses with the WORKSPACE's own `typescript` and `@angular/compiler` - borrowed, never pinned, because
Angular's grammar differs between majors and a pinned compiler error-recovers into a wrong map - and
`rust/fbtcore` stores, so the exe stays one file. Every row carries `half = "typescript"`: this half replaces its
rows WHOLE, and most of its tables have no `file` column (a binding hangs off a node, a call off a member), so
`drop_half` (`rust/fbtcore/src/rows/half.rs`) deletes by `half`.

**THE RENDER CLOSURE IS IN RUST** (`rust/fbtcore/src/rows/ts/`), derived from every other table and therefore
LAST. Handing node's rows back to node for a partial run is JSON just under `JSON.parse`'s ceiling (about 537 MB)
on a large workspace. So `mapper/deep/ts.rs` hands node's rows to `rows::calls::ts_apply`, which WRITES them,
derives the graph from the payload, DROPS it, and only then runs the closure over `Store`
(`rust/fbtcore/src/rows/ts/store.rs`) reading every row back from SQLITE - full run and partial alike: holding
both put the exe at many GB. `Store::from_payload` is a test fixture's door only. **A held list or object cell is
decoded when a pass first reads it** (`store::Cell`); one never read goes back to the write as its text. A pass
must not collect every `ast` into a map up front: map ids to ROWS and read on lookup. `rows/ts/gate/` (with
`gate/ts/`, the readers of a class's own TypeScript) and `rows/ts/key/` are folders by `#[path]`, not modules -
every `super::` there still means `rows::ts`.

**A PARTIAL RUN READS A FILE AGAIN WHEN SOMETHING IT READ CHANGED** (`src/TsRows/TsSetup/TsReads.mjs`,
`rust/fbtcore/src/rows/partial/reads.rs`). Each file records `files.reads` (every in-tree file the checker answered
from while it was extracted - the checker is wrapped, so no call site is listed) and `files.surface` (its forced
declaration emit, unions sorted, plus its decorators, emitted by a separate program). Read again: a changed
file's readers, and the readers - and readers' readers - of every file whose surface moved, decided per project
in program order. A FULL run proves the reads against every row it wrote (`reads::unread`: whatever a row names
must be its owner's read, a scope file, a rebuilt table or `node_modules`); only `reads:typescript = proven`
lets a partial run stop short, otherwise every hop is read. A partial run refuses to write when a kept row
would point at an id it replaced (`reads::dangling`). **A bounded hop count was built and removed - it kept
stale rows.** An importer also depends on the file declaring each name and every barrel between
(`rust/fbtcore/src/rows/partial/deps.rs`).

AN EDIT TO A `scope` FILE (selector, pipe, directive, NgModule - what a template resolves through) GOES THE SHORT
WAY WHILE ITS SHAPE HOLDS (`src/TsRows/TsSetup/TsShape.mjs`, `files.shape`): every token outside a function body,
hashed by node on both runs. A comment or a method body keeps it; a selector, an input, a `templateUrl`, a
decorator or a const a decorator names moves it, and the run reads everything.

**BYTES AND ORDER FOLLOW NODE** - each found only by diffing against node on a real workspace: python's `open()`
TRANSLATES `\r\n` and node does not (`rust/fbtcore/src/rows/pyjson.rs`, `universal_newlines`); `trimStart` strips
a BOM and other trims disagree (`JS_SPACE` in `rust/fbtcore/src/rows/ts/jsstr.rs` spells the set out);
`localeCompare` is NOT code-point order (`jsstr::sort_key` bakes the collation, because the gate compares
`key_reach` IN ORDER).

**THIS HALF IS NOT INCREMENTAL PER FILE, and must not become so: its rows are SEMANTIC.** Rename a component and
the resolved rows of files that did not change become wrong. It refuses to work when the answer is already right:
the tree's hashes AND the `setup` (compiler versions, `structuregate.ts.json` - OUTSIDE the tree - and the
extractor's `rows` number in `src/TsRows/TsMap.mjs`) against what the database recorded. The output database is
excluded from that walk, since a consumer may write it inside the tree it maps.

**AN UNCHANGED TREE DOES NOT START NODE** (`mapper/deep/ts.rs`, key built in `mapper/deep/mod.rs`): the key is the
tree-map entries under the WORKSPACE the last run mapped (`fe`, kept in the record) - so a C# edit beside it does
not launch node - the files it is given, the embedded scripts, `node --version`, its arguments, and what it reads
from OUTSIDE the tree (the borrowed `typescript`/`@angular/compiler` `package.json`, `structuregate.ts.json`), kept
in `_meta` as `skip:typescript` after a clean run. A changed tree is hashed by the plan run, whose hashes ride
`plan.json` into the parse run (`--hashes`).

**ITS IDS RESTART WHEN IT IS THE ONLY HALF IN THE DATABASE** (`alone` in `rust/fbtcore/src/rows/half.rs`), so the
same tree always numbers the same way: continuing them renumbered everything, and `key_reach` sorts its gate lists
by id STRING, so the gate reported thousands of changed rows with no fact different. It is conditional because
nine prefixes are shared with the C# half (`f`, `c`, `x`, `fn`, `br`, `p`, `k`, `e`, `i`); the other halves
stamp `lang`, never `half`, so the per-language tally in `_meta` says who is present.

**THE THREE OUTPUTS THAT ARE NOT TABLES ARE OPTIONS**, off by default because nothing in a build reads them:
`--ts-html <dir>` (one JSON per template, every node carrying the rows' `n:` id), `--map-row-fts` (`row_fts` +
`row_map`: which row anywhere mentions a word) and `--map-atlas <dir>` (`atlas.json` + `atlas.md`: projects,
NgModule areas, every route with what it reaches). The last two run over the finished DATABASE
(`rust/fbtcore/src/rows/fts.rs`, `rust/fbtcore/src/atlas.rs`); they are not a half. The atlas reports a route's
real `children` count (signed-off divergence #2: the predecessor read the number as a list and wrote 0) - a
document for a person, which nothing joins on.
