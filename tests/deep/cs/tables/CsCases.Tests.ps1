<#
    The C# deep map's `enums` and `switch_cases` rows: an enum with every
    member and its NUMBER, and a row per `case` section and per switch-expression arm - written into the tables
    the TypeScript half writes, in its shape, so a backend value joins a frontend one. See
    `src/Map/CsRows/CsBody/CsRowsCases.cs`.

    Its helpers are its own - `-Only CsCases` runs this suite alone.
#>

$script:CsCasesProject = '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework></PropertyGroup></Project>'

# Lines as numbered in the cases: the sections on 9, 12 and 14, the arms on 20, 21 and 22.
$script:CsCasesSource = @'
namespace Demo;
public enum Kinds { Base = 0, Extra = 40, Next, Neg = -2, Shifted = 1 << 3, After }
public class Rules
{
    public int Score(Kinds id, bool flag)
    {
        switch (id)
        {
            case Kinds.Extra:
            case Kinds.Next:
                return 1;
            case Kinds.Neg when flag:
                return 2;
            default:
                return 0;
        }
    }
    public string Name(Kinds id) => id switch
    {
        Kinds.Base => "base",
        Kinds.Shifted when true => "shift",
        _ => "other",
    };
}
'@

function New-CsCasesDb([hashtable]$Files) {
    $tree = Use-Tree $Files
    $db = Join-Path $tree 'map.sqlite'
    Assert-Exit (Invoke-Gate --root $tree --ext .cs --map-sqlite $db) 0
    return $db
}

# One value: the first line of the lens's answer that is not its header or its rule.
function Get-CsCasesValue([string]$Db, [string]$Sql) {
    $result = Invoke-Gate --map-query $Db --sql $Sql
    Assert-Exit $result 0
    return @($result.Lines | Select-Object -Skip 2 | Where-Object { $_.Trim() })[0].Trim()
}

# ONE MEMBER, ONE QUERY: asked over every member at once, the output holds the other members' numbers too.
function Get-CsCasesMember([string]$Db, [string]$Name) {
    return Get-CsCasesValue $Db ("SELECT coalesce(json_extract(m.value, '$.value'), 'null') FROM enums e, json_each(e.members) m " +
        "WHERE e.name = 'Kinds' AND json_extract(m.value, '$.name') = '$Name'")
}

Test-Case 'cscases: a bound enum carries every member with the number the compiler folded' {
    $db = New-CsCasesDb @{ 'Ids.cs' = $script:CsCasesSource; 'Demo.csproj' = $script:CsCasesProject }
    Assert-Equal (Get-CsCasesValue $db "SELECT count(*) FROM enums") '1' 'enums rows'
    Assert-Equal (Get-CsCasesValue $db "SELECT symbol || '|' || exported || '|' || underlying || '|' || (owner_file = file) FROM enums") 'Demo.Kinds|1|int|1' 'the enum row'
    Assert-Equal (Get-CsCasesValue $db "SELECT json_array_length(members) FROM enums") '6' 'members'
    Assert-Equal (Get-CsCasesMember $db 'Extra') '40' 'Extra'
    Assert-Equal (Get-CsCasesMember $db 'Next') '41' 'Next, inherited from Extra'
    Assert-Equal (Get-CsCasesMember $db 'Neg') '-2' 'Neg'
    Assert-Equal (Get-CsCasesMember $db 'Shifted') '8' 'Shifted, folded by the compiler'
    Assert-Equal (Get-CsCasesMember $db 'After') '9' 'After, inherited from a folded member'
    # THE `consts` ROWS STAY: a query written against them before this table existed still answers.
    Assert-Equal (Get-CsCasesValue $db "SELECT count(*) FROM consts WHERE cls = 'Kinds'") '6' 'consts rows'
}

Test-Case 'cscases: without a model a member is numbered only where the source alone defines it' {
    $db = New-CsCasesDb @{ 'Ids.cs' = $script:CsCasesSource }
    Assert-Equal (Get-CsCasesMember $db 'Base') '0' 'Base'
    Assert-Equal (Get-CsCasesMember $db 'Next') '41' 'Next, one past a literal'
    Assert-Equal (Get-CsCasesMember $db 'Neg') '-2' 'Neg, a negated literal'
    # `1 << 3` needs a fold, and the member after it needs that fold: both are null, never a guess.
    Assert-Equal (Get-CsCasesMember $db 'Shifted') 'null' 'Shifted'
    Assert-Equal (Get-CsCasesMember $db 'After') 'null' 'After'
}

