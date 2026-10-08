<#
    GateTemplates.ps1 - the files Connect-Gate.ps1 writes into a consumer, as text.

    WHY TEXT AND NOT FILES IN THIS REPO. A second copy on disk here would be a file the gate of THIS repo
    counts and the map lists as imported by nothing, and it would drift from the copies already deployed in
    the consumer trees. A template with one placeholder each cannot drift from itself.

    Both carry the same note to their reader: Connect-Gate.ps1 wrote this, editing it is fine, and a
    re-connect leaves an existing file alone.
#>

# The npm launcher, as text. A template rather than a file in this repo: a second copy on disk here would be
# a file the gate of THIS repo counts, and it would drift from the one already deployed in every consumer.
function Get-NpmLauncherText([string]$GateRelative, [string]$GateArgs, [bool]$Deep) {
    $template = @'
// Structure gate. Runs as `prebuild`, so `npm run build` - and anything that goes through it - stops on a
// violation. `npm run check:structure` runs it alone; extra flags are passed through (`-- --worst`).
//
// The RULES are not here. They are counted by __GATE__/structuregate.exe, the shared NativeAOT gate built
// from github.com/blogic-cz/structuregate-map2 and deployed into this tree, so every project that enforces the
// same limits shares ONE implementation instead of a per-repo script that drifts.
//
// Connect-Gate.ps1 wrote this file. It is yours to edit; a re-connect leaves an existing one alone.
import { spawnSync } from 'child_process';
import { existsSync } from 'fs';
import { fileURLToPath } from 'url';
import { dirname, join } from 'path';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');
// `.exe` on Windows only: the gate is published per OS, and the name follows it.
const GATE = join(ROOT, ...'__GATE__'.split('/'), process.platform === 'win32' ? 'structuregate.exe' : 'structuregate');
const ARGS = '__ARGS__'.split(' ').filter(Boolean);

if (!existsSync(GATE)) {
  console.error(`FAIL: structure gate missing at ${GATE}\n`
    + '  -> rebuild it from a clone of github.com/blogic-cz/structuregate-map2:\n'
    + '     dotnet publish src/StructureGate.csproj -c Release -r win-x64 -p:PublishAot=true (linux-x64 off Windows)');
  process.exit(1);
}

// `--hook` is consumed here, never forwarded: a Claude Code Stop hook must exit 2 for the output to be
// handed back as something to fix. Exit 1 would only print, and the turn would end anyway.
const passThrough = process.argv.slice(2);
const asHook = passThrough.includes('--hook');
const rest = passThrough.filter((flag) => flag !== '--hook');
// The size limits ratchet against structure-baseline.json: a file over the limit when the gate arrived may
// only shrink, and a new one must be under it.
const BASELINE = ['--baseline', join(ROOT, 'structure-baseline.json')];
const result = spawnSync(GATE, ['--root', ROOT, ...ARGS, ...BASELINE, ...rest], { stdio: 'inherit' });
if (result.error) { console.error(`FAIL: ${result.error.message}`); process.exit(1); }
let status = result.status ?? 1;

// THE MAP, as a SECOND run over the same files. `--map` REPLACES the limit check rather than adding to it,
// so one run cannot do both - a launcher that passed `--map` here would stop checking the limits. A map
// nobody rebuilds answers about last month's tree; `--map-if-stale` makes it cheap enough to rebuild on
// every run (no file newer than the map, no parser runs). `--map-check` fails only on a broken import or a
// half that died; what the map cannot resolve is ratcheted by map-baseline.json. A pass-through flag
// (`-- --worst`) is a question to the gate alone, so the map is skipped.
if (rest.length === 0) {
  const map = ['--root', ROOT, ...ARGS, '--map', '--map-if-stale', '--map-check', '--map-out', join(ROOT, 'buildmap.json')];
  if (existsSync(join(ROOT, 'map-baseline.json'))) map.push('--map-baseline', join(ROOT, 'map-baseline.json'));
  if (__DEEP__) map.push('--map-sqlite', join(ROOT, 'buildmap.sqlite'));
  const mapped = spawnSync(GATE, map, { stdio: 'inherit' });
  if (status === 0) status = mapped.error ? 1 : (mapped.status ?? 1);
}
process.exit(status === 0 ? 0 : (asHook ? 2 : status));
'@
    $deepText = if ($Deep) { 'true' } else { 'false' }
    return $template.Replace('__GATE__', $GateRelative).Replace('__ARGS__', $GateArgs).Replace('__DEEP__', $deepText)
}

