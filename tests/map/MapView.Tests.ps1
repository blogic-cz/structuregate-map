<#
    `--map-view`: the deep map drawn as ONE self-contained HTML page (`rust/fbtcore/src/view/`, `src/MapView/`).

    What is held: the page is written, fetches nothing, and carries the graph the halves RESOLVED - a python call
    and import between files, a C# call bound by Roslyn, a call's caller as the innermost function around it, and a
    rust `mod` edge from the file-level map beside the database. The model the page reasons with (drill-down,
    roll-up, neighbourhood, cycles, matrix order, treemap) is pure JavaScript, and `node` runs it here.

    Its helpers are its own - `-Only MapView` runs this suite alone.
#>

$script:MapViewProject = '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework></PropertyGroup></Project>'

# The data block of a written page, as an object.
function Read-MapViewData([string]$Html) {
    $text = [System.IO.File]::ReadAllText($Html)
    $open = 'id="map-data">'
    $start = $text.IndexOf($open) + $open.Length
    $end = $text.IndexOf('</script>', $start)
    return $text.Substring($start, $end - $start).Replace('<\/', '</') | ConvertFrom-Json
}

# Whether the data holds a file edge from -> to with at least one edge of `kind` (calls, imports, refs, renders).
function Test-MapViewEdge($Data, [string]$From, [string]$To, [string]$Kind) {
    $paths = @($Data.files | ForEach-Object { $_[0] })
    $a = [array]::IndexOf($paths, $From); $b = [array]::IndexOf($paths, $To)
    $column = @{ calls = 2; imports = 3; refs = 4; renders = 5 }[$Kind]
    return [bool](@($Data.fileEdges | Where-Object { $_[0] -eq $a -and $_[1] -eq $b -and $_[$column] -gt 0 }).Count)
}

if ($script:Python) {
    Test-Case 'map view: one page, nothing fetched, the resolved edges of every half' {
        $tree = Use-Tree @{
            'app.py'       = "from lib import helper`n`ndef main():`n    def inner():`n        return helper()`n    return inner()`n"
            'lib.py'       = "def helper():`n    return 1`n"
            'A.cs'         = "namespace D; public class A { public int Go() { return new B().Run(); } }`n"
            'B.cs'         = "namespace D; public class B { public int Run() { return 2; } }`n"
            'Demo.csproj'  = $script:MapViewProject
            'src/lib.rs'   = "mod util;`npub fn go() -> u32 { util::one() }`n"
            'src/util.rs'  = "pub fn one() -> u32 { 1 }`n"
        }
        $db = Join-Path $tree 'buildmap.sqlite'
        Assert-Exit (Invoke-Gate --root $tree --ext '.py,.cs,.rs' --map --map-sqlite $db) 0
        $result = Invoke-Gate --map-view $db
        Assert-Exit $result 0
        Assert-Line $result 'file edges from'
        $html = Join-Path $tree 'buildmap.html'
        Assert-Equal (Test-Path $html) $true 'the page beside the database'
        $text = [System.IO.File]::ReadAllText($html)
        # SELF-CONTAINED: no script or stylesheet comes from anywhere but the page itself.
        Assert-Equal ($text.Contains('<script src=') -or $text.Contains('<link ')) $false 'an external script or stylesheet'
        $data = Read-MapViewData $html
        Assert-Equal (Test-MapViewEdge $data 'app.py' 'lib.py' 'calls') $true 'the python call app.py -> lib.py'
        Assert-Equal (Test-MapViewEdge $data 'app.py' 'lib.py' 'imports') $true 'the python import app.py -> lib.py'
        Assert-Equal (Test-MapViewEdge $data 'A.cs' 'B.cs' 'calls') $true 'the C# call A.cs -> B.cs (bound by Roslyn)'
        Assert-Equal (Test-MapViewEdge $data 'src/lib.rs' 'src/util.rs' 'imports') $true 'the rust mod edge, from buildmap.json'
        # THE CALLER IS THE INNERMOST FUNCTION: `helper()` is called from `inner`, not from `main` around it.
        $names = @($data.functions | ForEach-Object { $_[1] })
        $inner = [array]::IndexOf($names, 'main.inner'); $helper = [array]::IndexOf($names, 'helper')
        Assert-Equal ([bool](@($data.functionEdges | Where-Object { $_[0] -eq $inner -and $_[1] -eq $helper }).Count)) $true 'the call edge main.inner -> helper'
        $main = [array]::IndexOf($names, 'main')
        Assert-Equal ([bool](@($data.functionEdges | Where-Object { $_[0] -eq $main -and $_[1] -eq $helper }).Count)) $false 'a call edge from the outer main'
    }
}

