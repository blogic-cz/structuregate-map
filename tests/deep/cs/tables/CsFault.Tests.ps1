<#
    ONE C# FILE THAT THROWS IS ONE FILE UNPARSED. A NullReferenceException in the deep C# pass escaped the
    batch: the whole C# half ended on "the deep map failed - NullReferenceException" plus a second, misleading "EOF
    while parsing" from storing an empty payload, with no file named and no other file's rows stored.

    The real trigger is a tree nobody here has, so the throw is forced: `STRUCTUREGATE_TEST_THROW_ON=<rel>` makes the
    pass throw a NullReferenceException for that one file (`src/Map/CsRows/DeepMap.cs`).

    Its helpers are its own - `-Only CsFault` runs this suite alone.
#>

$script:CsFaultProject = '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework></PropertyGroup></Project>'

function Invoke-CsFaultMap([string]$Tree, [string]$ThrowOn) {
    $saved = $env:STRUCTUREGATE_TEST_THROW_ON
    try {
        $env:STRUCTUREGATE_TEST_THROW_ON = $ThrowOn
        return Invoke-Gate --root $Tree --ext .cs --map-sqlite (Join-Path $Tree 'map.sqlite')
    } finally {
        if ($null -eq $saved) { Remove-Item Env:STRUCTUREGATE_TEST_THROW_ON -ErrorAction SilentlyContinue }
        else { $env:STRUCTUREGATE_TEST_THROW_ON = $saved }
    }
}

Test-Case 'csfault: a C# file that throws is named, and every other file is still stored' {
    $tree = Use-Tree @{
        'A.cs'        = "namespace Demo; public class A { public int Go() { return Helper(); } int Helper() => 1; }`n"
        'B.cs'        = "namespace Demo; public class B { public int Run() { return 2; } }`n"
        'Demo.csproj' = $script:CsFaultProject
    }
    $result = Invoke-CsFaultMap $tree 'B.cs'
    # THE STORE NEVER SEES AN EMPTY PAYLOAD - asked first, so a caller that does fail is still told apart from it.
    Assert-NoLine $result 'could not be read'
    Assert-NoLine $result 'the deep map failed'
    # THE FILE IS ALREADY NAMED: the store's "stored N of M" would count it a second time.
    Assert-NoLine $result 'said nothing about the rest'
    Assert-Exit $result 1
    Assert-Line $result 'UNPARSED  B.cs: the deep C# pass could not read it (NullReferenceException: thrown by STRUCTUREGATE_TEST_THROW_ON'
    $query = Invoke-Gate --map-query (Join-Path $tree 'map.sqlite') --sql "SELECT path FROM files WHERE lang = 'csharp' ORDER BY path"
    Assert-Exit $query 0
    Assert-Line $query 'A.cs'
    $calls = Invoke-Gate --map-query (Join-Path $tree 'map.sqlite') --sql "SELECT count(*) FROM calls WHERE callee = 'Helper'"
    Assert-Line $calls '1'

    # THE NEXT RUN, without the throw, reads B.cs and is green: one bad file did not leave the database broken.
    $clean = Invoke-CsFaultMap $tree ''
    Assert-Exit $clean 0
}
