<#
    SEVEN WAYS A TEMPLATE READ WAS LOST OR WRONG, found by checking a real workspace's reads against Angular's own type checker (rows 80, `TsTpl/TsTplReads.mjs`):
    a `#ref` is the component its element renders IN SCOPE (two components share a selector); a `keyvalue` pipe's loop variable is `{key, value}`; `$event` of an
    in-tree output is the type its emitter was declared with; a loop over a ternary takes the arm that is no empty list; an `as` alias of `a && b` is `b`; and a chain
    goes on past a union step whose arms agree on the type. Its helpers are `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '../../TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

$script:TsGapFiles = @{
    'apps/shop/src/gap.models.ts' = @'
export class Row { RowID = 0; }
export class Section { sectionName = ''; items: Row[] = []; }
export class Child { label = ''; }
export class Pin { Value = ''; }
export interface Picked { row: Row; }
export class DocA { Number!: Pin; }
export class DocB { Number!: Pin; }
export class DocC { Number!: Pin; }
export class DocD { Alt!: Pin; }
export class DocE { Alt!: Child; }
export interface Bag { first: Child; second: Child; }
export interface Mixed { first: Child; count: number; }
'@
    'apps/shop/src/gap.popper.ts' = @'
import { Component } from '@angular/core';
@Component({ selector: 'demo-popper', standalone: true, template: '' })
export class LibPopperComponent {
  close() {}
}
@Component({ selector: 'demo-popper', standalone: true, template: '' })
export class SharedPopperComponent {
  open = false;
  close() {}
}
'@
    'apps/shop/src/gap.rows.ts' = @'
import { Component, Directive, EventEmitter, Input, Output, output } from '@angular/core';
import { Child, Picked } from './gap.models';
@Component({ selector: 'demo-rows', standalone: true, template: '' })
export class RowsComponent {
  @Output() picked = new EventEmitter<Picked>();
  chosen = output<Picked>();
}
@Component({ selector: 'demo-grid', standalone: true, template: '' })
export class GridComponent<T> {
  @Input() rows: T[] = [];
  @Output() cellClick = new EventEmitter<T & { cell: Child }>();
}
@Directive()
export abstract class PickerBase<P> {
  @Output() handed = new EventEmitter<P>();
}
@Component({ selector: 'demo-heir', standalone: true, template: '' })
export class HeirComponent extends PickerBase<Picked> {}
'@
    'apps/shop/src/gap.host.ts' = @'
import { Component } from '@angular/core';
import { AsyncPipe, KeyValuePipe, NgFor, NgIf } from '@angular/common';
import { Observable, of } from 'rxjs';
import { Bag, Child, DocA, DocB, DocC, DocD, DocE, Mixed, Section } from './gap.models';
import { GridComponent, HeirComponent, RowsComponent } from './gap.rows';
import { SharedPopperComponent } from './gap.popper';
@Component({
  selector: 'demo-host',
  standalone: true,
  imports: [SharedPopperComponent, RowsComponent, GridComponent, HeirComponent, KeyValuePipe, AsyncPipe, NgFor, NgIf],
  template: `
