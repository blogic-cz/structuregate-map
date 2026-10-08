---
name: dotnet-trace
description: "Find where a .NET program spends its time before optimizing it: record a run with `dotnet-trace` (a sampling profiler - every method, framework and library code included, no code change), then summarize the trace per method with this skill's `summarize.py`. Use it when something is slow and the cause is not already measured, to compare a change before and after, or to check a guess like 'it is the parse' before acting on it. Covers what the numbers mean (inclusive vs self, summed over threads), the traps (the tracer inflates time, a warm cache hides disk, NativeAOT exes cannot be traced this way) and how to prove an optimization kept the output identical."
allowed-tools: Bash(dotnet-trace:*), Bash(dotnet:*), Bash(python:*), Read
argument-hint: "[the command to measure, or what is slow]"
---

# Measure a .NET run with dotnet-trace

**A skill for working on THIS repository**, kept in its own `.claude/skills/` and never shared with a consumer
(the shared ones live in `skills/`).

MEASURE, THEN CHANGE. A guess about where time goes is wrong often enough to cost more than the trace: on
the structuregate deep map the guess was "compiling referenced projects from source", and the trace said
reading assembly metadata again for every project and regenerating Razor nobody had edited.

## 1. Have the tool, and a traceable build

```powershell
dotnet-trace --version                     # or: dotnet tool install -g dotnet-trace
```

Trace the MANAGED build (`dotnet app.dll ...`). A NativeAOT exe has almost no EventPipe support, and native
code - a rust library linked in, a C++ dependency - shows up only as `UNMANAGED_CODE_TIME`. For those, use a
native sampler (`samply`, or Windows Performance Recorder).

**THE SHIPPED EXE, TRACED NATIVELY** (an elevated shell; `xperf` comes with the Windows Performance Toolkit).
The exe's `.pdb` sits beside it, so the compiled C#, Roslyn, the GC and the linked rust all resolve:

```powershell
$xperf = 'C:\Program Files (x86)\Windows Kits\10\Windows Performance Toolkit\xperf.exe'
& $xperf -on PROC_THREAD+LOADER+PROFILE -stackwalk Profile -BufferSize 1024 -MinBuffers 256 -MaxBuffers 1024
& .\out\win-x64\publish\app.exe <args>
& $xperf -d run.etl
$env:_NT_SYMBOL_PATH = 'C:\path\to\out\win-x64\publish'     # local only - no symbol server round trips
& $xperf -i run.etl -symbols -o profile.txt -a profile -detail  # weight per Module!Function, in microseconds
```

It settled what the managed trace could not: a slow deep-map project was a quarter Roslyn binding, a fifth its
lexer, a seventh the GC and almost none this repo's own C# - where the managed trace had charged most of it to
`UNMANAGED_CODE_TIME` on the rows thread.

## 2. Fix the case you measure

Pick ONE representative input and keep it identical between runs - the same tree, the same database, the
same flags. A case that edits real sources can be faked: for a tool with a cache, mark one entry stale in a
COPY of its state rather than touching someone's files.

Decide whether you measure COLD or WARM. Right after a big run the file cache is cold: one run of the same
tool took nearly ten times longer cold than warm with nothing else different. Run twice and time the second for warm.

## 3. Record

```powershell
dotnet-trace collect --format speedscope -o run.nettrace -- dotnet path\to\app.dll <args>
```

It writes `run.nettrace` and `run.speedscope.json`. The run is slower under the tracer (roughly 2x): the
ratios hold, the totals do not - time the plain run separately for the real wall clock.

**A MANAGED RUN UNDER THE TRACER IS NOT THE SHIPPED PROGRAM.** A whole-solution fresh run of the deep map took
several times the memory of the NativeAOT exe this way, and nearly ran the machine out of memory: check free memory
first and trace a SLICE of a big input. The managed build also showed minutes of GC polling and lock contention in
a parallel parse that barely exists in the AOT exe - four GC settings on the shipped exe were all within a second.
Confirm a finding on the program that ships before acting on it.

## 4. Read it

```powershell
python <this skill>\summarize.py run.speedscope.json --cpu --match MyApp.Namespace --top 25
python <this skill>\summarize.py run.speedscope.json --cpu --match Razor Roslyn ParseText
python <this skill>\summarize.py run.speedscope.json --cpu              # the biggest CPU consumers, any code
python <this skill>\summarize.py run.speedscope.json --threads      # threads and their spans
```

Or open `run.speedscope.json` at https://www.speedscope.app for a flame graph (the file is loaded in the
browser; nothing is uploaded).

- **INCLUSIVE** is a method with everything it called; **SELF** is the method alone. Start from inclusive on
  your own code (`--match` your namespace) and walk down to the biggest child.
- **SUMMED OVER THREADS**: a parallel loop on 8 cores adds 8x its wall time. Compare with the thread spans
  before calling something slow.
- **`UNMANAGED_CODE_TIME`** is time outside managed code: waiting, sleeping, native libraries. A thread pool
  idles there too, so it is large in every trace. **`--cpu` counts only samples where a thread was running
  managed code** - without it the top of the list is thread-pool semaphores.
- **LAZY WORK IS CHARGED TO WHOEVER TOUCHES IT FIRST.** Roslyn binds a method body when it is first asked a
  question about it, so "rows" can carry what looks like compilation cost.

## 5. Change one thing, then prove it

- Re-measure the SAME case, plain run, several times; report min/max, not one number.
- **PROVE THE OUTPUT DID NOT CHANGE.** Compare the full output (for a database: every table's rows hashed,
  ignoring columns that legitimately differ) against the version before the change. Parallelism is where
  this bites: binding Roslyn models ahead on all cores looked like a win and changed what the compiler
  reported - a few extra errors in one project that a serial run does not produce. It was reverted.
- A cache needs a test that an EDIT is seen through it - and that test must fail when the cache key leaves
  that input out (break the key, watch the test fail, restore).
- Temporary timers or `Console.Error` probes come out before the commit; an always-on phase breakdown that
  the program prints itself is a different thing and can stay.

## In this repository

The deep map's edit case on a large tree, against a copy of a current database:

```powershell
dotnet build src/StructureGate.csproj -p:SkipTests=true -p:SkipStructureGate=true
dotnet-trace collect --format speedscope -o leaf.nettrace -- dotnet out\structuregate.dll --root <tree> --ext .cs --map-sqlite <copy.sqlite>
python .claude\skills\dotnet-trace\summarize.py leaf.speedscope.json --cpu --match StructureGate --top 30
```

The deep map already prints its own coarse split (`the C# half spent: compile …, rows …, payload …,
store …`); trace when that line does not say enough.
