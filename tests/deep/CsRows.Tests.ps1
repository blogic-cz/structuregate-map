<#
    --map-sqlite over C#: the DEEP map of a Roslyn tree, and the one database it shares with python.

    WHAT IS ASSERTED IS A ROW. This half exists so a question about an EXPRESSION can be asked at all, and
    every claim it makes - what a call is called, what an expression reads, which member a row sits in -
    is acted on directly, so the cases open the database and check the cells.

    The rows are read by Roslyn IN THE EXE and stored by python's own `sqlite3`, so these need a `python`
    on PATH and are skipped with a printed line rather than silently when there is none.

    THE HELPERS HERE ARE NAMED FOR THIS SUITE. Every suite is dot-sourced into ONE scope in name order, so
    a `New-Db` here would silently replace the one the python suite defines for every suite that sorts
    later - which is how three cases in other suites once broke over a helper they never called.
#>

. (Join-Path $PSScriptRoot 'cs/CsRows.Helpers.ps1')

# A file with one of everything the half reads, so a case can assert on the table it cares about.
function Get-CsSample {
    return @'
using System.Text;
using static System.Math;
using Shorthand = System.Collections.Generic.List<string>;

namespace Demo.Core;

/// <summary>Reads a thing.</summary>
public sealed class Reader : IReader
{
    public const string Prefix = "api/v1";
    private static readonly string Root = Prefix + Defaults.Suffix;

    [Route("api/x")]
    public string Load(string name, int size = 10)
    {
        if (name.Length > size) { throw new ArgumentException("too long"); }
        var built = Registry.Resolve(name, key: size);
        return built + Root;
    }
}
'@
}

