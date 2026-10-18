<#
    The helpers the deep C# suites share - `CsRows` and its parts in `cs/` - so each can run alone with
    `-Only`. NAMED FOR THESE SUITES: every suite is dot-sourced into ONE scope in name order, and a helper
    named like another suite's would silently replace it for every suite that sorts later.
#>

# The node rows' SQL (a list cell is `<table>__<column>`, never JSON text).
. (Join-Path $PSScriptRoot '../nodes/NodeSql.Helpers.ps1')

$script:CsRowsPython = [bool]$script:Python
if (-not $script:CsRowsPython) { Write-Host '    (no python on this machine - the deep C# map cases are not run)' }

# Build the deep map over a throwaway tree and hand back the database path. `-Shared` is a case's PROMISE that it only
# READS this map - never edits the tree, maps it again or writes the database - so it may come from the harness's cache
# (`Get-SharedMap`, harness/SharedMap.ps1), another case's copy restored at the path it was built at.
function New-CsDb([hashtable]$Files, [string]$Ext = '.cs', [switch]$Shared) {
    $tree = Use-Tree $Files
    $db = Join-Path $tree 'map.sqlite'
    $map = @('--root', $tree, '--ext', $Ext, '--map', '--map-out', (Join-Path $tree 'm.json'), '--map-sqlite', $db)
    if ($Shared) { $made = Get-SharedMap $tree $map; $db = $made.Db; $result = $made.Result }
    else { $result = Invoke-Gate $map }
    Assert-Exit $result 0
    if (-not (Test-Path $db)) { throw "no database was written. Output:`n$($result.Text)" }
    return $db
}

function Invoke-CsQ {
    param([string]$Path, [Parameter(ValueFromRemainingArguments)][object[]]$LensArgs)
    $result = Invoke-Gate --map-query $Path @LensArgs
    Assert-Exit $result 0
    return $result
}

function Get-CsScalar([string]$Db, [string]$Sql) {
    $r = Invoke-CsQ $Db --sql $Sql
    return ($r.Lines | Where-Object { $_ -match '^\s*\d+\s*$' } | Select-Object -First 1).Trim()
}

function New-MixedTree {
    return Use-Tree @{
        'a.py'    = 'import os' + [char]10 + 'TOTAL = 3' + [char]10 + 'def go(name):' + [char]10 + '    return helper.run(name, key=TOTAL)' + [char]10
        'B.cs'    = 'namespace Demo; public class B { public int Go(string name) { return name.Length; } }'
        # A SECOND C# FILE, so "only the changed one was re-read" is a claim that can fail: with one file in
        # the tree, a half that re-read everything re-read exactly one file and the count agreed anyway.
        'C.cs'    = 'namespace Demo; public class C { public int Other() { return 2; } }'
        'keep.py' = 'OTHER = 4' + [char]10
    }
}


# A tree with a real .csproj in it. That is all a compilation needs here: no `bin`, no restore, no MSBuild -
# the framework reference pack on this machine is enough to bind `string`, `object` and everything the file
# declares itself.
function New-ProjectTree([hashtable]$Files, [switch]$Shared) {
    $project = @'
<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup><TargetFramework>net10.0</TargetFramework></PropertyGroup>
</Project>
'@
    return New-CsDb ($Files + @{ 'Demo.csproj' = $project }) -Shared:$Shared
}
