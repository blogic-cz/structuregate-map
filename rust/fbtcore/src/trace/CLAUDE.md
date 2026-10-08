# trace: where a run's time went

`mod.rs` records spans, `otlp.rs` writes one run as OTLP/JSON, `report.rs` is `--trace-report`. Cases are
`tests/trace/Trace.Tests.ps1`.

## When a run is traced

- **Asked for**: `STRUCTUREGATE_TRACE=<file>` (or `--trace <file>`) appends ONE line per run. Nothing is
  written without it. **A run never fails over its trace**: it prints a `NOTE:` and keeps its own exit code.
- **Always, for a deep map**: `--map-sqlite <db>` writes `<db>.last-run.jsonl` beside the database (`LAST_RUN`),
  replaced by the next run that does WORK — a run that re-read nothing and replayed the passes leaves it and
  `_meta.last_refresh` alone, so the slow run stays readable after a slow refresh. Detail off, a few KB. Every
  walk that hashes the tree skips it with the database's other siblings, so writing it is never a change.
- A gate run, or `--map` without `--map-sqlite`, writes nothing unless asked.
- **`--trace-detail`** (`STRUCTUREGATE_TRACE_DETAIL=1`) adds a span per C# batch (the host's compile, rows,
  source read and payload, plus the wait for the store), EVERY C# project instead of the 20 slowest, and the
  1000 slowest files instead of 20. A larger line; leave it off unless investigating.
- **Rollover at 20 MB** (`STRUCTUREGATE_TRACE_MAX_BYTES` moves it): the file becomes `trace.1.jsonl`, replacing
  the previous one — at most two files on disk, and `--trace-report` reads both.

```bash
STRUCTUREGATE_TRACE=~/.structuregate/trace.jsonl dotnet build    # every gate run of this build
structuregate --root . --map-sqlite map.sqlite --trace /tmp/slow.jsonl --trace-detail
structuregate --trace-report /tmp/slow.jsonl
```

**Switching it on for a consumer**: the MSBuild property `StructureGateTrace` (e.g. in `Directory.Build.props`)
reaches the gate's `Exec` ONLY when set, so an empty property never blanks the environment's variable
(`StructureGate.targets`). Machine-wide: `scripts/Update-Gate.ps1 -Trace` sets the user's
`STRUCTUREGATE_TRACE` to `trace.jsonl` beside the release under local app data, keeps a value the user set
themselves, stays on across updates, and `-NoTrace` removes only its own. Programs already running see it
after a restart. Off Windows it prints the `export` line for a shell profile instead.

## What a line holds

One OTLP/JSON `ExportTraceServiceRequest`: the run is the root span `structuregate`, every stage a child of
the stage open when it started. One run at a time, one thread of stages — work fanned out inside a stage is
timed as the stage, never span by span from the workers.

| span | covers |
|---|---|
| `structuregate` | the run: `structuregate.root`, `.roots`, `.mode` (`gate`, `map`, `deep`, `query`), `process.command_args`, `process.exit.code` |
| `walk and count`, `<lang> rules`, `judge`, `plugins` | the gate's stages; `structuregate.gate.cached` when the pass cache answered |
| `walk`, `map: <half>`, `write the map` | the file map, one span per half with its file count and host |
| `deep map`, `deep: <half>`, `deep: links, seeds, index` | the deep halves in run order; `deep: python` carries `structuregate.early.python` (`used`, or `stale` when a half stored before it) |
| `csharp: compile`, `typescript: <phase>`, … | totals a HOST reported (`structuregate.timing = reported`), laid back from the end of their stage |
| `csharp: load references` / `parse sources` / `razor` / `declarations`, `csharp project: <csproj>` | where `csharp: compile` went: phase totals and the slowest projects with `structuregate.files`, `structuregate.csharp.assemblies_opened` / `.assemblies_shared` |
| `csharp: hash the files and projects`, `read what the database recorded`, `open the session`, `order the files by project`, `batches`, `close the session`, `find unrestored projects` | every step of the deep C# half; `csharp: batches` carries `structuregate.batches` and `structuregate.csharp.store_thread_ms` |
| `csharp batch` (detail) | one batch's `csharp: compile`, `rows`, `read the sources`, `payload`, and `csharp: wait for the store`; the rest is the round trip |
| `deep: key the passes over the finished rows`, `pass: <name>` | the replay check, then each pass: `sql links`, `sql seeds and typed views`, `duplicate types`, `document facts`, `row search index`, `atlas`, `count the tables`, `keep the key` |
| `file: csharp` | the slowest single files, with `code.filepath` |

**`structuregate.timing = breakdown`** marks a span that EXPLAINS its parent rather than adding to it (a compile
phase, a project, the detailed C# totals already inside the batches). Add a span to every new stage — time
outside any span shows up as `(untraced)`.

The resource carries `service.version`, `structuregate.build`, `host.name`, `os.type` and the process.
`OTEL_RESOURCE_ATTRIBUTES` (`team=build,deployment.environment=ci`) and `OTEL_SERVICE_NAME` are read the way
the OpenTelemetry SDKs read them and override the exe's own values.

## `--trace-report`

Per tree, every stage across every run in the file: runs, median, max, total, ordered by total. Every span
with children gets an `(untraced)` row for the time its children do not cover (breakdown spans are not
cover) — **a large `(untraced)` row is where a span is missing**. Then the last run of each tree as a tree,
same-named siblings folded (`csharp batch x40`), then the 15 slowest single files. A trace from v1.5.8 or
older has no `breakdown` mark; its compile phases and projects are recognized by name. A line that is not a
run (a torn write, another tool's line) is counted and passed over.

## No socket, ever

The exe runs inside every consumer's build and the paths are theirs, so it sends nothing. A machine that
wants traces centrally runs an OpenTelemetry Collector whose `otlpjsonfile` receiver reads this file as is:

```yaml
receivers:
  otlpjsonfile: { include: [ "/home/me/.structuregate/trace.jsonl" ], start_at: end }
exporters:
  otlphttp: { endpoint: https://otel.example.com }
service:
  pipelines:
    traces: { receivers: [otlpjsonfile], exporters: [otlphttp] }
```

Any OTLP backend (Jaeger, Tempo, Honeycomb, Datadog) is then the Collector's choice, with no change here.