if ($script:CsRowsPython) {

Test-Case 'csrows: a call is a row, with its callee, its scope and its own source' {
    $db = New-CsDb @{ 'Reader.cs' = Get-CsSample }
    $r = Invoke-CsQ $db --sql "SELECT callee, cls, func, args, kwargs, source FROM calls WHERE callee = 'Registry.Resolve'"
    Assert-Line $r 'Registry.Resolve'
    # SCOPE IS RECORDED, NOT INFERRED: the row says which type and which member it sits in.
    Assert-Line $r 'Reader'
    Assert-Line $r 'Load'
    # An argument passed BY NAME is counted the way the python half counts a keyword argument.
    Assert-Equal (Get-CsScalar $db "SELECT kwargs FROM calls WHERE callee = 'Registry.Resolve'") '1' 'one argument by name'
}

Test-Case 'csrows: `new Thing()` is a call OF Thing, the way a python class call is' {
    # Both languages answer "who constructs this" with one query, and a row that said `new` would join to
    # nothing.
    $db = New-CsDb @{ 'Reader.cs' = Get-CsSample }
    Assert-Equal (Get-CsScalar $db "SELECT count(*) FROM calls WHERE callee = 'ArgumentException'") '1' 'the construction is a call of the type'
}

Test-Case 'csrows: an expression carries what it READS and CALLS, so a query never re-parses it' {
    $db = New-CsDb @{ 'Reader.cs' = Get-CsSample }
    # The READS COLUMN ALONE, because `calls` carries the same name for this expression: a case that asked
    # for both passed while the chain was not being read at all.
    $r = Invoke-CsQ $db --sql "SELECT reads FROM expressions WHERE source LIKE '%Registry.Resolve(%'"
    # EVERY PREFIX OF A CHAIN IS A READ, which is what makes "who reads this" answerable.
    Assert-Line $r 'Registry.Resolve'
    Assert-Line $r 'name'
}

Test-Case 'csrows: the tables a C# tree produces' {
    $db = New-CsDb @{ 'Reader.cs' = Get-CsSample }
    foreach ($pair in @(@('imports', 'System.Text'), @('consts', 'Prefix'), @('exports', 'Reader'),
                        @('classes', 'IReader'), @('functions', 'Load'), @('decorators', 'Route'),
                        @('parameters', 'size'), @('branches', 'if'), @('raises', 'ArgumentException'),
                        @('returns', 'built'), @('string_literals', 'api/v1'), @('assignments', 'built'))) {
        $r = Invoke-CsQ $db --sql "SELECT * FROM $($pair[0])"
        Assert-Line $r $pair[1]
    }
}

Test-Case 'csrows: a static readonly field reads the constant it is built from' {
    # `static readonly` is how C# writes what python writes as a module-level capital name. Without the row,
    # a constant used only to build another one looks dead.
    $db = New-CsDb @{ 'Reader.cs' = Get-CsSample }
    $r = Invoke-CsQ $db --sql "SELECT reads FROM consts WHERE name = 'Root'"
    Assert-Line $r 'Prefix'
    # AND THE CHAIN IT IS BUILT WITH, written out: `Defaults.Suffix` is a read of `Defaults` and of
    # `Defaults.Suffix`, and the second is the one a query for that constant asks for.
    Assert-Line $r 'Defaults.Suffix'
}

Test-Case 'csrows: the three ways of writing a using are told apart by kind' {
    $db = New-CsDb @{ 'Reader.cs' = Get-CsSample }
    Assert-Equal (Get-CsScalar $db "SELECT count(*) FROM imports WHERE kind = 'static' AND module = 'System.Math'") '1' 'a using static'
    Assert-Equal (Get-CsScalar $db "SELECT count(*) FROM imports WHERE kind = 'alias' AND alias = 'Shorthand'") '1' 'an alias'
}

Test-Case 'csrows: an expression-bodied member is a RETURN' {
    # A tree written in `=>` members would otherwise have an empty `returns` table, which reads as a
    # codebase whose methods return nothing.
    $db = New-CsDb @{ 'A.cs' = 'public class A { public int Go() => Store.Count; }' }
    Assert-Line (Invoke-CsQ $db --sql 'SELECT source, reads FROM returns') 'Store.Count'
}

Test-Case 'csrows: a qualname carries the namespace, so two Program classes do not collide' {
    $db = New-CsDb @{
        'One.cs' = 'namespace A.One; public class Program { public void Go() { } }'
        'Two.cs' = 'namespace A.Two; public class Program { public void Go() { } }'
    }
    Assert-Equal (Get-CsScalar $db "SELECT count(*) FROM functions WHERE qualname = 'A.One.Program.Go'") '1' 'the first'
    Assert-Equal (Get-CsScalar $db "SELECT count(*) FROM functions WHERE qualname = 'A.Two.Program.Go'") '1' 'the second'
}

Test-Case 'csrows: the source is carried, FTS5-indexed, and --cat prints it' {
    # The file-level map POINTS at files and never carries them. This is the half that does.
    $db = New-CsDb @{ 'A.cs' = 'public class A { public string Go() { return "a distinctive phrase"; } }' }
    Assert-Line (Invoke-CsQ $db --text 'distinctive') 'A.cs'
    Assert-Line (Invoke-CsQ $db --cat 'A.cs' --lines 1-1) 'distinctive phrase'
}

Test-Case 'csrows: the deep C# half accounts for every file it was given' {
    # It answers in TABLES, not per file, so the guarantee the mapping halves get from a record per file is
    # checked the only way it can be here: against the count on its DONE line.
    $tree = Use-Tree @{
        'A.cs' = 'public class A { public int Go() { return 1; } }'
        'B.cs' = 'public class B { public int Go() { return 2; } }'
    }
    $result = Invoke-Gate --root $tree --ext .cs --map --map-out (Join-Path $tree 'm.json') `
        --map-sqlite (Join-Path $tree 'map.sqlite') --map-check
    Assert-Exit $result 0
    Assert-NoLine $result 'said nothing about the rest'
    Assert-NoLine $result 'HALF'
}

}

# ---------------------------------------------------------------------------------------------------
# ONE DATABASE, TWO EXTRACTORS: neither half may drop the other's rows
# ---------------------------------------------------------------------------------------------------


function Invoke-MixedMap([string]$Tree) {
    # QUOTED: PowerShell parses a bare `.py,.cs` as an ARRAY, and the harness flattens an array into one
    # argument per element - so the gate received `--ext .py` and a stray `.cs` it printed its usage over.
    return Invoke-Gate --root $Tree --ext '.py,.cs' --map --map-out (Join-Path $Tree 'm.json') `
        --map-sqlite (Join-Path $Tree 'map.sqlite')
}

if ($script:CsRowsPython) {

Test-Case 'mixed: one database holds both languages and each file says which half wrote it' {
    $tree = New-MixedTree
    Assert-Exit (Invoke-MixedMap $tree) 0
    $db = Join-Path $tree 'map.sqlite'
    Assert-Equal (Get-CsScalar $db "SELECT count(*) FROM files WHERE lang = 'python'") '2' 'the python files'
    Assert-Equal (Get-CsScalar $db "SELECT count(*) FROM files WHERE lang = 'csharp'") '2' 'the C# files'
}

Test-Case 'mixed: the C# half does not take the python rows with it' {
    # Unscoped, the second half reads the first half's files as deleted and drops every row it wrote - and
    # a run that mapped both languages ends with a database holding one.
    $tree = New-MixedTree
    Assert-Exit (Invoke-MixedMap $tree) 0
    $db = Join-Path $tree 'map.sqlite'
    Assert-Equal (Get-CsScalar $db "SELECT count(*) FROM consts WHERE name = 'TOTAL'") '1' 'the python const survived'
    Assert-Equal (Get-CsScalar $db "SELECT count(*) FROM calls WHERE callee = 'helper.run'") '1' 'the python call survived'
    Assert-Equal (Get-CsScalar $db "SELECT count(*) FROM functions WHERE name = 'Go'") '1' 'the C# member is there too'
}

Test-Case 'mixed: no row id is handed out twice across the two halves' {
    # The ids are continued from what the database recorded, not restarted per half: two rows sharing one id
    # is a join that silently pulls the wrong row.
    $tree = New-MixedTree
    Assert-Exit (Invoke-MixedMap $tree) 0
    $db = Join-Path $tree 'map.sqlite'
    Assert-Equal (Get-CsScalar $db 'SELECT count(*) FROM (SELECT id FROM files GROUP BY id HAVING count(*) > 1)') '0' 'no duplicate file ids'
    Assert-Equal (Get-CsScalar $db 'SELECT count(*) FROM (SELECT id FROM functions GROUP BY id HAVING count(*) > 1)') '0' 'no duplicate function ids'
}

Test-Case 'mixed: a column one language has and the other does not survives both' {
    # The table takes its shape from whichever half wrote it first; a `kind` the python `imports` row does
    # not carry would otherwise be dropped on every insert, and the row would land missing the one field the
    # query was written for.
    $tree = New-MixedTree
    Assert-Exit (Invoke-MixedMap $tree) 0
    # The python half wrote `imports` FIRST, and its rows have no `kind` at all - so the column has to be
    # added to the table rather than dropped from the row that carries it.
    Assert-Equal (Get-CsScalar (Join-Path $tree 'map.sqlite') 'SELECT count(*) FROM imports') '1' 'only the python import so far'
    [System.IO.File]::WriteAllText((Join-Path $tree 'B.cs'),
        'using System.Text;' + [char]10 + 'namespace Demo; public class B { public int Go() { return 1; } }')
    Assert-Exit (Invoke-MixedMap $tree) 0
    Assert-Equal (Get-CsScalar (Join-Path $tree 'map.sqlite') "SELECT count(*) FROM imports WHERE kind = 'using' AND module = 'System.Text'") '1' 'the C# kind landed'
}

Test-Case 'incremental C#: only the CHANGED file is re-read' {
    $tree = New-MixedTree
    Assert-Exit (Invoke-MixedMap $tree) 0
    [System.IO.File]::WriteAllText((Join-Path $tree 'B.cs'),
        'namespace Demo; public class B { public int Go(string name) { return name.Length + 1; } }')
    $result = Invoke-MixedMap $tree
    Assert-Exit $result 0
    Assert-Line $result '1 file(s) re-read'
}

Test-Case 'incremental C#: a run where no C# file moved opens no session, and one that re-reads says which and why' {
    # THE DEEP MAP ALONE, which prints its notes - what a per-turn hook runs.
    $tree = New-MixedTree
    $deep = @('--root', $tree, '--ext', '.py,.cs', '--map-sqlite', (Join-Path $tree 'map.sqlite'))
    Assert-Exit (Invoke-Gate @deep) 0
    $quiet = Invoke-Gate @deep
    Assert-Exit $quiet 0
    Assert-Line $quiet 'the deep C# half had nothing to do: none of its 2 file(s) moved'
    [System.IO.File]::WriteAllText((Join-Path $tree 'C.cs'), 'namespace Demo; public class C { public int Other() { return 3; } }')
    $edited = Invoke-Gate @deep
    Assert-Exit $edited 0
    Assert-Line $edited 'the deep C# half re-reads 1 file(s): 1 whose content moved - e.g. C.cs (whose content moved)'
}

Test-Case 'incremental C#: a changed file REPLACES its rows, it does not add to them' {
    $tree = New-MixedTree
    Assert-Exit (Invoke-MixedMap $tree) 0
    $db = Join-Path $tree 'map.sqlite'
    Assert-Equal (Get-CsScalar $db "SELECT count(*) FROM functions WHERE name = 'Go'") '1' 'the member is there once'
    [System.IO.File]::WriteAllText((Join-Path $tree 'B.cs'),
        'namespace Demo; public class B { public int Renamed(string name) { return name.Length; } }')
    Assert-Exit (Invoke-MixedMap $tree) 0
    Assert-Equal (Get-CsScalar $db "SELECT count(*) FROM functions WHERE name = 'Go'") '0' 'the old row is gone'
    Assert-Equal (Get-CsScalar $db "SELECT count(*) FROM functions WHERE name = 'Renamed'") '1' 'the new one is there'
}

Test-Case 'incremental C#: touch and restore returns the tree to exactly the rows it started with' {
    $tree = New-MixedTree
    Assert-Exit (Invoke-MixedMap $tree) 0
    $db = Join-Path $tree 'map.sqlite'
    $start = Get-CsScalar $db 'SELECT count(*) FROM expressions'
    $original = [System.IO.File]::ReadAllText((Join-Path $tree 'B.cs'))
    [System.IO.File]::WriteAllText((Join-Path $tree 'B.cs'), $original + '// a touch' + [char]10)
    Assert-Exit (Invoke-MixedMap $tree) 0
    [System.IO.File]::WriteAllText((Join-Path $tree 'B.cs'), $original)
    Assert-Exit (Invoke-MixedMap $tree) 0
    Assert-Equal (Get-CsScalar $db 'SELECT count(*) FROM expressions') $start 'back to where it started'
}

Test-Case 'incremental C#: the LAST C# file deleted takes its rows with it and leaves python alone' {
    # A tree with no C# left in it is not a tree with nothing to do: the rows for the file that just left
    # are still in there, and rows describing a file the tree no longer has look real.
    $tree = New-MixedTree
    Assert-Exit (Invoke-MixedMap $tree) 0
    $db = Join-Path $tree 'map.sqlite'
    Remove-Item (Join-Path $tree 'B.cs') -Force
    Remove-Item (Join-Path $tree 'C.cs') -Force
    Assert-Exit (Invoke-MixedMap $tree) 0
    Assert-Equal (Get-CsScalar $db "SELECT count(*) FROM files WHERE lang = 'csharp'") '0' 'the C# file left'
    Assert-Equal (Get-CsScalar $db "SELECT count(*) FROM functions WHERE name = 'Go'") '0' 'and its rows with it'
    Assert-Equal (Get-CsScalar $db "SELECT count(*) FROM consts WHERE name = 'TOTAL'") '1' 'the python rows stayed'
}

}

# ---------------------------------------------------------------------------------------------------
# BATCHES: one run is several payloads, and the boundary is where the mistakes live
# ---------------------------------------------------------------------------------------------------

# The same map, with the rows forced through batches of $Rows instead of the default 100 000. A very large
# solution is what made batching necessary; a case that had to build one could not run at all.
function Invoke-BatchedMap([string]$Tree, [int]$Rows) {
    $previous = $env:STRUCTUREGATE_MAP_BATCH
    $env:STRUCTUREGATE_MAP_BATCH = "$Rows"
    try { return Invoke-MixedMap $Tree } finally { $env:STRUCTUREGATE_MAP_BATCH = $previous }
}

if ($script:CsRowsPython) {

Test-Case 'batches: a tree sent in several batches lands exactly as one batch does' {
    $one = New-MixedTree
    Assert-Exit (Invoke-MixedMap $one) 0
    $whole = Get-CsScalar (Join-Path $one 'map.sqlite') "SELECT count(*) FROM files WHERE lang = 'csharp'"
    $rows = Get-CsScalar (Join-Path $one 'map.sqlite') 'SELECT count(*) FROM functions'

    $many = New-MixedTree
    Assert-Exit (Invoke-BatchedMap $many 1) 0
    $db = Join-Path $many 'map.sqlite'
    Assert-Equal (Get-CsScalar $db "SELECT count(*) FROM files WHERE lang = 'csharp'") $whole 'every file landed'
    Assert-Equal (Get-CsScalar $db 'SELECT count(*) FROM functions') $rows 'and every row of them'
    # A LATER BATCH MUST NOT REPLACE THE DATABASE: only the first may, or each batch takes the one before
    # it with it and the last file is all that is left.
    Assert-Equal (Get-CsScalar $db "SELECT count(*) FROM consts WHERE name = 'TOTAL'") '1' 'the python rows survived them all'
    Assert-Equal (Get-CsScalar $db 'SELECT count(*) FROM (SELECT id FROM functions GROUP BY id HAVING count(*) > 1)') '0' 'no id restarted at a batch'
}

Test-Case 'batches: a batched run does not ask for a full rewrite it does not need' {
    # Every batch but the last describes a tree the database does not hold all of yet. Verified per batch,
    # that reads as a cache disagreeing with the tree, and the run rewrites itself every single time.
    # --map-check, because that is what PRINTS the findings: the rewrite says so as a note, and a case
    # asserting on a line the run never prints asserts on nothing.
    $tree = New-MixedTree
    $previous = $env:STRUCTUREGATE_MAP_BATCH
    $env:STRUCTUREGATE_MAP_BATCH = '1'
    try {
        $result = Invoke-Gate --root $tree --ext '.py,.cs' --map --map-out (Join-Path $tree 'm.json') `
            --map-sqlite (Join-Path $tree 'map.sqlite') --map-check
    } finally { $env:STRUCTUREGATE_MAP_BATCH = $previous }
    Assert-Exit $result 0
    Assert-NoLine $result 'written again in full'
    Assert-NoLine $result 'HALF'
}

}

# ---------------------------------------------------------------------------------------------------
# THE SEMANTIC MODEL: what the compiler knows and the text does not
# ---------------------------------------------------------------------------------------------------

function Get-SemanticSample {
    return @'
namespace Demo;

public static class Mail
{
    public const string Prefix = "api/";
    public const string Route = Prefix + "send";

    public static void Send(string address, bool retry, bool silent) { }
    public static void Send(int id) { }

    public static void Go()
    {
        Send("a@example.com", true, false);
        Send(3);
        var total = Route.Length + 1;
    }
}
'@
}

if ($script:CsRowsPython) {

Test-Case 'semantic: a call carries the symbol it binds to, and the overload it picked' {
    # `Send` is one string in the text and two methods in the type. A map that cannot tell them apart
    # answers "who calls Send" with both, forever.
    $db = New-ProjectTree @{ 'Mail.cs' = Get-SemanticSample }
    Assert-Equal (Get-CsScalar $db "SELECT count(*) FROM calls WHERE symbol = 'Demo.Mail.Send'") '2' 'both calls bound'
    Assert-Equal (Get-CsScalar $db "SELECT count(*) FROM calls WHERE signature = 'Demo.Mail.Send(string, bool, bool)'") '1' 'the three-argument overload'
    Assert-Equal (Get-CsScalar $db "SELECT count(*) FROM calls WHERE signature = 'Demo.Mail.Send(int)'") '1' 'and the other one'
}

Test-Case 'semantic: an argument carries the parameter name the CALLEE declares' {
    # `Send("a@example.com", true, false)` is three values and no meaning until the signature is in hand. This is
    # the row no parse tree can produce, and the reason the semantic pass is worth its cost.
    $db = New-ProjectTree @{ 'Mail.cs' = Get-SemanticSample }
    Assert-Equal (Get-CsScalar $db "SELECT count(*) FROM arguments WHERE name = 'retry' AND source = 'true'") '1' 'retry is named'
    # AND THE FOLDED VALUE IS SPELLED AS C# SPELLS IT: `True` is the .NET ToString of a bool and appears in
    # no C# file, so a query joining `const` back to source would find nothing.
    Assert-Equal (Get-CsScalar $db "SELECT count(*) FROM arguments WHERE name = 'retry' AND const = 'true'") '1' 'folded as true, not True'
    Assert-Equal (Get-CsScalar $db "SELECT count(*) FROM arguments WHERE name = 'silent' AND source = 'false'") '1' 'and silent'
    Assert-Line (Invoke-CsQ $db --sql "SELECT name, type FROM arguments WHERE source = '3'") 'id'
}

Test-Case 'semantic: a const is folded to what it actually holds' {
    # `Prefix + "send"` is two names and an operator in the text. The value is what somebody searching for a
    # route is looking for.
    $db = New-ProjectTree @{ 'Mail.cs' = Get-SemanticSample }
    Assert-Equal (Get-CsScalar $db "SELECT count(*) FROM consts WHERE name = 'Route' AND value = 'api/send'") '1' 'folded'
}

Test-Case 'semantic: an expression carries its static type' {
    # `var` hides the type in the text, and a query about a type cannot do without it. A written-out name
    # (`Route.Length`) is deliberately NOT an expression row - it is already carried as a read - so the row
    # asserted on here is the arithmetic around it.
    $db = New-ProjectTree @{ 'Mail.cs' = Get-SemanticSample }
    Assert-Line (Invoke-CsQ $db --sql "SELECT type FROM expressions WHERE source = 'Route.Length + 1'") 'int'
    Assert-Line (Invoke-CsQ $db --sql "SELECT type FROM expressions WHERE source LIKE '%Prefix + %'") 'string'
}

Test-Case 'semantic: a file says which project compiled it, and whether it was bound at all' {
    $db = New-ProjectTree @{ 'Mail.cs' = Get-SemanticSample }
    Assert-Equal (Get-CsScalar $db "SELECT count(*) FROM files WHERE path = 'Mail.cs' AND project = 'Demo' AND semantic = 1") '1' 'bound by Demo'
}

Test-Case 'semantic: a file under no project is still mapped, and SAYS it was not bound' {
    # A .cs the build does not compile either - a script, a sample, a leftover - keeps every syntax row. The
    # column is what stops a reader taking an empty `symbol` for "this call resolves to nothing".
    $db = New-CsDb @{ 'Loose.cs' = 'namespace Demo; public class Loose { public int Go() { return Helper.Run(1); } }' }
    Assert-Equal (Get-CsScalar $db "SELECT count(*) FROM files WHERE semantic = 0") '1' 'not bound'
    Assert-Equal (Get-CsScalar $db "SELECT count(*) FROM calls WHERE callee = 'Helper.Run'") '1' 'and the call row is there anyway'
    Assert-Equal (Get-CsScalar $db "SELECT count(*) FROM calls WHERE symbol <> ''") '0' 'with nothing resolved'
}

}
