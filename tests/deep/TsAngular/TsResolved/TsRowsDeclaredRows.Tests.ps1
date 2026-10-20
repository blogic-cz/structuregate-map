<#
    ROWS A SOURCE DECLARES AND THE MAP USED TO MISS (rows 70): a constructor parameter property is a `members` row
    (`TsDecls/TsClassRows.mjs::extractClass`), an ES `#name` is private (`TsModifiers.mjs`), a single-member enum's `type_ref` names
    the enum (`TsTypeRef.mjs::enumOfMember`) and a type literal nested in a union or a type argument has `type_members`
    (`TsTypeRows.mjs::nestedLiterals`). Its helpers are `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '../../TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

$script:TsDeclaredFiles = @{
    'apps/shop/src/decl.component.ts' = "import { Component } from '@angular/core';`n" +
        "export interface Cfg { id: string; }`n" +
        "export enum Kind { Alpha = 0 }`n" +
        "export interface Box { state: { a: string; b: number } | null; tab: NTab<{ img: string }>; }`n" +
        "export interface NTab<T> { item: T; }`n" +
        "export class Order { kind: Kind; one: Kind.Alpha; desc: Desc; }`n" +
        "@Component({ selector: 'app-decl', template: '<p>{{ cfg.id }}</p>' })`n" +
        "export class DeclComponent {`n" +
        "  #value = 1;`n" +
        "  constructor(public cfg: Cfg, private readonly dep: Cfg) {}`n" +
        "}`n" +
        "export type Desc = Readonly<{ a: string }>;`n"
    # Every place a type literal can sit in a type alias's or an interface's type.
    'apps/shop/src/shapes.ts' = "export interface Base { id: string; }`n" +
        "export type Model = Base & { Input: { Flag?: Base } };`n" +
        "export type Res = { [k: string]: Base } | Base;`n" +
        "export type Dict = { [k: string]: number };`n" +
        "export type Rows = { value: number; date: string }[];`n" +
        "export type Wrap = Array<{ unit: string }>;`n" +
        "export type Either = { left: string } | { right: number };`n" +
        "export interface Prop { x: Base & { b: string }; }`n" +
        "export enum Solo { Alpha = 0 }`n" +
        "export class Holder { runSolo(k: Solo | null) { void k; } }`n"
    # `Item` sits on line 7: the array cases below name it by that line.
    'apps/shop/src/conv.ts' = "export class Converter {`n" +
        "  toItems(): Item[] { return []; }`n" +
        "  items: Item[] = [];`n" +
        "  arr: Array<Item> = [];`n" +
        "  toNumbers(): number[] { return []; }`n" +
        "}`n" +
        "export interface Item { id: string; }`n"
}

# The type_ref name of the Nth parameter of `Holder.runSolo`.
function Get-TsEnumParamRef([string]$Db, [int]$Ord = 0) {
    return Invoke-TsRowsQ $Db ("SELECT 'param=' || zn.value AS r FROM members m JOIN members__params zi ON zi.owner = m.id AND zi.parent = 0 AND zi.ord = $Ord " +
        "JOIN members__params zr ON zr.owner = zi.owner AND zr.parent = zi.n AND zr.role = 'type_ref' " +
        "JOIN members__params zn ON zn.owner = zr.owner AND zn.parent = zr.n AND zn.role = 'name' WHERE m.name = 'runSolo'")
}

function Get-TsDeclaredTargets([string]$Db, [string]$Source) {
    return Invoke-TsRowsQ $Db ("SELECT 'targets=' || group_concat(n.value, ',') AS r FROM (SELECT j.owner, j.n FROM expressions x " +
        "JOIN expressions__ast j ON j.owner = x.id AND j.role = 'target' WHERE x.source = '$Source' ORDER BY j.n) t " +
        "JOIN expressions__ast n ON n.owner = t.owner AND n.parent = t.n AND n.role = 'name'")
}

