<#
    The C# deep map's `enums` and `switch_cases` rows: an enum with every
    member and its NUMBER, and a row per `case` section and per switch-expression arm - written into the tables
    the TypeScript half writes, in its shape, so a backend value joins a frontend one. See
    `src/Map/CsRows/CsBody/CsRowsCases.cs`. And, over a file of every structured cell the C# half writes, that
    none of them is left as text (`Get-NodeJsonCell`).

    Its helpers are its own - `-Only CsCases` runs this suite alone.
#>

# The node rows' SQL (a list cell is `<table>__<column>`, never JSON text).
. (Join-Path $PSScriptRoot '../../nodes/NodeSql.Helpers.ps1')

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

# `-Shared`: the case only READS the map, so it may be another case's (`Get-SharedMap`, harness/SharedMap.ps1).
function New-CsCasesDb([hashtable]$Files, [switch]$Shared) {
    $tree = Use-Tree $Files
    $db = Join-Path $tree 'map.sqlite'
    $map = @('--root', $tree, '--ext', '.cs', '--map-sqlite', $db)
    if ($Shared) { $made = Get-SharedMap $tree $map; Assert-Exit $made.Result 0; return $made.Db }
    Assert-Exit (Invoke-Gate $map) 0
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
    # a member is an object node: its `name` and its `value` are node rows below it, each with its key in `role`
    return Get-CsCasesValue $Db ("SELECT coalesce(v.value, 'null') FROM enums e JOIN enums__members m ON m.owner = e.id AND m.parent = 0 " +
        "JOIN enums__members nm ON nm.owner = m.owner AND nm.parent = m.n AND nm.role = 'name' LEFT JOIN enums__members v ON v.owner = m.owner AND v.parent = m.n AND v.role = 'value' " +
        "WHERE e.name = 'Kinds' AND nm.value = '$Name'")
}

Test-Case 'cscases: a bound enum carries every member with the number the compiler folded' {
    $db = New-CsCasesDb -Shared @{ 'Ids.cs' = $script:CsCasesSource; 'Demo.csproj' = $script:CsCasesProject }
    Assert-Equal (Get-CsCasesValue $db "SELECT count(*) FROM enums") '1' 'enums rows'
    Assert-Equal (Get-CsCasesValue $db "SELECT symbol || '|' || exported || '|' || underlying || '|' || (owner_file = file) FROM enums") 'Demo.Kinds|1|int|1' 'the enum row'
    Assert-Equal (Get-CsCasesValue $db "SELECT (SELECT count(*) FROM enums__members j WHERE j.owner = enums.id AND j.parent = 0) FROM enums") '6' 'members'
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
    $db = New-CsCasesDb -Shared @{ 'Ids.cs' = $script:CsCasesSource; 'Demo.csproj' = $script:CsCasesProject }
    Assert-Equal (Get-CsCasesValue $db "SELECT count(*) FROM switch_cases WHERE kind = 'case'") '3' 'case sections'
    # TWO LABELS, ONE SECTION: `case Extra: case Next:` share their statements, so they share their row.
    $both = "SELECT (SELECT count(*) FROM switch_cases__labels j WHERE j.owner = switch_cases.id AND j.parent = 0) || '|' || (SELECT j.value FROM switch_cases__labels j WHERE j.owner = switch_cases.id AND j.parent = 0 AND j.ord = 1) || '|' || (SELECT j.value FROM switch_cases__label_symbols j WHERE j.owner = switch_cases.id AND j.parent = 0 AND j.ord = 0) || '|' || (SELECT j.value FROM switch_cases__label_values j WHERE j.owner = switch_cases.id AND j.parent = 0 AND j.ord = 0) || ',' || (SELECT j.value FROM switch_cases__label_values j WHERE j.owner = switch_cases.id AND j.parent = 0 AND j.ord = 1) FROM switch_cases WHERE line = 9"
    Assert-Equal (Get-CsCasesValue $db $both) '2|Kinds.Next|Demo.Kinds.Extra|40,41' 'the two-label section'
    Assert-Equal (Get-CsCasesValue $db "SELECT guard || '|' || is_default FROM switch_cases WHERE line = 12") 'flag|0' 'the guarded section'
    Assert-Equal (Get-CsCasesValue $db "SELECT (SELECT count(*) FROM switch_cases__labels j WHERE j.owner = switch_cases.id AND j.parent = 0) || '|' || is_default FROM switch_cases WHERE line = 14") '0|1' 'default'
    # WHICH SWITCH: every section points at the `branches` row of its own switch.
    $joined = "SELECT count(*) FROM switch_cases s JOIN branches b ON b.id = s.branch WHERE b.kind = 'switch' AND b.line = 7"
    Assert-Equal (Get-CsCasesValue $db $joined) '3' 'sections joined to their switch'
    Assert-Equal (Get-CsCasesValue $db "SELECT discriminant_source || '|' || discriminant_type FROM switch_cases WHERE line = 9") 'id|Demo.Kinds' 'the discriminant'
}

