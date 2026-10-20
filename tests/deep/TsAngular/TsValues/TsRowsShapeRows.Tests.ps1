<#
    EVERY ANONYMOUS OBJECT TYPE IS A ROW AND A TEMPLATE READ GOES THROUGH IT (rows 71, `TsDecls/TsExpr/TsShapes.mjs`,
    `TsDecls/TsTypeRef.mjs::siteOf`, `TsTpl/TsTplReads.mjs`): a `shapes` row per type literal and object literal, a `shape_members` row
    per member, a `type_ref` to one is `{shape: true, file, line, col}` (the place of its `{`), and a read chained off `labels.nested.deep`,
    `result?.error?.message` or `vm.a` lands on the member row of each literal. An index signature - in a literal, an interface or a
    `Record` - answers a keyed read. A `.json` source and a declaration file have no shapes. A partial run keeps every id valid.
    Its helpers are `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '../../TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

# Lines as numbered in the cases: the literals on 8, 9, 10 and 15, the index signatures on 4 and 10, `Item` on 3.
$script:TsShapeFiles = @{
    'apps/shop/tsconfig.app.json' = '{"compilerOptions":{"strict":true,"resolveJsonModule":true},"include":["src/**/*.ts","src/**/*.json"]}'
    'apps/shop/src/shape.component.ts' = @(
        "import { Component } from '@angular/core';",
        "import data from './data.json';",
        "export interface Item { label: string; }",
        "export interface Bag { [k: string]: Item; }",
        "@Component({ selector: 'app-shape', template: '<p>{{ labels.name }}</p><p>{{ labels.nested.deep }}</p><p>{{ result?.error?.message }}</p><p>{{ vm.a }}</p><p>{{ vm.b }}</p><p>{{ items[key].label }}</p><p>{{ bag[key].label }}</p><p>{{ rec[key].label }}</p><p>{{ list[0].label }}</p><p>{{ key }}</p>' })",
        "export class ShapeComponent {",
        "  key = 'k'; data = data;",
        "  labels = { name: 'x', nested: { deep: 1 } };",
        "  result: { error?: { message: string } } = {};",
        "  items: { [k: string]: Item } = {};",
        "  bag: Bag = {};",
        "  rec: Record<string, Item> = {};",
        "  list: Item[] = [];",
        "  vm = this.make();",
        "  make() { const a = this.key; return { a, b: 1 }; }",
        "}") -join "`n"
    'apps/shop/src/types.d.ts' = "export interface Decl { a: { b: string }; }`ndeclare const k: { x: number };`n"
    'apps/shop/src/data.json' = '{"alpha":{"beta":1}}'
    # A LOOP VARIABLE has an owner and no ref: over a list of literals it is the literal, over a list of index-signature types a keyed read holds `Item`.
    'apps/shop/src/loop.component.ts' = @'
import { Component } from '@angular/core';
import { Item } from './shape.component';
@Component({
  selector: 'app-loop',
  template: `<p *ngFor="let m of rows">{{ m[key].label }}</p><p *ngFor="let t of tags">{{ t.id }}</p><p>{{ box.v }}</p>`,
})
export class LoopComponent {
  key = 'k';
  rows: { [k: string]: Item }[] = [];
  tags = [{ id: 1 }];
  box = { get v() { return 1; }, set v(x: number) { void x; } };
}
'@
    'apps/shop/src/other.ts' = "export const other = { one: 1, two: { three: 3 } };`nexport type Kind = { id: number };`n"
}

# The literals and their members, by place - never by id.
function Get-TsShapeFacts([string]$Db) {
    $shapes = "SELECT 'sh=' || f.path || ':' || s.kind || ':' || s.line || ':' || (SELECT count(*) FROM shape_members m WHERE m.shape = s.id) AS r " +
        "FROM shapes s JOIN files f ON f.id = s.file"
    $members = "SELECT 'sm=' || f.path || ':' || m.line || ':' || m.name || ':' || m.kind || ':' || coalesce(m.optional, 0) || ':' || coalesce(m.type, '-') AS r " +
        "FROM shape_members m JOIN files f ON f.id = m.file"
    $r = Invoke-Gate --map-query $Db --width 0 --limit 0 --sql "$shapes UNION ALL $members"
    Assert-Exit $r 0
    return [pscustomobject]@{ Lines = @($r.Lines | Where-Object { $_.Trim().StartsWith('sh=') -or $_.Trim().StartsWith('sm=') } | ForEach-Object { $_.Trim() } | Sort-Object) }
}

