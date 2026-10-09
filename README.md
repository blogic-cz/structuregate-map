# structuregate-map

**A structure gate and a code map for repositories that AI agents work in.** One native executable, no
runtime beside it.

- **The gate** fails the build when the repository grows out of a shape an agent can read: a file over its
  line limit, a folder over its file limit, a context doc (`CLAUDE.md`, `.claude/rules`, skills) too long to
  load or linking to nothing. It runs where it cannot be skipped: inside `dotnet build`, as an npm
  `prebuild`, or as a Claude Code Stop hook.
- **The map** answers what the code *does*: what each file declares, calls, reads, imports and never
  reaches. One SQLite database across languages, so a question like "who reads this constant" or "which C#
  call runs this SQL" is a query, not a grep.

Every answer comes from a real parser - Roslyn, the TypeScript and Angular compilers, the PowerShell AST,
python's `ast`, `syn`, `gosyn`, Microsoft's T-SQL parser. **There is no regex in this tool, and its build
refuses one**: a pattern over source text is a second, worse parser that error-recovers into a wrong answer.

## What it reads

| Language | Gate | Map |
|---|---|---|
| C#, `.razor`, `.cshtml` | lines counted by Roslyn, `--async-discipline` | semantic rows from a real compilation: calls, refs, values folded past the compiler |
| T-SQL | lines | objects, columns, keys, refs - and `sql_links` from the C# that runs them (EF, Dapper) |
| TypeScript / Angular | lines, `--ts-discipline` | declarations, templates, render paths, what each translation key and gate depends on |
| Python | lines | defs, calls, reads, imports, dead code |
| PowerShell | parsed, 15 `--ps-discipline` rules | imports, calls |
| Rust, Go | lines | constants, error handlers, file graph |
| Markdown | doc limits, broken links | mentions of code, orphan and missing docs |

## Install

