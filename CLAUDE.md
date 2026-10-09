# structuregate

A NativeAOT exe shipped INTO other repositories. Two files land in a consumer —
`structuregate.exe` and `StructureGate.targets` — and everything else it needs is embedded in the exe.
Most rules below exist to keep that true.

This file is only what a session must hold before touching anything. There is no `docs/`: detail lives
beside what it describes. A folder's own `CLAUDE.md` (`rust/fbtcore/src/gate/`, `src/PsGate/`, `src/TsGate/`,
`src/TsRows/`, `rust/fbtcore/src/rows/ts/`, `rust/fbtcore/src/trace/`, `tests/`) loads when you work in it;
`.claude/rules/` holds each half's deep detail (`deep-csharp-map.md`, `typescript-half.md`, `rust-half.md`,
`sql-half.md`, `markdown-half.md`); what a CONSUMER reads is a skill (`skills/gate-connect/`, `skills/buildmap/`,
`skills/map-sqlite/`, `skills/map-findings-issues/`). New detail goes to one of those three places, never to a
new top-level doc. [README.md](README.md) is the public front page; `rust/fbt/README.md` is the change detector's.

## Commands

```bash
dotnet build src/StructureGate.csproj                      # builds AND runs every black-box test (minutes)
dotnet build src/StructureGate.csproj -p:SkipTests=true    # skip them
powershell -NoProfile -File tests\Run-Tests.ps1 -Only Map   # one suite
cargo test --manifest-path rust/fbtcore/Cargo.toml          # the rust unit tests (not run by the build)
powershell -NoProfile -File scripts\Connect-Gate.ps1 -Path <tree>  # wire a new consumer (-DryRun first)
dotnet publish src/StructureGate.csproj -c Release -r win-x64 -p:PublishAot=true
pwsh -NoProfile -File scripts/Install-Dependencies.ps1 -DryRun  # what this machine lacks to build (-DryRun off to install)
```

