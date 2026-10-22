<#
    EVERY LOOP AND EVERY BARE EXPRESSION STATEMENT HAS A ROW (rows 70, `TsDecls/TsExpr/TsLoops.mjs` and
    `TsDecls/TsBody.mjs::collectLoop`): a `loops` row per `for`, `for..of`, `for..in`, `while` and `do` - its header
    expression's tree in `expressions` - and a `loop` column, beside `branch`, on every row the loop's BODY holds. A loop is
    never a `branches` row: those are conditions to the rust closure. `x++` is an assignment, and `this.a.b;` an
    `expressions` row of role `statement`. A partial run keeps every `loop` id valid. Its helpers are `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '../../TsRows.Helpers.ps1')

# Lines as numbered in the cases: the loops on 8, 10, 16, 17, 18, 20, 23, 24 and 25, the bare statement on 19, the `if` on 20.
$script:TsLoopWalker = @(
    'export class Walker {', '  items: string[] = [];', '  queue: string[] = [];', '  pairs: [string, { b: string }][] = [];',
    '  note = { a: { b: 1 } };', '  count = 0;', '  run(): void {',
    '    for (const item of this.items) {', '      this.touch(item);', '      for (let i = 0; i < 3; i++) {',
    "        this.touch('inner');", '      }', '      this.items.forEach((x) => this.touch(x));', '    }',
    "    this.touch('after');", '    for (const k in this.note) { this.count += 1; }',
    '    while (this.queue.length) { this.queue.pop(); }', '    do { this.count--; } while (this.count > 0);',
    '    this.note.a;', '    if (this.count) { for (const q of this.queue) { this.touch(q); } }', '  }',
    '  async drain(src: AsyncIterable<string>): Promise<void> {', '    for await (const s of src) { this.touch(s); }',
    '    for (const [a, { b }] of this.pairs) { this.touch(a + b); }', '    for (;;) { break; }', '  }',
    '  touch(v: string): void { this.count++; }', '}') -join "`n"

# What a case asks of a map: the loops, and the loop each call and assignment sits in (by that loop's own line).
function Get-TsLoopFacts([string]$Db) {
    $loops = "SELECT 'lp=' || loops.line || ':' || loops.kind || ':' || loops.source || ':' || $(Get-NodeRootListSql 'loops__variables' 'loops.id') || ':' || " +
        "coalesce(loops.`"await`", 0) || ':' || coalesce(loops.incrementor_source, '-') || ':' || (SELECT count(*) FROM expressions x WHERE x.id = loops.expr) || ':' || " +
        "coalesce((SELECT p.line FROM loops p WHERE p.id = loops.loop), '-') || ':' || (loops.branch IS NOT NULL) || ':' || (loops.member IS NOT NULL) FROM loops"
    $calls = "SELECT 'c=' || c.line || ':' || c.callee || ':' || coalesce((SELECT l.line FROM loops l WHERE l.id = c.loop), '-') || ':' || (c.branch IS NOT NULL) FROM calls c"
    $writes = "SELECT 'a=' || a.line || ':' || a.target || ':' || a.operator || ':' || coalesce((SELECT l.line FROM loops l WHERE l.id = a.loop), '-') FROM assignments a"
    $r = Invoke-Gate --map-query $Db --width 0 --limit 0 --sql "$loops UNION ALL $calls UNION ALL $writes"
    Assert-Exit $r 0
    $kept = @($r.Lines | Where-Object { $_.Trim().StartsWith('lp=') -or $_.Trim().StartsWith('c=') -or $_.Trim().StartsWith('a=') } | ForEach-Object { $_.Trim() } | Sort-Object)
    return [pscustomobject]@{ Lines = $kept }
}

if ($script:TsRowsModules -and $script:TsRowsPython) {

Test-Case 'tsrows: every loop kind is a loops row with its header, its variables and the tree of its header' {
    $made = New-TsRowsDb (New-TsRowsWorkspace @{ 'apps/shop/src/walker.ts' = $script:TsLoopWalker }) -Shared
    Assert-Exit $made.Result 0
    $facts = Get-TsLoopFacts $made.Db
    # line:kind:source:variables:await:incrementor:expr rows:enclosing loop's line:in an if arm:owned
    Assert-Line $facts 'lp=8:for_of:this.items:["item"]:0:-:1:-:0:1'
    Assert-Line $facts 'lp=10:for:i < 3:["i"]:0:i++:1:8:0:1'
    Assert-Line $facts 'lp=16:for_in:this.note:["k"]:0:-:1:-:0:1'
    Assert-Line $facts 'lp=17:while:this.queue.length:[]:0:-:1:-:0:1'
    Assert-Line $facts 'lp=18:do:this.count > 0:[]:0:-:1:-:0:1'
    Assert-Line $facts 'lp=20:for_of:this.queue:["q"]:0:-:1:-:1:1'
    Assert-Line $facts 'lp=23:for_of:src:["s"]:1:-:1:-:0:1'
    Assert-Line $facts 'lp=24:for_of:this.pairs:["a","b"]:0:-:1:-:0:1'
    Assert-Line $facts 'lp=25:for::[]:0:-:0:-:0:1'
    # THE GUARD OF THE CLOSURE: no loop is a `branches` row - the one `if` is the only arm.
    $b = Invoke-TsRowsQ $made.Db "SELECT 'br=' || b.sense || ':' || b.line FROM branches b JOIN files f ON f.id = b.file WHERE f.path LIKE '%walker.ts' ORDER BY b.line"
    Assert-Line $b 'br=then:20'
    Assert-Equal (@($b.Lines | Where-Object { $_.Trim().StartsWith('br=') }).Count) 1 'branches rows'
}

Test-Case 'tsrows: a row in a loop body carries the innermost loop, a callback in it does too, the code after it does not' {
    $made = New-TsRowsDb (New-TsRowsWorkspace @{ 'apps/shop/src/walker.ts' = $script:TsLoopWalker }) -Shared
    Assert-Exit $made.Result 0
    $facts = Get-TsLoopFacts $made.Db
    # line:callee:line of its loop:in an if arm
    Assert-Line $facts 'c=9:this.touch:8:0'
    Assert-Line $facts "c=11:this.touch:10:0"
    Assert-Line $facts 'c=13:this.items.forEach:8:0'
    Assert-Line $facts 'c=13:this.touch:8:0'   # a callback in the body is still in the loop (rows 71: lexical, as a branch)
    Assert-Line $facts "c=15:this.touch:-:0"
    Assert-Line $facts 'c=17:this.queue.pop:17:0'
    Assert-Line $facts 'c=20:this.touch:20:1'
    Assert-Line $facts 'c=23:this.touch:23:0'
    # An update writes, and one in a loop body says so.
    Assert-Line $facts 'a=16:count:+=:16'
    Assert-Line $facts 'a=18:count:--:18'
    Assert-Line $facts 'a=27:count:++:-'
    # The incrementor is the header of the inner loop, in the OUTER loop's body.
    Assert-Line $facts 'a=10:i:++:8'
}

# Four conditionals no row sits under an arm of (rows 71): a callee receiver, an index, a `void` operand and a `??` receiver - each was
# described only when something under an arm asked, so none had an `expressions` row. Line 9 is the control: a call under each arm.
$script:TsCondFiles = @{
    'apps/shop/src/cond.ts' = @('export class Cond {', '  list = [1, 2];', '  go(a: boolean, b: string | null): void {',
        '    (a ? this : this).use(1);', '    this.list[a ? 0 : 1];', '    void (a || b);', '    (b ?? a).toString();', '  }',
        '  arm(a: boolean): void { a ? this.use(1) : this.use(2); }', '  use(n: unknown): void { void n; }', '}') -join "`n"
}