Test-Case 'cscases: each switch-expression arm is a row, with what it evaluates to' {
    $db = New-CsCasesDb -Shared @{ 'Ids.cs' = $script:CsCasesSource; 'Demo.csproj' = $script:CsCasesProject }
    Assert-Equal (Get-CsCasesValue $db "SELECT count(*) FROM switch_cases WHERE kind = 'arm'") '3' 'arms'
    Assert-Equal (Get-CsCasesValue $db "SELECT (SELECT j.value FROM switch_cases__label_values j WHERE j.owner = switch_cases.id AND j.parent = 0 AND j.ord = 0) || '|' || result FROM switch_cases WHERE line = 20") '0|"base"' 'the first arm'
    Assert-Equal (Get-CsCasesValue $db "SELECT guard FROM switch_cases WHERE line = 21") 'true' 'the guarded arm'
    Assert-Equal (Get-CsCasesValue $db "SELECT is_default || '|' || (SELECT count(*) FROM switch_cases__labels j WHERE j.owner = switch_cases.id AND j.parent = 0) FROM switch_cases WHERE line = 22") '1|0' 'the discard arm'
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
    $db = New-CsCasesDb -Shared @{ 'Ids.cs' = $script:CsCasesSource; 'Demo.csproj' = $script:CsCasesProject }
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

Test-Case 'cscases: an attribute on an enum member is a decorators row of that member and named on its entry' {
    $source = @'
using System;
namespace Demo.Models
{
    [AttributeUsage(AttributeTargets.Field)]
    public class ExternalIdAttribute : Attribute
    {
        public ExternalIdAttribute(int id) { Id = id; }
        public int Id { get; }
    }
    public enum HueKind : short
    {
        [ExternalId(7001)]
        Alpha = 0,
        [ExternalId(7002)]
        Beta = 1,
        Gamma = 9999,
    }
}
'@
    $db = New-CsCasesDb @{ 'Hue.cs' = $source; 'Demo.csproj' = $script:CsCasesProject }
    $row = "SELECT name || '|' || target_kind || '|' || $(Get-NodeRootListSql 'decorators__arg_values' 'decorators.id') FROM decorators WHERE target = '{0}'"
    Assert-Equal (Get-CsCasesValue $db ($row -f 'Alpha')) 'ExternalId|enum_member|["7001"]' 'Alpha'
    Assert-Equal (Get-CsCasesValue $db ($row -f 'Beta')) 'ExternalId|enum_member|["7002"]' 'Beta'
    # bound: the attribute's constructor, as every `decorators.symbol`
    Assert-Equal (Get-CsCasesValue $db "SELECT symbol FROM decorators WHERE target = 'Alpha'") 'Demo.Models.ExternalIdAttribute.ExternalIdAttribute' 'symbol'
    Assert-Equal (Get-CsCasesValue $db "SELECT count(*) FROM decorators WHERE target = 'Gamma'") '0' 'Gamma has none'
    # an enum argument is a constant too: it once read [""] beside the int arguments that folded
    Assert-Equal (Get-CsCasesValue $db "SELECT $(Get-NodeRootListSql 'decorators__arg_values' 'decorators.id') FROM decorators WHERE name = 'AttributeUsage'") '["System.AttributeTargets.Field"]' 'enum argument'
    $entry = "SELECT coalesce($(Get-NodeListSql 'enums__members' 'm' 'attributes'), 'null') FROM enums e JOIN enums__members m ON m.owner = e.id AND m.parent = 0 " +
        "JOIN enums__members nm ON nm.owner = m.owner AND nm.parent = m.n AND nm.role = 'name' WHERE e.name = 'HueKind' AND nm.value = '{0}'"
    Assert-Equal (Get-CsCasesValue $db ($entry -f 'Alpha')) '["ExternalId"]' 'Alpha entry'
    Assert-Equal (Get-CsCasesValue $db ($entry -f 'Gamma')) '[]' 'Gamma entry'
}

Test-Case 'cscases: an attribute on a field, a const and an event field is a decorators row of each declarator' {
    $source = @'
using System;
namespace Demo.Models
{
    public class TagAttribute : Attribute { public TagAttribute(string t) { } }
    public class Order
    {
        [Tag("f")] public int Alpha, Beta;
        [Tag("c")] public const int Gamma = 1;
        [Tag("e")] public event EventHandler Changed;
    }
}
'@
    $db = New-CsCasesDb @{ 'Order.cs' = $source; 'Demo.csproj' = $script:CsCasesProject }
    $row = "SELECT target_kind || '|' || $(Get-NodeRootListSql 'decorators__arg_values' 'decorators.id') || '|' || (symbol = 'Demo.Models.TagAttribute.TagAttribute') FROM decorators WHERE name = 'Tag' AND target = '{0}'"
    Assert-Equal (Get-CsCasesValue $db ($row -f 'Alpha')) 'field|["f"]|1' 'Alpha'
    Assert-Equal (Get-CsCasesValue $db ($row -f 'Beta')) 'field|["f"]|1' 'Beta'
    Assert-Equal (Get-CsCasesValue $db ($row -f 'Gamma')) 'const|["c"]|1' 'Gamma'
    Assert-Equal (Get-CsCasesValue $db ($row -f 'Changed')) 'event|["e"]|1' 'Changed'
}

# EVERY STRUCTURED CELL THE C# HALF WRITES, in one file: an enum with an attributed member, attributes with
# positional, named and array arguments, a dictionary's index initializer, collection expressions, tuples,
# case sections with several labels, a switch expression, an interpolated SQL template, catch clauses.
$script:CsCasesShapes = @'
using System;
using System.Collections.Generic;
using System.Linq;
namespace Demo;
[AttributeUsage(AttributeTargets.Class | AttributeTargets.Method, AllowMultiple = true)]
public sealed class TagAttribute : Attribute
{
    public TagAttribute(string name, params string[] more) { }
    public int Rank { get; set; }
    public Type[] Kinds { get; set; } = [];
}
[Flags]
public enum Hue { [Obsolete("old")] Alpha = 1, Beta = 2, Gamma = Alpha | Beta, Delta = 1 << 3 }
public record Order(int Id, string Region, Hue Hue);
[Tag("orders", "a", "b", Rank = 2, Kinds = new[] { typeof(Order), typeof(Hue) })]
public class Orders
{
    private static readonly Dictionary<string, int> Limits = new() { ["EU"] = 3, ["US"] = 5 };
    private static readonly string[] Regions = ["EU", "US", "APAC"];
    private static readonly (string Code, int Rank)[] Pairs = [("EU", 1), (Code: "US", Rank: 2)];
    private readonly List<Order> items = new() { new Order(1, "EU", Hue.Alpha), new(2, "US", Hue.Beta) };
    private const string Table = "Orders";
    [Tag("query")]
    public string Query(int id, string region) =>
        $"SELECT Id, Region FROM {Table} WHERE Id = {id} AND Region = '{region}'";
    public string Label(Hue hue, int n)
    {
        switch (hue)
        {
            case Hue.Alpha:
            case Hue.Beta when n > 1:
                return "low";
            case Hue.Gamma or Hue.Delta:
                return "high";
            default:
                return n switch { 0 => "none", > 10 and < 20 => "teen", _ => $"n={n}" };
        }
    }
    public int Total(IEnumerable<Order> orders)
    {
        try
        {
            var picked = orders.Where(o => Regions.Contains(o.Region)).Select(o => new { o.Id, o.Region }).ToList();
            var (a, b) = (picked.Count, Limits["EU"]);
            int[] grid = { 1, 2, 3 };
            return a + b + grid.Sum() + Pairs.Length + items.Count;
        }
        catch (InvalidOperationException ex) when (ex.Message.Length > 0)
        {
            throw new ArgumentException($"bad {nameof(orders)}", ex);
        }
        catch (Exception)
        {
            return -1;
        }
    }
}
'@

Test-Case 'cscases: no cell of the C# map is a structure as text - every list and object is node rows' {
    $db = New-CsCasesDb @{ 'Shapes.cs' = $script:CsCasesShapes; 'Demo.csproj' = $script:CsCasesProject }
    # The fixture must keep reaching the node tables, or a clean scan proves nothing.
    $cells = [int](Get-CsCasesValue $db 'SELECT count(*) FROM _cells')
    if ($cells -lt 28) { throw "only $cells structured column(s) in _cells - the fixture no longer reaches the node tables" }
    # `members.initializer` is the initializer AS WRITTEN - `[]`, `["EU", "US", "APAC"]` - code like `source`; the
    # collection itself is an `objects` row with its slots.
    $found = @(Get-NodeJsonCell $db | Where-Object { -not $_.StartsWith('members.initializer = ') })
    if ($found) { throw "$($found.Count) structure(s) as text:`n$($found -join "`n")" }
    # An index entry is named by its KEY: `["EU"] = 3` once wrote `["EU"]`, a list spelled as text.
    Assert-Equal (Get-CsCasesValue $db ("SELECT group_concat(a.name || '=' || a.const, ',' ORDER BY a.position) FROM arguments a " +
        "JOIN objects o ON o.id = a.object WHERE o.target = 'Limits'")) 'EU=3,US=5' 'the index entries'
}
