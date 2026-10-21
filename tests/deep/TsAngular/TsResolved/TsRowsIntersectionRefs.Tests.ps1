<#
    AN INTERSECTION NAMES WHAT ITS PARTS NAME (#97, `TsDecls/TsTypeRef.mjs::refsOfParts`), and a template read follows a member
    whose `type_refs` lists several types, resolving a name ONE of them declares (`TsTpl/TsTplReads.mjs::ownerOfRow`). A hop whose
    type is unknown ends the chain - it kept the previous owner and named that owner's member. Its helpers are `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '../../TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

$script:TsIntersectionFiles = @{
    'apps/shop/src/box.component.ts' = "import { Component } from '@angular/core';`n" +
        "export interface Flags { On: boolean; }`n" +
        "export interface Payload { Flags: Flags; }`n" +
        "export interface Extra { Note: string; }`n" +
        "export class Box<TPayload> { Payload!: TPayload & { Extra?: Extra; }; Both!: TPayload & Extra; }`n" +
        "export type PayloadBox = Box<Payload>;`n" +
        "class Holder { get<T>(): T { return {} as T; } }`n" +
        "@Component({ selector: 'app-box', template: '<p>{{ vm?.Payload.Flags.On }}</p><p>{{ vm?.Both.Note }}</p><p>{{ vm?.Both.Flags.On }}</p><p>{{ loose.Note }}</p>' })`n" +
        "export class BoxComponent {`n" +
        "  holder = new Holder();`n" +
        "  get vm() { return this.holder.get<PayloadBox>(); }`n" +
        "  loose: any;`n" +
        "  Note = 'own';`n" +
        "}`n"
}

# The names an expression's `Read` nodes resolved, in node order.
function Get-TsIntersectionTargets([string]$Db, [string]$Source) {
    return Invoke-TsRowsQ $Db ("SELECT 'targets=' || group_concat(n.value, ',') AS r FROM (SELECT j.owner, j.n FROM expressions x " +
        "JOIN expressions__ast j ON j.owner = x.id AND j.role = 'target' WHERE x.source = '$Source' ORDER BY j.n) t " +
        "JOIN expressions__ast n ON n.owner = t.owner AND n.parent = t.n AND n.role = 'name'")
}

Test-Case 'tsrows: an intersection names what its parts name, and reads resolve past it (#97)' {
    $made = New-TsRowsDb (New-TsRowsWorkspace $script:TsIntersectionFiles)
    Assert-Exit $made.Result 0
    # ONE named part: `Payload & { Extra?: Extra }` names Payload.
    Assert-Line (Get-TsIntersectionTargets $made.Db '{{ vm?.Payload.Flags.On }}') 'targets=vm,Payload,Flags,On'
    # TWO named parts: `Payload & Extra` lists both, and a name one of them declares resolves.
    Assert-Line (Get-TsIntersectionTargets $made.Db '{{ vm?.Both.Note }}') 'targets=vm,Both,Note'
    Assert-Line (Get-TsIntersectionTargets $made.Db '{{ vm?.Both.Flags.On }}') 'targets=vm,Both,Flags,On'
}

# AN INTERSECTION IS ALL OF ITS PARTS (rows 72): a member BOTH parts declare is ONE property whose first declaration is the part written first - the
# checker's `declarations[0]` - never "two rows, no answer" (that is a union's, where a member of one arm may not be the other's). The fixture's
# lines are pinned.
$script:TsAndFiles = @{
    'apps/shop/src/and.models.ts' = "export interface InA { shallow: number; }`n" +
        "export interface InB { deep: number; }`n" +
        "export class Alpha {`n" +
        "  id = 1;`n" +
        "  inner!: InA;`n" +
        "}`n" +
        "export class Gamma {`n" +
        "  id = 2;`n" +
        "}`n" +
        "export type Perf = {`n  month: number;`n};`n" +
        "export type Instr = {`n  perf: Perf;`n};`n" +
        "export type Item = {`n  id: number;`n  instrument: Instr;`n};`n" +
        "export type InstrDto = {`n  instrument: Instr;`n  oneTime?: number;`n};`n" +
        "export type ViewDto = InstrDto & {`n  extra?: number;`n};`n"
    'apps/shop/src/and.component.ts' = "import { Component } from '@angular/core';`n" +
        "import { Alpha, Gamma, InB, Item, ViewDto } from './and.models';`n" +
        "@Component({ selector: 'app-and', standalone: true, template: '<p>{{ x.id }}</p><p>{{ y.id }}</p><p>{{ z.inner.deep }}</p><p>{{ z.inner.shallow }}</p><p>{{ u.id }}</p><p>{{ w.extra }}</p><p>{{ w.id }}</p><p>{{ q.instrument?.perf?.month }}</p><p>{{ q.oneTime }}</p><p>{{ q.extra }}</p>' })`n" +
        "export class AndComponent {`n" +
        "  x!: Alpha & { id: number };`n" +
        "  y!: { id: number } & Alpha;`n" +
        "  z!: { inner: InB } & Alpha;`n" +
        "  u!: Alpha | Gamma;`n" +
        "  w!: Gamma & Alpha & { extra: number };`n" +
        "  q!: Item & ViewDto;`n" +
        "}`n"
}

# `read=<name>=<row prefix>@<file>:<line>` of the declaration each Read of the expression's `target` names, or `none`.
function Get-TsAndTargets([string]$Db, [string]$Source) {
    return Invoke-TsRowsQ $Db ("SELECT 'read=' || nm.value || '=' || (CASE WHEN rw.value IS NULL THEN 'none' ELSE " +
        "substr(rw.value, 1, instr(rw.value, ':') - 1) || '@' || tf.value || ':' || tl.value END) AS r " +
        "FROM expressions x JOIN expressions__ast k ON k.owner = x.id AND k.role = 'k' AND k.value IN ('Read', 'SafeRead') " +
        "JOIN expressions__ast nm ON nm.owner = x.id AND nm.parent = k.parent AND nm.role = 'name' " +
        "LEFT JOIN expressions__ast t ON t.owner = x.id AND t.parent = k.parent AND t.role = 'target' " +
        "LEFT JOIN expressions__ast rw ON rw.owner = x.id AND rw.parent = t.n AND rw.role = 'row' " +
        "LEFT JOIN expressions__ast tf ON tf.owner = x.id AND tf.parent = t.n AND tf.role = 'file' " +
        "LEFT JOIN expressions__ast tl ON tl.owner = x.id AND tl.parent = t.n AND tl.role = 'line' " +
        "WHERE x.source = '$Source' ORDER BY nm.n")
}

Test-Case 'tsrows: a member both parts of an intersection declare is the first part written''s, and the chain goes on through every part' {
    $made = New-TsRowsDb (New-TsRowsWorkspace $script:TsAndFiles)
    Assert-Exit $made.Result 0
    $m = 'apps/shop/src/and.models.ts'; $c = 'apps/shop/src/and.component.ts'
    # The class first: `Alpha & { id }` is Alpha's `id`; the literal first: `{ id } & Alpha` is the literal's.
    Assert-Line (Get-TsAndTargets $made.Db '{{ x.id }}') "read=id=m@${m}:4"
    Assert-Line (Get-TsAndTargets $made.Db '{{ y.id }}') "read=id=sm@${c}:6"
    # `inner` is declared by both with different types and `deep` by the literal's alone: the chain continues as an intersection of the two.
    Assert-Line (Get-TsAndTargets $made.Db '{{ z.inner.deep }}') "read=deep=tm@${m}:2"
    Assert-Line (Get-TsAndTargets $made.Db '{{ z.inner.shallow }}') "read=shallow=tm@${m}:1"
    # SEVERAL NAMED PARTS AND A LITERAL (`Gamma & Alpha & { .. }`): the literal's own name resolves, and a name two classes declare is the first written's.
    Assert-Line (Get-TsAndTargets $made.Db '{{ w.extra }}') "read=extra=sm@${c}:9"
    Assert-Line (Get-TsAndTargets $made.Db '{{ w.id }}') "read=id=m@${m}:8"
    # AN ALIAS OF AN INTERSECTION IN ONE (`Item & ViewDto`, `ViewDto = InstrDto & { .. }`): the name both aliases declare is the first's, and the chain goes on.
    Assert-Line (Get-TsAndTargets $made.Db '{{ q.instrument?.perf?.month }}') "read=instrument=tm@${m}:18"
    Assert-Line (Get-TsAndTargets $made.Db '{{ q.instrument?.perf?.month }}') "read=perf=tm@${m}:14"
    Assert-Line (Get-TsAndTargets $made.Db '{{ q.instrument?.perf?.month }}') "read=month=tm@${m}:11"
    Assert-Line (Get-TsAndTargets $made.Db '{{ q.oneTime }}') "read=oneTime=tm@${m}:22"
    Assert-Line (Get-TsAndTargets $made.Db '{{ q.extra }}') "read=extra=sm@${m}:25"
    # The control that was always right: a UNION both arms declare has no one answer.
    Assert-Line (Get-TsAndTargets $made.Db '{{ u.id }}') 'read=id=none'
}

Test-Case 'tsrows: a hop of unknown type ends the read chain, never naming the previous owner''s member' {
    $made = New-TsRowsDb (New-TsRowsWorkspace $script:TsIntersectionFiles)
    Assert-Exit $made.Result 0
    # `loose: any` - its `Note` is not the component's own `Note`.
    $loose = Get-TsIntersectionTargets $made.Db '{{ loose.Note }}'
    Assert-Line $loose 'targets=loose'
    Assert-NoLine $loose 'loose,Note'
}

}
