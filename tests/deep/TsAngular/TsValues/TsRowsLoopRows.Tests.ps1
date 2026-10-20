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

Test-Case 'tsrows: a row in a loop body carries the innermost loop, a callback and the code after it do not' {
    $made = New-TsRowsDb (New-TsRowsWorkspace @{ 'apps/shop/src/walker.ts' = $script:TsLoopWalker }) -Shared
    Assert-Exit $made.Result 0
    $facts = Get-TsLoopFacts $made.Db
    # line:callee:line of its loop:in an if arm
    Assert-Line $facts 'c=9:this.touch:8:0'
    Assert-Line $facts "c=11:this.touch:10:0"
    Assert-Line $facts 'c=13:this.items.forEach:8:0'
    Assert-Line $facts 'c=13:this.touch:-:0'
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

}
