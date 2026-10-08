# tests: the black-box suites

```
tests/Run-Tests.ps1             every suite, one exit code   (-Only <prefix> runs suites by file name)
tests/Run-Tests.ps1 -WindowsOnly   only the Test-WindowsCase cases (what CI on Windows asks)
tests/Run-PsGateTests.ps1       the PowerShell rules alone   (-Update rewrites fixtures/expected.txt - then DIFF it)
tests/fixtures/                 violations.ps1 (every rule broken), clean.ps1 (none), expected.txt
```

`dotnet build` runs them through `RunGateTests` (`AfterTargets="Build"`, so they test the binary that build
just produced — before it they would test the previous one). Under test is `out/structuregate.dll`, or the
published exe when no dll exists; `PSGATE_TEST_GATE` points at any other binary (a deployed copy). Windows
runs them under `powershell.exe` 5.1, elsewhere `pwsh`. Most of the minutes are the TypeScript halves, which
install a compiler into a temp prefix and map a real workspace.

**Black box on purpose**: consumers call the CLI from MSBuild, a Stop hook, a node launcher — so what is
asserted is which files are measured, what they count as, what is printed and what is exited with. A unit
test around an internal method passes while all of that regresses. (The rust unit tests are separate:
`cargo test`, not run by the build.)

## The harness (`Assert.ps1`)

- `Test-Case <name> { … }` — one case; a throw is a failure. `Test-WindowsCase` is a case whose subject exists
  only on Windows (5.1's grammar, `cmd`, .NET Framework); elsewhere it prints `(Windows only, not run here)`,
  never a pass.
- `New-Tree @{ path = content }` builds a temp tree and registers it; `Register-Tree` adds a folder made beside
  one. A passing case deletes its trees, a FAILING one keeps them and prints their paths.
- `Invoke-Gate`, `Invoke-GateRaw`, `Get-Dump` / `Get-Count` (a NUMBER out of `--dump`), `Assert-Exit`,
  `Assert-Line`, `Assert-NoLine`, `Assert-Equal`; the TypeScript compiler helpers (`Get-TsModules`, …).
- A suite that throws OUTSIDE a case is one failure, and the suites after it still run.

**Suites are found RECURSIVELY and sorted by FILE NAME, not path**, then dot-sourced into one scope. A suite
can move into a subfolder without changing the order; a suite helper named like a harness function replaces
it for every later suite; a suite must never lean on another suite's helper, or `-Only` breaks it. Shared
helpers for a folder live in a non-suite file (`map/Map.Helpers.ps1`, `deep/TsRows.Helpers.ps1`,
`deep/cs/CsRows.Helpers.ps1`). This folder holds the top-level suites; new ones go into a topic subfolder.

## What each suite pins

Every parser is its own way of being wrong, so each has its own cases. Assert a NUMBER or a ROW or an EDGE,
never a sentence: a message can stay right while a count drifts.

| suites | pin |
|---|---|
| `Lines` | a source line per language as a number out of `--dump` (Roslyn, `.py`, the C-style scanner, `.ps1` by token, doc non-blank lines), and both STREAMED counters over a fixture built past 4 MB |
| `Walk` | junctions, a junction to its own ancestor, a deny-ACE file, a 5 MB non-source file — survived and NAMED |
| `Modes` | `--worst`, `--dump`, `--tracked`, `--include-untracked` on a real git repo (with a non-ASCII path), every argument error |
| `rules/Rules`, `rules/Docs`, `rules/Ratchet` | the three limits and the 90 % ceiling; split advice vs. a full folder; context-doc classification, the four remedies, the link check; the ratchet's verdicts and the baseline FILE FORMAT (malformed included) |
| `rules/Discipline` | `--async-discipline` (and the shapes it stays silent on), `--plugin` and its quote-aware splitter; a check that could not run is a violation |
| `PsGate`, `TsGate` | each rules half end to end; `PsGate` also runs `Run-PsGateTests.ps1`; `TsGate` fires every rule through both compilers (5.x in process, 7.x native) |
| `Map`, `map/*` | `--map` edges each half draws and deliberately does NOT; `paths` aliases, run-time-named imports, plugins, `--map-check`, `--map-if-stale`; the re-parse of only what moved, Go, packages in a subfolder, `--map-view` |
| `Rust`, `Markdown`, `Fbt` | the in-process halves (`syn`, `pulldown-cmark`) and the change detector behind `--fbt-scan` |
| `deep/Py*`, `deep/py/*` | the python deep map: rows, binding, lenses, incremental update |
| `deep/CsRows`, `deep/cs/**` | the C# deep map, syntax and semantic, sharing one database with python |
| `deep/Ts*`, `deep/TsAngular/*`, `deep/TsKeys/*`, `deep/TsPlain/*` | the TypeScript deep map with and without Angular |
| `deep/SqlMap`, `deep/sql/*` | the T-SQL half, `sql_links`, document facts |
| `deep/FullText`, `deep/magic/*` | nothing stored is cut; `--magic` over every half |
| `Connect`, `update/UpdateGate`, `trace/Trace` | the consumer wiring, `Update-Gate.ps1 -Trace`, the trace file and `--trace-report` |

## Mutation-proving a case

The root CLAUDE.md requires every new case be shown to fail with its rule broken. Why it is not ceremony:

- **the first mutation survived the whole suite** — deleting the `//` check from the C-style counter left every
  case green, because no case had a `//` line;
- a "rebuilt, never appended" case compared row counts; skipping the delete made `CREATE TABLE` throw, the old
  database survived, and the counts matched. Rewritten to delete a file and assert it LEAVES the database;
- the bugs writing cases found were mostly one shape — a check that a *value exists* where the question was
  what kind of node it is (a TS `Identifier`'s own `.text` read as a module specifier; a fully qualified
  `System.Type.GetType(…)` missed because only a bare receiver was accepted).

Restore a mutated file by editing it back or `touch` it: a backup copy keeps its OLD timestamp, MSBuild skips
the rebuild, and the "restored" run tests the mutant.

## The fixtures are wrong on purpose

`fixtures/violations.ps1` breaks every PowerShell rule; `expected.txt` pins the WHOLE output — findings and
token counts — line for line, because the failure it guards against is a rule going QUIET, which no loose
assertion notices. After `-Update`, read the diff before committing it. This repo's own gate does not run
`--ps-discipline`, so the fixtures are measured for size only.