Test-Case 'tsrows: every code conditional has its expressions row, and an arm row names it' {
    $made = New-TsRowsDb (New-TsRowsWorkspace $script:TsCondFiles) -Shared
    Assert-Exit $made.Result 0
    $r = Invoke-TsRowsQ $made.Db ("SELECT 'cond=' || x.role || ':' || x.line || ':' || x.source AS r FROM expressions x JOIN files f ON f.id = x.file " +
        "WHERE f.path LIKE '%cond.ts' AND x.role IN ('ternary', 'logic') UNION ALL SELECT 'count=' || count(*) FROM expressions x " +
        "JOIN files f ON f.id = x.file WHERE f.path LIKE '%cond.ts' AND x.role IN ('ternary', 'logic') AND x.line BETWEEN 4 AND 7")
    Assert-Line $r 'count=4'
    Assert-Line $r 'cond=ternary:4:a'
    Assert-Line $r 'cond=ternary:5:a'
    Assert-Line $r 'cond=logic:6:a'
    Assert-Line $r 'cond=logic:7:b'
    # The control: each call under an arm of the line-9 ternary still names that ternary's row (`!` for the false arm).
    $arm = Invoke-TsRowsQ $made.Db ("SELECT 'arm=' || (substr(c.value, 1, 1) = '!') || ':' || x.role || ':' || x.line AS r FROM calls k " +
        "JOIN calls__choices c ON c.owner = k.id JOIN expressions x ON x.id = ltrim(c.value, '!') WHERE k.line = 9 AND k.callee = 'this.use'")
    Assert-Line $arm 'arm=0:ternary:9'
    Assert-Line $arm 'arm=1:ternary:9'
    Assert-Equal (@($arm.Lines | Where-Object { $_.Trim().StartsWith('arm=') }).Count) 2 'calls under an arm'
}

Test-Case 'tsrows: a bare expression statement is an expressions row, a call or a write is not one' {
    $made = New-TsRowsDb (New-TsRowsWorkspace @{ 'apps/shop/src/walker.ts' = $script:TsLoopWalker }) -Shared
    Assert-Exit $made.Result 0
    $r = Invoke-TsRowsQ $made.Db ("SELECT 's=' || x.line || ':' || x.source FROM expressions x " +
        "JOIN files f ON f.id = x.file WHERE x.role = 'statement' AND f.path LIKE '%walker.ts' ORDER BY x.line")
    Assert-Line $r 's=19:this.note.a'
    Assert-Equal (@($r.Lines | Where-Object { $_.Trim().StartsWith('s=') }).Count) 1 'statement rows'
}

