<#
    Compare-Release.ps1 - what a new build changes in a REAL tree's deep map, before it is released.

      pwsh -NoProfile -File scripts/Compare-Release.ps1 -Tree <tree> [-Old <exe>] [-New <exe or dll>] [-Out <dir>]
           [-OldDb <file>] [-NewDb <file>] [-TsNodeModules <dir>] [-AllowLoss]

    The black-box suites are small invented fixtures: a change passes all of them and still breaks a shape no
    case has. v2.1.13 renamed an input bound to an enum CONSTANT as if it were a variable, and every key behind
    such a binding lost its restriction - 806 cases green, the regression found by a consumer hours later. So a
    release is mapped over a real tree with the OLD exe and the NEW one, and every key is compared:

      LOST     its `always_values` restricted it before and no longer do (or the key is gone) - exit 1
      CHANGED  restricted both times, differently - read each one
      GAINED   unrestricted before, restricted now
      gates    every `gate_values` row, matched by the gate's file, line, column and source (ids shift)

    Both maps are written to `-Out` (a temp folder by default), never into the tree. `-OldDb`/`-NewDb` reuse a
    map already built - the old one is the same for every candidate of one release. `-AllowLoss` exits 0 on a
    loss the reader has looked at and accepts.
#>
param(
    [Parameter(Mandatory = $true)][string]$Tree,
    [string]$Old = '',
    [string]$New = '',
    [string]$Out = '',
    [string]$OldDb = '',
    [string]$NewDb = '',
    [string]$TsNodeModules = '',
    [switch]$AllowLoss
)
$ErrorActionPreference = 'Stop'
$repo = Split-Path $PSScriptRoot -Parent
$onWindows = [System.Environment]::OSVersion.Platform -eq [System.PlatformID]::Win32NT
$exeName = if ($onWindows) { 'structuregate.exe' } else { 'structuregate' }
if (-not $Old) { $Old = Join-Path (Join-Path $repo 'buildtools') $exeName }
if (-not $New) { $New = Join-Path (Join-Path $repo 'out') 'structuregate.dll' }
if (-not $Out) { $Out = Join-Path ([System.IO.Path]::GetTempPath()) ('compare-release-' + [guid]::NewGuid().ToString('N').Substring(0, 8)) }
[void](New-Item -ItemType Directory -Path $Out -Force)

# A `.dll` runs under `dotnet`, an exe on its own: the deployed release is an exe, a fresh build the dll.
function Invoke-Build([string]$Build, [string[]]$Arguments) {
    if ($Build.EndsWith('.dll')) { & dotnet $Build @Arguments } else { & $Build @Arguments }
    if ($LASTEXITCODE -ne 0) { throw "$Build $($Arguments -join ' ') exited $LASTEXITCODE" }
}

# A FRESH map: an old database beside it would be read incrementally and keep rows the build no longer writes.
function New-Map([string]$Build, [string]$Db) {
    foreach ($suffix in '', '-wal', '-shm', '.last-run.jsonl') { Remove-Item -LiteralPath ($Db + $suffix) -ErrorAction SilentlyContinue }
    Write-Host "compare-release: mapping $Tree with $Build"
    $arguments = @('--root', $Tree, '--map-sqlite', $Db)
    if ($TsNodeModules) { $arguments += @('--ts-node-modules', $TsNodeModules) }
    Invoke-Build $Build $arguments | Out-Null
}

# ONE JSON OBJECT PER LINE, so nothing is parsed out of a table: the query names a single column, the rule line
# under its header is skipped, and every later line is one row.
function Get-Rows([string]$Build, [string]$Db, [string]$Sql) {
    $text = Invoke-Build $Build @('--map-query', $Db, '--width', '0', '--limit', '100000000', '--sql', $Sql)
    $rows = @()
    $body = $false
    foreach ($line in @($text)) {
        $t = "$line".Trim()
        if (-not $body) { if ($t.StartsWith('---')) { $body = $true }; continue }
        if ($t.StartsWith('{')) { $rows += ($t | ConvertFrom-Json) }
    }
    return , $rows
}

if (-not $OldDb) { $OldDb = Join-Path $Out 'old.sqlite'; New-Map $Old $OldDb }
if (-not $NewDb) { $NewDb = Join-Path $Out 'new.sqlite'; New-Map $New $NewDb }