Test-Case 'tsrows: a constructor parameter property is a members row and a template read resolves through it' {
    $made = New-TsRowsDb (New-TsRowsWorkspace $script:TsDeclaredFiles)
    Assert-Exit $made.Result 0
    $rows = Invoke-TsRowsQ $made.Db ("SELECT 'param=' || m.name || ':' || m.kind || ':' || m.visibility || ':' || m.readonly || ':' || m.parameter AS r " +
        "FROM members m JOIN classes c ON c.id = m.class WHERE c.name = 'DeclComponent' AND m.parameter = 1 ORDER BY m.name")
    Assert-Line $rows 'param=dep:property:private:1:1'
    Assert-Line $rows 'param=cfg:property:public:0:1'
    # The control: the constructor itself is still one member row.
    Assert-Line (Invoke-TsRowsQ $made.Db ("SELECT 'ctor=' || count(*) AS r FROM members m JOIN classes c ON c.id = m.class " +
        "WHERE c.name = 'DeclComponent' AND m.kind = 'constructor'")) 'ctor=1'
    Assert-Line (Get-TsDeclaredTargets $made.Db '{{ cfg.id }}') 'targets=cfg,id'
}

Test-Case 'tsrows: an ES #private member is private' {
    $made = New-TsRowsDb (New-TsRowsWorkspace $script:TsDeclaredFiles)
    Assert-Exit $made.Result 0
    Assert-Line (Invoke-TsRowsQ $made.Db "SELECT 'vis=' || name || ':' || visibility AS r FROM members WHERE name = '#value'") 'vis=#value:private'
}

Test-Case 'tsrows: a single-member enum type_ref names the enum, and a written member names the member' {
    $made = New-TsRowsDb (New-TsRowsWorkspace $script:TsDeclaredFiles)
    Assert-Exit $made.Result 0
    $r = Invoke-TsRowsQ $made.Db ("SELECT 'ref=' || m.name || ':' || t.value AS r FROM members m JOIN members__type_ref t ON t.owner = m.id " +
        "AND t.role = 'name' WHERE m.name IN ('kind', 'one') ORDER BY m.name")
    Assert-Line $r 'ref=kind:Kind'
    Assert-Line $r 'ref=one:Alpha'
}

Test-Case 'tsrows: type_ref carries the type alias WRITTEN as alias, though it names what the checker reaches' {
    $made = New-TsRowsDb (New-TsRowsWorkspace $script:TsDeclaredFiles)
    Assert-Exit $made.Result 0
    $r = Invoke-TsRowsQ $made.Db ("SELECT 'alias=' || m.name || ':' || a.value || ':' || (SELECT l.value FROM members__type_ref l WHERE l.owner = al.owner AND l.parent = al.n AND l.role = 'line') AS r " +
        "FROM members m JOIN members__type_ref al ON al.owner = m.id AND al.role = 'alias' " +
        "JOIN members__type_ref a ON a.owner = al.owner AND a.parent = al.n AND a.role = 'name' WHERE m.name IN ('desc', 'kind')")
    Assert-Line $r 'alias=desc:Desc:12'
    # The control: the root is still what the checker reached, and an enum written directly has no alias.
    Assert-Line (Invoke-TsRowsQ $made.Db ("SELECT 'root=' || t.value AS r FROM members m JOIN members__type_ref t ON t.owner = m.id AND t.parent = 0 AND t.role = 'name' " +
        "WHERE m.name = 'desc'")) 'root=Readonly'
    Assert-NoLine $r 'alias=kind'
}

Test-Case 'tsrows: a type literal nested in a union or a type argument has its type_members' {
    $made = New-TsRowsDb (New-TsRowsWorkspace $script:TsDeclaredFiles)
    Assert-Exit $made.Result 0
    $r = Invoke-TsRowsQ $made.Db "SELECT 'tm=' || path AS r FROM type_members ORDER BY path"
    Assert-Line $r 'tm=state'
    Assert-Line $r 'tm=state.a'
    Assert-Line $r 'tm=state.b'
    Assert-Line $r 'tm=tab.img'
}

Test-Case 'tsrows: an array-typed member names its ELEMENT declaration (type_ref.element), a primitive array none' {
    $made = New-TsRowsDb (New-TsRowsWorkspace $script:TsDeclaredFiles)
    Assert-Exit $made.Result 0
    # ONLY the element's name and line are selected, never beside `type`: the text `Item[]` would satisfy a bare match.
    $r = Invoke-TsRowsQ $made.Db ("SELECT 'elem=' || m.name || '=' || coalesce((SELECT n.value || ':' || l.value FROM members__type_ref r " +
        "JOIN members__type_ref e ON e.owner = r.owner AND e.parent = r.n AND e.role = 'element' " +
        "JOIN members__type_ref n ON n.owner = e.owner AND n.parent = e.n AND n.role = 'name' " +
        "JOIN members__type_ref l ON l.owner = e.owner AND l.parent = e.n AND l.role = 'line' " +
        "WHERE r.owner = m.id AND r.n = 0), '-') AS r FROM members m JOIN classes c ON c.id = m.class " +
        "WHERE c.name = 'Converter' ORDER BY m.name")
    Assert-Line $r 'elem=toItems=Item:7'
    Assert-Line $r 'elem=items=Item:7'
    Assert-Line $r 'elem=arr=Item:7'
    Assert-Line $r 'elem=toNumbers=-'
}