# The Stop hook wrapper, as text. Same reason as the launcher: one copy, and it lives beside the exe it runs.
function Get-HookWrapperText([string]$GateArgs, [bool]$Deep) {
    $template = @'
<#
    Stop hook: refuse to finish a turn that left a structure rule broken.

    EXIT 2 IS THE WHOLE POINT. Claude Code hands a Stop hook's output back as something to fix only on
    exit 2. The gate exits 1 on a violation, which is right for a build, so the translation happens here.

    Connect-Gate.ps1 wrote this file. Add flags to $GateArgs below; a re-connect leaves it alone.
#>
$ErrorActionPreference = 'Continue'
$gate = Join-Path $PSScriptRoot $(if ($env:OS -eq 'Windows_NT') { 'structuregate.exe' } else { 'structuregate' })
$tree = Split-Path $PSScriptRoot -Parent
$GateArgs = @(__ARGS__)

if (-not (Test-Path $gate)) {
    Write-Host "structure gate missing at $gate - rebuild it from a clone of github.com/blogic-cz/structuregate-map2:"
    Write-Host '  dotnet publish src\StructureGate.csproj -c Release -r win-x64 -p:PublishAot=true (linux-x64 off Windows)'
    exit 2
}

# The size limits ratchet against structure-baseline.json: a file over the limit when the gate arrived may
# only shrink, and a new one must be under it.
& $gate --root $tree @GateArgs --baseline (Join-Path $tree 'structure-baseline.json')
$status = $LASTEXITCODE

# THE MAP, as a SECOND run: `--map` REPLACES the limit check rather than adding to it. `--map-if-stale` keeps
# it cheap on every turn, `--map-check` fails only on a broken import or a half that died, and what the map
# cannot resolve is ratcheted by map-baseline.json.
$map = @('--root', $tree) + $GateArgs + @('--map', '--map-if-stale', '--map-check', '--map-out', (Join-Path $tree 'buildmap.json'))
$baseline = Join-Path $tree 'map-baseline.json'
if (Test-Path $baseline) { $map += @('--map-baseline', $baseline) }
if (__DEEP__) { $map += @('--map-sqlite', (Join-Path $tree 'buildmap.sqlite')) }
& $gate @map
if ($status -eq 0) { $status = $LASTEXITCODE }
if ($status -ne 0) { exit 2 }
exit 0
'@
    $quoted = ''
    foreach ($flag in $GateArgs.Split([char]' ')) {
        if (-not $flag) { continue }
        if ($quoted) { $quoted += ', ' }
        $quoted += "'" + $flag + "'"
    }
    $deepText = if ($Deep) { '$true' } else { '$false' }
    return $template.Replace('__ARGS__', $quoted).Replace('__DEEP__', $deepText)
}

# THE MAP'S OUTPUTS ARE CACHES, so they are ignored where the entry point writes them. Appended only when
# missing, and only in a git tree; `map-baseline.json` is NOT here - it is the ratchet and stays tracked.
function Add-GateIgnore([string]$Path) {
    if (-not (Test-Path (Join-Path $Path '.git'))) { return 'no git' }
    $file = Join-Path $Path '.gitignore'
    $text = if (Test-Path $file) { [System.IO.File]::ReadAllText($file) } else { '' }
    $held = @($text.Split([char]10) | ForEach-Object { $_.Trim() })
    $missing = @(@('buildmap.json', 'buildmap.sqlite') | Where-Object { $held -notcontains $_ })
    if ($missing.Count -eq 0) { return 'present' }
    $lead = if ($text.Length -gt 0 -and -not $text.EndsWith([string][char]10)) { [string][char]10 } else { '' }
    $block = $lead + '# the structure gate''s code map - a cache, rebuilt by the entry point' + [char]10 + (($missing -join [char]10) + [char]10)
    [System.IO.File]::AppendAllText($file, $block)
    return 'added'
}
