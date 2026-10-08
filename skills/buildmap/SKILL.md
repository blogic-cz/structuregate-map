---
name: buildmap
description: "Ask the tree about itself, through the JSON map structuregate writes to buildmap.json — what imports what, what breaks if a file moves, what can ship apart, what nothing imports at all, what is imported only optionally, what is written twice, which imports are built at run time. Built by `structuregate --map` from a parse tree per language (python, TypeScript/JavaScript, PowerShell, C#, rust, Go, and markdown for which files the docs point at), rebuilt on every run by a tree's Stop hook or npm launcher, and gated by `--map-check`. REACH FOR THIS INSTEAD OF A GREP for any of those questions: a grep is cut off by whatever limit was passed, cannot tell an import from a mention in a docstring, and says nothing about an import resolved through a package. For a question about an EXPRESSION rather than a file — which functions read a constant, where a phrase appears in the code — use `map-sqlite` instead; this map deliberately carries no source text."
allowed-tools: Bash(structuregate.exe:*), Bash(python:*), Read, Grep, Glob
argument-hint: "[a file, a module name, or a section of the map]"
---

# Ask the tree about itself

**The origin of this skill is `skills\buildmap` in the `structuregate-map` repository**, beside the tool that
produces the map; the copy here is a junction to it.

**A tree whose map carries `artifacts` or `steps` has a `--map-plugin`** that knows its build. The plugin's
own sections, findings and paths are that tree's business — when a `<consumer>.md` beside this skill names
the tree, read it before answering.

Deeper pages beside this one: [edges.md](edges.md) (what an edge is per language, findings, duplication,
`--map-plugin`), [outputs.md](outputs.md) (how the deep map is built and refreshed, `--map-exclude`, the
plain TypeScript half), [limits.md](limits.md) (the ratchet, the Stop-hook cost, the blind spots),
[view.md](view.md) (`--map-view`, the map as one offline HTML page).

## Where it is, and who builds it

`scripts\Connect-Gate.ps1` builds the first map when it connects a tree, and the npm `prebuild` launcher
and the Claude Code Stop hook it writes rebuild it on every run. The MSBuild target does NOT — an
MSBuild-wired tree's map is as old as its last connect or hand-run, so check its date before quoting it.
All of them write to the same place:

```
<tree>\buildmap.json       the graph (this skill)
<tree>\buildmap.sqlite     the deep map, where a half can write rows (map-sqlite)
<tree>\map-baseline.json   the ratchet — tracked, unlike the two caches
```

A tree wired by hand may pass another `--map-out`; the entry point's command line is the truth. Run the
entry point rather than a second spelling of its command — `--map-if-stale` compares the source, the file
set and the gate's own mtime against `--map-out` alone, so a map built elsewhere goes stale beside the one
the entry point keeps fresh, and nothing reports the gap. A newly published gate re-parses an unchanged
tree once, and says so (`the gate is newer than the map`).

```powershell
powershell -NoProfile -File buildtools\StructureGate.Hook.ps1   # a hook-wired tree: map + gate, one exit code
node scripts\checkStructure.mjs                                 # an npm-wired tree (its prebuild)
```

An MSBuild-wired tree has no such command. Run the map by hand with the same `--ext`/`--skip` flags its
`StructureGate.targets` import passes — NOT by re-running `Connect-Gate.ps1`, which passes
`--update-map-baseline` and would record every new blind spot instead of failing on it:

```powershell
buildtools\structuregate.exe --root . <the tree's flags> --map --map-check --map-out buildmap.json --map-baseline map-baseline.json
```

## What it holds

| section | answers |
|---|---|
| `files` | per file: language, measured lines, its docstring headline (`summary`), what it `declares`, whether it is an `entry` point (a `__main__` guard, a bare `main()`, or a file a LAUNCHER names: `[project.scripts]` in `pyproject.toml`, `setup(entry_points=...)`, `Analysis([...])` in a `.spec`), `registered` — the decorators that hand its defs to an object (`@app.route`), or the convention a C# framework activates a class by (a `*Controller`, `[ApiController]`, a `PageModel` or `Hub` base, a test class or test method of MSTest, NUnit or xUnit), which is why it has no importer — and `generated`, a file that contributes no duplicate group on purpose |
| `imports` / `imported_by` | file -> file, resolved. Function-level imports COUNT |
| `soft_imports` / `imported_softly_by` | the same for imports under an `ImportError` handler — an OPTIONAL module, not a dependency. ALSO carries name-literal edges: a string that exactly spells a module the tree declares AND sits where a string is a KEY — a `getattr`/`__import__` argument, or a module-level registry (`STEPS = ['worker']`), never a dict LOOKUP key (`_S['server']`, `.get('worker')`) — and a string that names a FILE to launch (an entry point `pkg.cli:main_x`, only when that module binds `main_x`; a script path `Analysis(['app.py'])`), never from a docstring. Evidence rather than proof, so never a hard edge |
| `mentions` / `mentioned_by` | doc -> the files it points a reader at: a code span, a link, a word of a shell block, resolved under its `cd`, beside the doc, then at the root. **`mentioned_by` IS "WHICH DOCS GO STALE IF I MOVE THIS".** Never an import: a doc naming a dead file leaves it `NO READER` |
| `external` | the third-party packages each file names |
| `ambiguous` | a name declared in more than one file, which therefore gets NO edge to any of them. A python name with exactly one candidate in the importer's OWN folder is not here: it binds to that sibling — and so does `from util import a, b` when exactly one candidate binds both names. A candidate listed here is never reported `NO READER` — something names it |
| `cycles` | import cycles, as the loop |
| `computed_imports` | every place a target is built at RUN TIME — `importlib.import_module(name)`, `import(expr)`, `. $lib`, `Type.GetType(name)` |
| `duplicate_bodies` / `duplicate_expressions` | identical function bodies, and repeated expressions across files, with local names blanked. Size is in source CHARACTERS |
| `similar_bodies` | the NEAR copies the digest cannot see: pairs of bodies (python only today) sharing most of their statement shapes - names AND literals blanked, one shape per statement - but not all. `score` is shared/total in percent (one literal changed is 100, one line added to four is 80), `shared`/`total` the counts, `at` the two places. Below 75, or fewer than 3 shared, is not a pair; an identical pair is in `duplicate_bodies` instead |
| `halves` | which language halves and plugins ran — a half missing here mapped nothing, so its files' edges are absent, not empty |
| `findings` | everything the run concluded, each line carrying its own severity |
| `artifacts` / `steps` / `produced_by` / `read_by` | EMPTY unless a `--map-plugin` fills them — see the top of this skill |