**Off Windows** the same commands run with `pwsh` for `powershell` and `-r linux-x64` for the publish; the exe
is then `structuregate`, without `.exe`. **Off Windows a publish deploys nothing**: every consumer on the machine,
this repo's `buildtools/` included, is a hard link to the GitHub release the updater
(`scripts/Update-Gate.ps1`) downloaded (its `consumers.txt` lists them), so a consumer changes only through a release.
Cases that need Windows itself (5.1's grammar, `cmd`, .NET Framework) are `Test-WindowsCase` and say so.

**IMPORTANT: `dotnet publish` DEPLOYS the exe to every consumer listed as a `GateConsumer` in
`src/StructureGate.csproj` or the gitignored `src/GateConsumers.local.props`, this repo included.** It writes ONE release, this repo's gitignored `buildtools\`,
and each consumer's two files are HARD LINKS to it (`scripts\Link-Gate.ps1`) - never a copy, never in a
consumer's git. Add `-p:SkipGateDeploy=true` to publish without touching them. Publishing also needs
`vswhere.exe` on PATH or the link step fails with exit 123:

```
$env:PATH = "C:\Program Files (x86)\Microsoft Visual Studio\Installer;$env:PATH"
```

## No regex, anywhere — the build refuses one

Every question this tool asks is answered from a parse tree: Roslyn for C#, the PowerShell AST, the
TypeScript compiler, python's `ast`, `syn` for rust, `gosyn` for Go. The `BanRegex` target fails the build on a pattern in
the gate's own sources. **Its globs are not recursive (rust excepted, searched with `/S`) — a new subfolder
has to be added to the target, or its files are unbanned and nothing says so. Every search is written twice — `findstr` for Windows, `grep -F` for elsewhere — and a folder goes into BOTH.**

The ban is a literal `findstr`, so a needle can match its own rationale comment — `import re` once matched
`import resolves` and failed the build over a docstring. Needles carry a leading space for that reason.

## No consumer's names — the build refuses one

This repo is public; every half was measured on PRIVATE trees. A fixture, comment or doc uses invented names
(`Demo.*`, `Order`, `Alpha`) and relative figures ("most rows", "a large workspace"), never a consumer's
identifiers, layout, domain words or counts. `LeakGuard` fails the build on any term of the gitignored
`leak-deny.txt` at the root - add a term there the moment one leaks, never to a committed file.

## NativeAOT constraints

- `JsonSerializer.Serialize` is unreachable (IL2026/IL3050). Use `Utf8JsonWriter`.
- No native dependency may be added. `rust/fbtcore` (the SQLite store, the change detector, the rust
  parser) is a static library linked INTO the exe, so the exe stays one file. Pure-rust crates compiled
  into it are fine; anything needing a DLL beside the exe is not.
- **No MSBuild, ever** — this runs INSIDE an MSBuild target. The semantic map builds its own
  `CSharpCompilation` from references found on disk (`rust/fbtcore/src/csproj/`, opened by `src/Map/CsRows/CsProject.cs`), never from a workspace.
- `IL2104` is suppressed in the csproj for Roslyn's binder only (see the comment there). `IL2026`/`IL3050`
  still fail the build for our own code — do not widen that NoWarn.
- Every MB of the exe reaches every consumer. Check the published size before adding a dependency.

## Adding a language half

A SCRIPT half is EMBEDDED in the exe and run in the host that owns its parser. Adding or renaming one means
two places:

1. its set in `rust/fbtcore/src/embedded/mod.rs` — `include_bytes!` compiles every script INTO fbtcore, so a
   name that is not there fails the BUILD (a resource list only failed at run time, as `ERR_MODULE_NOT_FOUND`)
2. its launch in rust — a `halves::Script` in `rust/fbtcore/src/mapper/mod.rs`, a deep one in `mapper/deep/` —
   hosts disagree about command lines, so the shape is data (`before`, `list_flag`, `root_flag`), not a branch.

**RUST OWNS THE PROCESS** (`cli/`, `gate/run.rs`, `mapper/`): the arguments, which question a run asks, the
walk, every count, the hosts, the joins and the output; C#'s `Program.cs` is one call into `fbt_main` and a
printer (.NET writes the console in the code page MSBuild reads). C# is called back only for what needs a .NET parser — a `.cs` file's
count and async rule (`RustGate`), its graph and its deep rows (`RustMapper`, `DeepMap`), and T-SQL's deep
rows (`SqlDeep`, `SqlLinks`); the deep map's order and every other deep half are `mapper/deep/`. An IN-PROCESS half (rust by `syn`, Go by `gosyn` - file map only, a name keyed by its package folder - markdown by `pulldown-cmark`) is called from `mapper/halves.rs`, and
every line counter but Roslyn's is `count/` — the gate and the map must agree on what a source line is.
Markdown's files are the `--doc-scope` set, not `--ext`, and its edges are MENTIONS, never imports.

**New work after the parse goes in rust.** A host parses its own language and emits rows; the joins,
graph, findings, storage and queries over those rows belong in `fbtcore`, and a language with no host of
its own (markdown) is parsed there too.

A half that answers per FILE gets `per_file: true` and every file it was given is checked against its
output; one that answers in TABLES sets `per_file: false` and is checked against its DONE count.

The DEEP map (`--map-sqlite`) is its halves into ONE database, every one of them STORED by `rust/fbtcore`
inside this process. Every `files` row carries its `lang`, and what is recorded, what has gone and what
disagrees are all scoped by it; unscoped, the half that runs second deletes the rows of the half that ran
first. `--map-sqlite` WITHOUT `--map` is the deep map alone — what a per-turn hook runs — and exits 1 on a
half that failed, since there is no `--map-check` there to ask. Every other map flag still needs `--map`.

## Structure of this repo

**This repo GATES ITSELF**: `dotnet build` runs the gate before CoreCompile through the same
`StructureGate.targets` a consumer imports, over `.cs .mjs .ps1 .py .rs`, so a limit crossed here fails the
build (`-p:SkipStructureGate=true` opts out). `structure-baseline.json` holds what was already over when a
language was first measured, and may only shrink. The build runs the DEPLOYED exe in `buildtools/`, not the
one it just built — a rule change reaches this repo's own gate only after a publish.

A folder stops at 14 files and a file at 450 source lines. When one fills, split by topic into a subfolder
(`src/PsGate/`, `src/Map/CsRows/`, `src/Map/Fbt/`, `src/TsRows/TsDecls/` all exist for that reason). An
imported python module has to be staged with whatever launches it — see the script lists in
`RustMapper.cs`.

`tests/` runs `Run-Tests.ps1`, which globs `tests\**\*.Tests.ps1` RECURSIVELY, sorted by file name and not
by path — the sort decides which suite's helpers overwrite which, so moving a suite into a folder must not
move it in that order. `scripts/` holds the consumer wiring (`Connect-Gate.ps1` + its helpers); the release ships them in its own
`scripts/`, where Connect-Gate.ps1 registers in `consumers.txt` instead of `GateConsumers.local.props`.

Every build output goes to `out\` (gitignored): the dll the tests run is `out\structuregate.dll`, the native
exe a publish deploys is `out\win-x64\publish\structuregate.exe` (`out/linux-x64/publish/structuregate` on linux). Nothing is built into the repo root.

The repo is also a Claude Code PLUGIN and its marketplace (`.claude-plugin/`): `skills/` and `agents/` as they are, plus
`hooks/hooks.json`, which runs the tree's own `buildtools/structuregate --claude-hook` (`rust/fbtcore/src/cli/hook.rs`).

`skills/` is the ORIGIN of each skill. A consumer holds a JUNCTION to it, never a copy — see the README
section on wiring. Junctions are not walked by the gate, so a junctioned skill is never counted against a
consumer's limits. `agents/` is the same for the map agents (`python-map-audit`, `python-map-reader`, `angular-map-audit`, `dry-guard`); an
agent is one FILE, so a consumer holds a HARD LINK to it in its `.claude/agents/`.

## Tests

Black-box over the built CLI, because every consumer calls the CLI. `tests/fixtures/expected.txt` is
pinned line-for-line; `Run-PsGateTests.ps1 -Update` rewrites it.

**YOU MUST prove a new case fails when the rule it covers is broken.** Write the case, break the rule,
watch it fail, restore. Three cases written in one session passed while proving nothing — the sharpest was
a "database is rebuilt, never appended" case that compared row counts, when the mutation makes
`CREATE TABLE` throw and leaves the counts identical. Restoring a file from a backup keeps its OLD
timestamp, and MSBuild then skips the rebuild — touch it, or the "restored" run tests the mutant.

Suites are dot-sourced into ONE scope in name order, so a helper named like the harness's own silently
replaces it for every suite that sorts later — and a suite must not lean on another suite's helper, or
`-Only` breaks it.

## Commits

Conventional Commits, lowercase subject, and the body says WHY rather than what — read `git log` for the
register. Commit only when asked.
