<#
    Compare-Release.ps1 - what a new build changes in a REAL tree's deep map, before it is released.

      pwsh -NoProfile -File scripts/Compare-Release.ps1 -Tree <tree> [-Old <exe>] [-New <exe or dll>] [-Out <dir>]
           [-OldDb <file>] [-NewDb <file>] [-TsNodeModules <dir>] [-TsConfig <file>] [-Ext <list>] [-Skip <list>] [-AllowLoss]

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
    # THE CONSUMER'S OWN CONFIG: a `gateInputs` or `featureChecks` it declares forms the gates a key is restricted by,
    # so a tree mapped under another config compares keys nobody reads. v2.1.15 restricted two keys too much through a
    # declared `[disabled]` gate, and a compare under the tree's default config saw no change at all.
    [string]$TsConfig = '',
    # `--ext` for both maps: `.ts` alone skips the C# half, which no key comparison reads.
    [string]$Ext = '',
    # `--skip` for both maps, as the tree's own hook passes it (it REPLACES the default list).
    [string]$Skip = '',
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
    if ($TsConfig) { $arguments += @('--ts-config', $TsConfig) }
    if ($Ext) { $arguments += @('--ext', $Ext) }
    if ($Skip) { $arguments += @('--skip', $Skip) }
    Invoke-Build $Build $arguments | Out-Null
}

# ONE ROW PER LINE, its fields joined by `|~|` in the query itself: the query names a single column, the rule line
# under its header is skipped, and every later line is one row, split back into its fields.
$sep = '|~|'
function Get-Lines([string]$Build, [string]$Db, [string]$Sql) {
    $text = Invoke-Build $Build @('--map-query', $Db, '--width', '0', '--limit', '0', '--sql', $Sql)
    $rows = @()
    $body = $false
    foreach ($line in @($text)) {
        $t = "$line".Trim()
        if (-not $body) { if ($t.StartsWith('---')) { $body = $true }; continue }
        if ($t.Contains($sep)) { $rows += , $t.Split([string[]]@($sep), [System.StringSplitOptions]::None) }
    }
    return , $rows
}

