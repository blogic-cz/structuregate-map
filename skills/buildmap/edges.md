# What an edge is, what a finding is

How `--map` draws the graph in [SKILL.md](SKILL.md): per language, what it refuses to guess, what may
fail a build, what duplication means, and the plugin for rows no parser can derive.

```
structuregate --root . --map                          # -> ./buildmap.json
structuregate --root . --map --map-out build/map.json --map-check
structuregate --root . --ext .py --map --map-if-stale --map-check
```

Same walk and file set as the gate: `--root`, `--ext`, `--skip`, `--tracked` and `--include-untracked` all
apply, so `--ext .py` is what puts python in the map. `--map-if-stale` parses nothing when the map is newer
than every source AND the file set is the one in its head AND the exe is older than the map
(`rust/fbtcore/src/mapper/stale.rs`) - a publish is a changed rule set over the same sources. **The map is
derived, never written**: a hand-written map is a second copy of the source that drifts.

## An edge is a name one file declares, matched against a name another file uses

| language | read by | declares | an edge is |
|---|---|---|---|
| C# | Roslyn, in this process | every type and delegate | a **type reference**: an identifier exactly matching a type another file declares, plus the literal in `Type.GetType("…")` / `Assembly.Load` / `Activator.CreateInstance` (namespace, assembly and generic arity stripped) |
| TypeScript / JavaScript | the repo's own compiler, under `--ts-host` | its own path | a **resolved specifier**: `./util`, `./b.js` → `b.ts`, `./x` → `x/index.ts`, a `paths` alias (`~/util` → `src/util.ts`) |
| python | the `ast` of `--py-host` | its module name (`__init__.py` declares its package) | a **package path** in a package layout, a **module name** in a flat `sys.path` tree |
| PowerShell | `[Parser]::ParseFile` in `--ps-host` (5.1) | its path **and** every function it defines | a dot-source / `Import-Module` / `using module` naming a `.ps1`/`.psm1`/`.psd1`, **or** a call to a function another file defines |
| rust | `syn`, in the exe | every item another file can NAME (`pub`/`pub(crate)` items, every `macro_rules!`) | a **`mod name;`** resolved as rustc would (`name.rs`, `name/mod.rs`, `#[path]`; naming no file is `BROKEN`), or a path segment naming another file's item. A method after `.` is a member. A crate root (`lib.rs`, `main.rs`, `build.rs`, `src/bin/*`, `tests/*`) is an entry point |

