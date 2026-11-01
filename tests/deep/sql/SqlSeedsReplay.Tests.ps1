<#
    THE SEEDS PASS IS NOT RUN AGAIN OVER THE SAME ROWS (#145, `rust/fbtcore/src/rows/seeds/kept.rs`): it reads only the SQL
    half's rows, so an edit of a C# file says it again from the run before - `structuregate.deep.seeds_replayed` in the
    trace, the rows as they were - and an edit of a seed script runs it again.
    Its helpers are its own - `-Only SqlSeedsReplay` runs this suite alone.
#>

function Invoke-SqlSeedsReplayMap([string]$Tree) {
    $r = Invoke-Gate --root $Tree --ext '.cs,.sql' --map-sqlite (Join-Path $Tree 'map.sqlite')
    Assert-Exit $r 0
}

Test-Case 'sqlseedsreplay: a C# edit says the seeds again, a seed script edit runs them again' {
    $tree = Use-Tree @{
        'src/App.csproj' = '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework></PropertyGroup></Project>'
        'src/OrderService.cs' = "namespace Demo;`npublic class OrderService { public int Count() => 1; }`n"
        'db/Tables.sql' = "CREATE TABLE demo.Alpha (id INT NOT NULL, name NVARCHAR(10) NOT NULL);`n"
        'db/Seed.sql' = "INSERT INTO demo.Alpha (id, name) VALUES (1, N'a'), (2, N'b');`n"
    }
    $db = Join-Path $tree 'map.sqlite'
    $trace = "$db.last-run.jsonl"
    $replayed = { [bool](Select-String -LiteralPath $trace -SimpleMatch 'structuregate.deep.seeds_replayed' -Quiet) }
    $rows = "SELECT 'row=' || s.name || '|' || s.line || '|' || s.status AS r FROM sql_seeds s UNION ALL SELECT 'view=' || count(*) FROM seed_demo_Alpha"
    $seeds = { (Invoke-Gate --map-query $db --width 0 --limit 0 --sql $rows).Text }
    Invoke-SqlSeedsReplayMap $tree
    Assert-Equal (& $replayed) $false 'replayed on the first run'
    $before = & $seeds
    if (-not $before.Contains('view=2')) { throw "two seed rows in the typed view:`n$before" }
    Add-Content -LiteralPath (Join-Path $tree 'src/OrderService.cs') -Value '// demo: one comment'
    Invoke-SqlSeedsReplayMap $tree
    Assert-Equal (& $replayed) $true 'replayed after a C# edit'
    Assert-Equal (& $seeds) $before 'the seed rows after a C# edit'
    Set-Content -LiteralPath (Join-Path $tree 'db/Seed.sql') -Value "INSERT INTO demo.Alpha (id, name) VALUES (1, N'a'), (2, N'b'), (3, N'c');"
    Invoke-SqlSeedsReplayMap $tree
    Assert-Equal (& $replayed) $false 'replayed after a seed script edit'
    $after = & $seeds
    if (-not $after.Contains('view=3')) { throw "the new seed row is written:`n$after" }
}
