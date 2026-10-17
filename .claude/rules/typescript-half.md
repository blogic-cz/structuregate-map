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
and the plain half (`src/TsRows/TsPlain/`, `lang = 'ts'`) maps it - and every script file OUTSIDE a workspace
(`plain_files` in `mapper/deep/mod.rs`; its paths are root-relative, the Angular half's workspace-relative; it borrows
`--ts-node-modules`, then the workspace's own `node_modules` - `plain_modules`) - with syntactic, per-file rows - its columns are the
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
rebuilt from its node rows when a pass first reads it** (`Cell::Nodes`, `cells::load`); one never read stays rows and is never rebuilt. A pass
must not collect every `ast` into a map up front: map ids to ROWS and read on lookup; a scan that asks something of
EVERY row's tree reads it with `Row::peek`, which keeps nothing decoded, and checks the id before it asks. The store
is RELEASED before `key_reach` (`Store::release`): nothing after reads the rows, and holding them was most of the peak. `rows/ts/gate/` (with
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
would point at an id it replaced (`rows/partial/dangling.rs`, asked BEFORE the replace deletes a row - asked after, the ids were already gone and it found nothing). The graph and the scope set are the tables `ts_deps` (`target`,
`dependent`, in the graph's order) and `ts_scope`, rewritten by `rows/partial/remember.rs` - whole on a full run,
repaired on a partial one - and handed to node as JSON. **A bounded hop count was built and removed - it kept
stale rows.** An importer also depends on the file declaring each name and every barrel between
(`rust/fbtcore/src/rows/partial/deps.rs`).

AN EDIT TO A `scope` FILE (selector, pipe, directive, NgModule - what a template resolves through) GOES THE SHORT
WAY WHILE ITS SHAPE HOLDS (`src/TsRows/TsSetup/TsShape.mjs`, `files.shape`): every token outside a function body,
hashed by node on both runs. A comment or a method body keeps it; a selector, an input, a `templateUrl`, a
decorator or a const a decorator names moves it, and the run reads everything.

**A CARRIED ROW COMES BACK WITHOUT THE STRUCTURES NO PASS READS** (`src/TsRows/TsSetup/TsCarried.mjs`, handed to
`rows/partial/uncarried.rs` through the plan): such a cell is its kind alone, a STUB that refuses every look inside;
a table node rebuilds whole sends no rows at all. A pass that reads a stub, replaces one or would send one back marks
the run (`MAP-UNCARRIED`), which is asked again with every cell carried - so a list that misses a read costs time,
never a row. On a run that stopped short, `dangling` asks every kept row - its plain cells and its NODE rows - and every
row of a whole table the payload sends (`Digest::named`) for an id the replace removes and the payload does not hold
again (`nested`), which node once asked of every row it held; a string that only looks like an id is no reason to retry. THE CARRY IS WRITTEN BESIDE THE PARSE
(`carried_parse`, `mapper/deep/ts.rs`): node builds every program ahead without a checker (`prewarm`,
`TsSetup/TsProgram.mjs` - each parse given back as it was parsed, or a compiler before 5.3 resolves and notes less)
and then loads the rows as rust writes them (`readCarry`, `--carry-part`: the file read while it grows, whole once renamed).

**A PARTIAL RUN KEEPS THE CLOSURE WHEN NOTHING IT READ MOVED** (`rust/fbtcore/src/rows/derive/unchanged.rs`). Every
derivation records what its passes READ - each column asked of a row (`Store::reads`, flagged by `Row::get`) - in
`_closure_reads`, and what it was derived under (setup, feature API, options, the running build, the spec's word on those columns) in
`_meta` `closure:typescript`. A partial run hashes the rows it removes before they go and the rows it writes in their
place, per table in rowid order, every read cell with a structure as its nodes, plus what the dead-key trace makes of
each affected file's text (`file_text` before, the disk after): all equal, and the four tables stay. That needs the
same ids back: EVERY per-file table with a unique key is claimed (`identityKeys`), not only those another file names,
with sparse ALTERNATIVES for the rows a first key leaves out (`identityOf`: a template expression has no line), and a
rollup takes its old ids back in the order it made them (`rollupId`, handed over by `carry` as `rollup`). A table read
whole (no `half`) is never kept; `forget` runs before any row is written. A closure that MOVED is written as what
changed (`rows/derive/delta.rs`): a stored row is matched to an emitted one by its content as the store holds it (its
cells, then each structure's node rows), and only the rows left over on either side are deleted or inserted.

**BYTES AND ORDER FOLLOW NODE** - each found only by diffing against node on a real workspace: python's `open()`
TRANSLATES `\r\n` and node does not (`rust/fbtcore/src/rows/pyjson.rs`, `universal_newlines`); `trimStart` strips
a BOM and other trims disagree (`JS_SPACE` in `rust/fbtcore/src/rows/ts/jsstr.rs` spells the set out);
`localeCompare` is NOT code-point order (`jsstr::sort_key` bakes the collation, because the gate compares
`key_reach` IN ORDER).

**THIS HALF IS NOT INCREMENTAL PER FILE, and must not become so: its rows are SEMANTIC.** Rename a component and
the resolved rows of files that did not change become wrong. It refuses to work when the answer is already right:
the tree's hashes AND the `setup` (compiler versions, `structuregate.ts.json` - OUTSIDE the tree - the
extractor's `rows` number in `src/TsRows/TsMap.mjs`, and WHAT PRODUCES ITS ROWS, handed as `--build`: the embedded
scripts and the rust their payload passes through, `embedded::node_rows::extractor`) against what the database
recorded. The output database is excluded from that walk, since a consumer may write it inside the tree it maps.

**AN UNCHANGED TREE DOES NOT START NODE** (`mapper/deep/ts.rs`, key built in `mapper/deep/mod.rs`): the key is the
tree-map entries under the WORKSPACE the last run mapped (`fe`, kept in the record) - so a C# edit beside it does
not launch node - the files it is given, the embedded scripts, `node --version`, its arguments (what produces its rows
among them, never the running build), and what it reads
from OUTSIDE the tree (the borrowed `typescript`/`@angular/compiler` `package.json`, `structuregate.ts.json`), kept
in `_meta` as `skip:typescript` after a clean run. A changed tree is hashed by the plan run, whose hashes ride
`plan.json` into the parse run (`--hashes`).

**A RELEASE THAT CHANGES ONLY WHAT RUST DERIVES READS NO TREE AGAIN.** The rust a stored node row passes through -
node's arguments and state, the payload parse, the cells, the insert, the ids and claims, the graph, the reads proof -
is LISTED in `rust/fbtcore/build.rs` and hashed into `--build` beside the scripts: touch a listed file and every
consumer reads its tree once. What it derives - the closure (`rows/ts/`), the stylesheet tables (`cssmap/`), whether a
partial run keeps them (`rows/derive/`) - is keyed by the RUNNING BUILD instead, recorded in `_meta`
`rederive:typescript` after every derivation: when node's rows stand and another build derived them, the half does
not start node and derives everything again from the stored rows (`mapper/deep/derived.rs::derive_again`,
`rows/derive/tsderive.rs`), dropping the derived tables as a full run does - the closure's time, not the tree's. A
record it cannot derive under forgets the rows' setup, and node reads every file. **A file the list names may only
call a module it names or `NOT_SHAPING` names with why**: `node_rows::every_module_the_listed_code_names_is_classified`
resolves every path a listed file names and fails on one that is neither.

**ITS IDS RESTART WHEN IT IS THE ONLY HALF IN THE DATABASE** (`alone` in `rust/fbtcore/src/rows/half.rs`), so the
same tree always numbers the same way: continuing them renumbered everything, and `key_reach` sorts its gate lists
by id STRING, so the gate reported thousands of changed rows with no fact different. It is conditional because
nine prefixes are shared with the C# half (`f`, `c`, `x`, `fn`, `br`, `p`, `k`, `e`, `i`); the other halves
stamp `lang`, never `half`, so the per-language tally in `_meta` says who is present.

**IT PARSES IN FLIGHT** (`rust/fbtcore/src/mapper/deep/flight.rs`): node runs on a thread of its own while python,
rust and C# store, and its rows are stored after theirs - so it NUMBERS IN A LANE, `LANE` above every counter it was
handed (`floor` in `TsStore.mjs` for a prefix never recorded), and a lane the halves beside it crossed is parsed
again in its turn. Not flown when ALONE (the ids must restart) or on a REBUILD; then it runs first, as before.

**THE THREE OUTPUTS THAT ARE NOT TABLES ARE OPTIONS**, off by default because nothing in a build reads them:
`--ts-html <dir>` (one JSON per template, every node carrying the rows' `n:` id), `--map-row-fts` (`row_fts` +
`row_map`: which row anywhere mentions a word) and `--map-atlas <dir>` (`atlas.json` + `atlas.md`: projects,
NgModule areas, every route with what it reaches). The last two run over the finished DATABASE
(`rust/fbtcore/src/rows/fts.rs`, `rust/fbtcore/src/atlas.rs`); they are not a half. The atlas reports a route's
real `children` count (signed-off divergence #2: the predecessor read the number as a list and wrote 0) - a
document for a person, which nothing joins on.
