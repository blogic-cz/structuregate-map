<#
    A value this pass FOLDS is the value the code has where it is read, or nothing. An audit of a large solution found
    half of the folded arguments it checked wrong: a local folded to its initializer after a later line reassigned it, a
    loop counter, a `ref` argument, and an integer `+` joined as text (`i + 1` recorded as "01").

    Its helpers are its own - `-Only` runs this suite alone.
#>

$script:CsFoldProject = '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework></PropertyGroup></Project>'

# `const/const_kind` of the argument written as `$Source` in a call to `Take`, as one cell.
function Get-CsFoldArgument([string]$Db, [string]$Source) {
    $r = Invoke-Gate --map-query $Db --width 0 --sql ("SELECT 'v=' || coalesce(a.const,'') || '/' || coalesce(a.const_kind,'') AS v " +
        "FROM arguments a JOIN calls c ON c.id = a.call WHERE c.callee = 'Take' AND a.source = '$Source'")
    Assert-Exit $r 0
    $line = @($r.Lines | Where-Object { $_.TrimStart().StartsWith('v=') })
    if ($line.Count -ne 1) { throw "expected one argument '$Source', got $($line.Count):`n$($r.Text)" }
    return $line[0].Trim().Substring(2)
}

Test-Case 'csfold: a name written after its declaration is not folded to its initializer' {
    $nl = [string][char]10
    $tree = Use-Tree @{
        'Demo.csproj' = $script:CsFoldProject
        'Fold.cs' = "namespace Demo;$nl" +
            "public class Fold$nl{$nl" +
            "    private static readonly string Fixed = `"fixed`";$nl" +
            "    private const string Suffix = `"/x`";$nl" +
            "    private static readonly int Two = 2;$nl" +
            "    private static readonly int Three = 3;$nl" +
            "    private static string Mutable = `"before`";$nl" +
            "    private readonly string set = `"initial`";$nl" +
            "    public Fold() { set = `"ctor`"; }$nl" +
            "    public static void Take(object value) { }$nl" +
            "    public static void Bump(ref int n) { n++; }$nl" +
            "    public void Run(bool flag)$nl    {$nl" +
            "        var kept = `"kept`";$nl" +
            "        var enabled = false;$nl" +
            "        if (flag) enabled = true;$nl" +
            "        var count = 0;$nl" +
            "        Bump(ref count);$nl" +
            "        var index = 0;$nl" +
            "        index++;$nl" +
            "        Take(kept); Take(enabled); Take(count); Take(index); Take(Fixed); Take(Mutable); Take(set);$nl" +
            "        for (var i = 0; i < 3; i++) Take(i + 1);$nl" +
            "        Take(Fixed + Suffix);$nl" +
            "        Take(Two + Three);$nl" +
            "    }$nl}$nl"
    }
    $db = Join-Path $tree 'fold.sqlite'
    Assert-Exit (Invoke-Gate --root $tree --ext .cs --map-sqlite $db) 0
    # Still folded: nothing writes these after the declaration.
    Assert-Equal (Get-CsFoldArgument $db 'kept') 'kept/folded' 'a local nothing writes again'
    Assert-Equal (Get-CsFoldArgument $db 'Fixed') 'fixed/folded' 'a static readonly field'
    Assert-Equal (Get-CsFoldArgument $db 'Fixed + Suffix') 'fixed/x/folded' 'a string concatenation'
    # Not folded: the initializer is not the value where it is read.
    Assert-Equal (Get-CsFoldArgument $db 'enabled') '/symbol' 'a local assigned again'
    Assert-Equal (Get-CsFoldArgument $db 'count') '/symbol' 'a local passed by ref'
    Assert-Equal (Get-CsFoldArgument $db 'index') '/symbol' 'a local incremented'
    Assert-Equal (Get-CsFoldArgument $db 'Mutable') '/symbol' 'a static field that is not readonly'
    Assert-Equal (Get-CsFoldArgument $db 'set') '/symbol' 'a readonly field the constructor assigns'
    Assert-Equal (Get-CsFoldArgument $db 'i + 1') '/' 'a sum over a loop counter'
    # Both operands fold, and the sum is still not "23".
    Assert-Equal (Get-CsFoldArgument $db 'Two + Three') '/' 'an integer sum is not a concatenation'
}
