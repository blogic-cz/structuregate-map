<#
    The C# deep map's `members`, `locals` and `comments` rows: a property, a field, a constant and an event
    of a type, each with the symbol `refs` uses for it; every variable a body declares; every comment, with the
    declaration it sits in. Written into the tables the TypeScript half writes. See
    `src/Map/CsRows/CsBody/CsMembers.cs`.

    Its helpers are its own - `-Only CsMembers` runs this suite alone.
#>

$script:CsMembersProject = '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework></PropertyGroup></Project>'

$script:CsMembersSource = @'
namespace Demo.Models
{
    public class Account
    {
        /// <summary>The reference code.</summary>
        public string Reference { get; private set; } = "";
        private int count, total;
        public const int Max = 5;
        public event System.EventHandler Changed;
        public int Run(object o)
        {
            // trimmed first
            var reference = Reference.Trim();
            foreach (var c in reference) { total++; }
            if (o is string s && int.TryParse(s, out var n)) { return n; }
            return reference.Length + count;
        }
    }
}
'@

function New-CsMembersDb {
    $tree = Use-Tree @{ 'App/Account.cs' = $script:CsMembersSource; 'App/App.csproj' = $script:CsMembersProject }
    $db = Join-Path $tree 'map.sqlite'
    Assert-Exit (Invoke-Gate --root $tree --ext .cs --map-sqlite $db) 0
    return $db
}

# Every row of one query, as `v=` lines.
function Get-CsMembersRows([string]$Db, [string]$Sql) {
    $result = Invoke-Gate --map-query $Db --sql $Sql --width 0 --limit 0
    Assert-Exit $result 0
    return @($result.Lines | Where-Object { $_.TrimStart().StartsWith('v=') } | ForEach-Object { $_.Trim().Substring(2) })
}

function Assert-CsMembersRow([string]$Db, [string]$Sql, [string]$Expected, [string]$What) {
    $rows = Get-CsMembersRows $Db $Sql
    if ($rows -notcontains $Expected) { throw "$What`: no row '$Expected' in:`n$($rows -join "`n")" }
}

Test-Case 'csmembers: every member of a type is a row, and joins to its uses by symbol' {
    $db = New-CsMembersDb
    $members = "SELECT 'v=' || m.name || '|' || m.kind || '|' || m.symbol || '|' || m.type || '|' || m.visibility || '|' || m.accessors FROM members m JOIN files f ON f.id = m.file WHERE f.lang = 'csharp'"
    Assert-CsMembersRow $db $members 'Reference|property|Demo.Models.Account.Reference|string|public|["get", "private set"]' 'a property'
    Assert-CsMembersRow $db $members 'total|field|Demo.Models.Account.total|int|private|[]' 'the second declarator of a field'
    Assert-CsMembersRow $db $members 'Max|const|Demo.Models.Account.Max|int|public|[]' 'a constant'
    Assert-CsMembersRow $db $members 'Changed|event|Demo.Models.Account.Changed|System.EventHandler|public|[]' 'an event'
    # The question these rows answer: where a used member is declared.
    Assert-CsMembersRow $db "SELECT DISTINCT 'v=' || m.name || '|' || m.line FROM refs r JOIN members m ON m.symbol = r.symbol WHERE r.symbol LIKE '%.Reference'" 'Reference|6' 'a use joined to its declaration'
}

Test-Case 'csmembers: every local a body declares is a row, and every comment, with where it sits' {
    $db = New-CsMembersDb
    $locals = "SELECT 'v=' || l.name || '|' || l.declared || '|' || l.type || '|' || l.func FROM locals l JOIN files f ON f.id = l.file WHERE f.lang = 'csharp'"
    Assert-CsMembersRow $db $locals 'reference|local|string|Run' 'a declared variable, its var resolved'
    Assert-CsMembersRow $db $locals 'c|foreach|char|Run' 'a foreach variable'
    Assert-CsMembersRow $db $locals 's|pattern|string|Run' 'a pattern variable'
    Assert-CsMembersRow $db $locals 'n|out|int|Run' 'an out var'
    $comments = "SELECT 'v=' || c.line || '|' || c.kind || '|' || c.context || '|' || c.text FROM comments c JOIN files f ON f.id = c.file WHERE f.lang = 'csharp'"
    Assert-CsMembersRow $db $comments '5|doc|Account.Reference|/// <summary>The reference code.</summary>' 'a doc comment, with what it documents'
    Assert-CsMembersRow $db $comments '12|line|Account.Run|// trimmed first' 'a line comment in a method'
}
