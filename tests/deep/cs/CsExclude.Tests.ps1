<#
    `--map-exclude`: a C# file the consumer asked to keep out of the deep map is LISTED and never walked - still
    compiled, so every other file binds against it - and taking the pattern away brings its rows back. And a run that
    would write every row again (every C# file read again, no TypeScript to keep) starts on an EMPTY database
    (`mapper/deep/fresh.rs`) - a map of 50 files or more: deleting a large map's rows half by half took longer than a first build.

    Its helpers are its own - `-Only CsExclude` runs this suite alone.
#>

$script:CsExcludeProject = '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework></PropertyGroup></Project>'

function Get-CsExcludeCell([string]$Db, [string]$Sql) {
    $r = Invoke-Gate --map-query $Db --width 0 --sql $Sql
    Assert-Exit $r 0
    return ($r.Lines | Where-Object { $_ -match '^v=' } | Select-Object -First 1)
}

Test-Case 'map-exclude: a matched file keeps its files row, has no rows of its own, and others still bind to it' {
    $tree = Use-Tree @{
        'Demo.csproj' = $script:CsExcludeProject
        'Data/Migrations/Seed.cs' = "namespace Demo; public static class Seed { public static int Rows() { return Count(3); } static int Count(int n) { return n; } }`n"
        'Use.cs' = "namespace Demo; public class Use { public int Go() { return Seed.Rows(); } }`n"
    }
    $db = Join-Path $tree 'map.sqlite'
    $call = @('--root', $tree, '--ext', '.cs', '--map-sqlite', $db)
    Assert-Exit (Invoke-Gate @call --map-exclude '**/Migrations/*.cs') 0
    $seed = "(SELECT id FROM files WHERE path = 'Data/Migrations/Seed.cs')"
    Assert-Equal (Get-CsExcludeCell $db "SELECT 'v=' || excluded FROM files WHERE path = 'Data/Migrations/Seed.cs'") 'v=1' 'the files row says why it has no rows'
    Assert-Equal (Get-CsExcludeCell $db "SELECT 'v=' || count(*) FROM calls WHERE file = $seed") 'v=0' 'nothing of the excluded file is walked'
    # STILL COMPILED: the call INTO it binds to its declaration.
    Assert-Equal (Get-CsExcludeCell $db "SELECT 'v=' || count(*) FROM calls WHERE callee LIKE '%Rows%' AND symbol LIKE '%Demo.Seed.Rows%'") 'v=1' 'a file that calls it still binds'
    # AND THE PATTERN GONE, its rows come back - the exclusion is in the file's sha, so it is re-read.
    Assert-Exit (Invoke-Gate @call) 0
    Assert-Equal (Get-CsExcludeCell $db "SELECT 'v=' || excluded FROM files WHERE path = 'Data/Migrations/Seed.cs'") 'v=0' 'no longer excluded'
    Assert-Equal (Get-CsExcludeCell $db "SELECT 'v=' || count(*) FROM calls WHERE file = $seed") 'v=1' 'its call is a row again'
}

$script:CsFreshProject = '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework></PropertyGroup></Project>'

function Get-CsFreshCell([string]$Db, [string]$Sql) {
    $r = Invoke-Gate --map-query $Db --width 0 --sql $Sql
    Assert-Exit $r 0
    return ($r.Lines | Where-Object { $_.TrimStart().StartsWith('v=') } | Select-Object -First 1).Trim()
}

# A TABLE NO HALF WRITES: it survives a run that keeps the file, and only a new file loses it. Written by python's
# sqlite3 - the gate's own query is read-only.
function Add-CsFreshMarker([string]$Db) {
    $python = Get-Command python3 -ErrorAction SilentlyContinue
    if (-not $python) { $python = Get-Command python -ErrorAction SilentlyContinue }
    if (-not $python) { throw 'no python to write the marker table with' }
    & $python.Source -I -c "import sqlite3,sys; c=sqlite3.connect(sys.argv[1]); c.execute('CREATE TABLE fresh_marker (x)'); c.commit()" $Db
    if ($LASTEXITCODE -ne 0) { throw 'the marker table was not written' }
}

Test-Case 'csfresh: every row about to be written again starts on an empty database, a run keeping rows keeps it' {
    # A MAP LARGE ENOUGH: a few files' rows go in seconds either way, and the empty database is kept for real maps.
    $files = @{ 'Demo.csproj' = $script:CsFreshProject; 'Use.cs' = "namespace Demo; public class Use { public int Go() { return Count(3); } static int Count(int n) { return n; } }`n" }
    foreach ($i in 1..50) { $files["Part$i.cs"] = "namespace Demo; public class Part$i { public int N() { return $i; } }`n" }
    $tree = Use-Tree $files
    $db = Join-Path $tree 'map.sqlite'
    $call = @('--root', $tree, '--ext', '.cs', '--map-sqlite', $db)
    Assert-Exit (Invoke-Gate @call) 0
    Add-CsFreshMarker $db
    # NOTHING MOVED: the database is kept, the marker with it.
    $kept = Invoke-Gate @call
    Assert-Exit $kept 0
    Assert-NoLine $kept 'starts on an empty database'
    Assert-Equal (Get-CsFreshCell $db "SELECT 'v=' || count(*) FROM sqlite_master WHERE name = 'fresh_marker'") 'v=1' 'a run keeping rows keeps the file'
    # EVERY C# FILE READ AGAIN, no TypeScript to keep: a new file, rows and all, the marker gone.
    $fresh = Invoke-Gate @call --map-reread csharp
    Assert-Exit $fresh 0
    Assert-Line $fresh 'starts on an empty database: 51 of 51 C# file(s) moved'
    Assert-Equal (Get-CsFreshCell $db "SELECT 'v=' || count(*) FROM sqlite_master WHERE name = 'fresh_marker'") 'v=0' 'the old file is gone'
    Assert-Equal (Get-CsFreshCell $db "SELECT 'v=' || count(*) FROM calls WHERE callee = 'Count'") 'v=1' 'the rows are written again'
}
