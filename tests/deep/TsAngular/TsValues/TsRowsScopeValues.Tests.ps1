<#
    A NAME THE TEMPLATE BINDS HOLDS A VALUE, NOT ONLY AN OWNER (rows 73, `TsTpl/TsTplReads.mjs::scopeAt`): a loop variable over a list of lists is itself a
    list, so the loop inside it takes ITS element (any depth of nesting, `*ngFor` and `@for`); and a `#ref` is the component instance of the element it sits
    on, or the directive whose `exportAs` it names - a plain element is the DOM node and types nothing. A ref still shadows the member of its name.
    Its helpers are `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '../../TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

$script:TsScopeFiles = @{
    'apps/shop/src/scope.models.ts' = @'
export class Item { Name = ''; }
export class Base { Tariff = ''; }
export class Slot<A = Base, B = Base> { Result!: B; Other!: A; }
export type Picked = Pick<Slot<unknown>, 'Result'>;
'@
    'apps/shop/src/scope.component.ts' = @'
import { Component, Directive } from '@angular/core';
import { Item, Picked } from './scope.models'; import { of } from 'rxjs';
@Component({ selector: 'app-child', standalone: true, template: '' })
export class ChildComponent {
  ready = false;
  get focused() { return true; }
}
@Directive({ selector: '[demoMark]', exportAs: 'demoExport, demoOther', standalone: true })
export class MarkDirective {
  marked = false;
}
@Component({
  selector: 'app-scope',
  standalone: true,
  imports: [ChildComponent, MarkDirective],
  template: `