# What each name of one template expression resolved to: `name:line:row prefix:kind/name of the shape member`.
function Get-TsShapeTargets([string]$Db, [string]$Source) {
    $name = Get-TsFieldSql 'expressions__ast' 'n' 'name'
    $line = Get-TsFieldSql 'expressions__ast' 'n' 'line'
    $row = Get-TsFieldSql 'expressions__ast' 'n' 'row'
    return Invoke-TsRowsQ $Db ("SELECT 'tg=' || $name || ':' || $line || ':' || substr($row, 1, instr($row, ':') - 1) || ':' || " +
        "coalesce((SELECT m.kind || '/' || m.name FROM shape_members m WHERE m.id = $row), '-') AS r FROM expressions x " +
        "JOIN expressions__ast n ON n.owner = x.id AND n.role = 'target' WHERE x.source = '$Source' ORDER BY n.n")
}

Test-Case 'tsrows: every type literal and object literal is a shapes row with a row per member' {
    $made = New-TsRowsDb (New-TsRowsWorkspace $script:TsShapeFiles) -Shared
    Assert-Exit $made.Result 0
    $facts = Get-TsShapeFacts $made.Db
    # path:kind:line:members - a nested literal is a row of its own, an empty `{}` too.
    Assert-Line $facts 'sh=apps/shop/src/shape.component.ts:object:8:2'
    Assert-Line $facts 'sh=apps/shop/src/shape.component.ts:object:8:1'
    Assert-Line $facts 'sh=apps/shop/src/shape.component.ts:type:9:1'
    Assert-Line $facts 'sh=apps/shop/src/shape.component.ts:type:10:1'
    Assert-Line $facts 'sh=apps/shop/src/shape.component.ts:object:15:2'
    Assert-Line $facts 'sh=apps/shop/src/other.ts:object:1:2'
    Assert-Line $facts 'sh=apps/shop/src/other.ts:type:2:1'
    # path:line:name:kind:optional:type
    Assert-Line $facts 'sm=apps/shop/src/shape.component.ts:8:name:property:0:string'
    Assert-Line $facts 'sm=apps/shop/src/shape.component.ts:8:nested:property:0:{ deep: number; }'
    Assert-Line $facts 'sm=apps/shop/src/shape.component.ts:8:deep:property:0:number'
    Assert-Line $facts 'sm=apps/shop/src/shape.component.ts:9:error:signature:1:{ message: string }'
    Assert-Line $facts 'sm=apps/shop/src/shape.component.ts:9:message:signature:0:string'
    Assert-Line $facts 'sm=apps/shop/src/shape.component.ts:10:[k]:index:0:Item'
    Assert-Line $facts 'sm=apps/shop/src/shape.component.ts:15:a:shorthand:0:string'
    Assert-Line $facts 'sm=apps/shop/src/shape.component.ts:15:b:property:0:number'
    # THE NESTED LITERAL'S MEMBER IS TYPED BY A SHAPE REF: `nested` holds the `{ deep }` literal at its `{`.
    $shape = Get-TsFieldSql 'shape_members__type_ref' 'r' 'shape'
    $at = Get-TsFieldSql 'shape_members__type_ref' 'r' 'line'
    $col = Get-TsFieldSql 'shape_members__type_ref' 'r' 'col'
    $nest = Invoke-TsRowsQ $made.Db ("SELECT 'ref=' || $shape || ':' || $at || ':' || $col || ':' || (SELECT count(*) FROM shapes s2 WHERE s2.line = $at AND s2.col = $col) AS r FROM shape_members m " +
        "JOIN shape_members__type_ref r ON r.owner = m.id AND r.n = 0 WHERE m.name = 'nested'")
    Assert-Line $nest 'ref=1:8:33:1'
}

Test-Case 'tsrows: a read through an inferred object literal resolves every hop to its member row' {
    $made = New-TsRowsDb (New-TsRowsWorkspace $script:TsShapeFiles) -Shared
    Assert-Exit $made.Result 0
    $one = Get-TsShapeTargets $made.Db '{{ labels.name }}'
    Assert-Line $one 'tg=labels:8:m:-'
    Assert-Line $one 'tg=name:8:sm:property/name'
    $two = Get-TsShapeTargets $made.Db '{{ labels.nested.deep }}'
    Assert-Line $two 'tg=nested:8:sm:property/nested'
    Assert-Line $two 'tg=deep:8:sm:property/deep'
}

Test-Case 'tsrows: a read through a type literal, optional and nested, resolves to the signatures' {
    $made = New-TsRowsDb (New-TsRowsWorkspace $script:TsShapeFiles) -Shared
    Assert-Exit $made.Result 0
    $r = Get-TsShapeTargets $made.Db '{{ result?.error?.message }}'
    Assert-Line $r 'tg=result:9:m:-'
    Assert-Line $r 'tg=error:9:sm:signature/error'
    Assert-Line $r 'tg=message:9:sm:signature/message'
}

Test-Case 'tsrows: a read through what a method returns, shorthand or not, resolves to the member row' {
    $made = New-TsRowsDb (New-TsRowsWorkspace $script:TsShapeFiles) -Shared
    Assert-Exit $made.Result 0
    Assert-Line (Get-TsShapeTargets $made.Db '{{ vm.a }}') 'tg=a:15:sm:shorthand/a'
    Assert-Line (Get-TsShapeTargets $made.Db '{{ vm.b }}') 'tg=b:15:sm:property/b'
}