# A MAP FROM BEFORE NODE ROWS (schema 10 and older) holds `always_values` and `values_json` as JSON TEXT, read whole
# here and opened by PowerShell; a map with `_cells` holds them as node rows, read by role. Both come out as ONE
# canonical spelling - a restriction WITHOUT ITS ROW IDS, `enum_name|dimension|op|values|domain` per entry, sorted -
# so a schema change alone reads as no change at all. `always_values` names its enum by id (`e:2`), and ids can
# shift between two builds of one tree: compared as written, every restricted key would read as changed.
function Test-Nodes([string]$Build, [string]$Db) {
    $r = Get-Lines $Build $Db "SELECT 'n' || '$sep' || count(*) FROM sqlite_master WHERE name = '_cells'"
    return $r.Count -gt 0 -and $r[0][1] -ne '0'
}
function Get-TextCanon([string]$Text) {
    if (-not $Text -or $Text -eq '[]') { return '' }
    return ((ConvertFrom-Json $Text) | ForEach-Object {
        "$($_.enum_name)|$($_.dimension)|$($_.op)|$(@($_.values) -join ',')|$($_.domain)"
    } | Sort-Object) -join ' ; '
}
function Get-Keys([string]$Build, [string]$Db) {
    $map = @{}
    if (Test-Nodes $Build $Db) {
        $f = { param($role) "(SELECT x.value FROM key_reach__always_values x WHERE x.owner = it.owner AND x.parent = it.n AND x.role = '$role')" }
        $values = "(SELECT group_concat(v.value, ',' ORDER BY v.ord) FROM key_reach__always_values l " +
            "JOIN key_reach__always_values v ON v.owner = l.owner AND v.parent = l.n WHERE l.owner = it.owner AND l.parent = it.n AND l.role = 'values')"
        $entry = "coalesce($(& $f 'enum_name'), '') || '|' || coalesce($(& $f 'dimension'), '') || '|' || " +
            "coalesce($(& $f 'op'), '') || '|' || coalesce($values, '') || '|' || coalesce($(& $f 'domain'), '')"
        $sql = "SELECT k.key || '$sep' || coalesce(k.route, '') || '$sep' || coalesce(k.n_paths, '') || '$sep' || " +
            "coalesce((SELECT group_concat(e, char(30)) FROM (SELECT $entry AS e FROM key_reach__always_values it " +
            "WHERE it.owner = '#' || k.rowid AND it.parent = 0)), '') FROM key_reach k"
        foreach ($r in (Get-Lines $Build $Db $sql)) {
            $canon = (@($r[3].Split([char]30) | Where-Object { $_ }) | Sort-Object) -join ' ; '
            $map[$r[0]] = [pscustomobject]@{ r = $r[1]; n = $r[2]; v = $canon }
        }
    } else {
        $sql = "SELECT key || '$sep' || coalesce(route, '') || '$sep' || coalesce(n_paths, '') || '$sep' || coalesce(always_values, '') FROM key_reach"
        foreach ($r in (Get-Lines $Build $Db $sql)) { $map[$r[0]] = [pscustomobject]@{ r = $r[1]; n = $r[2]; v = (Get-TextCanon $r[3]) } }
    }
    return $map
}
function Get-Gates([string]$Build, [string]$Db) {
    $at = "coalesce(f.path, '') || ':' || g.line || ':' || g.col || ' ' || coalesce(g.source, '') || ' | ' || coalesce(v.dimension, '')"
    $join = "FROM gate_values v JOIN gates g ON g.id = v.gate LEFT JOIN files f ON f.id = g.owner_file"
    $map = @{}
    if (Test-Nodes $Build $Db) {
        $values = "(SELECT group_concat(i.value, ',' ORDER BY i.ord) FROM gate_values__values_json i WHERE i.owner = '#' || v.rowid AND i.parent = 0)"
        foreach ($r in (Get-Lines $Build $Db "SELECT $at || '$sep' || coalesce(v.op, '') || '$sep' || coalesce($values, '') $join")) {
            $map[$r[0]] = "$($r[1]) [$($r[2])]"
        }
    } else {
        foreach ($r in (Get-Lines $Build $Db "SELECT $at || '$sep' || coalesce(v.op, '') || '$sep' || coalesce(v.values_json, '') $join")) {
            $listed = if ($r[2]) { @(ConvertFrom-Json $r[2]) -join ',' } else { '' }
            $map[$r[0]] = "$($r[1]) [$listed]"
        }
    }
    return $map
}

if (-not $OldDb) { $OldDb = Join-Path $Out 'old.sqlite'; New-Map $Old $OldDb }
if (-not $NewDb) { $NewDb = Join-Path $Out 'new.sqlite'; New-Map $New $NewDb }

# A key is restricted when its canonical spelling holds an entry: `[]` and an absent value are both "no restriction".
function Test-Restricted($Row) { return $null -ne $Row -and [string]$Row.v -ne '' }

$oldKeys = Get-Keys $Old $OldDb
$newKeys = Get-Keys $New $NewDb
$lost = [System.Collections.Generic.List[string]]::new()
$changed = [System.Collections.Generic.List[string]]::new()
$gained = [System.Collections.Generic.List[string]]::new()
$routes = 0
foreach ($k in ($oldKeys.Keys + $newKeys.Keys | Sort-Object -Unique)) {
    $o = $oldKeys[$k]; $n = $newKeys[$k]
    if ($o -and $n -and $o.r -ne $n.r) { $routes++ }
    $was = Test-Restricted $o; $is = Test-Restricted $n
    if ($was -and -not $is) { $lost.Add("$k`n    old $($o.v)`n    new $(if ($n) { $n.v } else { '(key gone)' })") }
    elseif ($was -and $is -and $o.v -ne $n.v) { $changed.Add("$k`n    old $($o.v)`n    new $($n.v)") }
    elseif (-not $was -and $is) { $gained.Add("$k`n    new $($n.v)") }
}

$oldGates = Get-Gates $Old $OldDb
$newGates = Get-Gates $New $NewDb
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
