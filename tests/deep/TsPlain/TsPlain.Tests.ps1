<#
    --map-sqlite over a tree with TypeScript and NO Angular workspace: the PLAIN TypeScript half.

    WHAT IS ASSERTED IS A ROW. The half exists so a Solid, node or Apps Script tree has expression rows at
    all, and the one question it was built for - is this body written somewhere else, and which def does
    this call run - is answered by cells. So the cases open the database and read them.

    The compiler is typescript 5 (the in-process parser); 7.x's native package does not export the helpers
    this half walks with. Its helpers are its own - `-Only TsPlain` runs this suite alone.
#>

$script:TsPlainModules = Get-TypeScriptPath 'typescript@5'
if (-not $script:TsPlainModules) { Write-Host '    (no node/npm-installed typescript 5 - the plain typescript cases are not run)' }

# The deep map over a tree, the compiler borrowed from the cache. `-Extra` for a case that adds --map.
function Invoke-TsPlainMap([string]$Tree, [string[]]$Extra = @()) {
    $db = Join-Path $Tree 'map.sqlite'
    $run = Invoke-Gate --root $Tree --ext '.ts,.tsx,.mjs' --map-sqlite $db --ts-node-modules $script:TsPlainModules @Extra
    Assert-Exit $run 0
    return $run
}

# One query, every cell whole.
function Get-TsPlainRows([string]$Tree, [string]$Sql) {
    $r = Invoke-Gate --map-query (Join-Path $Tree 'map.sqlite') --width 0 --sql $Sql
    Assert-Exit $r 0
    return $r
}

function Get-TsPlainScalar([string]$Tree, [string]$Sql) {
    $r = Get-TsPlainRows $Tree $Sql
    return ($r.Lines | Where-Object { $_ -match '^\s*\d+\s*$' } | Select-Object -First 1).Trim()
}

# A price module and a basket that imports it twice over - once by name, once as a namespace.
function New-TsPlainTree {
    return Use-Tree @{
        'src/price.ts'  = "export function total(items: number[]): number {`n    let sum = 0;`n" +
                          "    for (const x of items) sum += x;`n    return sum;`n}`nexport const RATE = 10;`n"
        'src/basket.ts' = "import { total as sumOf } from './price';`nimport * as P from './price';`n" +
                          "export function basket(items: number[]) {`n    return sumOf(items) * P.RATE + P.total(items);`n}`n" +
                          "export function other(sumOf: (x: number[]) => number) {`n    return sumOf([1, 2]);`n}`n"
    }
}

