<#
    The C# deep map's `regexes` rows: a constructor call of the framework's `Regex`, a static method of it
    taking a pattern, `[GeneratedRegex]` and `[RegularExpression]` - decided by the BOUND symbol, never by the
    name. See `src/Map/CsRows/CsRowsRegex.cs`.

    Its helpers are its own - `-Only CsRegex` runs this suite alone.
#>

$script:CsRegexProject = '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework></PropertyGroup></Project>'

# The deep map of a throwaway project, as its database path.
function New-CsRegexDb([hashtable]$Files) {
    $tree = Use-Tree ($Files + @{ 'Demo.csproj' = $script:CsRegexProject })
    $db = Join-Path $tree 'map.sqlite'
    Assert-Exit (Invoke-Gate --root $tree --ext .cs --map-sqlite $db) 0
    return $db
}

# One query, every cell whole.
function Get-CsRegexRows([string]$Db, [string]$Sql) {
    $r = Invoke-Gate --map-query $Db --width 0 --sql $Sql
    Assert-Exit $r 0
    return $r
}

# `|`-joined cells, so a case compares one exact line and a containment check cannot pass on a neighbour.
function Assert-CsRegexRow($Result, [string]$Expected) {
    $hits = @($Result.Lines | Where-Object { $_.Trim() -ceq $Expected })
    if ($hits.Count -eq 0) { throw "no line is exactly '$Expected'. Output:`n$($Result.Text)" }
}

$script:CsRegexColumns = "SELECT 'rx=' || kind || '|' || api || '|' || pattern || '|' || pattern_kind || '|' || used_by FROM regexes ORDER BY line"

Test-Case 'csregex: a built regex is a row, its pattern read through the parameter it fills' {
    # The partial method has no generator here, so the file does not compile - the attribute still binds.
    $db = New-CsRegexDb @{
        'Rx.cs' = @'
using System.Text.RegularExpressions;
using System.ComponentModel.DataAnnotations;
namespace Demo;
public partial class Rx
{
    public const string Digits = "a+";
    private static readonly Regex Built = new Regex(Digits, RegexOptions.Compiled);
    [GeneratedRegex("^x$", RegexOptions.IgnoreCase)] private static partial Regex Gen();
    [RegularExpression(@"\w+")] public string Name { get; set; } = "";
    public bool Go(string s) => Regex.IsMatch(s, @"^\d+$", RegexOptions.IgnoreCase) && Built.IsMatch(s);
    public string Esc(string s) => Regex.Escape(s);
}
'@
    }
    $r = Get-CsRegexRows $db $script:CsRegexColumns
    Assert-CsRegexRow $r 'rx=call|System.Text.RegularExpressions.Regex.Regex|a+|const|= Built'
    Assert-CsRegexRow $r 'rx=attribute|System.Text.RegularExpressions.GeneratedRegexAttribute.GeneratedRegexAttribute|^x$|const|Gen'
    Assert-CsRegexRow $r 'rx=attribute|System.ComponentModel.DataAnnotations.RegularExpressionAttribute.RegularExpressionAttribute|\w+|const|Name'
    Assert-CsRegexRow $r 'rx=call|System.Text.RegularExpressions.Regex.IsMatch|^\d+$|const|.IsMatch'
    # No row for the instance call on `Built`, nor for `Escape`: neither takes a pattern.
    Assert-CsRegexRow (Get-CsRegexRows $db "SELECT 'n=' || count(*) FROM regexes") 'n=4'
}

Test-Case 'csregex: a type of the same name declared here is not the framework''s' {
    # `Real.cs` is there so the table is: a database with no regex row has no `regexes` table to count.
    $db = New-CsRegexDb @{
        'Own.cs'  = 'namespace Demo; public static class Regex { public static bool IsMatch(string s, string pattern) => true; }'
        'Use.cs'  = 'namespace Demo; public class Use { public bool Go(string s) => Regex.IsMatch(s, "x"); }'
        'Real.cs' = 'namespace Other; public class Real { public bool Go(string s) => System.Text.RegularExpressions.Regex.IsMatch(s, "y"); }'
    }
    Assert-CsRegexRow (Get-CsRegexRows $db "SELECT 'n=' || count(*) FROM regexes WHERE api LIKE 'Demo.%'") 'n=0'
    Assert-CsRegexRow (Get-CsRegexRows $db "SELECT 'n=' || count(*) FROM regexes") 'n=1'
    Assert-CsRegexRow (Get-CsRegexRows $db "SELECT 'n=' || count(*) FROM calls WHERE symbol = 'Demo.Regex.IsMatch'") 'n=1'
}