if ($script:Python) {
    Test-Case 'map view: a map of several roots hangs from the folder that holds them all, not the first root' {
        $tree = Use-Tree @{ 'one/a.py' = "from b import go`n"; 'two/b.py' = "def go():`n    return 1`n" }
        $db = Join-Path $tree 'buildmap.sqlite'
        Assert-Exit (Invoke-Gate --root (Join-Path $tree 'one') --root (Join-Path $tree 'two') --ext .py --map-sqlite $db) 0
        Assert-Exit (Invoke-Gate --map-view $db) 0
        $data = Read-MapViewData (Join-Path $tree 'buildmap.html')
        Assert-Equal ($data.root.Replace('\', '/').TrimEnd('/')) ($tree.Replace('\', '/').TrimEnd('/')) 'the root the page names'
    }
}

Test-Case 'map view: a missing database is exit 2, and its own flags need --map-view' {
    $tree = Use-Tree @{ 'a.py' = "X = 1`n" }
    $missing = Invoke-Gate --map-view (Join-Path $tree 'none.sqlite')
    Assert-Exit $missing 2
    Assert-Line $missing 'no such database'
    $orphan = Invoke-Gate --root $tree --map-view-out (Join-Path $tree 'x.html')
    Assert-Exit $orphan 2
    Assert-Line $orphan '--map-view-out and --map-view-graph belong to --map-view'
}

# THE MODEL THE PAGE REASONS WITH, run by node over a small fixed graph:
#   app/main -> core/a -> core/b -> core/a (a cycle inside core), app/main -> util/x
$script:MapViewNode = Get-Command node -ErrorAction SilentlyContinue
if ($script:MapViewNode) {
    Test-Case 'map view: the model drills down, rolls up, finds the cycle and orders the matrix' {
        $model = Join-Path $script:Root 'src/MapView/MapView.Model.js'
        $script = @"
const M = require(process.argv[1]);
const data = { root: '/t', files: [['app/main.py','python',10,0,1],['core/a.py','python',20,0,0],['core/b.py','python',30,0,0],['util/x.py','python',5,0,0]],
  functions: [[0,'main',1,10],[1,'a',1,20],[2,'b',1,30]],
  fileEdges: [[0,1,1,0,0,0],[1,2,2,0,0,0],[2,1,0,1,0,0],[0,3,0,1,0,0]], functionEdges: [[0,1,1],[1,2,2]] };
const m = M.load(data);
const out = [];
const top = M.frontier(m, 3);
out.push('frontier ' + [...top].sort().join(','));
const rolled = M.rollup(m, top).map(e => e.from + '>' + e.to + ':' + e.total).sort();
out.push('rollup ' + rolled.join(','));
const open = M.expand(m, top, 'd:core');
out.push('expanded ' + [...open].sort().join(','));
out.push('collapsed ' + [...M.collapse(m, open, 'f:1')].sort().join(','));
out.push('cyclic ' + [...M.cyclic([...open], M.rollup(m, open))].sort().join(','));
const order = M.matrixOrder([...open], M.rollup(m, open));
const above = M.rollup(m, open).filter(e => order.indexOf(e.to) > order.indexOf(e.from)).length;
out.push('above ' + above);
out.push('hood ' + M.neighbourhood(m, 'f:0', 1, 'out').nodes.map(n => n.id + '@' + n.distance).sort().join(','));
out.push('fnhood ' + M.neighbourhood(m, 'n:0', 2, 'out').nodes.length);
out.push('capped ' + M.neighbourhood(m, 'f:0', 3, 'both', 2).truncated);
const boxes = M.squarify([{weight: 6}, {weight: 3}, {weight: 1}], 0, 0, 100, 50);
out.push('area ' + Math.round(boxes.reduce((s, b) => s + b.w * b.h, 0)));
out.push('unreached ' + [...M.unreached(m)].sort().join(','));
console.log(out.join('\n'));
"@
        $lines = @(& node -e $script $model 2>&1 | ForEach-Object { "$_" })
        $text = $lines -join "`n"
        foreach ($expected in 'frontier d:app,d:core,d:util', 'rollup d:app>d:core:1,d:app>d:util:1',
            'expanded d:app,d:util,f:1,f:2', 'collapsed d:app,d:core,d:util', 'cyclic f:1,f:2', 'above 1',
            'hood f:0@0,f:1@1,f:3@1', 'fnhood 3', 'capped true', 'area 5000', 'unreached ') {
            if (-not ($lines -contains $expected)) { throw "the model did not answer '$expected':`n$text" }
        }
    }
}