if ($script:TsPlainModules) {

Test-Case 'tsplain: a call binds through the import, and a parameter of the same name shadows it' {
    $tree = New-TsPlainTree
    Invoke-TsPlainMap $tree | Out-Null
    $r = Get-TsPlainRows $tree ("SELECT 'call=' || func || '|' || callee || '|' || target_path || '|' || target_name AS c " +
        "FROM calls ORDER BY func, callee")
    Assert-Line $r 'call=basket|sumOf|src/price.ts|total'
    Assert-Line $r 'call=basket|P.total|src/price.ts|total'
    # A BINDING IS NEVER A GUESS BY NAME: `other` calls its own parameter, not the import.
    Assert-Line $r 'call=other|sumOf||'
    Assert-Equal (Get-TsPlainScalar $tree "SELECT count(*) FROM files WHERE lang = 'ts'") '2' 'files stamped with the plain lang'
}

Test-Case 'tsplain: the duplicate groups are the ones buildmap.json lists, with the source beside them' {
    $tree = Use-Tree @{
        'a.ts' = "export function total(items: number[]): number {`n    let sum = 0;`n    for (const x of items) sum += x;`n    return sum;`n}`n" +
                 "export const pick = (o: any) => ({ id: o.id, name: o.name, price: o.price, shop: o.shop, when: o.when });`n"
        'b.ts' = "export function add(values: number[]): number {`n    let acc = 0;`n    for (const v of values) acc += v;`n    return acc;`n}`n" +
                 "export const take = (r: any) => ({ id: r.id, name: r.name, price: r.price, shop: r.shop, when: r.when });`n"
    }
    $json = Join-Path $tree 'm.json'
    # The FILE map borrows its compiler through NODE_PATH, not --ts-node-modules.
    $previous = $env:NODE_PATH
    $env:NODE_PATH = $script:TsPlainModules
    try { Invoke-TsPlainMap $tree @('--map', '--map-out', $json) | Out-Null }
    finally { $env:NODE_PATH = $previous }
    $map = Get-Content $json -Raw | ConvertFrom-Json
    $bodies = @($map.duplicate_bodies)
    Assert-Equal $bodies.Count 1 'the file map has one body group'
    # ONE FINGERPRINT: the group the file map lists is one GROUP BY away, local names blanked.
    Assert-Equal (Get-TsPlainScalar $tree "SELECT count(*) FROM (SELECT body_shape FROM functions WHERE body_shape <> '' GROUP BY body_shape HAVING count(*) > 1)") '1' 'body groups'
    $r = Get-TsPlainRows $tree "SELECT 'fn=' || name FROM functions WHERE body_shape = (SELECT body_shape FROM functions WHERE name = 'total')"
    Assert-Line $r 'fn=add'
    Assert-Line $r 'fn=total'
    # THE FILE MAP KEEPS THE WIDEST SHAPE PER SET OF SITES - a nested fragment repeats wherever its parent
    # does - so its groups are the DISTINCT site sets of the shapes spanning two files.
    $exprs = @($map.duplicate_expressions)
    Assert-Equal $exprs.Count 1 'the file map has one expression group'
    $sites = "SELECT shape, group_concat(p, ',') AS sites FROM (SELECT x.shape, x.file, f.path || ':' || x.line AS p " +
        "FROM expressions x JOIN files f ON f.id = x.file ORDER BY p) GROUP BY shape HAVING count(DISTINCT file) > 1"
    Assert-Equal (Get-TsPlainScalar $tree "SELECT count(DISTINCT sites) FROM ($sites)") '1' 'expression site sets'
    Assert-Line (Get-TsPlainRows $tree "SELECT 'sites=' || sites FROM ($sites) LIMIT 1") "sites=$($exprs[0].at -join ',')"
}

Test-Case 'tsplain: an importer is read again when the file it imports goes, and its binding goes with it' {
    $tree = New-TsPlainTree
    Invoke-TsPlainMap $tree | Out-Null
    Remove-Item (Join-Path $tree 'src/price.ts') -Force
    # basket.ts did not change; only what it was BOUND THROUGH did.
    Assert-Line (Invoke-TsPlainMap $tree) '1 file(s) re-read'
    Assert-Equal (Get-TsPlainScalar $tree "SELECT count(*) FROM calls WHERE target_path = 'src/price.ts'") '0' 'no call runs a file that is gone'
}

Test-Case 'tsplain: an unchanged tree re-reads nothing, and a changed file REPLACES its rows' {
    $tree = New-TsPlainTree
    Invoke-TsPlainMap $tree | Out-Null
    $calls = Get-TsPlainScalar $tree 'SELECT count(*) FROM calls'
    Assert-Line (Invoke-TsPlainMap $tree) '0 file(s) re-read'
    Assert-Equal (Get-TsPlainScalar $tree 'SELECT count(*) FROM calls') $calls 'no row added or lost'
    [System.IO.File]::WriteAllText((Join-Path $tree 'src/basket.ts'), "export const none = 1;`n")
    Assert-Line (Invoke-TsPlainMap $tree) '1 file(s) re-read'
    Assert-Equal (Get-TsPlainScalar $tree "SELECT count(*) FROM calls WHERE callee = 'sumOf'") '0' 'the old rows are gone'
    Assert-Equal (Get-TsPlainScalar $tree 'SELECT count(*) FROM (SELECT id FROM calls GROUP BY id HAVING count(*) > 1)') '0' 'no id handed out twice'
}

# A FILE THE TREE MAP HAS HASHED IS NOT READ unless it is parsed: rust hands the host the hashes. HELD OPEN
# EXCLUSIVELY here, so a host that still read every file to hash it would say it cannot be read.
Test-Case 'tsplain: an unchanged file is not even read when another one changed' {
    $tree = New-TsPlainTree
    Invoke-TsPlainMap $tree | Out-Null
    [System.IO.File]::WriteAllText((Join-Path $tree 'src/basket.ts'), "export const none = 2;`n")
    $held = [System.IO.File]::Open((Join-Path $tree 'src/price.ts'), 'Open', 'ReadWrite', 'None')
    try { $again = Invoke-TsPlainMap $tree } finally { $held.Dispose() }
    Assert-NoLine $again 'cannot be read'
    Assert-Line $again '1 file(s) re-read'
}

Test-Case 'tsplain: the last script file gone takes the rows with it' {
    $tree = New-TsPlainTree
    Invoke-TsPlainMap $tree | Out-Null
    Get-ChildItem (Join-Path $tree 'src') -Filter '*.ts' | Remove-Item -Force
    [System.IO.File]::WriteAllText((Join-Path $tree 'keep.cs'), "class Keep { }`n")
    $run = Invoke-Gate --root $tree --ext '.ts,.cs' --map-sqlite (Join-Path $tree 'map.sqlite')
    Assert-Exit $run 0
    Assert-Equal (Get-TsPlainScalar $tree "SELECT count(*) FROM files WHERE lang = 'ts'") '0' 'no plain file left'
    Assert-Equal (Get-TsPlainScalar $tree 'SELECT count(*) FROM calls') '0' 'no plain row left'
}

Test-Case 'tsplain: a regex literal is a row, naming what uses it' {
    $tree = Use-Tree @{ 'a.ts' = "export const words = (s: string) => String(s).split(/\s+/g);`n" }
    Invoke-TsPlainMap $tree | Out-Null
    $r = Get-TsPlainRows $tree "SELECT 'rx=' || kind || '|' || api || '|' || pattern || '|' || pattern_kind || '|' || flags || '|' || used_by || '|' || func FROM regexes"
    Assert-Line $r 'rx=literal||\s+|literal|g|.split|words'
}

Test-Case 'tsplain: a RegExp construction is a row, its pattern known only from a literal' {
    $tree = Use-Tree @{ 'a.ts' = "const a = new RegExp('^\\d+$', 'i');`nconst b = RegExp(a.source);`nexport const both = [a, b];`n" }
    Invoke-TsPlainMap $tree | Out-Null
    $r = Get-TsPlainRows $tree "SELECT 'rx=' || kind || '|' || api || '|' || pattern || '|' || pattern_kind || '|' || flags || '|' || used_by || '|' || func FROM regexes ORDER BY line"
    Assert-Line $r 'rx=call|RegExp|^\d+$|literal|i|= a|'
    Assert-Line $r 'rx=call|RegExp||||= b|'
    Assert-Equal (Get-TsPlainScalar $tree "SELECT count(*) FROM calls WHERE callee = 'RegExp'") '2' 'the call rows stay'
}

Test-Case 'tsplain: a RegExp declared in the file is not the global one' {
    # The literal is there so the table is: a database with no regex row has no `regexes` table to count.
    $tree = Use-Tree @{ 'a.ts' = "function RegExp(s: string) { return s; }`nexport const x = RegExp('a');`nexport const y = /b/;`n" }
    Invoke-TsPlainMap $tree | Out-Null
    Assert-Equal (Get-TsPlainScalar $tree "SELECT count(*) FROM regexes WHERE kind = 'call'") '0' 'a local RegExp is no regex'
}

Test-Case 'tsplain: a tree with no typescript to borrow is a note, not an error' {
    $tree = Use-Tree @{ 'tool.mjs' = "export const x = 1;`n" }
    $run = Invoke-Gate --root $tree --ext .mjs --map-sqlite (Join-Path $tree 'map.sqlite')
    Assert-Exit $run 0
    Assert-Line $run 'the plain typescript half did not run: no typescript to borrow'
    Assert-NoLine $run 'HALF      plain-ts rows'
}

# THE TWO TYPESCRIPT HALVES NEVER MAP ONE TREE: an Angular workspace is the Angular half's. Needs the
# cache the Angular suites fill (typescript + @angular/compiler); without it the case is not run.
$script:TsPlainAngular = Join-Path ([System.IO.Path]::GetTempPath()) 'sgtest-tsrows\node_modules'
if (Test-Path (Join-Path $script:TsPlainAngular '@angular\compiler\package.json')) {
Test-Case 'tsplain: an Angular workspace is the Angular half''s, and leaves no plain row' {
    $tree = Use-Tree @{ 'apps/shop/src/main.ts' = "export const started = true;`n" }
    $db = Join-Path $tree 'map.sqlite'
    Assert-Exit (Invoke-Gate --root $tree --ext .ts --map-sqlite $db --ts-node-modules $script:TsPlainAngular) 0
    Assert-Equal (Get-TsPlainScalar $tree "SELECT count(*) FROM files WHERE lang = 'ts'") '1' 'the plain half mapped it first'
    [System.IO.File]::WriteAllText((Join-Path $tree 'angular.json'), '{"projects":{"shop":{"projectType":"application",' +
        '"root":"apps/shop","sourceRoot":"apps/shop/src","architect":{"build":{"options":{"tsConfig":"apps/shop/tsconfig.app.json"}}}}}}')
    [System.IO.File]::WriteAllText((Join-Path $tree 'apps/shop/tsconfig.app.json'), '{"compilerOptions":{"strict":true},"include":["src/**/*.ts"]}')
    Assert-Exit (Invoke-Gate --root $tree --ext .ts --map-sqlite $db --ts-node-modules $script:TsPlainAngular) 0
    Assert-Equal (Get-TsPlainScalar $tree "SELECT count(*) FROM files WHERE lang = 'ts'") '0' 'the plain rows were dropped'
    Assert-Equal (Get-TsPlainScalar $tree "SELECT count(*) FROM files WHERE lang = 'typescript' AND path = 'apps/shop/src/main.ts'") '1' 'the Angular half has it'
}
}

Test-Case 'tsplain: a catch clause is a handlers row, the nested arrow''s throw is not its own' {
    $tree = Use-Tree @{
        'src/guard.ts' = "export function run(log: { warn(m: string, e: unknown): void }, a: () => void, b: () => void) {`n" +
                         "    try { a(); }`n    catch (e) { // best effort`n        log.warn('x', e);`n" +
                         "        const again = () => { throw e; };`n        throw e;`n    } finally { b(); }`n" +
                         "    try { b(); } catch { /* swallow */ }`n}`n"
    }
    Invoke-TsPlainMap $tree | Out-Null
    # Digits: bare, name_read, passes, raises, reraises, finally.
    $r = Get-TsPlainRows $tree ("SELECT 'h=' || line || ':' || try_line || ':' || name || ':' || bare || name_read || passes" +
        " || raises || reraises || finally || ':' || comment || ':' || calls || ':' || types FROM handlers ORDER BY line")
    Assert-Line $r 'h=3:2:e:110111:// best effort:["log.warn"]:[]'
    Assert-Line $r 'h=8:8::101000:/* swallow */:[]:[]'
    Assert-Equal (Get-TsPlainScalar $tree "SELECT count(*) FROM handlers WHERE func = 'run'") '2' 'both clauses sit in run'
    # The caught name is the handler's; a try tests nothing.
    Assert-Equal (Get-TsPlainScalar $tree "SELECT count(*) FROM branches WHERE kind = 'try' AND test = ''") '2' 'a try tests nothing'
}

}

if (Get-Command node -ErrorAction SilentlyContinue) {

Test-Case 'tsplain: a half with no compiler to borrow says why in the map summary, and keeps saying it on the next run' {
    # A FRONTEND WHOSE DEPENDENCIES WERE NEVER INSTALLED: the half stores an empty payload cleanly, and the runs after
    # it only said "had nothing to do" - the halves line named it as run while the database held none of its rows.
    $tree = Use-Tree @{ 'web/package.json' = '{"name":"web"}'; 'web/src/a.ts' = "export const A = 1;`n" }
    $empty = Join-Path $tree 'no-modules'
    [void](New-Item -ItemType Directory -Path $empty -Force)
    $map = @('--root', $tree, '--ext', '.ts', '--map', '--map-out', (Join-Path $tree 'm.json'), '--map-sqlite', (Join-Path $tree 'map.sqlite'), '--ts-node-modules', $empty)
    $first = Invoke-Gate @map
    Assert-Exit $first 0
    Assert-Line $first 'not stored   the plain typescript half did not run: no typescript to borrow'
    $again = Invoke-Gate @map
    Assert-Exit $again 0
    Assert-Line $again 'not stored   the plain typescript half did not run: no typescript to borrow'
}

}