<div *ngFor="let row of grid"><p *ngFor="let cell of row">{{ cell.Name }}</p></div>
<div *ngFor="let plane of cube"><div *ngFor="let line of plane"><p *ngFor="let deep of line">{{ deep.Name }}</p></div></div>
@for (r of grid; track $index) { @for (c of r; track c.Name) { <b>{{ c.Name }}</b> } }
<p *ngFor="let one of flat">{{ one.Name }}</p>
<app-child #c></app-child><p>{{ c.ready }}</p>
<p demoMark #d="demoExport">{{ d.marked }}</p>
<p demoMark #o="demoOther">{{ o.marked }}</p>
<div #e>{{ e.prop }}</div>
<app-child #ghost>{{ ghost }}{{ ghost.focused }}</app-child>
<div *ngFor="let s of slots">{{ s.Result.Tariff }}</div>
<p>{{ lone.Result.Tariff }}</p>
<p *ngIf="opts$ | async as o">{{ o?.[0]?.value }}</p>
<p>{{ options?.[1]?.label }}</p>
`,
})
export class ScopeComponent {
  grid!: Item[][];
  cube!: Item[][][];
  flat!: Item[];
  prop = 1;
  ghost = 2;
  slots!: Picked[];
  lone!: Picked;
  options = [{ label: 'a', value: 1 }, { label: 'b', value: 2 }];
  opts$ = of(this.options);
}
'@
}

# ONE MAP PER SUITE PROCESS (`-Shared`): every case reads the same tree.
function Get-TsScopeDb {
    $made = New-TsRowsDb (New-TsRowsWorkspace $script:TsScopeFiles) -Shared
    Assert-Exit $made.Result 0
    return $made.Db
}

# Every Read of the expressions whose source is `$Source`: `read=<source>|<name>=<row prefix>@<file>:<line>` of the declaration its `target` names, or `none`.
function Get-TsScopeTargets([string]$Source) {
    $r = Invoke-Gate --map-query (Get-TsScopeDb) --width 0 --limit 0 --sql ("SELECT 'read=' || x.source || '|' || nm.value || '=' || (CASE WHEN rw.value IS NULL THEN 'none' ELSE " +
        "substr(rw.value, 1, instr(rw.value, ':') - 1) || '@' || tf.value || ':' || tl.value END) AS r " +
        "FROM expressions x JOIN expressions__ast k ON k.owner = x.id AND k.role = 'k' AND k.value IN ('Read', 'SafeRead') " +
        "JOIN expressions__ast nm ON nm.owner = x.id AND nm.parent = k.parent AND nm.role = 'name' " +
        "LEFT JOIN expressions__ast t ON t.owner = x.id AND t.parent = k.parent AND t.role = 'target' " +
        "LEFT JOIN expressions__ast rw ON rw.owner = x.id AND rw.parent = t.n AND rw.role = 'row' " +
        "LEFT JOIN expressions__ast tf ON tf.owner = x.id AND tf.parent = t.n AND tf.role = 'file' " +
        "LEFT JOIN expressions__ast tl ON tl.owner = x.id AND tl.parent = t.n AND tl.role = 'line' " +
        "WHERE x.source = '$Source' ORDER BY r")
    Assert-Exit $r 0
    return $r
}

# N1: A LOOP VARIABLE OVER A LIST OF LISTS IS A LIST, whose own element the loop inside it takes.
Test-Case 'tsrows scope: a loop inside a loop over a list of lists takes the element of the outer variable' {
    Assert-Line (Get-TsScopeTargets '{{ one.Name }}') 'read={{ one.Name }}|Name=m@apps/shop/src/scope.models.ts:1'
    Assert-Line (Get-TsScopeTargets '{{ cell.Name }}') 'read={{ cell.Name }}|Name=m@apps/shop/src/scope.models.ts:1'
    Assert-Line (Get-TsScopeTargets '{{ deep.Name }}') 'read={{ deep.Name }}|Name=m@apps/shop/src/scope.models.ts:1'
    Assert-Line (Get-TsScopeTargets '{{ c.Name }}') 'read={{ c.Name }}|Name=m@apps/shop/src/scope.models.ts:1'
}

# N2: A REF IS TYPED BY THE ELEMENT IT SITS ON.
Test-Case 'tsrows scope: a template reference is its component, or the directive whose exportAs it names' {
    Assert-Line (Get-TsScopeTargets '{{ c.ready }}') 'read={{ c.ready }}|ready=m@apps/shop/src/scope.component.ts:5'
    Assert-Line (Get-TsScopeTargets '{{ d.marked }}') 'read={{ d.marked }}|marked=m@apps/shop/src/scope.component.ts:10'
    Assert-Line (Get-TsScopeTargets '{{ o.marked }}') 'read={{ o.marked }}|marked=m@apps/shop/src/scope.component.ts:10'
}

Test-Case 'tsrows scope: a reference on a plain element types nothing, and a reference shadows the member of its name' {
    Assert-Line (Get-TsScopeTargets '{{ e.prop }}') 'read={{ e.prop }}|prop=none'
    Assert-Line (Get-TsScopeTargets '{{ e.prop }}') 'read={{ e.prop }}|e=none'
    $g = Get-TsScopeTargets '{{ ghost }}{{ ghost.focused }}'
    Assert-Line $g 'read={{ ghost }}{{ ghost.focused }}|ghost=none'
    Assert-Line $g 'read={{ ghost }}{{ ghost.focused }}|focused=m@apps/shop/src/scope.component.ts:6'
    Assert-NoLine $g 'ghost=m@'
}

# N3 (a regression of the first rows 73 build): A TYPE PARAMETER'S DEFAULT STILL APPLIES THROUGH A LOOP VARIABLE. `Pick<Slot<unknown>, 'Result'>` binds `A` alone, so `B`
# is the default `Base` - `s.Result.Tariff` reaches `Tariff`; the loop took the whole ref of its element, a mapped type with no bindings, and ended at `Result`.
Test-Case 'tsrows scope: a loop variable typed by a Pick of a generic keeps the default of the parameter nobody passed' {
    $t = Get-TsScopeTargets '{{ s.Result.Tariff }}'
    Assert-Line $t 'read={{ s.Result.Tariff }}|Result=tm@apps/shop/src/scope.models.ts:3'
    Assert-Line $t 'read={{ s.Result.Tariff }}|Tariff=m@apps/shop/src/scope.models.ts:2'
}

Test-Case 'tsrows scope: a member typed by a Pick of a generic keeps the default of the parameter nobody passed' {
    $t = Get-TsScopeTargets '{{ lone.Result.Tariff }}'
    Assert-Line $t 'read={{ lone.Result.Tariff }}|Result=tm@apps/shop/src/scope.models.ts:3'
    Assert-Line $t 'read={{ lone.Result.Tariff }}|Tariff=m@apps/shop/src/scope.models.ts:2'
}

# N4: A KEYED READ OVER AN `async` ALIAS takes the element of the list the observable emits (`Observable<{..}[]>`), not of its innermost type.
Test-Case 'tsrows scope: a safe keyed read over an async alias of a list of object literals reaches the member of the element' {
    Assert-Line (Get-TsScopeTargets '{{ o?.[0]?.value }}') 'read={{ o?.[0]?.value }}|value=sm@apps/shop/src/scope.component.ts:40'
    Assert-Line (Get-TsScopeTargets '{{ options?.[1]?.label }}') 'read={{ options?.[1]?.label }}|label=sm@apps/shop/src/scope.component.ts:40'
}

}