Test-Case 'tsrows: a keyed read of an index signature is what it holds, in a literal, an interface and a Record' {
    $made = New-TsRowsDb (New-TsRowsWorkspace $script:TsShapeFiles) -Shared
    Assert-Exit $made.Result 0
    # `Item.label` is the `type_members` row of the interface at line 3.
    Assert-Line (Get-TsShapeTargets $made.Db '{{ items[key].label }}') 'tg=label:3:tm:-'
    Assert-Line (Get-TsShapeTargets $made.Db '{{ bag[key].label }}') 'tg=label:3:tm:-'
    Assert-Line (Get-TsShapeTargets $made.Db '{{ rec[key].label }}') 'tg=label:3:tm:-'
    # THE CONTROLS: an array's element and a plain member resolved before.
    Assert-Line (Get-TsShapeTargets $made.Db '{{ list[0].label }}') 'tg=label:3:tm:-'
    Assert-Line (Get-TsShapeTargets $made.Db '{{ key }}') 'tg=key:7:m:-'
}

Test-Case 'tsrows: a loop variable over a list of literals is the literal, and a keyed read of one holds its index type' {
    $made = New-TsRowsDb (New-TsRowsWorkspace $script:TsShapeFiles) -Shared
    Assert-Exit $made.Result 0
    Assert-Line (Get-TsShapeTargets $made.Db '{{ t.id }}') 'tg=id:10:sm:property/id'
    Assert-Line (Get-TsShapeTargets $made.Db '{{ m[key].label }}') 'tg=label:3:tm:-'
    # An ACCESSOR PAIR of a literal is one property, read by its getter (rows 70, as a class's).
    Assert-Line (Get-TsShapeTargets $made.Db '{{ box.v }}') 'tg=v:11:sm:getter/v'
}

Test-Case 'tsrows: a json source and a declaration file have no shapes, a source file does' {
    $made = New-TsRowsDb (New-TsRowsWorkspace $script:TsShapeFiles) -Shared
    Assert-Exit $made.Result 0
    Assert-Line (Invoke-TsRowsQ $made.Db "SELECT 'json=' || count(*) AS r FROM files WHERE path LIKE '%data.json'") 'json=1'
    Assert-Line (Invoke-TsRowsQ $made.Db ("SELECT 'bare=' || count(*) AS r FROM shapes s JOIN files f ON f.id = s.file " +
        "WHERE f.path LIKE '%data.json' OR f.path LIKE '%.d.ts'")) 'bare=0'
    Assert-Line (Invoke-TsRowsQ $made.Db ("SELECT 'src=' || (count(*) > 0) AS r FROM shapes s JOIN files f ON f.id = s.file " +
        "WHERE f.path LIKE '%shape.component.ts'")) 'src=1'
}

Test-Case 'tsrows: a partial run keeps every shape id valid, writes the shapes a full run writes and still resolves the reads' {
    $tree = New-TsRowsWorkspace $script:TsShapeFiles
    Assert-Exit (New-TsRowsDb $tree).Result 0
    # Lines move in the file read again; the kept file's rows stay.
    $path = Join-Path $tree 'apps/shop/src/other.ts'
    Set-Content -LiteralPath $path -Value ("// moved`n" + (Get-Content -LiteralPath $path -Raw)) -NoNewline
    $partial = New-TsRowsDb $tree
    Assert-Exit $partial.Result 0
    Assert-Line $partial.Result 'put back'
    $kept = Get-TsShapeFacts $partial.Db
    Assert-Line $kept 'sh=apps/shop/src/other.ts:object:2:2'
    Assert-Line $kept 'sh=apps/shop/src/shape.component.ts:object:8:2'
    $dangling = Invoke-TsRowsQ $partial.Db ("SELECT 'dangling=' || count(*) FROM shape_members WHERE shape NOT IN (SELECT id FROM shapes)")
    Assert-Line $dangling 'dangling=0'
    Assert-Line (Get-TsShapeTargets $partial.Db '{{ labels.nested.deep }}') 'tg=deep:8:sm:property/deep'
    $copy = "$tree-fresh"
    Register-Tree $copy
    if (Test-Path $copy) { Remove-Item $copy -Recurse -Force }
    Copy-Item -LiteralPath $tree -Destination $copy -Recurse
    Remove-Item (Join-Path $copy 'map.sqlite*') -Force -ErrorAction SilentlyContinue
    Remove-Item (Join-Path $copy '.fbt') -Recurse -Force -ErrorAction SilentlyContinue
    $full = New-TsRowsDb $copy
    Assert-Exit $full.Result 0
    Assert-NoLine $full.Result 'put back'
    Assert-Equal ((Get-TsShapeFacts $partial.Db).Lines -join "`n") ((Get-TsShapeFacts $full.Db).Lines -join "`n") 'the shapes after a partial run'
}

}