Download the archive for your platform from [Releases](https://github.com/blogic-cz/structuregate-map/releases)
(`win-x64` zip or `linux-x64` tar.gz), unpack it, and wire a repository in one command:

```powershell
pwsh -NoProfile -File scripts/Connect-Gate.ps1 -Path <your-repo> -DryRun   # what it would do
pwsh -NoProfile -File scripts/Connect-Gate.ps1 -Path <your-repo>           # do it
```

It links the gate into `<your-repo>/buildtools`, writes the entry point the repo needs (an MSBuild
`Import`, an npm `prebuild`, or a Claude Code Stop hook), checks the language hosts, links the skills, and
freezes a first baseline so the gate is green on arrival. Windows PowerShell 5.1 (`powershell`) runs it
too. Later releases are one command, for every connected repository at once:

```powershell
pwsh -NoProfile -File scripts/Update-Gate.ps1
```

### For an AI agent wiring a repository

Given only this page, an agent connects a repository like this:

1. Install PowerShell 7 (`pwsh`) if the machine lacks it. Download the newest release archive for the
   platform, unpack it, and run `scripts/Connect-Gate.ps1 -Path <repo> -DryRun`, then without `-DryRun`.
2. **Several `.csproj` files** stop the script with AMBIGUOUS: pass `-Project <file>` to import into one of
   them, or `-Entry hook` for a Claude Code Stop hook that touches no project file.
3. **TypeScript with no `package.json` at the root** (the workspace sits in a subfolder) reports
   `typescript: NO package.json here`: set `NODE_PATH` to that workspace's `node_modules`, in the hook
   too, and connect again - otherwise every `.ts` file is `UNMAPPED`.
4. **Red on arrival.** The size baseline freezes source files only. Two things it cannot freeze still fail:
   - docs over the limit: documents have no baseline, by design. `-GateArgs '--doc-scope context'` limits
     only what is loaded every session (`CLAUDE.md`, `AGENTS.md`, `.claude/`), or split the doc;
   - `BROKEN` imports, from `--map-check`: fix them, or keep dead or vendored folders out of every check
     with `--skip`. It REPLACES the default list, so restate it:
     `--skip out,bin,obj,dist,node_modules,.git,.vs,.venv,__pycache__,<folder>`.
     `--skip-file <glob>` is the gate only - the map keeps the file.

   `-GateArgs` is written into the wiring on the FIRST connect only - an existing wiring file is never
   rewritten, so after that the flags are edited in it (`$GateArgs` in the hook). Run the entry point once by hand (the hook is `buildtools/StructureGate.Hook.ps1`) and read its exit
   code before calling the repository connected.

### The Claude Code plugin

This repository is also a plugin, and its own marketplace. It carries the map skills and agents, and two hooks
that make a session in a connected repository use the map: a note at session start that the map exists and how to
ask it, and - on a `Grep` or `grep`/`rg` for a word spelled like a code symbol - the `--map-query` that answers it.
The search still runs; set `STRUCTUREGATE_HOOK=deny` to refuse it instead. `Connect-Gate.ps1` enables the plugin
in the repository's `.claude/settings.json`, so everyone who opens it is offered it. By hand:

```
/plugin marketplace add blogic-cz/structuregate-map
/plugin install structuregate@structuregate
```

## Quick start

```bash
structuregate --root .                                   # the gate: limits, docs, links
structuregate --root . --map                             # the file graph -> buildmap.json
structuregate --root . --ext .cs,.ts,.sql --map-sqlite map.sqlite   # the deep map
structuregate --map-query map.sqlite --find OrderService # who declares, calls and reads it
structuregate --map-query map.sqlite --sql "SELECT path FROM files WHERE lang = 'csharp'"
structuregate --map-view map.sqlite                      # one offline HTML page of the map
```

`structuregate --help` lists every flag. The skills in [skills/](skills/) and the agents in
[agents/](agents/) teach Claude Code to read the map instead of grepping.

## Build from source

Needs the .NET 10 SDK, a Rust toolchain, Node (for the TypeScript half) and PowerShell 7 off Windows.
`pwsh -NoProfile -File scripts/Install-Dependencies.ps1 -DryRun` says what a machine lacks.

```bash
dotnet build src/StructureGate.csproj                     # builds and runs every black-box test
dotnet build src/StructureGate.csproj -p:SkipTests=true   # without them
cargo test --manifest-path rust/fbtcore/Cargo.toml        # the rust unit tests
dotnet publish src/StructureGate.csproj -c Release -r linux-x64 -p:PublishAot=true   # or win-x64
```

The repository gates itself: `dotnet build` runs the gate over this tree before it compiles, through the
same `StructureGate.targets` a consumer imports. `structure-baseline.json` lists what was already over a
limit when a language was first measured, and may only shrink.

## Documentation

Everything lives beside what it describes: a skill for what a consumer runs, a `CLAUDE.md` for what a
maintainer edits, and `.claude/rules/` for a half's deep detail.

| where | what it covers |
|---|---|
| [skills/gate-connect/](skills/gate-connect/SKILL.md) | wiring a repository, and what each entry point does |
| [skills/buildmap/](skills/buildmap/SKILL.md) | `--map`: edges, findings, duplication, `--map-plugin`, outputs, `--map-view`, limits |
| [skills/map-sqlite/](skills/map-sqlite/SKILL.md) | `--map-sqlite` and `--map-query`, per language; `--facts-pull` and `doc_facts` |
| [skills/map-findings-issues/](skills/map-findings-issues/SKILL.md) | filing what a map gets wrong as a public issue, with nothing concrete in it |
| [rust/fbtcore/src/gate/CLAUDE.md](rust/fbtcore/src/gate/CLAUDE.md) | the disk walk, the limits, the modes, `--plugin`, the doc rules |
| [src/PsGate/CLAUDE.md](src/PsGate/CLAUDE.md) | `--ps-discipline`: the PowerShell rules |
| [src/TsGate/CLAUDE.md](src/TsGate/CLAUDE.md) | `--ts-discipline`: the TypeScript rules |
| [src/TsRows/CLAUDE.md](src/TsRows/CLAUDE.md) | the Angular half: the rows it writes |
| [rust/fbtcore/src/rows/ts/CLAUDE.md](rust/fbtcore/src/rows/ts/CLAUDE.md) | the Angular closure, gates and key reachability |
| [rust/fbtcore/src/trace/CLAUDE.md](rust/fbtcore/src/trace/CLAUDE.md) | `STRUCTUREGATE_TRACE`, `--trace-report`, OpenTelemetry |
| [tests/CLAUDE.md](tests/CLAUDE.md) | the black-box suites and what each one pins |

## License

MIT - see [LICENSE](LICENSE). The graph libraries vendored in `src/MapView/vendor/` keep their own licenses
([LICENSES.txt](src/MapView/vendor/LICENSES.txt)).
