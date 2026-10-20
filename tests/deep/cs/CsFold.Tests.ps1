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

# FOLLOWED TO THE END: a readonly built out of readonlies is folded however many links it has (a depth of six cut it
# at three), and only a symbol already on the way - two readonlies built out of each other, which compiles - stops it.
Test-Case 'csfold: a chain of twelve readonly links folds whole, and a cycle of two is the symbol' {
    $nl = [string][char]10
    $links = (0..11 | ForEach-Object { "    private static readonly string L$_ = L$($_ + 1) + `".`";$nl" }) -join ''
    $tree = Use-Tree @{
        'Demo.csproj' = $script:CsFoldProject
        'Chain.cs' = "namespace Demo;$nl" +
            "public class Chain$nl{$nl" + $links +
            "    private static readonly string L12 = `"x`";$nl" +
            "    private static readonly string Ping = Pong + `"i`";$nl" +
            "    private static readonly string Pong = Ping + `"o`";$nl" +
            "    public static void Take(object value) { }$nl" +
            "    public void Run() { Take(L0); Take(Ping); }$nl}$nl"
    }
    $db = Join-Path $tree 'chain.sqlite'
    Assert-Exit (Invoke-Gate --root $tree --ext .cs --map-sqlite $db) 0
    Assert-Equal (Get-CsFoldArgument $db 'L0') ('x' + ('.' * 12) + '/folded') 'thirteen readonlies, folded to the end'
    Assert-Equal (Get-CsFoldArgument $db 'Ping') '/symbol' 'a cycle is the symbol it binds, not a hang'
}

# EVERY STRING A TEMPLATE CAN BE: two loops over six literals each are 36 strings, past the 32 a product was once cut
# at (the whole argument was then no template at all).
Test-Case 'csfold: a template of two six-way loops keeps all thirty-six strings' {
    $nl = [string][char]10
    $six = { param($p) (0..5 | ForEach-Object { "`"$p$_`"" }) -join ', ' }
    $tree = Use-Tree @{
        'Demo.csproj' = $script:CsFoldProject
        'Many.cs' = "namespace Demo;$nl" +
            "public class Many$nl{$nl" +
            "    public static void Take(object value) { }$nl" +
            "    public void Run()$nl    {$nl" +
            "        foreach (var a in new[] { $(& $six 'a') })$nl" +
            "            foreach (var b in new[] { $(& $six 'b') })$nl" +
            "                Take(`"x`" + a + `"-`" + b);$nl    }$nl}$nl"
    }
    $db = Join-Path $tree 'many.sqlite'
    Assert-Exit (Invoke-Gate --root $tree --ext .cs --map-sqlite $db) 0
    $r = Invoke-Gate --map-query $db --width 0 --sql ("SELECT 'v=' || count(*) AS v FROM arguments a JOIN calls c ON c.id = a.call " +
        "JOIN arguments__template s ON s.owner = a.id AND s.parent = 0 WHERE c.callee = 'Take'")
    Assert-Exit $r 0
    Assert-Line $r 'v=36'
}

# A BRANCH'S TEST AS A TREE (`branches.condition`) in Roslyn's shape, each name bound to its symbol: `tier == Tier.Gold` is findable as
# the enum member and not as text. Its helpers (`CsCond`) are its own.
$script:CsCondProject = '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework></PropertyGroup></Project>'

# The members of the node reached from the root of the condition of the `$Kind` branch on `$Line` by the roles in `$Path`,
# as `role=value` pairs sorted by role, one cell. A member that is itself a node has no value and is left out.
function Get-CsCondNode([string]$Db, [int]$Line, [string]$Kind, [string[]]$Path = @()) {
    $from = "branches b JOIN branches__condition c0 ON c0.owner = b.id AND c0.n = 0"
    $at = 'c0'
    for ($i = 0; $i -lt $Path.Count; $i++) {
        $from += " JOIN branches__condition c$($i + 1) ON c$($i + 1).owner = b.id AND c$($i + 1).parent = ${at}.n AND c$($i + 1).role = '$($Path[$i])'"
        $at = "c$($i + 1)"
    }
    $sql = "SELECT 'v=' || coalesce((SELECT group_concat(m.role || '=' || m.value, ';' ORDER BY m.role) FROM branches__condition m " +
        "WHERE m.owner = b.id AND m.parent = ${at}.n AND m.value IS NOT NULL), '') AS v FROM $from WHERE b.line = $Line AND b.kind = '$Kind'"
    $r = Invoke-Gate --map-query $Db --width 0 --sql $sql
    Assert-Exit $r 0
    $found = @($r.Lines | Where-Object { $_.TrimStart().StartsWith('v=') })
    if ($found.Count -ne 1) { throw "expected one node at $Kind/$Line [$($Path -join '/')], got $($found.Count):`n$($r.Text)" }
    return $found[0].Trim().Substring(2)
}

function Get-CsCondCount([string]$Db, [int]$Line, [string]$Kind) {
    $r = Invoke-Gate --map-query $Db --width 0 --sql ("SELECT 'v=' || count(c.n) AS v FROM branches b " +
        "JOIN branches__condition c ON c.owner = b.id WHERE b.line = $Line AND b.kind = '$Kind'")
    Assert-Exit $r 0
    return (@($r.Lines | Where-Object { $_.TrimStart().StartsWith('v=') })[0]).Trim().Substring(2)
}

Test-Case 'cscondition: a test is a tree of Roslyn kinds with what each name binds to' {
    $nl = [string][char]10
    $tree = Use-Tree @{
        'Demo.csproj' = $script:CsCondProject
        'Cond.cs' = "namespace Demo;$nl" +
            "public enum Tier { Basic = 1, Gold = 2 }$nl" +
            "public class Store { public bool Ready; }$nl" +
            "public class Cond$nl{$nl" +
            "    public int Run(Tier tier, bool a, Store b, int n)$nl    {$nl" +
            "        if (tier == Tier.Gold) return 1;$nl" +
            "        if (a && !b.Ready) return 2;$nl" +
            "        if (tier is Tier.Gold or Tier.Basic) return 3;$nl" +
            "        while (n > 0) n--;$nl" +
            "        switch (tier) { default: break; }$nl" +
            "        try { n++; } catch { }$nl" +
            "        return 0;$nl" +
            "    }$nl}$nl"
    }
    $db = Join-Path $tree 'cond.sqlite'
    Assert-Exit (Invoke-Gate --root $tree --ext .cs --map-sqlite $db) 0
    # a) the issue's repro: `tier == Tier.Gold`
    Assert-Equal (Get-CsCondNode $db 8 'if') 'kind=EqualsExpression;op===;source=tier == Tier.Gold' 'the root of an equality'
    Assert-Equal (Get-CsCondNode $db 8 'if' @('right')) 'const=Demo.Tier.Gold;const_kind=enum;kind=SimpleMemberAccessExpression;source=Tier.Gold;symbol=Demo.Tier.Gold' 'the enum member'
    Assert-Equal (Get-CsCondNode $db 8 'if' @('left')) 'const_kind=symbol;kind=IdentifierName;source=tier;symbol=tier' 'the parameter'
    # b) a negated member read under `&&`
    Assert-Equal (Get-CsCondNode $db 9 'if') 'kind=LogicalAndExpression;op=&&;source=a && !b.Ready' 'a conjunction'
    Assert-Equal (Get-CsCondNode $db 9 'if' @('right')) 'kind=LogicalNotExpression;op=!;source=!b.Ready' 'a negation'
    Assert-Equal (Get-CsCondNode $db 9 'if' @('right', 'operand')) 'const_kind=symbol;kind=SimpleMemberAccessExpression;source=b.Ready;symbol=Demo.Store.Ready' 'a dotted read is one leaf'
    # c) a pattern
    Assert-Equal (Get-CsCondNode $db 10 'if') 'kind=IsPatternExpression;op=is;source=tier is Tier.Gold or Tier.Basic' 'an is-pattern'
    Assert-Equal (Get-CsCondNode $db 10 'if' @('pattern')) 'kind=OrPattern;op=or;source=Tier.Gold or Tier.Basic' 'an or-pattern'
    Assert-Equal (Get-CsCondNode $db 10 'if' @('pattern', 'left')) 'const=Demo.Tier.Gold;const_kind=enum;kind=ConstantPattern;source=Tier.Gold;symbol=Demo.Tier.Gold' 'the first constant pattern'
    Assert-Equal (Get-CsCondNode $db 10 'if' @('pattern', 'right')) 'const=Demo.Tier.Basic;const_kind=enum;kind=ConstantPattern;source=Tier.Basic;symbol=Demo.Tier.Basic' 'the second constant pattern'
    # d) other statements; a try tests nothing
    Assert-Equal (Get-CsCondNode $db 11 'while') 'kind=GreaterThanExpression;op=>;source=n > 0' 'a while'
    Assert-Equal (Get-CsCondNode $db 12 'switch') 'const_kind=symbol;kind=IdentifierName;source=tier;symbol=tier' 'a switch'
    Assert-Equal (Get-CsCondCount $db 13 'try') '0' 'a try has no condition nodes'
}