**TypeScript `paths`** are resolved through the whole `extends` chain - a package base (`@base/cfg`, asked of
node's resolver) and an array of them (read in reverse; a later entry wins) - and through every `references`
project, each with ITS OWN `baseUrl`. The tsconfig is read as JSONC (`tsc --init` writes comments and
trailing commas). A `baseUrl` resolves against the file that wrote it, as TypeScript does.

**PowerShell needs both halves.** A dot-sourced file's functions are GLOBAL, so a caller usually names a
function, not a file: dot-sources alone draw a star out of the entry script and call every library dead.
A command no file defines is a cmdlet and is not reported. `& tool.exe` is a process launch, not an import -
only a PowerShell file counts.

**C# is not read off `using`.** A `using` names a namespace, usually a whole project: the graph would say
everything imports everything, or (same-namespace access needs no `using`) nothing imports anything.

**An OPTIONAL import is kept apart.** A python `import` under `except ImportError` lands in `soft_imports` /
`imported_softly_by`. A module imported both ways is HARD; a file only an optional import reads is not dead.

**A name built at RUN TIME is counted, never guessed.** `. $lib`, `importlib.import_module(name)`,
`import(expr)`, `Type.GetType(name)` go into `computed_imports`, and the count qualifies every dead-file
finding. `typeof(X)` / `nameof(X)` are already references, so they are neither. A computed path that still
SPELLS the file - `. (Join-Path $AppRoot 'lib/Thing.ps1')` - is resolved when its tail names exactly ONE
mapped file; two matches draw nothing.

**What it refuses to guess.** A name declared in two files draws NO edge and is `AMBIGUOUS`: which one a use
binds to is scope and compilation order, and a guessed edge cannot be told from a proven one. The `Foo` in
`x.Foo` is a member; a `using` segment is a namespace part. Both are pinned by cases.

## Findings, and which may fail a build

`--map-check` prints every finding and exits 1 only on those that **cannot be a legitimate state** (the
full table, with the ratchet's `DYNAMIC`/`UNREAD` and `PLUGIN`, is in [SKILL.md](SKILL.md)):

* `UNPARSED` - the parser error-recovered into a PARTIAL tree, so every edge from the file is a guess.
* `BROKEN` - a relative specifier naming neither a mapped file nor anything on disk.
* `HALF` - a language half that started and died part way, so its map is truncated.

Notes only: `UNMAPPED` (no host - no `python` on PATH, no `typescript` in the tree - written against EVERY
file it would have covered), `NO READER` (a TS/JS file with no `import` and no `export` is not a module and
counts as an entry point instead), `DOC-MISSING`, `DOC-ORPHAN` (none when the tree has no CLAUDE.md or
AGENTS.md), `AMBIGUOUS`, `CYCLE`, `DUPLICATE`, `SIMILAR`, `COPIED`. A missing tool is a note on purpose: a
Stop hook red forever for want of `typescript` is a hook that gets removed.

**Docs above several code roots:** `--doc-root <dir>` maps every `.md` under `<dir>` once, keyed from there,
and the code roots map none; a path a doc names inside a code root is keyed as that root keys it, so the
mention joins the code half's file. `--doc-skip <glob>` keeps a `.md` that is DATA out of map and gate.

## Duplication, in two units, and the near copy

`DUPLICATE` groups whole function bodies; `COPIED` groups expressions repeated across files - the idiom
pasted INSIDE larger functions, which a body fingerprint cannot see. Each half fingerprints its parse tree
with **local names blanked, member names kept** (`x.Normalize` makes the expression; whether its input is
`s` or `word` does not), comments never enter. The threshold is in each parser's own unit (C# tokens,
python AST nodes, PowerShell tokens, TypeScript leaves); the REPORTED size is source characters, so all
groups rank in one list. C#, python, PowerShell and TypeScript report both; rust reports `DUPLICATE` only.

`SIMILAR` is the PAIR the digest misses - a pasted function whose one edit was a limit, a message or a line.
Python sends each body as per-statement shapes (names and literals blanked) in a seventh `MAP-BODY` field;
`rust/fbtcore/src/graph/shape.rs` pairs bodies sharing at least 3 shapes and 75 % of their union, through an
inverted index. An equal-digest pair is a `DUPLICATE` instead. Pairs, never groups: a group would join two
bodies that share nothing through a third.

## `--map-plugin` - the rows no parser can derive

Every producer here is a PARSER, deliberately: a tool that IMPORTS the tree it checks can hang on a
database call, need env vars, or run whatever a module does at import time. But some true rows about a
build - an artifact path computed from settings at run time - do not exist until its code runs. So **this
tool parses; the project's own script may run**:

```
structuregate --root src --ext .py --map --map-check --map-plugin "python tools/map_rows.py"
```

The script prints the `MAP-*` protocol every embedded half speaks, into the same JSON, findings and exit:

| record | states |
|---|---|
| `MAP-ARTIFACT\|name\|field\|value` | one declared output, **one field per line** |
| `MAP-STEP\|name\|field\|value` | one build step; the ORDER is arrival order |
| `MAP-PRODUCES\|artifact\|script` / `MAP-READS\|artifact\|script` | who writes it, who reads it |
| `MAP-FINDING\|error\|text` / `MAP-FINDING\|note\|text` | the project's own finding, with **its own severity** |

**A field name is not an enum** - a project can declare a column this tool never heard of and it lands in
the JSON unchanged. The plugin gets `STRUCTUREGATE_MAP_ROOT` and `STRUCTUREGATE_MAP_LIST` (every mapped file)
in its ENVIRONMENT, so its command line stays the caller's and it agrees about the file set. **A plugin that
cannot launch FAILS**: it was named on the command line, and a map silently missing its rows is worse than
none. Checked against the hand-written map script it replaced: every artifact, step and reader, 0 lost.
