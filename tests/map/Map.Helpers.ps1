<#
    The helpers the map suites share - `Map` and `map/MapHalves` - so each can run alone with `-Only`.
    NAMED FOR THESE SUITES: every suite is dot-sourced into ONE scope in name order, and a helper named like
    another suite's would silently replace it for every suite that sorts later.
#>

$script:MapPython = [bool]$script:Python
if (-not $script:MapPython) { Write-Host '    (no python on this machine - the python map cases are not run)' }

# THE FILE MAP OVER $Tree, checked, written INTO the tree as `m.json` (a failing case keeps it beside its sources), and
# the map read back - for a half that needs no compiler on NODE_PATH, which `Get-Map` would install.
function Invoke-TreeMap([string]$Tree, [Parameter(ValueFromRemainingArguments)][object[]]$GateArgs) {
    $result = Invoke-Gate --root $Tree @GateArgs --map --map-check --map-out (Join-Path $Tree 'm.json')
    $map = [System.IO.File]::ReadAllText((Join-Path $Tree 'm.json')) | ConvertFrom-Json
    return [pscustomobject]@{ Result = $result; Map = $map }
}

# The map as an object. --map writes a FILE rather than stdout, so every assertion about content reads it
# back the way a consumer would.
function Get-Map {
    param([Parameter(ValueFromRemainingArguments)][object[]]$GateArgs)
    $path = Join-Path ([System.IO.Path]::GetTempPath()) "sgmap-$([System.Guid]::NewGuid().ToString('N').Substring(0,8)).json"
    # NODE_PATH so the node the gate spawns can resolve a compiler, exactly as Invoke-TsGate does. Harmless
    # for a tree with no TypeScript in it, and the alternative is two nearly identical helpers.
    # `$script:MapTsModules` names ANOTHER compiler for one case - typescript@5, whose in-process parser is a
    # different code path from 7.x's native one.
    $previous = $env:NODE_PATH
    $env:NODE_PATH = if ($script:MapTsModules) { $script:MapTsModules } else { Get-TsModules }
    try { $result = Invoke-Gate @GateArgs --map --map-out $path }
    finally { $env:NODE_PATH = $previous }
    if (-not (Test-Path $path)) { throw "no map was written. Output:`n$($Result.Text)" }
    $map = Get-Content $path -Raw | ConvertFrom-Json
    Remove-Item $path -Force -ErrorAction SilentlyContinue
    return [pscustomobject]@{ Exit = $result.Exit; Lines = $result.Lines; Text = $result.Text; Map = $map }
}

# The targets of one file's imports, as a plain array. An absent entry is no imports, and `@($null).Count`
# is 1 - which would make "this file imports nothing" pass on a map that never listed the file.
function Get-Imports($Map, [string]$Rel) {
    $property = $Map.imports.PSObject.Properties | Where-Object { $_.Name -eq $Rel }
    if (-not $property) { return ,@() }
    return ,@($property.Value)
}

# What a file imports, sorted and joined - '' for none.
function Get-ImportsText($Map, [string]$Rel) {
    $listed = Get-Imports $Map $Rel
    return ($listed | Sort-Object) -join ','
}
