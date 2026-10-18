<#
    WHAT ONE TURN NO LONGER WAITS FOR: a host started beside another instead of after it, and a host not started at all.

    - THE ANGULAR HALF asks no node in a tree with no Angular workspace: rust walks for the markers exactly as
      `findWorkspaceRoot` does (`ts::has_workspace`), so a `.ts` edit no longer starts node twice to hear "not here".
    - THE PLAIN TYPESCRIPT HALF starts beside the file map (`payload::Early`), as python does - `PyEarly.Tests.ps1`.
    - THE POWERSHELL FILE MAP runs on beside the deep map (`script::background`) and is read last.

    Read off the run's trace. Its helpers are its own - `-Only MapOverlap` runs this suite alone.
#>

# A TypeScript 5 the PLAIN half can read - the cache the TsRows suites fill, or a fresh install there.
function Get-MapOverlapModules {
    $modules = Join-Path ([System.IO.Path]::GetTempPath()) 'sgtest-tsrows/node_modules'
    if (-not (Get-Command node -ErrorAction SilentlyContinue)) { return $null }
    if (Test-Path (Join-Path $modules 'typescript/package.json')) { return $modules }
    if (-not (Get-Command npm -ErrorAction SilentlyContinue)) { return $null }
    $cache = Split-Path $modules
    [void](New-Item -ItemType Directory -Path $cache -Force)
    # Under the cache's lock, and only if no other process filled it meanwhile: two shards installing at once
    # corrupt the prefix, and a narrower install after a wider one prunes what the wider one added.
    Use-CacheLock $cache {
        if (-not (Test-Path (Join-Path $modules 'typescript/package.json'))) { & npm install --no-save --silent --prefix $cache 'typescript@5' 2>&1 | Out-Null }
    }
    if (Test-Path (Join-Path $modules 'typescript/package.json')) { return $modules }
    return $null
}
$script:MapOverlapModules = Get-MapOverlapModules
if (-not $script:MapOverlapModules) { Write-Host '    (no node/npm-installed typescript 5 - the typescript overlap cases are not run)' }

# One `--map --map-sqlite` run over $Tree, traced; the result and its spans by name.
function Invoke-MapOverlap([string]$Tree, [string]$Ext) {
    $trace = Join-Path $Tree "trace-$([guid]::NewGuid().ToString('N').Substring(0, 6)).jsonl"
    $saved = $env:STRUCTUREGATE_TRACE
    $modules = if ($script:MapOverlapModules) { @('--ts-node-modules', $script:MapOverlapModules) } else { @() }
    try {
        $env:STRUCTUREGATE_TRACE = $trace
        $result = Invoke-Gate --root $Tree --ext $Ext --map --map-out (Join-Path $Tree 'm.json') --map-sqlite (Join-Path $Tree 'map.sqlite') @modules
    } finally {
        if ($null -eq $saved) { Remove-Item Env:STRUCTUREGATE_TRACE -ErrorAction SilentlyContinue } else { $env:STRUCTUREGATE_TRACE = $saved }
    }
    $request = [System.IO.File]::ReadAllText($trace) | ConvertFrom-Json
    Remove-Item $trace
    $spans = @{}
    foreach ($span in $request.resourceSpans[0].scopeSpans[0].spans) { $spans[$span.name] = $span }
    return [pscustomobject]@{ Result = $result; Spans = $spans }
}

function Get-MapOverlapAttribute($Span, [string]$Key) {
    $value = ($Span.attributes | Where-Object { $_.key -eq $Key }).value
    if ($null -eq $value) { return '' }
    return [string]($value.stringValue + $value.boolValue)
}

if ($script:MapOverlapModules) {

Test-Case 'mapoverlap: a .ts edit in a tree with no Angular workspace starts no node for the Angular half' {
    $tree = Use-Tree @{ 'src/a.ts' = "export const A = 1;`n"; 'src/b.ts' = "export const B = 2;`n" }
    Assert-Exit (Invoke-MapOverlap $tree '.ts').Result 0
    [System.IO.File]::AppendAllText((Join-Path $tree 'src/a.ts'), "export const C = 3;`n")
    $edited = Invoke-MapOverlap $tree '.ts'
    Assert-Exit $edited.Result 0
    Assert-Equal $edited.Spans.ContainsKey('typescript: plan (node)') $false 'no node asked for a plan'
    Assert-Equal (Get-MapOverlapAttribute $edited.Spans['deep: plain typescript'] 'structuregate.early.ts') 'used' 'the plain half, started beside the file map'
}

# THE WORKSPACE SEARCH HAS NO DEPTH: rust walks for the marker breadth first to the bottom of the tree, as node's
# `findWorkspaceRoot` does, and the SHALLOWEST marked folder is the workspace. Which one rust took is read off the
# plain half's rows: it maps every script file OUTSIDE the workspace, so the shallow workspace's file is no row and the
# deep one's - a folder with a marker of its own, but not the workspace - is.
Test-Case 'mapoverlap: a workspace is found at any depth, and the shallowest marker wins - as node walks' {
    $deep = Use-Tree @{ 'src/a.ts' = "export const A = 1;`n"; 'a/b/c/d/e/f/angular.json' = '{"projects":{}}' }
    Assert-Equal (Invoke-MapOverlap $deep '.ts').Spans.ContainsKey('typescript: plan (node)') $true 'node asked, with the marker six down'
    $both = Use-Tree @{
        'src/a.ts' = "export const A = 1;`n"
        'w/w.ts' = "export const W = 1;`n"; 'w/angular.json' = '{"projects":{}}'
        'a/b/c/d/e/f/d.ts' = "export const D = 1;`n"; 'a/b/c/d/e/f/angular.json' = '{"projects":{}}'
    }
    $run = Invoke-MapOverlap $both '.ts'
    Assert-Exit $run.Result 0
    Assert-Equal $run.Spans.ContainsKey('typescript: plan (node)') $true 'node asked, with two markers'
    $r = Invoke-Gate --map-query (Join-Path $both 'map.sqlite') --width 0 --sql "SELECT 'p=' || group_concat(path, ',' ORDER BY path) AS p FROM files"
    Assert-Exit $r 0
    $paths = @($r.Lines | Where-Object { $_.Trim().StartsWith('p=') })[0].Trim()
    Assert-Equal $paths 'p=a/b/c/d/e/f/d.ts,src/a.ts' 'the plain half mapped everything outside the shallow workspace'
}

}

if (Get-Command python3, python -ErrorAction SilentlyContinue) {

Test-Case 'mapoverlap: the PowerShell file map runs on beside the deep map, and its rows are all there' {
    $tree = Use-Tree @{
        'lib/One.ps1' = "function Get-One { 1 }`n"
        'run.ps1'     = ". `$PSScriptRoot/lib/One.ps1`nGet-One`n"
        'x.py'        = "def one():`n    return 1`n"
    }
    $run = Invoke-MapOverlap $tree '.ps1,.py'
    Assert-Exit $run.Result 0
    $powershell = $run.Spans['map: powershell']
    $deep = $run.Spans['deep map']
    Assert-Equal ([decimal]$deep.startTimeUnixNano -lt [decimal]$powershell.endTimeUnixNano) $true 'the deep map began before PowerShell answered'
    $map = [System.IO.File]::ReadAllText((Join-Path $tree 'm.json'))
    Assert-Equal ($map.Contains('Get-One')) $true 'the PowerShell answer is in the map'
}

}
