<#
    `--magic`: the numbers, strings and split/join separators nothing names, over every half that writes a
    literal's `use` - python, C#, rust, plain TypeScript and JavaScript (the Angular half's own rows are pinned
    in TsAngular/TsRowsMagic.Tests.ps1). What is asserted is the REPORT a reader acts on - which rows are in
    each section and which are not - and the `use` each literal was given to get there.

    Its helpers are prefixed `Magic` and are its own: `-Only Magic` runs this file alone.
#>

# The lens's lines with every run of spaces made one, so a row reads `app.py 6 45 argument sleep go`
# whatever width its columns were padded to.
function Get-MagicReport([string]$Db) {
    $r = Invoke-Gate --map-query $Db --magic --width 0 --limit 0
    Assert-Exit $r 0
    $lines = @($r.Lines | ForEach-Object { (@($_.Split([char]' ') | Where-Object { $_ -ne '' })) -join ' ' })
    return [pscustomobject]@{ Exit = $r.Exit; Lines = $lines; Text = ($lines -join "`n") }
}

$script:MagicNl = [string][char]10

if ($script:Python) {

Test-Case 'magic: numbers, strings and splitting nothing names - python, C# and rust in one report, tests left out' {
    $nl = $script:MagicNl
    $tree = Use-Tree @{
        'app.py'      = "TIMEOUT = 30" + $nl + "MODE = `"fast`"" + $nl + "def go(line, mode, n=9):" + $nl +
                        "    if mode == `"fast`":" + $nl + "        return line.split(`":`")[3]" + $nl +
                        "    sleep(45)" + $nl + "    return `",`".join([mode, `"fast`", `"fast`"])" + $nl
        'A.cs'        = "namespace D;" + $nl + "public static class C" + $nl + "{" + $nl +
                        "    const int Limit = 450;" + $nl +
                        "    public static string Go(string line, string mode)" + $nl + "    {" + $nl +
                        "        if (mode == `"slow`") return line.Split(':')[4];" + $nl +
                        "        System.Threading.Thread.Sleep(250 + Limit);" + $nl +
                        "        return string.Join(`";`", new[] { mode, `"x`" });" + $nl + "    }" + $nl + "}" + $nl
        'src/lib.rs'  = "pub fn go(line: &str, mode: &str) -> Option<String> {" + $nl +
                        "    if mode == `"lazy`" { return line.split('#').nth(7).map(String::from); }" + $nl +
                        "    let wait = 3600 * 2 + 1;" + $nl + "    let p = std::path::Path::new(line).join(`"package.json`");" + $nl +
                        "    Some(vec![mode, `"x`"].join(`"-`"))" + $nl + "}" + $nl +
                        "#[cfg(test)]" + $nl + "mod tests { #[test] fn t() { let n = 777; } }" + $nl
        'tests/test_app.py' = "def test_go():" + $nl + "    assert go(`"a`", `"b`") == 888" + $nl
    }
    $db = Join-Path $tree 'map.sqlite'
    Assert-Exit (Invoke-Gate --root $tree --ext '.py,.cs,.rs' --map-sqlite $db) 0
    $report = Get-MagicReport $db
    Assert-Line $report 'MAGIC NUMBERS (3)'
    Assert-Line $report 'A.cs 8 250 arith Go'
    Assert-Line $report 'app.py 6 45 argument sleep go'
    Assert-Line $report 'src/lib.rs 3 3600 arith go'
    # A constant's own value, a default, a 0/1/2, test code and a split position are not magic numbers.
    foreach ($quiet in 'app.py 1 30', 'A.cs 4 450', 'app.py 3 9', '777', '888', 'src/lib.rs 3 1 ', 'src/lib.rs 3 2 ') {
        Assert-NoLine $report $quiet
    }

    # ONE ROW PER VALUE: value, uses, files, compared, the constant that already names it, where it first is.
    Assert-Line $report 'MAGIC STRINGS (3)'
    Assert-Line $report 'fast 3 1 1 MODE app.py:4'
    Assert-Line $report 'slow 1 1 1 A.cs:7'
    Assert-Line $report 'lazy 1 1 1 src/lib.rs:2'

    Assert-Line $report 'SPLIT POSITIONS (3)'
    foreach ($row in "A.cs 7 4 line.Split(':')", 'app.py 5 3 line.split(":")', "src/lib.rs 2 7 line.split('#')") {
        Assert-Line $report $row
    }
    Assert-Line $report 'SEPARATORS (6)'
    foreach ($row in "Split ':' 1 1 A.cs:7", "Join ';' 1 1 A.cs:9", "split ':' 1 1 app.py:5", "join ',' 1 1 app.py:7",
                     "split '#' 1 1 src/lib.rs:2", "join '-' 1 1 src/lib.rs:5") {
        Assert-Line $report $row
    }
    # AN ELEMENT IS NOT THE ARGUMENT: the list's strings are not separators of the join they are handed to. And
    # a path is not a separator: `Path::join("package.json")` is spelled like a string join.
    Assert-NoLine $report "join 'fast'"
    Assert-NoLine $report "Join 'x'"
    Assert-NoLine $report 'package.json'
    # A number is a NUMBER in every half: written as text, C#'s compared unequal to every number in SQL.
    $kind = Invoke-Gate --map-query $db --sql ("SELECT 'v=' || count(*) || '|' || sum(typeof(n.number) IN ('integer', 'real')) " +
        "FROM number_literals n JOIN files f ON f.id = n.file WHERE f.lang = 'csharp'")
    Assert-Line $kind 'v=3|3'
}

}

