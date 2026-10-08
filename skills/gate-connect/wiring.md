# Wiring, by hand and by shape

What `Connect-Gate.ps1` writes, for a tree wired by hand or a wiring that has to be checked. The command
itself is in [SKILL.md](SKILL.md).

## Where the two files come from

**ONE release per machine, every consumer a HARD LINK to it.** From a clone, `dotnet publish` writes the
release into the clone's gitignored `buildtools/` and links `structuregate.exe` and `StructureGate.targets`
into each folder listed as a `GateConsumer` in `src/GateConsumers.local.props` (gitignored, imported by
`src/StructureGate.csproj`) - two files, no runtime, no dependencies, never in the consumer's git. Off a
clone, and off Windows always, the release is the one `Update-Gate.ps1` downloaded and `consumers.txt` beside
it is the list. Either way a consumer changes only when the release does.

## The MSBuild import

```xml
<PropertyGroup>
  <StructureGateRoot>$(MSBuildProjectDirectory)\..</StructureGateRoot>
</PropertyGroup>
<Import Project="..\buildtools\StructureGate.targets" />
```

It runs `BeforeTargets="CoreCompile"`, so a violation fails the build with no binary produced - from
`dotnet build`, the IDE and any wrapper script alike. `-p:SkipStructureGate=true` disables it for one build
and leaves a trace in the log. The MSBuild target does NOT rebuild the map; see the `buildmap` skill.

## The shapes a consumer takes

* **An MSBuild project** - the import above. `--async-discipline` for a UI, where one blocking wait delays
  every action queued behind it.
* **An npm project** - `scripts/checkStructure.mjs` runs as `prebuild`, so `npm run build` and any script
  that builds first go through it. `--ext` adds what the tree holds (`.gs`, `.css`, ...).
* **No build step at all** - a Claude Code **Stop hook** runs `buildtools/StructureGate.Hook.ps1`, so the turn
  itself is the gate. The wrapper exists for ONE reason: a Stop hook must exit 2 for Claude Code to hand the
  output back as work to fix; exit 1 only prints. The gate exits 1 (correct for MSBuild), so the translation
  lives in the wrapper, not the exe. The npm launcher does the same when passed `--hook` (consumed, never
  forwarded). A second Stop hook can run the deep map.
* **A test runner or a publish script as the host** - `pytest`, or the top of a script that ships, calls the
  same launcher, so the flags live in ONE place and the hosts cannot disagree. A MISSING binary is a
  failure, never a skip.
* **A Windows PowerShell 5.1 tree** - `--ps-discipline` (a closure reading a builder's locals after it
  returned wants `.GetNewClosure()`); long files and full folders go into the baseline as a shrinking list.

## The skills are junctions, never copies

`skills/` in the tool's repo (and in the release) is the ORIGIN of `buildmap`, `map-sqlite` and `gate-connect`.
A consumer's `.claude/skills/<name>` is a JUNCTION to it, so an edit at the origin is live there with nothing
to redeploy:

```powershell
New-Item -ItemType Junction -Path "<tree>\.claude\skills\map-sqlite" -Target "<origin>\skills\map-sqlite"
```

Two properties come free:

* **The structure gate does not walk into it** - junctions are not followed, so the skill's files are not
  counted against the consumer's folder or file limits; they belong to the tool's repo, which gates itself.
* **The map does not list it either**, so a junctioned skill never turns up as a file nothing imports.

Each skill is written for ANY connected tree. What one tree adds - its plugin, its layout, where it keeps
its database - goes in that tree's own `<consumer>.md`, which the skill reads first when it names the tree.
The map agents are single files, so a consumer holds a HARD LINK to each in `.claude/agents/`.

## Why the two forgotten steps matter

**The host probe.** A missing parser is not a skipped check: every file that half owned is reported
`UNMAPPED`, one line each - honest, and useless as a map.

**The baseline.** A tree's existing unread entry points, run-time imports and over-limit files were true
before the gate arrived. `--update-map-baseline` and `--update-baseline` record them as the ratchet, so only
NEW ones fail. A connect that ends in `WARNING: n file(s) UNMAPPED` says outright that the baseline it just
wrote is over a map nobody could read.