Test-Case 'tsrows: a partial run keeps every loop id valid and writes the loops a full run writes' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/walker.ts' = $script:TsLoopWalker
        'apps/shop/src/other.ts' = "export class Other {`n  go(xs: string[]): void {`n    for (const x of xs) { this.go([x]); }`n  }`n}`n"
    }
    Assert-Exit (New-TsRowsDb $tree).Result 0
    # Lines move: every row of the file read again is minted anew, the kept file's loop ids stay.
    $path = Join-Path $tree 'apps/shop/src/walker.ts'
    Set-Content -LiteralPath $path -Value ("// moved`n" + (Get-Content -LiteralPath $path -Raw)) -NoNewline
    $partial = New-TsRowsDb $tree
    Assert-Exit $partial.Result 0
    Assert-Line $partial.Result 'put back'
    $kept = Get-TsLoopFacts $partial.Db
    Assert-Line $kept 'lp=9:for_of:this.items:["item"]:0:-:1:-:0:1'
    Assert-Line $kept 'c=10:this.touch:9:0'
    Assert-Line $kept 'c=3:this.go:3:0'
    $dangling = Invoke-TsRowsQ $partial.Db ("SELECT 'dangling=' || count(*) FROM (SELECT loop AS l FROM calls UNION ALL SELECT loop FROM assignments " +
        "UNION ALL SELECT loop FROM loops UNION ALL SELECT loop FROM locals) t WHERE l IS NOT NULL AND l NOT IN (SELECT id FROM loops)")
    Assert-Line $dangling 'dangling=0'
    $copy = "$tree-fresh"
    Register-Tree $copy
    if (Test-Path $copy) { Remove-Item $copy -Recurse -Force }
    Copy-Item -LiteralPath $tree -Destination $copy -Recurse
    Remove-Item (Join-Path $copy 'map.sqlite*') -Force -ErrorAction SilentlyContinue
    Remove-Item (Join-Path $copy '.fbt') -Recurse -Force -ErrorAction SilentlyContinue
    $full = New-TsRowsDb $copy
    Assert-Exit $full.Result 0
    Assert-NoLine $full.Result 'put back'
    Assert-Equal ((Get-TsLoopFacts $partial.Db).Lines -join "`n") ((Get-TsLoopFacts $full.Db).Lines -join "`n") 'the loops after a partial run'
}


# (rows 80) THE BODY OF A PROTOTYPE METHOD UNDER `if (!X.prototype.m)` CARRIES THE ARM'S `branch` as any function's body does (`branch` is lexical, `TsDecls/TsDeclRows.mjs::prototypeRow` walks the body from the statement's walk): its calls, locals and
# returns were NULL. The method's row stays detached on purpose (the row is the method). Control: the same assignment with no guard has no branch, and `note(this)` is a call on both lines.
# Lines: the guard on 3, its body on 5 to 7, the unguarded method on 10 to 12; `g` (an ordinary `if`, line 16) gives `calls`, `locals` and `returns` their `branch` column on any build.
Test-Case 'tsrows: a prototype method defined under an if arm has the arm on its body rows' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/ext.ts' = "declare global { interface Array<T> { demoLast(): T | undefined; demoFirst(): T | undefined; } }`nfunction note(x: unknown): void {}`n" +
            "if (!Array.prototype.demoLast) {`n  Array.prototype.demoLast = function () {`n    note(this);`n    const n = this.length;`n    return this[n - 1];`n  };`n}`n" +
            "Array.prototype.demoFirst = function () {`n  note(this);`n  return this[0];`n};`nexport {};`nexport function g(a: boolean) {`n  if (a) { const z = 1; note(z); return z; }`n  return 0;`n}`n"
    }
    $made = New-TsRowsDb $tree -Shared
    Assert-Exit $made.Result 0
    $in = "FROM {0} t JOIN files f ON f.id = t.file WHERE f.path LIKE '%ext.ts'"
    $r = Invoke-TsRowsQ $made.Db ("SELECT 'call=' || t.line || ':' || (t.branch IS NOT NULL) AS r " + ($in -f 'calls') + " AND t.callee = 'note' UNION ALL " +
        "SELECT 'local=' || t.line || ':' || (t.branch IS NOT NULL) " + ($in -f 'locals') + " UNION ALL SELECT 'ret=' || t.line || ':' || (t.branch IS NOT NULL) " + ($in -f 'returns'))
    Assert-Line $r 'call=5:1'
    Assert-Line $r 'local=6:1'
    Assert-Line $r 'ret=7:1'
    Assert-Line $r 'call=11:0'
    Assert-Line $r 'ret=12:0'
    # The method rows stay detached: no parent, no branch on the row itself.
    Assert-Line (Invoke-TsRowsQ $made.Db ("SELECT 'fn=' || t.name || ':' || t.form || ':' || (t.parent IS NULL) AS r " + ($in -f 'functions') + " AND t.form = 'prototype-method' ORDER BY t.line")) 'fn=Array.prototype.demoLast:prototype-method:1'
}
}