$script:MagicTs = Get-TypeScriptPath 'typescript@5'
if (-not $script:MagicTs) { Write-Host '    (no typescript 5 - the TypeScript and JavaScript magic case is not run)' }

if ($script:MagicTs) {

Test-Case 'magic: TypeScript and JavaScript too - and the first string a call is handed is a string literal' {
    $nl = $script:MagicNl
    $tree = Use-Tree @{
        'a.ts' = "export function go(line: string, mode: string): string {" + $nl +
                 "  if (mode === 'eager') return line.split('|')[5];" + $nl +
                 "  setTimeout(() => {}, 1500);" + $nl + "  return [mode, 'y'].join('/');" + $nl + "}" + $nl
        'b.js' = "export const parse = (s) => s.split('=')[6] + 99;" + $nl + "export const label = t('menu.save');" + $nl +
                 "export const isText = (v) => typeof v === 'string';" + $nl
    }
    $db = Join-Path $tree 'map.sqlite'
    Assert-Exit (Invoke-Gate --root $tree --ext '.ts,.js' --map-sqlite $db --ts-node-modules $script:MagicTs) 0
    $report = Get-MagicReport $db
    foreach ($row in 'a.ts 3 1500 argument setTimeout go', 'b.js 1 99 arith', 'eager 1 1 1 a.ts:2',
                     "a.ts 2 5 line.split('|')", "split '|' 1 1 a.ts:2", "join '/' 1 1 a.ts:4",
                     "b.js 1 6 s.split('=')", "split '=' 1 1 b.js:1") {
        Assert-Line $report $row
    }
    Assert-NoLine $report "join 'y'"
    # `typeof v === 'string'` is the language naming a type, not a magic string.
    Assert-NoLine $report 'string 1'
    # A call's FIRST string was taken for a module specifier and never written - `t('menu.save')` included.
    $first = Invoke-Gate --map-query $db --sql "SELECT 'v=' || use || '|' || callee FROM string_literals WHERE value = 'menu.save'"
    Assert-Line $first 'v=argument|t'
}

}

# A CONSTANT'S MEMBERS THROUGH A CONSTRUCTOR: `KINDS = frozenset({2, 3, 61})` declares its members as
# `SIZES = (1, 2)` does - the `frozenset(...)` around the set made each one an argument, then `other`, and a large
# share of a real report's magic numbers were that one constant. The same call inside a function still hides numbers.
if ($script:Python) {
    Test-Case 'magic: a module constant built with frozenset/tuple/set/list/dict declares its members' {
        $nl = $script:MagicNl
        $tree = Use-Tree @{
            'codes.py' = "KINDS = frozenset({2, 3, 61, 77})" + $nl + "LIMITS = tuple([640, 480])" + $nl +
                         "TABLE = dict({`"a`": 1234})" + $nl + "def pick(x):" + $nl + "    return x in frozenset({55, 66})" + $nl
        }
        $db = Join-Path $tree 'map.sqlite'
        Assert-Exit (Invoke-Gate --root $tree --ext '.py' --map-sqlite $db) 0
        $uses = Invoke-Gate --map-query $db --width 0 --sql "SELECT 'u=' || line || ':' || value || ':' || use AS u FROM number_literals ORDER BY line, value"
        foreach ($expected in 'u=1:61:declared', 'u=1:77:declared', 'u=2:640:declared', 'u=3:1234:declared') { Assert-Line $uses $expected }
        $report = Get-MagicReport $db
        foreach ($quiet in 'codes.py 1 61', 'codes.py 1 77', 'codes.py 2 640', 'codes.py 3 1234') { Assert-NoLine $report $quiet }
        Assert-Line $report 'codes.py 5 55'
    }
}
