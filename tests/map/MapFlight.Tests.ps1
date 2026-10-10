<#
    THE TYPESCRIPT HALF IN FLIGHT (`rust/fbtcore/src/mapper/deep/flight.rs`): node parses on a thread of its own while
    python, rust and C# store, and its rows are stored after theirs. On a large tree node's parse and Roslyn's compile
    were minutes each, one waiting for the other, and the python host started beside the file map was thrown away
    whenever the TypeScript half had stored first.

    WHAT IS HELD: the flight numbers in a LANE above the counters it was handed, so no id the halves beside it hand
    out meanwhile is one of its own - and the rows are the ones a run that was not flown writes.

    Read off the run's trace and the database. Its helpers are its own - `-Only MapFlight` runs this suite alone.
#>

. (Join-Path $PSScriptRoot '../deep/TsRows.Helpers.ps1')

# One `--map --map-sqlite` run over $Tree, traced: the result, and every span as { name, attributes by key }.
function Invoke-MapFlight([string]$Tree) {
    $trace = Join-Path $Tree "trace-$([guid]::NewGuid().ToString('N').Substring(0, 6)).jsonl"
    $saved = $env:STRUCTUREGATE_TRACE
    try {
        $env:STRUCTUREGATE_TRACE = $trace
        $result = Invoke-Gate --root $Tree --ext '.ts,.py,.cs' --map --map-out (Join-Path $Tree 'm.json') `
            --map-sqlite (Join-Path $Tree 'map.sqlite') --ts-node-modules $script:TsRowsModules
    } finally {
        if ($null -eq $saved) { Remove-Item Env:STRUCTUREGATE_TRACE -ErrorAction SilentlyContinue } else { $env:STRUCTUREGATE_TRACE = $saved }
    }
    $request = [System.IO.File]::ReadAllText($trace) | ConvertFrom-Json
    Remove-Item $trace
    $spans = foreach ($span in $request.resourceSpans[0].scopeSpans[0].spans) {
        $facts = @{}
        foreach ($a in $span.attributes) { $facts[$a.key] = [string]($a.value.stringValue + $a.value.boolValue + $a.value.intValue) }
        [pscustomobject]@{ Name = $span.name; Facts = $facts }
    }
    return [pscustomobject]@{ Result = $result; Spans = @($spans) }
}

# The first number a `--map-query --sql` prints.
function Get-MapFlightNumber([string]$Tree, [string]$Sql) {
    $r = Invoke-Gate --map-query (Join-Path $Tree 'map.sqlite') --sql $Sql
    foreach ($line in $r.Lines) {
        $trimmed = $line.Trim()
        if ($trimmed.Length -gt 0 -and ($trimmed.ToCharArray() | Where-Object { -not [char]::IsDigit($_) }).Count -eq 0) { return $trimmed }
    }
    return ''
}

# A workspace beside python and C#: every half the flight runs beside has a file to store.
function New-MapFlightTree {
    return New-TsRowsWorkspace @{
        'tools/x.py'   = "def one():`n    return 1`n"
        'api/Order.cs' = "namespace Demo;`npublic class Order { public int Id; }`n"
    }
}

if ($script:TsRowsModules -and $script:TsRowsPython) {

Test-Case 'mapflight: a .ts and a .py edit in one run - node parses beside python, whose early answer is used, and no id is handed out twice' {
    $tree = New-MapFlightTree
    Assert-Exit (Invoke-MapFlight $tree).Result 0
    # A NEW FILE IN EVERY HALF: each takes file ids, and the TypeScript ones were numbered before python's were stored.
    [System.IO.File]::WriteAllText((Join-Path $tree 'apps/shop/src/extra.ts'), "export const extra = 2;`n")
    [System.IO.File]::WriteAllText((Join-Path $tree 'tools/z.py'), "def four():`n    return 4`n")
    [System.IO.File]::WriteAllText((Join-Path $tree 'api/Line.cs'), "namespace Demo;`npublic class Line { public Order Owner; }`n")
    $both = Invoke-MapFlight $tree
    Assert-Exit $both.Result 0
    $flown = @($both.Spans | Where-Object { $_.Name -eq 'deep: typescript' -and $_.Facts['structuregate.flight'] -eq 'parse' })
    Assert-Equal $flown.Count 1 'the TypeScript half flown'
    $python = @($both.Spans | Where-Object { $_.Name -eq 'deep: python' })[0]
    Assert-Equal $python.Facts['structuregate.early.python'] 'used' 'the early python answer, with node parsing beside it'
    Assert-NoLine $both.Result 'disagreed'
    Assert-NoLine $both.Result 'parses again'
    Assert-Equal (Get-MapFlightNumber $tree 'SELECT count(*) FROM (SELECT id FROM files GROUP BY id HAVING count(*) > 1)') '0' 'no file id twice'
    Assert-Equal (Get-MapFlightNumber $tree ("SELECT count(*) FROM files WHERE (path = 'apps/shop/src/extra.ts' AND lang = 'typescript') " +
        "OR (path = 'tools/z.py' AND lang = 'python') OR (path = 'api/Line.cs' AND lang = 'csharp')")) '3' 'every new file recorded'
}

Test-Case 'mapflight: the rows a flown run stores are the rows a run in turn stores' {
    $tree = New-MapFlightTree
    Assert-Exit (Invoke-MapFlight $tree).Result 0
    [System.IO.File]::AppendAllText((Join-Path $tree 'apps/shop/src/helpers.ts'), "export const more = 2;`n")
    [System.IO.File]::AppendAllText((Join-Path $tree 'tools/x.py'), "`ndef two():`n    return 2`n")
    $flown = Invoke-MapFlight $tree
    Assert-Exit $flown.Result 0
    Assert-Equal @($flown.Spans | Where-Object { $_.Facts['structuregate.flight'] -eq 'parse' }).Count 1 'the TypeScript half flown'
    $count = "SELECT group_concat(n, ',') FROM (SELECT lang || '=' || count(*) AS n FROM files GROUP BY lang ORDER BY lang)"
    $tables = "SELECT (SELECT count(*) FROM functions) * 1000000 + (SELECT count(*) FROM calls)"
    $flownFiles = (Invoke-TsRowsQ (Join-Path $tree 'map.sqlite') $count).Lines -join "`n"
    $flownRows = Get-MapFlightNumber $tree $tables
    # IN TURN: a database rebuilt has no counters to lane, so the half runs first, as it always did.
    Remove-Item (Join-Path $tree 'map.sqlite')
    $turn = Invoke-MapFlight $tree
    Assert-Exit $turn.Result 0
    Assert-Equal @($turn.Spans | Where-Object { $_.Facts['structuregate.flight'] -eq 'parse' }).Count 0 'a rebuild flown'
    Assert-Equal ((Invoke-TsRowsQ (Join-Path $tree 'map.sqlite') $count).Lines -join "`n") $flownFiles 'files per half'
    Assert-Equal (Get-MapFlightNumber $tree $tables) $flownRows 'functions and calls'
}


Test-Case 'mapflight: beside a C# half with more than 1000 files to read again, the TypeScript half runs in its turn' {
    $tree = New-MapFlightTree
    $cs = Join-Path $tree 'api/many'
    [void](New-Item -ItemType Directory -Path $cs)
    foreach ($n in 1..1001) { [System.IO.File]::WriteAllText((Join-Path $cs "C$n.cs"), "namespace Demo;`npublic class C$n {}`n") }
    Assert-Exit (Invoke-MapFlight $tree).Result 0
    foreach ($n in 1..1001) { [System.IO.File]::WriteAllText((Join-Path $cs "C$n.cs"), "namespace Demo;`npublic class C$n { public int N; }`n") }
    [System.IO.File]::WriteAllText((Join-Path $tree 'apps/shop/src/extra.ts'), "export const extra = 2;`n")
    $many = Invoke-MapFlight $tree
    Assert-Exit $many.Result 0
    Assert-Equal @($many.Spans | Where-Object { $_.Facts['structuregate.flight'] -eq 'parse' }).Count 0 'the TypeScript half flown'
    $turn = @($many.Spans | Where-Object { $_.Facts['structuregate.flight'] -eq 'in turn' })
    Assert-Equal $turn.Count 1 'the TypeScript half said to run in its turn'
    Assert-Equal $turn[0].Facts['structuregate.csharp.moving'] '1001' 'the C# files it would have run beside'
}

}