Test-Case 'cscases: each case section is a switch_cases row - labels, the members they bind, their numbers, the guard' {
    $db = New-CsCasesDb @{ 'Ids.cs' = $script:CsCasesSource; 'Demo.csproj' = $script:CsCasesProject }
    Assert-Equal (Get-CsCasesValue $db "SELECT count(*) FROM switch_cases WHERE kind = 'case'") '3' 'case sections'
    # TWO LABELS, ONE SECTION: `case Extra: case Next:` share their statements, so they share their row.
    $both = "SELECT json_array_length(labels) || '|' || json_extract(labels, '$[1]') || '|' || json_extract(label_symbols, '$[0]') || '|' || json_extract(label_values, '$[0]') || ',' || json_extract(label_values, '$[1]') FROM switch_cases WHERE line = 9"
    Assert-Equal (Get-CsCasesValue $db $both) '2|Kinds.Next|Demo.Kinds.Extra|40,41' 'the two-label section'
    Assert-Equal (Get-CsCasesValue $db "SELECT guard || '|' || is_default FROM switch_cases WHERE line = 12") 'flag|0' 'the guarded section'
    Assert-Equal (Get-CsCasesValue $db "SELECT json_array_length(labels) || '|' || is_default FROM switch_cases WHERE line = 14") '0|1' 'default'
    # WHICH SWITCH: every section points at the `branches` row of its own switch.
    $joined = "SELECT count(*) FROM switch_cases s JOIN branches b ON b.id = s.branch WHERE b.kind = 'switch' AND b.line = 7"
    Assert-Equal (Get-CsCasesValue $db $joined) '3' 'sections joined to their switch'
    Assert-Equal (Get-CsCasesValue $db "SELECT discriminant_source || '|' || discriminant_type FROM switch_cases WHERE line = 9") 'id|Demo.Kinds' 'the discriminant'
}

Test-Case 'cscases: each switch-expression arm is a row, with what it evaluates to' {
    $db = New-CsCasesDb @{ 'Ids.cs' = $script:CsCasesSource; 'Demo.csproj' = $script:CsCasesProject }
    Assert-Equal (Get-CsCasesValue $db "SELECT count(*) FROM switch_cases WHERE kind = 'arm'") '3' 'arms'
    Assert-Equal (Get-CsCasesValue $db "SELECT json_extract(label_values, '$[0]') || '|' || result FROM switch_cases WHERE line = 20") '0|"base"' 'the first arm'
    Assert-Equal (Get-CsCasesValue $db "SELECT guard FROM switch_cases WHERE line = 21") 'true' 'the guarded arm'
    Assert-Equal (Get-CsCasesValue $db "SELECT is_default || '|' || json_array_length(labels) FROM switch_cases WHERE line = 22") '1|0' 'the discard arm'
}

Test-Case 'cscases: a re-read file REPLACES its enum and case rows' {
    $tree = Use-Tree @{ 'Ids.cs' = $script:CsCasesSource; 'Demo.csproj' = $script:CsCasesProject }
    $db = Join-Path $tree 'map.sqlite'
    Assert-Exit (Invoke-Gate --root $tree --ext .cs --map-sqlite $db) 0
    [System.IO.File]::WriteAllText((Join-Path $tree 'Ids.cs'), $script:CsCasesSource.Replace('Extra = 40', 'Extra = 42'))
    Assert-Exit (Invoke-Gate --root $tree --ext .cs --map-sqlite $db) 0
    Assert-Equal (Get-CsCasesValue $db "SELECT count(*) FROM enums") '1' 'enums rows after a re-read'
    Assert-Equal (Get-CsCasesValue $db "SELECT count(*) FROM switch_cases") '6' 'switch_cases rows after a re-read'
    Assert-Equal (Get-CsCasesMember $db 'Next') '43' 'Next after the edit'
}

# WHAT A CASE DOES, joined to the case: a row inside a section or an arm carries `case` = that switch_cases row, as the
# TypeScript half's rows do. A bare `returns` row of `return 1;` said the method returns 1, full stop.
Test-Case 'cscases: a row inside a case section or an arm carries the case it sits in' {
    $db = New-CsCasesDb @{ 'Ids.cs' = $script:CsCasesSource; 'Demo.csproj' = $script:CsCasesProject }
    $returnIn = "SELECT s.line || ':' || s.kind FROM returns r JOIN switch_cases s ON s.id = r.""case"" WHERE r.line = "
    Assert-Equal (Get-CsCasesValue $db ($returnIn + '11')) '9:case' 'return 1 sits in the Extra/Next section'
    Assert-Equal (Get-CsCasesValue $db ($returnIn + '13')) '12:case' 'return 2 sits in the guarded section'
    Assert-Equal (Get-CsCasesValue $db ($returnIn + '15')) '14:case' 'return 0 sits in default'
    $literal = "SELECT s.line || ':' || s.kind FROM string_literals l JOIN switch_cases s ON s.id = l.""case"" WHERE l.value = "
    Assert-Equal (Get-CsCasesValue $db ($literal + "'base'")) '20:arm' 'the string an arm returns'
    Assert-Equal (Get-CsCasesValue $db ($literal + "'other'")) '22:arm' 'the string the discard arm returns'
    # THE SWITCH ITSELF IS IN NO CASE: no branch here sits in a section, so the table has no `case` column at all
    # (a double-quoted name with no such column is a STRING in SQLite - the column is asked of the schema instead).
    Assert-Equal (Get-CsCasesValue $db "SELECT count(*) FROM pragma_table_info('branches') WHERE name = 'case'") '0' 'a switch in a case'
}
