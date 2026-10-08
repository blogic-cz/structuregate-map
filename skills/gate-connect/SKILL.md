---
name: gate-connect
description: "Wire a project into structuregate in one command - register it, deploy the two files, write the entry point that RUNS the gate (MSBuild import, npm prebuild, or a Claude Code Stop hook), install the language host the map needs, junction the skills, and freeze the first map's baseline and the files already over a limit so the tree starts green. Use it when a new repository should be gated, when someone asks to 'connect structuregate', 'add the gate to this project', 'gate this repo', or after a gate rebuild that has to reach a tree that was wired by hand. Also use it to CHECK a wiring: -DryRun prints what it would do and writes nothing, which is how to see whether a tree is connected and which entry point it uses."
allowed-tools: Bash(powershell:*), Bash(dotnet:*), Read, Grep, Glob
argument-hint: "<path to the project>, plus -Entry / -Ext / -GateArgs when the default guess is wrong"
---

One command per tree, repeatable. The script ships in the GitHub release, so no clone is needed: it runs
from the unpacked release the updater (`scripts/Update-Gate.ps1`) keeps up to date.

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File "$env:LOCALAPPDATA\structuregate\release\scripts\Connect-Gate.ps1" -Path <tree>
```

Off Windows: `pwsh -NoProfile -File ~/.local/share/structuregate/release/scripts/Connect-Gate.ps1 -Path <tree>`.
From the release it registers the tree in `consumers.txt` beside the release - the list `Update-Gate.ps1`
reads; from a clone (`scripts/Connect-Gate.ps1`) it registers a `GateConsumer` in the gitignored
`src/GateConsumers.local.props`, which `src/StructureGate.csproj` imports.

`-DryRun` first if the tree is not yours: it prints the six steps and writes nothing. What each wiring
looks like by hand, and why the skills are junctions: [wiring.md](wiring.md).

## The six steps, and why none is optional

| step | why |
|---|---|
| register the tree | that list is the ONLY thing a deploy reads, so an unregistered tree keeps a stale exe forever |
| HARD-LINK `structuregate.exe` + `StructureGate.targets` into `<tree>/buildtools` | one release on the machine, never a copy, nothing in the tree's git. Across drives a hard link cannot exist, so it is copied and the line says so: that tree then needs a re-run per release |
| write the entry point | everything above only puts an exe on disk; this is what runs it without being asked |
| probe the hosts | a missing host is not a skipped check - every file that half owned is reported `UNMAPPED` |
| junction the skills into `<tree>/.claude/skills` | a junction is not walked, so the skill costs the consumer no file budget |
| first map + `--update-map-baseline`, and `structure-baseline.json` for files already over a limit (written once, never re-frozen) | a gate that is red on arrival gets switched off |

In a git tree the gate measures what git sees (`--tracked --include-untracked`): a build output or a vendored
copy `.gitignore` keeps out is not the tree's code.

Each step prints its verb - `added` / `present` / `copied` / `current` / `linked` / `relinked` - and an
existing wiring file is never overwritten. A skill folder that is a REAL folder, not a junction, is left
alone and reported. **Re-run it after every gate rebuild.**

## The entry point is guessed, and the guess is printed

* one `.csproj` → an `Import` of `StructureGate.targets`, run before `CoreCompile`
* `package.json` → `scripts/checkStructure.mjs` plus a `prebuild` entry; npm runs prebuild before build by
  itself, so no command the project already owns is rewritten
* neither → a Claude Code Stop hook (`buildtools/StructureGate.Hook.ps1` in `.claude/settings.json`), because
  the turn is then the only thing that runs

Two or more `.csproj` files is reported as AMBIGUOUS rather than guessed: pass `-Project <file>` or
`-Entry msbuild|npm|hook|none`.

## What to check after it runs

**A `WARNING: n file(s) UNMAPPED` line means the map is not usable yet.** A half had no host, so those files
were never read - and the baseline just recorded "nothing imports this" for every one of them. Fix the
`[host]` line and re-run. Without a resolvable `typescript`, a tree once came back nearly all UNMAPPED under
a confident 0-edge summary.

`typescript@^5` is installed INTO the tree when the `.ts`/`.js` half needs one, because the compiler that may
judge a repo is the one the repo compiles with - and 5.x, not the newest, because the deep map's plain
TypeScript half reads only 5.x's in-process parser. A tree that already declares a version keeps it (`npm i`
restores the pin). A tree that borrows `typescript` from elsewhere has none, as far as the gate is concerned.

## Flags worth knowing

| flag | when |
|---|---|
| `-Ext .py,.ts` | override what the tree scan found |
| `-GateArgs '--doc-scope context --async-discipline'` | extra gate flags, recorded in the wiring itself |
| `-Entry none` | the owner calls the gate from their own script |
| `-Project <file>` | the `.csproj` to import into when there are several |
| `-GateDir <dir>` | where the two files land (default `buildtools`) |
| `-SkipMap` / `-SkipSkill` / `-SkipPrereq` / `-SkipRegister` | one step off, for a re-run |
| `-Registry <file>` | a different consumer list (the tests use this) |
| `-TypeScriptSpec <spec>` | what step 4 installs (default `typescript@^5`) |
