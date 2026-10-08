# The deep map, how it is built and refreshed

The JSON map ([SKILL.md](SKILL.md)) is about FILES and carries no code - a copy of the source in a JSON file
drifts. Questions about EXPRESSIONS (where is a scope id built, which calls pass a flag, what reads this
constant) need `--map-sqlite`; how to QUERY it is the `map-sqlite` skill. This page is how it is built.

```
structuregate --root . --ext .py --map --map-out map.json --map-sqlite map.sqlite
structuregate --root . --map-sqlite map.sqlite        # the deep map alone, no JSON map
```

Python, C#, TypeScript (with or without Angular), T-SQL and rust land in ONE database, every `files` row
tagged with its `lang`. Scale: a few hundred python files is over a hundred thousand rows in seconds; a
solution of thousands of C# files is millions of rows and minutes, with nearly every call carrying the
symbol Roslyn bound it to. `--map-sqlite` without `--map` skips the file-level JSON entirely, and exits 1 on
a half that failed (there is no `--map-check` to ask).

**The facts are extracted, the text is carried.** Every row holds its own `source` AND what was read off it
(`reads`, `calls`, `strings`), so a query never parses the text again. Pattern-matching over `source`
re-implements, badly, what the pass already resolved.

## `--map-exclude <glob>` - what a codebase keeps out

A generated C# file (marked, or `.Designer.cs`/`.g.cs`) is listed and never walked by default. Hand-written
code the map cannot tell apart - EF seed migrations above all, whose `InsertData` bodies can be most of a
project's rows and binding time - is the consumer's call:

```
structuregate --root . --ext .cs --map-sqlite map.sqlite --map-exclude "**/Migrations/*.cs"
```

A match is still COMPILED, so every other file binds against it; it gets a `files` row with `excluded = 1`
and no rows of its own. Repeatable; `**` crosses folders, `*` and `?` stay in one, case is ignored. The
exclusion is folded into the file's sha, so changing a pattern re-reads exactly the files it matches.

## What a refresh says about itself

Every run prints its steps - `the deep map took 60.0 s: hash the tree 0.2 s, typescript 20.0 s, csharp
35.0 s, sql 1.8 s, passes over the finished rows 3.0 s` - and each half that worked says why. C# counts its
re-reads by CAUSE and names the projects that forced them:

```
the deep C# half re-reads 120 file(s): 20 whose content moved, 100 whose project's inputs moved - e.g. …
the C# files re-read because their project's inputs moved, by project: src/Api/Api.csproj 100 (obj/project.assets.json), …
```

A C# file's sha folds its content, its project's fingerprint and the rows version. The fingerprint's parts
(the `.csproj`, `obj/project.assets.json`, each `Directory.*.props`, what a generator emitted) are kept in
`_meta` as `fingerprints:csharp`; a moved sha refolded from content-now and project-then tells "project
moved" from "content moved". Other causes: `not recorded before`, `written by an older C# extractor`, a
`--map-exclude` match that moved. The Angular half says how many files moved, by extension, and whether its
setup (compiler, node, config, rows version) moved.

`_meta` key `last_refresh` keeps it as JSON, replaced by every run that did work: `when`, `took_ms`,
`steps_ms`, `passes` (`ran`/`replayed`), `reread`, `csharp` (`causes`, `projects`), `typescript`, `trace`.
The full trace is `<db>.last-run.jsonl` beside the database; read it with `structuregate --trace-report <file>`.
Neither key feeds the passes over the finished rows, so a run that changed nothing still replays them.

## Only what moved is parsed again

**The file map** (`rust/fbtcore/src/mapper/kept.rs`): the PowerShell, TypeScript and C# halves keep each
file's answer in `.fbt/gate.sqlite` under its content hash, the build, the host, the script and the whole
file set (TypeScript also its `tsconfig`/`package.json` files); a half with nothing to parse is not started.
Python's answer about a file reads the modules it imports, so it is kept WHOLE, replayed while no `.py`,
`pyproject.toml` or `.spec` moved. A TypeScript half that failed for want of a compiler is kept too, until a
`node_modules/typescript` it would resolve appears or changes. The result is byte-identical to a full parse.

**The deep map**: a file whose sha has not moved is not re-read, and the invariant is CHECKED - a row whose
file disagrees with the tree throws the cache away and rebuilds once. The python and plain TypeScript halves
are keyed by what they read (their files, project files, the file set), never by the whole tree; the Angular
half runs whole when any `.ts`/`.html` under its workspace moved.

## TypeScript without Angular - the plain half

The Angular half needs `angular.json` (or `nx.json`) and `@angular/compiler`, and stops with `MAP-SKIP`
elsewhere. A Solid, React, node or Apps Script tree is mapped by the PLAIN half (`src/TsRows/TsPlain/`,
driven by `rust/fbtcore/src/mapper/deep/`): the tree's own `typescript` 5 parser, no program, no checker.
Its rows carry `lang = 'ts'` - never `typescript`, which is Angular's - and it runs only where no workspace
was found; when one appears, or the last script file goes, its rows are dropped.

Shaped like the python half: syntactic rows replaced per FILE, an unmoved file not re-read, an importer
re-read when what it imports appears, moves or goes (`deps:ts` in `_meta`). Tables are python's where the
meaning is the same, plus `types`, `jsx` and `regexes`. A call binds (`target_path`, `target_name`) through
the file's own imports and top-level declarations, never when an enclosing function declares the name. No
`typescript` 5 to borrow is a NOTE, not an error (a `.mjs` in a C# repo is not a TypeScript project); 7.x's
native package is not read.

**One fingerprint with the file map.** `functions.body_shape` and `expressions.shape` are the digests
`src/TsGate/TsGate.Map.mjs` groups `duplicate_bodies` / `duplicate_expressions` by, from the same function
over the same nodes. The JSON keeps the widest shape per set of sites; group by site set in SQL and the two
agree. The `dry-guard` agent (`agents/dry-guard.md`) is the reader built on these rows.