function Get-TsShapeRows([string]$Db) {
    return Invoke-TsRowsQ $Db ("SELECT 'tm=' || coalesce(a.name, i.name) || ':' || t.path || ':' || t.kind AS r FROM type_members t " +
        "LEFT JOIN type_aliases a ON a.id = t.owner LEFT JOIN interfaces i ON i.id = t.owner " +
        "JOIN files f ON f.id = t.file WHERE f.path LIKE '%shapes.ts' ORDER BY r")
}

Test-Case 'tsrows: a type literal anywhere in an alias or interface type has its type_members' {
    $made = New-TsRowsDb (New-TsRowsWorkspace $script:TsDeclaredFiles)
    Assert-Exit $made.Result 0
    $r = Get-TsShapeRows $made.Db
    # An intersection's literal, and the literal nested under its member.
    Assert-Line $r 'tm=Model:Input.Flag:PropertySignature'
    # A union holding an index signature, and the members of each branch of a union.
    Assert-Line $r 'tm=Res:IndexSignature:IndexSignature'
    Assert-Line $r 'tm=Either:left:PropertySignature'
    Assert-Line $r 'tm=Either:right:PropertySignature'
    # An array element and a type argument.
    Assert-Line $r 'tm=Rows:value:PropertySignature'
    Assert-Line $r 'tm=Wrap:unit:PropertySignature'
    # An interface property's intersection, and the controls: an alias that IS a literal.
    Assert-Line $r 'tm=Prop:x.b:PropertySignature'
    Assert-Line $r 'tm=Dict:IndexSignature:IndexSignature'
    # A member the checker computed is ONE row, never one more from the literal that declares it.
    Assert-Line (Invoke-TsRowsQ $made.Db ("SELECT 'input=' || count(*) AS r FROM type_members t JOIN type_aliases a ON a.id = t.owner " +
        "WHERE a.name = 'Model' AND t.path = 'Input'")) 'input=1'
}

Test-Case 'tsrows: a single-member enum through a union with null names the enum, strictNullChecks on' {
    $made = New-TsRowsDb (New-TsRowsWorkspace $script:TsDeclaredFiles)
    Assert-Exit $made.Result 0
    Assert-Line (Get-TsEnumParamRef $made.Db) 'param=Solo'
}

$script:TsLaxFiles = @{
    # strictNullChecks is OFF here: the checker drops `null` from `Solo | null` and answers the enum's lone member.
    'apps/shop/tsconfig.app.json' = '{"compilerOptions":{"resolveJsonModule":true},"include":["src/**/*.ts","src/**/*.json"]}'
    'apps/shop/src/lax.ts' = "import data from './data.json';`n" +
        "export enum Solo { Alpha = 0 }`n" +
        "export class Holder { runSolo(k: Solo | null, j: (Solo) | undefined) { void k; void j; void data; } }`n"
    'apps/shop/src/data.json' = '{"alpha":1}'
}

Test-Case 'tsrows: a single-member enum through a union with null names the enum, strictNullChecks off' {
    $made = New-TsRowsDb (New-TsRowsWorkspace $script:TsLaxFiles)
    Assert-Exit $made.Result 0
    Assert-Line (Get-TsEnumParamRef $made.Db 0) 'param=Solo'
    # Parenthesised and with `undefined`: `(Solo) | undefined`.
    Assert-Line (Get-TsEnumParamRef $made.Db 1) 'param=Solo'
}

Test-Case 'tsrows: a JSON source file the program holds writes no expressions row' {
    $made = New-TsRowsDb (New-TsRowsWorkspace $script:TsLaxFiles)
    Assert-Exit $made.Result 0
    # The control: the json IS a program file with a `files` row.
    Assert-Line (Invoke-TsRowsQ $made.Db "SELECT 'json=' || count(*) AS r FROM files WHERE path LIKE '%data.json'") 'json=1'
    Assert-Line (Invoke-TsRowsQ $made.Db ("SELECT 'exprs=' || count(*) AS r FROM expressions x JOIN files f ON f.id = x.file " +
        "WHERE f.path LIKE '%data.json'")) 'exprs=0'
}

}