$keySql = "SELECT json_object('k', key, 'v', always_values, 'r', route, 'n', n_paths) AS j FROM key_reach"
$gateSql = "SELECT json_object('g', coalesce(f.path, '') || ':' || g.line || ':' || g.col || ' ' || coalesce(g.source, ''), " +
    "'d', v.dimension, 'o', v.op, 'v', v.values_json) AS j FROM gate_values v JOIN gates g ON g.id = v.gate " +
    "LEFT JOIN files f ON f.id = g.owner_file"

function Get-ByKey($Rows, [string]$Field) {
    $map = @{}
    foreach ($r in $Rows) { $map[[string]$r.$Field] = $r }
    return $map
}
# `[]` and an absent value are both "no restriction".
function Test-Restricted($Row) { return $null -ne $Row -and $Row.v -and [string]$Row.v -ne '[]' }
# A restriction WITHOUT ITS ROW IDS: `always_values` names its enum by id (`e:2`), and ids can shift between two
# builds of one tree - compared as written, every restricted key would read as changed.
function Get-Canon($Row) {
    if (-not (Test-Restricted $Row)) { return '' }
    return ((ConvertFrom-Json ([string]$Row.v)) | ForEach-Object {
        "$($_.enum_name)|$($_.dimension)|$($_.op)|$(@($_.values) -join ',')|$($_.domain)"
    } | Sort-Object) -join ' ; '
}

$oldKeys = Get-ByKey (Get-Rows $Old $OldDb $keySql) 'k'
$newKeys = Get-ByKey (Get-Rows $New $NewDb $keySql) 'k'
$lost = [System.Collections.Generic.List[string]]::new()
$changed = [System.Collections.Generic.List[string]]::new()
$gained = [System.Collections.Generic.List[string]]::new()
$routes = 0
foreach ($k in ($oldKeys.Keys + $newKeys.Keys | Sort-Object -Unique)) {
    $o = $oldKeys[$k]; $n = $newKeys[$k]
    if ($o -and $n -and $o.r -ne $n.r) { $routes++ }
    $was = Test-Restricted $o; $is = Test-Restricted $n
    if ($was -and -not $is) { $lost.Add("$k`n    old $($o.v)`n    new $(if ($n) { $n.v } else { '(key gone)' })") }
    elseif ($was -and $is -and (Get-Canon $o) -ne (Get-Canon $n)) { $changed.Add("$k`n    old $($o.v)`n    new $($n.v)") }
    elseif (-not $was -and $is) { $gained.Add("$k`n    new $($n.v)") }
}

$gateKey = { param($r) "$($r.g) | $($r.d)" }
$oldGates = @{}; foreach ($r in (Get-Rows $Old $OldDb $gateSql)) { $oldGates[(& $gateKey $r)] = "$($r.o) $($r.v)" }
$newGates = @{}; foreach ($r in (Get-Rows $New $NewDb $gateSql)) { $newGates[(& $gateKey $r)] = "$($r.o) $($r.v)" }
$gateDiff = [System.Collections.Generic.List[string]]::new()
foreach ($g in ($oldGates.Keys + $newGates.Keys | Sort-Object -Unique)) {
    $o = $oldGates[$g]; $n = $newGates[$g]
    if ($o -ne $n) { $gateDiff.Add("$g`n    old $(if ($o) { $o } else { '(none)' })`n    new $(if ($n) { $n } else { '(none)' })") }
}

foreach ($part in @(@('lost', $lost), @('changed', $changed), @('gained', $gained), @('gates', $gateDiff))) {
    Set-Content -LiteralPath (Join-Path $Out "$($part[0]).txt") -Value ($part[1] -join "`n") -Encoding utf8
}
Write-Host ("compare-release: keys {0} -> {1}; LOST {2}, changed {3}, gained {4}, route moved {5}; gate_values differing {6}" -f `
    $oldKeys.Count, $newKeys.Count, $lost.Count, $changed.Count, $gained.Count, $routes, $gateDiff.Count)
Write-Host "compare-release: details in $Out (lost.txt changed.txt gained.txt gates.txt)"
foreach ($l in ($lost | Select-Object -First 20)) { Write-Host "  LOST $l" }
if ($lost.Count -gt 20) { Write-Host "  ... $($lost.Count - 20) more in lost.txt" }
if ($lost.Count -and -not $AllowLoss) { exit 1 }
exit 0
