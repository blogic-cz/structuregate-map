---
name: map-lab
description: "Debug, profile or extend a deep-map half against a REAL (private) workspace without paying a full map per try: cut a small sample of the workspace, iterate on it with a phase-only harness, measure memory per phase (anonymous vs file-backed, what the OOM killer counts) and time full vs partial runs, then confirm once on the full map with a row-by-row compare and Compare-Release. Use when a fix or an optimization has to be checked on a large real tree, when a run is slow or runs a machine out of memory, before and after a change to the store, the closure or a half's rows, or when asked to profile, benchmark or 'test on a sample'."
allowed-tools: Bash(python3:*), Bash(dotnet:*), Bash(cargo:*), Bash(pwsh:*), Read
argument-hint: "[the workspace, and what to debug or measure]"
---

# Iterate on a sample, confirm on the whole

**A skill for working on THIS repository**, kept in its own `.claude/skills/`; never shared with a consumer.

A large real workspace costs minutes per full map and gigabytes of memory, and a fix to the store or the closure
took two hours when every try ran over the whole of one (about 6 min a cycle). The same work on a slice holding a
third of the rows runs the expensive phase in seconds. So:

1. **Find where the bulk is**, then **cut a sample**, outside the repo (it is private code):
   `python3 make-sample.py <workspace>` prints `.ts`/`.html` per folder, biggest first - the bulk is often not
   where it looks (a root `src/` rather than `apps/`). Then
   `python3 make-sample.py <workspace> <sample> --drop <folder> --drop <folder> ...` copies it without
   `node_modules` (linked back, so the same compiler is borrowed) and removes the feature folders. Keep shared code
   and the locale files; drop whole features. Aim for about a third of the rows; map the sample ONCE.
2. **Iterate with a phase-only harness over the stored map**, not a fresh map per try: a `#[test]` (or env-gated
   entry) that opens a COPY of the sample's database and runs only the phase under work - e.g. the TypeScript
   closure, `keyreach::build_closure` over `rows::ts::store::Store::from_db` - and dumps what it emits to a file.
   Build it `--release`, put every probe behind an environment variable so a new measurement needs no rebuild, and
   diff the dump against the one from before the change: byte for byte, every step.
3. **Measure memory by phase**: `memprof.sh <dll> <tree> <out> [gate args]` maps a tree while sampling the gate
   process twice a second; `python3 memreport.py <out>` gives the peak per phase, `--timeline` the curve against
   each phase's start and end (from the run's own `.last-run.jsonl` trace). Read **anonymous** memory: total RSS
   also counts a memory-mapped payload's file pages, which the kernel can reclaim. The exe's allocator is
   **mimalloc** (`#[global_allocator]`), so glibc's `malloc_trim`/`MALLOC_ARENA_MAX` change nothing; to tell live
   data from retained memory, wrap the allocator in a counting one for the measurement.
4. **Time full and partial runs**: `bench.sh <dll> <tree> <out> <file to touch> [gate args]` - a full map, then one
   comment appended to one file and the partial run a per-turn hook pays, the file restored after. Compare against
   `main`'s build (a `git worktree` at `main`, its `buildtools/` linked in, built with `-p:SkipStructureGate=true`).
5. **Confirm ONCE on the full workspace**: the before and after maps compared row by row by content (node rows
   included, never by rowid or id - ids shift), and `pwsh -NoProfile -File scripts/Compare-Release.ps1 -Tree <tree>
   -OldDb <before> -NewDb <after> -Old <before dll> -New <after dll> -Out <dir>` - LOST, changed and gained all 0.

## The machine's limits

- **One full map of a large tree at a time.** One peaked near 7 GB; two at once ran a 14 GB machine out of memory
  and the OOM killer took the session with them. Tell every agent, and serialise benchmarks, compares and repros.
- **Never `/tmp`**: its per-user quota is small and a run's payload and kept test trees fill it (`EDQUOT`, `disk I/O
  error`, a node `UNKNOWN: unknown error, write`). Give every run its own `TMPDIR` on the home disk - `memprof.sh`
  and `bench.sh` do - and every agent its own, or one agent's cleanup deletes another run's live trees.
- **Snapshot the binary under test** (copy `out/` aside and point `PSGATE_TEST_GATE` or the scripts at the copy):
  a rebuild while a run is in flight tests a mix. A case that builds a "second build" from `out/` then reads the
  snapshot as the same build - run that one without the override.
- A worktree agent cannot run `pwsh` here (a hook refuses it): it can build, `cargo test` and run the CLI; the
  PowerShell suites and `Compare-Release.ps1` run from the main checkout.

## Never

- Commit the sample, a map of it, a measurement of it or any of its names, paths or counts - the repo is public
  and `LeakGuard` fails the build on a term from `leak-deny.txt`. Write findings with invented names and relative
  figures.
- Edit the real workspace. Work on a copy (`make-sample.py` without `--drop` is a full copy).
- Trust a speed or memory number from a run that shared the machine with another map or a build.
