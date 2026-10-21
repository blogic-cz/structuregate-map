<#
    WHAT THE TEMPLATE'S TYPES FOLLOW (rows 74, `TsTpl/TsTplContext.mjs`, `TsTplReads.mjs::scopeAt`/`loopValueOf`, `TsTplOwners.mjs::ownerOfRow`): the `let-` variables of a template a
    STRUCTURAL directive receives are typed by the context it declares (`ngTemplateContextGuard`'s predicate, else the `TemplateRef<Ctx>` it injects), its generic bound from the input
    that feeds it; a loop over an `async` alias takes the element of the list the observable emits; a `#ref` to a generic component binds the component's parameters from the inputs on
    its element; and a literal inside an intersection in a generic reads with the bindings of the declaration it is in. Its helpers are `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '../../TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

$script:TsTypesFiles = @{
    'apps/shop/src/types.models.ts' = @'
export class Item { Name = ''; }
export class Row { Id = 0; }
export class Box<T> { pair!: { item: T } & { more: number }; }
export interface EachContext<T> { $implicit: T; index: number; extra: Item; }
'@
    'apps/shop/src/types.component.ts' = @'
import { Component, Directive, Input, TemplateRef } from '@angular/core';
import { of } from 'rxjs';
import { Item, Row, Box, EachContext } from './types.models';

@Directive({ selector: '[demoEach]', standalone: true })
export class EachDirective<T> {
  @Input() demoEachOf!: T[];
  static ngTemplateContextGuard<U>(dir: EachDirective<U>, ctx: unknown): ctx is EachContext<U> { return true; }
  constructor(private tpl: TemplateRef<unknown>) {}
}

@Directive({ selector: '[demoVar]', standalone: true })
export class VarDirective<T> {
  @Input() set demoVar(context: T) { this.ctx = context; }
  ctx: unknown;
  constructor(private tpl: TemplateRef<{ $implicit: T; demoVar: T }>) {}
}

@Directive({ selector: '[demoPlain]', standalone: true })
export class PlainDirective {
  constructor(private tpl: TemplateRef<any>) {}
}

@Component({ selector: 'demo-table', standalone: true, template: '' })
export class TableComponent<T> {
  @Input() rows!: T[];
  selected!: T;
}

@Component({ selector: 'demo-bound', standalone: true, template: '' })
export class BoundComponent<T extends Row> {
  selected!: T;
}

@Component({
  selector: 'app-types',
  standalone: true,
  imports: [EachDirective, VarDirective, PlainDirective, TableComponent, BoundComponent],
  template: `
<ng-template demoEach [demoEachOf]="items" let-it let-n="index" let-e="extra">{{ it.Name }}{{ n }}{{ e.Name }}</ng-template>
<p *demoEach="let it2 of rows">{{ it2.Id }}</p>
<ng-template demoPlain let-q>{{ q.Name }}</ng-template>
<p *demoVar="first as v">{{ v.Name }}</p>
<div *ngIf="items$ | async as list"><p *ngFor="let one of list">{{ one.Name }}</p></div>
<div *ngIf="rows as plain"><p *ngFor="let each of plain">{{ each.Id }}</p></div>
<demo-table #t [rows]="items"></demo-table><p>{{ t.selected.Name }}</p>
<demo-table #u [rows]="rows"></demo-table><p>{{ u.selected.Id }}</p>
<demo-table #w></demo-table><p>{{ w.selected.Name }}</p>
<demo-bound #b></demo-bound><p>{{ b.selected.Id }}</p>
<p>{{ box.pair.item.Name }}</p>
`,
})
export class TypesComponent {
  items!: Item[];
  rows!: Row[];
  first!: Item;
  items$ = of([new Item()]);
  box!: Box<Item>;
}
'@
    'apps/shop/src/types.coded.ts' = @'
import { Component, Directive, Input, TemplateRef, ViewContainerRef, inject } from '@angular/core';
import { Item, Row } from './types.models';

@Directive({ selector: '[demoCoded]', standalone: true })
export class CodedDirective {
  @Input() demoCoded = '';
  rule: Item | null = null;
  private tpl = inject(TemplateRef);
  private vcr = inject(ViewContainerRef);
  render(): void { this.vcr.createEmbeddedView(this.tpl, { rule: this.rule }); }
}

@Directive({ selector: '[demoTyped]', standalone: true })
export class TypedDirective {
  constructor(private tpl: TemplateRef<any>, private vcr: ViewContainerRef) {}
  render(): void {
    const ctx: { $implicit: Row } = { $implicit: new Row() };
    this.vcr.createEmbeddedView(this.tpl, ctx);
  }
}

export class ContextBase { constructor(public tpl: TemplateRef<{ $implicit: Item }>) {} }
@Directive({ selector: '[demoChild]', standalone: true })
export class ChildDirective extends ContextBase {}

@Directive({ selector: '[demoSilent]', standalone: true })
export class SilentDirective { constructor(private tpl: TemplateRef<any>) {} }

@Component({
  selector: 'app-coded',
  standalone: true,
  imports: [CodedDirective, TypedDirective, ChildDirective, SilentDirective],
  template: `
<p *demoCoded="'k'; let r = rule">{{ r.Name }}</p>
<p *demoTyped="let row">{{ row.Id }}</p>
<p *demoChild="let base">{{ base.Name }}</p>
<p *demoSilent="let none">{{ none.Name }}</p>
`,
})
export class CodedComponent {}
'@
    # (rows 77) A directive that takes its template through `inject(TemplateRef)`, a base class that renders with a context, and an observable of lists of lists.
    'apps/shop/src/types.inject.ts' = @'
import { Component, Directive, TemplateRef, ViewContainerRef, inject } from '@angular/core';
import { Observable, of } from 'rxjs';
import { Item, Row } from './types.models';

@Directive({ selector: '[demoInject]', standalone: true })
export class InjectDirective { private tpl = inject(TemplateRef); }

@Directive({ selector: '[demoOther]', standalone: true })
export class OtherDirective { private vcr = inject(ViewContainerRef); }

export class RenderBase<T> {
  constructor(protected tpl: TemplateRef<any>, protected vcr: ViewContainerRef) {}
  protected render(item: T): void { this.vcr.createEmbeddedView(this.tpl, { $implicit: item, extra: new Row() }); }
}
@Directive({ selector: '[demoRendered]', standalone: true })
export class RenderedDirective extends RenderBase<Item> {
  constructor(tpl: TemplateRef<any>, vcr: ViewContainerRef) { super(tpl, vcr); }
}

@Component({
  selector: 'app-inject',
  standalone: true,
  imports: [InjectDirective, OtherDirective, RenderedDirective],
  template: `
<ng-template [demoInject]="injectedOn"></ng-template>
<ng-template [demoOther]="plainOn"></ng-template>
<p *demoRendered="let made; extra as more">{{ made.Name }}{{ more.Id }}</p>
<div *ngFor="let line of (grid$ | async)"><b>{{ line[0].Name }}</b><i *ngFor="let square of line">{{ square.Name }}</i></div>
<div *ngFor="let flat of (flat$ | async)">{{ flat.Name }}</div>
<p>{{ (grid$ | async)![0][0].Name }}</p>
`,
})
export class InjectComponent {
  injectedOn = true;
  plainOn = true;
  grid$: Observable<Item[][]> = of([[new Item()]]);
  flat$: Observable<Item[]> = of([new Item()]);
}
'@
}

# ONE MAP PER SUITE PROCESS (`-Shared`): every case reads the same tree.
function Get-TsTypesDb {
    $made = New-TsRowsDb (New-TsRowsWorkspace $script:TsTypesFiles) -Shared
    Assert-Exit $made.Result 0
    return $made.Db
}

# Every Read of the expressions whose source is `$Source`: `read=<source>|<name>=<row prefix>@<file>:<line>` of the declaration its `target` names, or `none`.
function Get-TsTypesTargets([string]$Source) {
    $r = Invoke-Gate --map-query (Get-TsTypesDb) --width 0 --limit 0 --sql ("SELECT 'read=' || x.source || '|' || nm.value || '=' || (CASE WHEN rw.value IS NULL THEN 'none' ELSE " +
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

# T1 (G1): A STRUCTURAL DIRECTIVE'S CONTEXT TYPES ITS `let-` VARIABLES. The guard's predicate is the context, its `U` the class's `T` (named by `dir: EachDirective<U>`), and `T` is the
# element of the list `[demoEachOf]` is bound to; `let-x` is `$implicit`, `let-x="prop"` that property, and `*demoEach="let x of rows"` the same.
Test-Case 'tsrows types: a let variable of a template takes the type the context guard of its directive names, its generic bound from the input' {
    $t = Get-TsTypesTargets '{{ it.Name }}{{ n }}{{ e.Name }}'
    Assert-Line $t 'read={{ it.Name }}{{ n }}{{ e.Name }}|Name=m@apps/shop/src/types.models.ts:1'
    Assert-Line $t 'read={{ it.Name }}{{ n }}{{ e.Name }}|n=none'
    Assert-Line (Get-TsTypesTargets '{{ it2.Id }}') 'read={{ it2.Id }}|Id=m@apps/shop/src/types.models.ts:2'
    # THE CONTROL: a directive that declares no context types nothing, and its variable still shadows the component.
    Assert-Line (Get-TsTypesTargets '{{ q.Name }}') 'read={{ q.Name }}|Name=none'
}

# T2 (G1): WITH NO GUARD THE `TemplateRef<Ctx>` THE DIRECTIVE INJECTS IS ITS CONTEXT (a literal of the class's `T`), `as v` is the property `demoVar`.
Test-Case 'tsrows types: an as variable of a structural directive takes the context of the TemplateRef it injects' {
    Assert-Line (Get-TsTypesTargets '{{ v.Name }}') 'read={{ v.Name }}|Name=m@apps/shop/src/types.models.ts:1'
}

# T3 (G2): A LOOP OVER AN `async` ALIAS takes the element of the list the observable emits, not the innermost type of the alias.
Test-Case 'tsrows types: a loop over an async alias of a list takes the element of the list' {
    Assert-Line (Get-TsTypesTargets '{{ one.Name }}') 'read={{ one.Name }}|Name=m@apps/shop/src/types.models.ts:1'
    # THE CONTROL: an alias of a list that is no pipe's value.
    Assert-Line (Get-TsTypesTargets '{{ each.Id }}') 'read={{ each.Id }}|Id=m@apps/shop/src/types.models.ts:2'
}

# T4 (G3): THE REF OF A GENERIC COMPONENT IS ITS INSTANCE: `T` is what the inputs on its element feed - each element its own - else the default, else the constraint.
Test-Case 'tsrows types: a reference to a generic component binds its parameters from the inputs on its element' {
    Assert-Line (Get-TsTypesTargets '{{ t.selected.Name }}') 'read={{ t.selected.Name }}|Name=m@apps/shop/src/types.models.ts:1'
    Assert-Line (Get-TsTypesTargets '{{ u.selected.Id }}') 'read={{ u.selected.Id }}|Id=m@apps/shop/src/types.models.ts:2'
    Assert-Line (Get-TsTypesTargets '{{ b.selected.Id }}') 'read={{ b.selected.Id }}|Id=m@apps/shop/src/types.models.ts:2'
    # THE CONTROL: nothing feeds `T` and nothing bounds it.
    Assert-Line (Get-TsTypesTargets '{{ w.selected.Name }}') 'read={{ w.selected.Name }}|Name=none'
}

# T5 (G4): A LITERAL IN AN INTERSECTION READS WITH THE BINDINGS OF THE INSTANTIATION IT WAS REACHED IN.
Test-Case 'tsrows types: a literal that is a part of an intersection in a generic takes the binding of the hop that reached it' {
    Assert-Line (Get-TsTypesTargets '{{ box.pair.item.Name }}') 'read={{ box.pair.item.Name }}|Name=m@apps/shop/src/types.models.ts:1'
}


# (rows 75) A CONTEXT BUILT IN CODE TYPES THE VARIABLES WHEN THE TEMPLATE REF DECLARES NONE (`TsTpl/TsTplContext.mjs`): `inject(TemplateRef)` and `TemplateRef<any>` say nothing, so the
# type of the object the directive hands `createEmbeddedView(this.tpl, ctx)` is the context - an object literal, or a local typed by a literal. A BASE class's context is the
# directive's when it declares none. The control: a `TemplateRef<any>` nothing renders with a context types nothing.
Test-Case 'tsrows types: a variable of a directive that builds its context in code takes the type of the object it hands the view' {
    Assert-Line (Get-TsTypesTargets '{{ r.Name }}') 'read={{ r.Name }}|Name=m@apps/shop/src/types.models.ts:1'
    Assert-Line (Get-TsTypesTargets '{{ row.Id }}') 'read={{ row.Id }}|Id=m@apps/shop/src/types.models.ts:2'
}

Test-Case 'tsrows types: a directive with no context of its own takes the one of its base class' {
    Assert-Line (Get-TsTypesTargets '{{ base.Name }}') 'read={{ base.Name }}|Name=m@apps/shop/src/types.models.ts:1'
    Assert-Line (Get-TsTypesTargets '{{ none.Name }}') 'read={{ none.Name }}|Name=none'
}


# (rows 77) A DIRECTIVE THAT TAKES ITS TEMPLATE THROUGH `inject(TemplateRef)` IS STRUCTURAL as one with the constructor parameter is: `[demoInject]="x"` on an `<ng-template>` is a gate of the
# `structural` kind. The control: a directive that `inject()`s anything else is an attribute directive, and its binding is no gate.
Test-Case 'tsrows types: a directive that injects TemplateRef with inject() is structural' {
    $r = Invoke-TsRowsQ (Get-TsTypesDb) "SELECT 'gate=' || name || '|' || source || '|' || gate_kind AS r FROM gates WHERE source IN ('injectedOn', 'plainOn')"
    Assert-Line $r 'gate=demoInject|injectedOn|structural'
    Assert-NoLine $r 'plainOn'
    # ITS `di` ROW NAMES THE DECLARATION `inject()` HANDS BACK, as a constructor parameter's does: the class in Angular's package (`file` ends there), each by its own class.
    $d = Invoke-TsRowsQ (Get-TsTypesDb) ("SELECT 'di=' || c.name || '|' || ifnull(n.value, 'none') || '|' || (CASE WHEN f.value LIKE '%@angular/core/%' THEN 'core' ELSE 'other' END) AS r " +
        "FROM di d JOIN classes c ON c.id = d.class JOIN di__type_ref n ON n.owner = d.id AND n.role = 'name' JOIN di__type_ref f ON f.owner = d.id AND f.parent = n.parent AND f.role = 'file' " +
        "WHERE c.name IN ('InjectDirective', 'OtherDirective') AND d.inject = 1")
    Assert-Line $d 'di=InjectDirective|TemplateRef|core'
    Assert-Line $d 'di=OtherDirective|ViewContainerRef|core'
}

# (rows 77) A context built by a BASE class's `createEmbeddedView(this.tpl, ctx)` types the `let-` variables of the directive that extends it, the base's `T` bound from `extends RenderBase<Item>`.
Test-Case 'tsrows types: a variable of a directive whose base class renders with a context takes the type the base hands the view' {
    $r = Get-TsTypesTargets '{{ made.Name }}{{ more.Id }}'
    Assert-Line $r 'read={{ made.Name }}{{ more.Id }}|Name=m@apps/shop/src/types.models.ts:1'
    Assert-Line $r 'read={{ made.Name }}{{ more.Id }}|Id=m@apps/shop/src/types.models.ts:2'
}

# (rows 77) A LOOP OVER `async` TAKES THE ELEMENT OF THE LIST THE OBSERVABLE EMITS: `Observable<Item[][]>` emits `Item[][]`, so `let line of (grid$ | async)` is an `Item[]` (a keyed read of it
# is an `Item`) and the loop inside it over `line` an `Item`. The controls: a list of items takes `Item`, and a keyed read of the alias chain itself.
Test-Case 'tsrows types: a loop over an async observable of lists of lists takes the list, and a loop in it the element' {
    Assert-Line (Get-TsTypesTargets '{{ line[0].Name }}') 'read={{ line[0].Name }}|Name=m@apps/shop/src/types.models.ts:1'
    Assert-Line (Get-TsTypesTargets '{{ square.Name }}') 'read={{ square.Name }}|Name=m@apps/shop/src/types.models.ts:1'
    Assert-Line (Get-TsTypesTargets '{{ flat.Name }}') 'read={{ flat.Name }}|Name=m@apps/shop/src/types.models.ts:1'
    Assert-Line (Get-TsTypesTargets '{{ (grid$ | async)![0][0].Name }}') 'read={{ (grid$ | async)![0][0].Name }}|Name=m@apps/shop/src/types.models.ts:1'
}

}