<demo-popper #pop></demo-popper><p (click)="pop.close()">a</p>
<div *ngFor="let section of sections | keyvalue: preserve"><b>{{ section.value.sectionName }}</b><i>{{ section.key.length }}</i><p *ngFor="let it of section.value.items">{{ it.RowID }}</p></div>
<div *ngFor="let each of named | keyvalue">{{ each.key.length }}</div>
<demo-rows (picked)="go($event.row.RowID)" (chosen)="go($event.row.RowID + 1)"></demo-rows>
<demo-grid [rows]="children" (cellClick)="go($event.label)"></demo-grid>
<demo-grid [rows]="children" (cellClick)="go($event.cell.label)"></demo-grid>
<demo-heir (handed)="go($event.row.RowID - 1)"></demo-heir>
<p (click)="go($event.target)">dom</p>
<p *ngFor="let kid of blocked ? [] : children">{{ kid.label }}</p>
<p *ngFor="let twin of filtering ? kids : children">{{ twin.label }}</p>
<p *ngFor="let odd of filtering ? kids : plain">{{ odd.label }}</p>
<p *ngIf="ok && find() as found">{{ found.label }}</p>
<u>{{ model.Number.Value }}</u>
<u>{{ ctl.Alt.Value }}</u>
<p *ngFor="let ka of children | keyvalue">{{ ka.value.label }}</p>
<p *ngFor="let kb of bag | keyvalue">{{ kb.value.label }}</p>
<p *ngFor="let km of mixed | keyvalue">{{ km.value.label }}</p>
<div *ngIf="rec$ | async as rec"><p *ngFor="let kc of rec | keyvalue">{{ kc.value.label }}</p></div>
<p *ngFor="let kd of rec$ | async | keyvalue">{{ kd.value.label }}</p>
`,
})
export class HostComponent {
  sections: Record<string, Section> = {};
  named: Map<string, Child> = new Map();
  preserve = () => 0;
  blocked = false; filtering = false; ok = true;
  children: Child[] = []; kids: Child[] = []; plain: Row[] = [];
  model!: DocA | DocB | DocC;
  ctl!: DocD | DocE;
  find(): Child | null { return null; }
  go(x: unknown) {}
  bag!: Bag;
  mixed!: Mixed;
  rec$: Observable<Record<string, Child>> = of({});
}
'@
}

# ONE MAP PER SUITE PROCESS (`-Shared`): every case reads the same tree.
function Get-TsGapDb {
    $made = New-TsRowsDb (New-TsRowsWorkspace $script:TsGapFiles) -Shared
    Assert-Exit $made.Result 0
    return $made.Db
}

# Every Read of the expressions whose source is `$Source`: `read=<source>|<name>=<row prefix>@<file>:<line>` of the declaration its `target` names, or `none`.
function Get-TsGapTargets([string]$Source) {
    $r = Invoke-Gate --map-query (Get-TsGapDb) --width 0 --limit 0 --sql ("SELECT 'read=' || x.source || '|' || nm.value || '=' || (CASE WHEN rw.value IS NULL THEN 'none' ELSE " +
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

Test-Case 'tsrows gaps: a template reference is the component its element renders in scope, not another of the same selector' {
    # The libs copy is declared first and matches too; the host imports only the shared one. The control: `open` is the shared one's alone.
    Assert-Line (Get-TsGapTargets 'pop.close()') 'read=pop.close()|close=m@apps/shop/src/gap.popper.ts:9'
    Assert-NoLine (Get-TsGapTargets 'pop.close()') 'gap.popper.ts:4'
}

Test-Case 'tsrows gaps: a keyvalue pipe hands a loop variable of key and value, the value being what the map holds' {
    Assert-Line (Get-TsGapTargets '{{ section.value.sectionName }}') 'read={{ section.value.sectionName }}|sectionName=m@apps/shop/src/gap.models.ts:2'
    Assert-Line (Get-TsGapTargets '{{ it.RowID }}') 'read={{ it.RowID }}|RowID=m@apps/shop/src/gap.models.ts:1'
    # THE CONTROL: the key is the key type (a string - no declaration in the tree), and a Map's value is read as a record's.
    Assert-Line (Get-TsGapTargets '{{ section.key.length }}') 'read={{ section.key.length }}|length=none'
}

# (rows 86) `keyvalue` OVER AN ARRAY, A PLAIN INTERFACE AND WHAT `async` EMITS (`TsTplHops.mjs::keyValueOf`): an array is `Record<number, T>` to the pipe, an interface with no base
# `Record<string, V>` with `V` its fields' types, and `rec$ | async` - piped on, or through an `as` alias - is the record the observable emits, not the element it unwraps to.
# THE CONTROL: an interface with a field typed by nothing the map holds (`count: number`) is refused, as a union with an unknown arm.
Test-Case 'tsrows gaps: a keyvalue pipe over an array, a plain interface or an async value hands the value it holds' {
    foreach ($v in 'ka', 'kb', 'kc', 'kd') { Assert-Line (Get-TsGapTargets "{{ $v.value.label }}") "read={{ $v.value.label }}|label=m@apps/shop/src/gap.models.ts:3" }
    Assert-Line (Get-TsGapTargets '{{ km.value.label }}') 'read={{ km.value.label }}|label=none'
}

Test-Case 'tsrows gaps: $event of an in-tree output is the type its emitter was declared with, a DOM event types nothing' {
    Assert-Line (Get-TsGapTargets 'go($event.row.RowID)') 'read=go($event.row.RowID)|row=tm@apps/shop/src/gap.models.ts:5'
    Assert-Line (Get-TsGapTargets 'go($event.row.RowID)') 'read=go($event.row.RowID)|RowID=m@apps/shop/src/gap.models.ts:1'
    Assert-Line (Get-TsGapTargets 'go($event.row.RowID + 1)') 'read=go($event.row.RowID + 1)|RowID=m@apps/shop/src/gap.models.ts:1'
    # A GENERIC EMITTER (`EventEmitter<T & { cell }>`) is read as the element instantiates it: `T` is what `[rows]` holds.
    Assert-Line (Get-TsGapTargets 'go($event.label)') 'read=go($event.label)|label=m@apps/shop/src/gap.models.ts:3'
    Assert-Line (Get-TsGapTargets 'go($event.target)') 'read=go($event.target)|target=none'
    # (rows 85) THE LITERAL PART of `T & { cell: Child }` answers too: `cell` is its member, and `label` is read on through it.
    Assert-Line (Get-TsGapTargets 'go($event.cell.label)') 'read=go($event.cell.label)|cell=sm@apps/shop/src/gap.rows.ts:11'
    Assert-Line (Get-TsGapTargets 'go($event.cell.label)') 'read=go($event.cell.label)|label=m@apps/shop/src/gap.models.ts:3'
}

# (rows 85) AN OUTPUT A COMPONENT INHERITS types `$event` too (`TsTplEvents.mjs::outputOf`): `handed` is declared on `PickerBase<P>`, and `HeirComponent extends PickerBase<Picked>` binds `P`.
Test-Case 'tsrows gaps: $event of an output inherited from a generic base is the type the base is bound to' {
    Assert-Line (Get-TsGapTargets 'go($event.row.RowID - 1)') 'read=go($event.row.RowID - 1)|RowID=m@apps/shop/src/gap.models.ts:1'
}

Test-Case 'tsrows gaps: a loop over a ternary takes the arm that is no empty list, or the one both arms name' {
    Assert-Line (Get-TsGapTargets '{{ kid.label }}') 'read={{ kid.label }}|label=m@apps/shop/src/gap.models.ts:3'
    Assert-Line (Get-TsGapTargets '{{ twin.label }}') 'read={{ twin.label }}|label=m@apps/shop/src/gap.models.ts:3'
    # THE CONTROL: arms of two different types name no one owner.
    Assert-Line (Get-TsGapTargets '{{ odd.label }}') 'read={{ odd.label }}|label=none'
}

Test-Case 'tsrows gaps: the as alias of an and condition is its last operand' {
    Assert-Line (Get-TsGapTargets '{{ found.label }}') 'read={{ found.label }}|label=m@apps/shop/src/gap.models.ts:3'
}

Test-Case 'tsrows gaps: a chain goes on past a union step whose arms agree on the type, the step itself naming no one place' {
    $r = Get-TsGapTargets '{{ model.Number.Value }}'
    Assert-Line $r 'read={{ model.Number.Value }}|Value=m@apps/shop/src/gap.models.ts:4'
    Assert-Line $r 'read={{ model.Number.Value }}|Number=none'
    # THE CONTROL: arms that disagree on the type stop the chain.
    Assert-Line (Get-TsGapTargets '{{ ctl.Alt.Value }}') 'read={{ ctl.Alt.Value }}|Value=none'
}

}
