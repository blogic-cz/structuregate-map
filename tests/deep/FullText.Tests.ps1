<#
    NOTHING THE DEEP MAP STORES IS CUT. `calls.source` stopped at 400 characters and `arguments.source` at 200,
    with no marker, so a reader took half a call for the whole of it. And an argument's `value` is every
    literal python can read without running anything, not only a bare constant.

    Its helpers are its own - `-Only` runs this suite alone.
#>

$script:FullTextPython = [bool]$script:Python

# One value out of the deep map of a tree, read as the text after `v=`.
function Get-FullTextValue([string]$Tree, [string]$Ext, [string]$Sql) {
    $db = Join-Path $Tree 'full.sqlite'
    if (-not (Test-Path $db)) { Assert-Exit (Invoke-Gate --root $Tree --ext $Ext --map-sqlite $db) 0 }
    $r = Invoke-Gate --map-query $db --sql $Sql --width 100000
    Assert-Exit $r 0
    $line = $r.Lines | Where-Object { $_.TrimStart().StartsWith('v=') } | Select-Object -First 1
    if ($null -eq $line) { throw "no v= row for: $Sql`n$($r.Text)" }
    return $line.Trim().Substring(2)
}

$nl = [string][char]10
# A string literal of 900 characters, written out in the source.
$long = 'x' * 900

if ($script:FullTextPython) {

Test-Case 'fulltext: a python call, its arguments, its strings and its docstring are stored whole' {
    $doc = 'd' * 700
    $tree = Use-Tree @{ 'app.py' = "def go(text, other):$nl    `"`"`"$doc`"`"`"$nl    return text$nl$nl$nl" +
        "go('$long', other=('$long' + 'y'))$nl" }
    $call = "go('$long', other=('$long' + 'y'))"
    Assert-Equal (Get-FullTextValue $tree '.py' "SELECT 'v=' || length(source) FROM calls WHERE callee = 'go'") $call.Length 'calls.source'
    Assert-Equal (Get-FullTextValue $tree '.py' "SELECT 'v=' || length(source) FROM arguments WHERE keyword = 'other'") "'$long' + 'y'".Length 'arguments.source'
    Assert-Equal (Get-FullTextValue $tree '.py' "SELECT 'v=' || max(length(value)) FROM string_literals") '900' 'string_literals.value'
    Assert-Equal (Get-FullTextValue $tree '.py' "SELECT 'v=' || length(doc) FROM functions WHERE name = 'go'") '700' 'functions.doc'
    Assert-Equal (Get-FullTextValue $tree '.py' "SELECT 'v=' || max(length(source)) FROM expressions") $call.Length 'expressions.source'
}

Test-Case 'fulltext: every string an expression touches is listed, not the first twelve' {
    $parts = (1..15 | ForEach-Object { "'s$_'" }) -join ' + '
    $tree = Use-Tree @{ 'app.py' = "WORD = $parts$nl" }
    Assert-Equal (Get-FullTextValue $tree '.py' "SELECT 'v=' || json_array_length(strings) FROM expressions ORDER BY size DESC LIMIT 1") '15' 'strings'
}

Test-Case 'fulltext: an argument that is a literal python reads without running it has a value' {
    $tree = Use-Tree @{ 'app.py' = "def go(a, b, c, d):$nl    return a$nl$nl$nl" + "go(-1, (1, 2), {'k': [1]}, len)$nl" }
    $value = { param($p) Get-FullTextValue $tree '.py' "SELECT 'v=' || value FROM arguments WHERE param = '$p'" }
    Assert-Equal (& $value 'a') '-1' 'a negative number'
    Assert-Equal (& $value 'b') '(1, 2)' 'a tuple'
    Assert-Equal (& $value 'c') "{'k': [1]}" 'a dict'
    Assert-Equal (& $value 'd') '' 'a name is not a literal'
}

}

Test-Case 'fulltext: a C# call and a folded string are stored whole' {
    $csproj = '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework></PropertyGroup></Project>'
    $tree = Use-Tree @{
        'Demo.csproj' = $csproj
        'Long.cs'     = "namespace Demo;$nl" + "public static class Long$nl{$nl" +
                        "    private static readonly string Head = `"$long`";$nl" +
                        "    public static string Go(string text) => text;$nl" +
                        "    public static string Run() => Go(Head + `"$long`");$nl}$nl"
    }
    Assert-Equal (Get-FullTextValue $tree '.cs' "SELECT 'v=' || length(source) FROM calls WHERE callee = 'Go'") "Go(Head + `"$long`")".Length 'calls.source'
    Assert-Equal (Get-FullTextValue $tree '.cs' "SELECT 'v=' || length(const) || '/' || const_kind FROM arguments WHERE source LIKE 'Head +%'") '1800/folded' 'the value this pass folded'
}

Test-Case 'fulltext: a cell the display cuts says so, and --width 0 shows it whole' {
    $tree = Use-Tree @{ 'Demo.csproj' = '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework></PropertyGroup></Project>'; 'A.cs' = "namespace Demo; public static class A { }" }
    $db = Join-Path $tree 'full.sqlite'
    Assert-Exit (Invoke-Gate --root $tree --ext .cs --map-sqlite $db) 0
    $sql = "SELECT 'v=' || '" + ('y' * 100) + "' AS c"
    $cut = Invoke-Gate --map-query $db --sql $sql --width 30
    $ellipsis = '...'
    Assert-Line $cut ('v=' + ('y' * 28) + $ellipsis + '[+72 chars]')
    $whole = Invoke-Gate --map-query $db --sql $sql --width 0
    Assert-Line $whole ('v=' + ('y' * 100))
    Assert-NoLine $whole ($ellipsis + '[+')
}

Test-Case 'fulltext: --text with a term FTS5 cannot read bare searches it as a phrase, never reports a missing index' {
    $tree = Use-Tree @{ 'Demo.csproj' = '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework></PropertyGroup></Project>'; 'A.cs' = "namespace Demo; public static class A { public static string S = `"settings.app`"; }" }
    $db = Join-Path $tree 'full.sqlite'
    Assert-Exit (Invoke-Gate --root $tree --ext .cs --map-sqlite $db) 0
    $found = Invoke-Gate --map-query $db --text 'settings.app'
    Assert-Exit $found 0
    Assert-Line $found 'searched as the phrase'
    Assert-Line $found 'A.cs'
    Assert-NoLine $found 'not in this database'
}
