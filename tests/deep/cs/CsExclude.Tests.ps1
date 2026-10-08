<#
    `--map-exclude`: a C# file the consumer asked to keep out of the deep map is LISTED and never walked - still
    compiled, so every other file binds against it - and taking the pattern away brings its rows back.

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
