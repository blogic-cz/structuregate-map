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
      targets  every literal, function and call that names a declaration (`target_id`, `callee_id`), matched by its file,
               line, column and text: one that bound before and binds nothing now is LOST - exit 1 (v2.8.36 lost
               literals' members inside `map<T>(..)` callbacks with every key and gate equal)

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
    # A LIST, not `$rows += ,`: appending to an array copies it, and the calls table made one run of this script take 25 minutes.
    $rows = [System.Collections.Generic.List[object]]::new()
    $body = $false
    foreach ($line in @($text)) {
        $t = "$line".Trim()
        if (-not $body) { if ($t.StartsWith('---')) { $body = $true }; continue }
        if ($t.Contains($sep)) { $rows.Add($t.Split([string[]]@($sep), [System.StringSplitOptions]::None)) }
    }
    return , $rows.ToArray()
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

# WHAT A ROW BINDS TO, BY WHERE IT IS (the targets check). A key restriction is not the only thing a release can lose: v2.8.36 dropped the
# member target of literals inside `map<T>(..)` callbacks while every key and gate stayed equal, and a rising total hid it. Every row that
# names a declaration - a literal's or function's `target_id`, a call's `target_id` and `callee_id` - is keyed by its file, line, column and
# text (ids shift between two builds of one tree), and a row that bound before and binds nothing now is LOST. A table or column a map lacks
# is skipped there; two rows at one place are told apart by their order.
$targetColumns = @(
    @{ t = 'string_literals'; c = @('target_id'); text = 't.value' },
    @{ t = 'number_literals'; c = @('target_id'); text = 't.value' },
    @{ t = 'template_literals'; c = @('target_id'); text = "''" },
    @{ t = 'functions'; c = @('target_id'); text = "coalesce(t.name, '')" },
    @{ t = 'calls'; c = @('target_id', 'callee_id', 'fills_id'); text = 't.callee' }
)
function Test-Column([string]$Build, [string]$Db, [string]$Table, [string]$Column) {
    $r = Get-Lines $Build $Db "SELECT 'c' || '$sep' || count(*) FROM pragma_table_info('$Table') WHERE name = '$Column'"
    return $r.Count -gt 0 -and $r[0][1] -ne '0'
}
function Get-Targets([string]$Build, [string]$Db) {
    $map = @{}
    foreach ($spec in $targetColumns) {
        if (-not (Test-Column $Build $Db $spec.t 'line')) { continue }
        $col = if (Test-Column $Build $Db $spec.t 'col') { 't.col' } else { "''" }
        foreach ($column in $spec.c) {
            # A COLUMN NO ROW FILLED IS NOT CREATED: its rows are there, unbound - never "gone".
            $bound = if (Test-Column $Build $Db $spec.t $column) { "(t.$column IS NOT NULL AND t.$column <> '')" } else { '0' }
            $sql = "SELECT '$($spec.t).$column' || '$sep' || coalesce(f.path, '') || ':' || t.line || ':' || coalesce($col, '') || ' ' || coalesce($($spec.text), '') " +
                "|| '$sep' || $bound FROM $($spec.t) t LEFT JOIN files f ON f.id = t.file ORDER BY t.rowid"
            foreach ($r in (Get-Lines $Build $Db $sql)) {
                $key = "$($r[0]) $($r[1])"
                $n = 1
                while ($map.ContainsKey("$key #$n")) { $n++ }
                $map["$key #$n"] = $r[2] -eq '1'
            }
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

$oldTargets = Get-Targets $Old $OldDb
$newTargets = Get-Targets $New $NewDb
$targetsLost = [System.Collections.Generic.List[string]]::new()
$targetsGained = 0
foreach ($t in $oldTargets.Keys) {
    if ($oldTargets[$t] -and -not $newTargets[$t]) { $targetsLost.Add($(if ($newTargets.ContainsKey($t)) { $t } else { "$t (row gone)" })) }
}
foreach ($t in $newTargets.Keys) { if ($newTargets[$t] -and -not $oldTargets[$t]) { $targetsGained++ } }

foreach ($part in @(@('lost', $lost), @('changed', $changed), @('gained', $gained), @('gates', $gateDiff), @('targets-lost', ($targetsLost | Sort-Object)))) {
    Set-Content -LiteralPath (Join-Path $Out "$($part[0]).txt") -Value ($part[1] -join "`n") -Encoding utf8
}
Write-Host ("compare-release: keys {0} -> {1}; LOST {2}, changed {3}, gained {4}, route moved {5}; gate_values differing {6}" -f `
    $oldKeys.Count, $newKeys.Count, $lost.Count, $changed.Count, $gained.Count, $routes, $gateDiff.Count)
Write-Host ("compare-release: bound rows LOST {0}, gained {1} (string/number/template literals, functions, calls)" -f $targetsLost.Count, $targetsGained)
Write-Host "compare-release: details in $Out (lost.txt changed.txt gained.txt gates.txt targets-lost.txt)"
foreach ($l in ($lost | Select-Object -First 20)) { Write-Host "  LOST $l" }
if ($lost.Count -gt 20) { Write-Host "  ... $($lost.Count - 20) more in lost.txt" }
foreach ($l in ($targetsLost | Sort-Object | Select-Object -First 20)) { Write-Host "  TARGET LOST $l" }
if ($targetsLost.Count -gt 20) { Write-Host "  ... $($targetsLost.Count - 20) more in targets-lost.txt" }
if (($lost.Count -or $targetsLost.Count) -and -not $AllowLoss) { exit 1 }
exit 0
