<#
    A python parse goes TO THE END: an expression's `shape` is its whole skeleton, however deep and however wide, and
    a call through a chain of package re-exports binds to the def at the end of it, however long. Each used to be a
    number - four levels and eight children, eight hops - and a tree past it was mapped short with nothing said.

    Its helpers are its own - `-Only PyWhole` runs this suite alone.
#>

. (Join-Path $PSScriptRoot '../nodes/NodeSql.Helpers.ps1')

$script:PyWholePython = [bool]$script:Python
if (-not $script:PyWholePython) { Write-Host '    (no python on this machine - the whole-parse cases are not run)' }

# One cell of one query over the deep map of a throwaway tree, read off its `v=` prefix.
function Get-PyWholeCell([hashtable]$Files, [string]$Sql) {
    $tree = Use-Tree $Files
    $db = Join-Path $tree 'whole.sqlite'
    Assert-Exit (Invoke-Gate --root $tree --ext .py --map-sqlite $db) 0
    $r = Invoke-Gate --map-query $db --width 0 --sql $Sql
    Assert-Exit $r 0
    $line = @($r.Lines | Where-Object { $_.TrimStart().StartsWith('v=') })
    if ($line.Count -ne 1) { throw "expected one v= row, got $($line.Count):`n$($r.Text)" }
    return $line[0].Trim().Substring(2)
}

if ($script:PyWholePython) {

$nl = [string][char]10

Test-Case 'pywhole: an expression''s shape is its WHOLE skeleton, every level and every child' {
    # A left-leaning chain of a hundred terms is ninety-nine BinOps deep; a list of twelve is twelve children wide.
    $chain = (1..100 | ForEach-Object { 'a' }) -join ' + '
    $wide = (1..12 | ForEach-Object { "$_" }) -join ', '
    $files = @{ 'deep.py' = "def go(a):" + $nl + "    total = $chain" + $nl + "    items = [$wide]" + $nl + "    return total, items" + $nl }
    $deepest = Get-PyWholeCell $files ("SELECT 'v=' || max(n) FROM (SELECT count(*) n FROM expressions__shape " +
        "WHERE role = 'k' AND value = 'BinOp' GROUP BY owner)")
    Assert-Equal $deepest '99' 'BinOps in the shape of the hundred-term chain'
    $widest = Get-PyWholeCell $files ("SELECT 'v=' || max(n) FROM (SELECT count(*) n FROM expressions__shape c " +
        "JOIN expressions__shape l ON l.owner = c.owner AND l.n = c.parent AND l.role = 'c' AND l.node = 'list' JOIN expressions__shape r ON r.owner = l.owner AND r.n = l.parent " +
        "JOIN expressions__shape rk ON rk.owner = r.owner AND rk.parent = r.n AND rk.role = 'k' WHERE rk.value = 'List' GROUP BY r.owner, r.n)")
    Assert-Equal $widest '12' 'children of the twelve-item list'
}

Test-Case 'pywhole: a call through ten package re-exports binds to the def at the end of the chain' {
    $files = @{ 'use.py' = "from p0 import run" + $nl + "def go():" + $nl + "    run()" + $nl }
    for ($i = 0; $i -lt 10; $i++) { $files["p$i/__init__.py"] = "from p$($i + 1) import run" + $nl }
    $files['p10/__init__.py'] = "from .impl import run" + $nl
    $files['p10/impl.py'] = "def run():" + $nl + "    return 1" + $nl
    $bound = Get-PyWholeCell $files "SELECT 'v=' || target_path || '>' || target_name FROM calls WHERE callee = 'run'"
    Assert-Equal $bound 'p10/impl.py>run' 'the def eleven hops away'
}

# A CYCLE ENDS THE WALK, A VISITED SET DOES - not a hop count - and what it ends on is no def: neither file writes
# `run`, they only pass it to each other. Binding the call to the `__init__` the walk stopped in is the guess
# `follow` exists to avoid, so the call stays unbound (an unbound call's target is '', never NULL).
Test-Case 'pywhole: two packages re-exporting each other still map, and a call no file defines stays unbound' {
    $files = @{
        'use.py'        = "from left import run" + $nl + "def go():" + $nl + "    run()" + $nl
        'left/__init__.py'  = "from right import run" + $nl
        'right/__init__.py' = "from left import run" + $nl
    }
    $bound = Get-PyWholeCell $files "SELECT 'v=' || count(*) || '|' || quote(target_path) || '|' || quote(target_name) FROM calls WHERE callee = 'run'"
    Assert-Equal $bound "1|''|''" 'the call into the cycle'
}

Test-Case 'pywhole: no cell of the python map is JSON text - every list and object is node rows' {
    $nl = [string][char]10
    $tree = Use-Tree @{
        'app.py' = "import os, json$nl" +
            "KINDS = ['STANDARD', 'PARTNER']$nl" +
            "HEADERS = {`"Content-Type`": `"text/plain`", `"Accept`": [1, 2]}$nl$nl" +
            "class Store:$nl    LIMIT = 3$nl$nl" +
            "    def load(self, path, mode='r', *rest, flag=True, **extra):  # noqa: E501$nl" +
            "        with open(path, mode) as handle, open(os.devnull) as other:$nl" +
            "            data = json.loads(handle.read())$nl" +
            "        try:$nl            first, *more = data[KINDS[0]]$nl" +
            "        except (KeyError, ValueError) as error:$nl            raise RuntimeError(str(error)) from error$nl" +
            "        if flag and HEADERS.get('Accept'):$nl            return {k: v for k, v in extra.items()}$nl" +
            "        return [first, more, rest]$nl$nl" +
            "Store().load('p', (1, 2), {'k': [1, {2, 3}]}, flag=False)$nl"
    }
    $db = Join-Path $tree 'whole.sqlite'
    Assert-Exit (Invoke-Gate --root $tree --ext .py --map-sqlite $db) 0
    $r = Invoke-Gate --map-query $db --width 0 --sql "SELECT 'v=' || count(*) AS v FROM _cells"
    Assert-Exit $r 0
    $tables = [int](@($r.Lines | Where-Object { $_.Trim().StartsWith('v=') })[0].Trim().Substring(2))
    if ($tables -lt 10) { throw "only $tables structured column(s) - the fixture no longer exercises the python cells" }
    $found = Get-NodeJsonCell $db
    if ($found) { throw "$($found.Count) JSON cell(s):`n$($found -join "`n")" }
}

}