## The rules that stop a wrong answer

**ASK `soft_imports` BEFORE ANSWERING "CAN THESE TWO SHIP APART".** A guarded import is a degradation the
file already handles, and counting it as a dependency gives an answer that is confidently wrong.
Independence is a question about HARD edges only.

**`imported_by` IS THE "WHAT BREAKS IF I MOVE THIS" QUERY.** That claim, made by grep, was wrong twice in
one session.

**AN AMBIGUOUS NAME IS NOT A COLLISION.** `ambiguous` says a name is declared in more than one mapped file;
most such pairs are never on one `sys.path` together. Do not quote one as a build break. With `--map-sqlite`, a C#
name is joined to the file the compiler bound it to, so a C# `ambiguous` entry means no bound file chose.

**`duplicate_expressions` BLANKS LOCAL NAMES, and without that it finds nothing worth finding.** So a group
is one SHAPE — read the sites before folding them — and a deliberate boilerplate (a bootstrap pasted into
every script) is reported like any other repeat, because it genuinely is one.

**A `NO READER` FINDING IS NOT PROOF NOTHING READS IT.** Two readers are invisible: something outside the
mapped roots (uvicorn starting `app.py`, a shell script, a CI step) and something naming its target at run
time. The finding names the `computed_imports` count for that reason.

**A folder the entry point `--skip`s is not in the map.** A test importing a module is not counted as a
consumer of it — deliberately, or a fixture would hide a dead file — so an absent edge from `tests/` is not
evidence. Check the command line for `--skip` before saying "nothing uses this".

**MULTI-ROOT TREES NEED ONE `--root` PER IMPORT ROOT.** A tree that puts several folders on `sys.path`
resolves `from core import x` inside each; one `--root` above them looks for `<root>/core/`, finds nothing,
and every module under them reads as unimported. A map full of `NO READER` notes is usually this.

## The findings, and which can fail a run

Only a state that cannot be legitimate fails `--map-check`:

| finding | fails? | |
|---|---|---|
| `UNPARSED` | **yes** | the parser error-recovers into a PARTIAL tree, so every edge from that file is a guess |
| `BROKEN` | **yes** | a relative import naming neither a mapped file nor anything on disk |
| `PLUGIN` / `HALF` | **yes** | a producer named on the command line that did not run, or died half way |
| `DYNAMIC` / `UNREAD` | **yes** | a ratchet entry that is new, grew, or should have left the list |
| `NO READER`, `AMBIGUOUS`, `CYCLE`, `DUPLICATE`, `SIMILAR`, `COPIED`, `UNMAPPED` | no | each can be a legitimate state, and a gate that fails on them gets switched off |
| `DOC-MISSING` | no | a CONTEXT doc (CLAUDE.md, `.claude/**`) names a path with a folder, or links a file, that is nowhere — gone, moved, or a claim about another tree |
| `DOC-ORPHAN` | no | a doc no CLAUDE.md, AGENTS.md or `.claude/` doc leads to by a link, a path in code or its folder (named with the trailing `/`) — name it from a doc that is reached, or delete it |

A plugin adds its own; the tree's plugin note says which of those fail.

## The ratchet — `map-baseline.json`

Two lists, both of which may only SHRINK: a new entry fails, a recorded one may not grow, and one that is
GONE must be removed.

* **`computed`** — imports whose target is built at run time, per file and per shape (never per line).
  Each one is a blind spot every dead-file finding is qualified by; resolve one and it becomes a proven edge.
* **`unread`** — files nothing in the tree imports: dead, or reached in a way no parse tree can see.

Record a new one deliberately with `--update-map-baseline`, and review the diff — the list is supposed to
get shorter.

## It is a CACHE, derived one way

The source tree in, one JSON out. Nothing at run time opens it, so a stale copy can only give a wrong
ANSWER. Rebuild before quoting a number, and never hand-edit it: every field is derived from the tree's
parse or from a plugin's declaration, and a hand-written entry is one more copy of something already
declared — the drift this exists to expose.
