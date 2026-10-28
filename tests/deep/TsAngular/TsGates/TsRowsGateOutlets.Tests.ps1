<#
    A key inside an `<ng-template #ref>` that only OUTLETS render (`[ngTemplateOutlet]="ref"`, `*ngTemplateOutlet="ref"`,
    `*ngIf="c; then ref"`): each outlet's own gate chain is a WAY in to the template's contents, so the key is
    restricted by what every outlet shares (`always_*`) and the rest is `maybe_gates` (#51). An outlet's own
    hand-over binding (`ngTemplateOutlet`, `ngIfThen`, `ngIfElse`) is no condition; an `else` is a way on the negated condition (#115).
    A template a non-outlet also reads - an event handler, a `@ViewChild` - stays as it was: the key has its own chain only. Read in
    `rust/fbtcore/src/rows/ts/key/routes/key_outlets.rs`, joined in `key_routes.rs`. Its helpers are `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '../../TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

# key | always_gates | maybe_gates | always_values, each gate by its source text, `-` for none.
$script:TsOutletsSql = "SELECT k.key || ' | ' || " +
    "COALESCE((SELECT group_concat(g.source, ' & ') FROM key_reach__always_gates a JOIN gates g ON g.id = a.value WHERE a.owner = '#' || k.rowid AND a.parent = 0), '-') || ' | ' || " +
    "COALESCE((SELECT group_concat(g.source, ' & ') FROM key_reach__maybe_gates m JOIN gates g ON g.id = m.value WHERE m.owner = '#' || k.rowid AND m.parent = 0), '-') || ' | ' || " +
    "COALESCE((SELECT group_concat($(Get-TsFieldSql 'key_reach__always_values' 'v' 'dimension') || ' ' || $(Get-TsFieldSql 'key_reach__always_values' 'v' 'op') || ' ' || " +
    "$(Get-NodeListSql 'key_reach__always_values' 'v' 'values'), ' & ') FROM key_reach__always_values v WHERE v.owner = '#' || k.rowid AND v.parent = 0), '-') AS reach " +
    "FROM key_reach k WHERE k.key LIKE 'demo.list.%' ORDER BY 1"

Test-Case 'tsrows: a key in an ng-template only outlets render takes the gates of the outlets' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/kinds.ts' = "export enum KindSet { Home = 1, Flat = 2, Life = 3 }`n"
        'apps/shop/src/translate.component.ts' = "import { Component, Input } from '@angular/core';`n" +
            "@Component({ selector: 'translate', template: '{{ text }}' })`n" +
            "export class TranslateComponent { @Input() text: string = ''; }`n"
        'apps/shop/src/list.component.ts' = "import { Component, ViewChild } from '@angular/core';`n" +
            "import { KindSet } from './kinds';`n" +
            "@Component({ selector: 'demo-list', template: ``n" +
            # THE ISSUE: one gated outlet.
            "  <ng-container *ngIf=`"vm.kind === KindSet.Home`" [ngTemplateOutlet]=`"selectAll`"></ng-container>`n" +
            "  <ng-template #selectAll><translate [text]=`"'demo.list.selectAll'`"></translate></ng-template>`n" +
            "  <translate [text]=`"'demo.list.title'`"></translate>`n" +
            # One outlet under no gate: its own `*ngTemplateOutlet` gate row is the hand-over, not a condition.
            "  <ng-container *ngTemplateOutlet=`"aTpl`"></ng-container>`n" +
            "  <ng-template #aTpl><translate [text]=`"'demo.list.a'`"></translate></ng-template>`n" +
            # Two outlets, one gated: the gate holds on SOME way only.
            "  <div *ngIf=`"vm.kind === KindSet.Flat`"><ng-container *ngTemplateOutlet=`"bTpl`"></ng-container></div>`n" +
            "  <ng-container [ngTemplateOutlet]=`"bTpl`"></ng-container>`n" +
            "  <ng-template #bTpl><translate [text]=`"'demo.list.b'`"></translate></ng-template>`n" +
            # A gated outlet, but the ref is also handed to a method: rendered who knows where, so as before.
            "  <ng-container *ngIf=`"vm.kind === KindSet.Home`" [ngTemplateOutlet]=`"cTpl`"></ng-container>`n" +
            "  <ng-template #cTpl><translate [text]=`"'demo.list.c'`"></translate></ng-template>`n" +
            "  <button (click)=`"show(cTpl)`"></button>`n" +
            # `then` renders under the condition; `else` under its negation (#115).
            "  <div *ngIf=`"vm.kind === KindSet.Life; then dTpl else eTpl`"></div>`n" +
            "  <ng-template #dTpl><translate [text]=`"'demo.list.d'`"></translate></ng-template>`n" +
            "  <ng-template #eTpl><translate [text]=`"'demo.list.e'`"></translate></ng-template>`n" +
            # A template a `@ViewChild` queries is handed to the class: as before, whatever outlets it has.
            "  <ng-container *ngIf=`"vm.kind === KindSet.Home`" [ngTemplateOutlet]=`"gTpl`"></ng-container>`n" +
            "  <ng-template #gTpl><translate [text]=`"'demo.list.g'`"></translate></ng-template>`n" +
            "`` })`n" +
            "export class ListComponent {`n" +
            "  @ViewChild('gTpl') held: unknown;`n" +
            "  KindSet = KindSet;`n  vm: { kind: KindSet } = { kind: KindSet.Life };`n" +
            "  show(t: unknown): void {}`n}`n"
        'apps/shop/src/root.component.ts' = "import { Component, NgModule } from '@angular/core';`n" +
            "import { ListComponent } from './list.component';`n" +
            "import { TranslateComponent } from './translate.component';`n" +
            "@Component({ selector: 'demo-root', template: '<demo-list></demo-list>' })`n" +
            "export class RootComponent {}`n" +
            "@NgModule({ declarations: [RootComponent, ListComponent, TranslateComponent], bootstrap: [RootComponent] })`n" +
            "export class DemoModule {}`n"
        'apps/shop/src/assets/locales/en.json' = '{"shop":{"title":"Shop","cart":{"empty":"Empty"}},' +
            '"demo":{"list":{"selectAll":"1","title":"2","a":"3","b":"4","c":"5","d":"6","e":"7","g":"8"}}}'
        'structuregate.ts.json' = '{"locales":["apps/shop/src/assets/locales"],"i18nCarriers":["money","translate:text"],' +
            '"gateInputs":["disabled"],"featureChecks":["FlagService.isOn"],"featureEnum":"FlagCodes"}'
    }
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    $r = Invoke-Gate --map-query $made.Db --width 0 --sql $script:TsOutletsSql
    Assert-Exit $r 0
    Assert-Line $r 'demo.list.selectAll | vm.kind === KindSet.Home | vm.kind === KindSet.Home | vm.kind in ["Home"]'
    Assert-Line $r 'demo.list.title | - | - | -'
    Assert-Line $r 'demo.list.a | - | - | -'
    Assert-Line $r 'demo.list.b | - | vm.kind === KindSet.Flat | -'
    Assert-Line $r 'demo.list.c | - | - | -'
    Assert-Line $r 'demo.list.d | vm.kind === KindSet.Life | vm.kind === KindSet.Life | vm.kind in ["Life"]'
    Assert-Line $r 'demo.list.e | !(vm.kind === KindSet.Life) | !(vm.kind === KindSet.Life) | vm.kind not_in ["Life"]'
    Assert-Line $r 'demo.list.g | - | - | -'
    # Two outlets are two ways in: counted as an arm's ways are.
    $n = Invoke-Gate --map-query $made.Db --width 0 --sql ("SELECT key || ' paths ' || n_paths AS n FROM key_reach " +
        "WHERE key IN ('demo.list.b', 'demo.list.selectAll') ORDER BY 1")
    Assert-Line $n 'demo.list.b paths 2'
    Assert-Line $n 'demo.list.selectAll paths 1'
}

# A COMPONENT inside an outlet-only `<ng-template #ref>` renders where its outlets do, as a key there does: its render edge holds the gate
# every outlet's way holds, so a key in the CHILD's own template is restricted by the outlet's `*ngIf` (it kept the edge's own chain, none).
Test-Case 'tsrows: a component in an ng-template only outlets render takes the gates of the outlets' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/kinds.ts' = "export enum KindSet { Home = 1, Flat = 2, Life = 3 }`n"
        'apps/shop/src/translate.component.ts' = "import { Component, Input } from '@angular/core';`n" +
            "@Component({ selector: 'translate', template: '{{ text }}' })`n" +
            "export class TranslateComponent { @Input() text: string = ''; }`n"
        'apps/shop/src/child.component.ts' = "import { Component } from '@angular/core';`n" +
            "@Component({ selector: 'demo-child', template: '<translate [text]=`"\'demo.child.title\'`"></translate>' })`nexport class ChildComponent {}`n"
        'apps/shop/src/host.component.ts' = "import { Component } from '@angular/core';`nimport { KindSet } from './kinds';`n" +
            "@Component({ selector: 'demo-host', template: ``n" +
            "  <ng-container *ngIf=`"vm.kind === KindSet.Home`" [ngTemplateOutlet]=`"childTpl`"></ng-container>`n" +
            "  <ng-template #childTpl><demo-child></demo-child></ng-template>`n`` })`n" +
            "export class HostComponent {`n  KindSet = KindSet;`n  vm: { kind: KindSet } = { kind: KindSet.Home };`n}`n"
        'apps/shop/src/app.module.ts' = "import { NgModule } from '@angular/core';`nimport { CommonModule } from '@angular/common';`n" +
            "import { TranslateComponent } from './translate.component';`nimport { ChildComponent } from './child.component';`nimport { HostComponent } from './host.component';`n" +
            "@NgModule({ declarations: [TranslateComponent, ChildComponent, HostComponent], imports: [CommonModule], bootstrap: [HostComponent] })`nexport class AppModule {}`n"
        'apps/shop/src/assets/locales/en.json' = '{"shop":{"title":"Shop"},"demo":{"child":{"title":"C"}}}'
        'structuregate.ts.json' = '{"locales":["apps/shop/src/assets/locales"],"i18nCarriers":["translate:text"]}'
    }
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    $r = Invoke-Gate --map-query $made.Db --width 0 --sql ($script:TsOutletsSql.Replace("LIKE 'demo.list.%'", "LIKE 'demo.child.%'"))
    Assert-Line $r 'demo.child.title | vm.kind === KindSet.Home | vm.kind === KindSet.Home | vm.kind in ["Home"]'
}

# A QUERY BY A COMPONENT'S TYPE HANDS NO TEMPLATE OVER (#114): a component never sits on an `ng-template`, so `@ViewChild(BoxComponent)` leaves the outlet
# lift as it was - the child takes the outlet's `*ngIf`. `@ViewChild(TemplateRef)` can return any template of the class: its child keeps its own chain.
Test-Case 'tsrows: a query by a component type keeps the outlet gates of an ng-template' {
    $hostOf = { param($sel, $tpl, $child, $query)
        "import { Component, TemplateRef, ViewChild } from '@angular/core';`nimport { KindSet } from './kinds';`nimport { BoxComponent } from './box.component';`n" +
        "@Component({ selector: '$sel', template: ``n" +
        "  <ng-container *ngIf=`"vm.kind === KindSet.Home`" [ngTemplateOutlet]=`"$tpl`"></ng-container>`n" +
        "  <ng-template #$tpl><$child></$child></ng-template><demo-box></demo-box>`n`` })`n" +
        "export class $($sel.Replace('-', ''))Component {`n  KindSet = KindSet;`n  vm: { kind: KindSet } = { kind: KindSet.Home };`n  $query held: unknown;`n}`n"
    }
    $leaf = { param($sel, $key) "import { Component } from '@angular/core';`n@Component({ selector: '$sel', template: '<translate [text]=`"\'$key\'`"></translate>' })`nexport class $($sel.Replace('-', ''))Component {}`n" }
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/kinds.ts' = "export enum KindSet { Home = 1, Flat = 2, Life = 3 }`n"
        'apps/shop/src/translate.component.ts' = "import { Component, Input } from '@angular/core';`n" +
            "@Component({ selector: 'translate', template: '{{ text }}' })`nexport class TranslateComponent { @Input() text: string = ''; }`n"
        'apps/shop/src/box.component.ts' = "import { Component } from '@angular/core';`n@Component({ selector: 'demo-box', template: '<b>b</b>' })`nexport class BoxComponent {}`n"
        'apps/shop/src/child.component.ts' = (& $leaf 'demo-child' 'demo.child.title')
        'apps/shop/src/other.component.ts' = (& $leaf 'demo-other' 'demo.other.title')
        'apps/shop/src/host.component.ts' = (& $hostOf 'demo-host' 'childTpl' 'demo-child' '@ViewChild(BoxComponent)')
        'apps/shop/src/held.component.ts' = (& $hostOf 'demo-held' 'otherTpl' 'demo-other' '@ViewChild(TemplateRef)')
        'apps/shop/src/app.module.ts' = "import { NgModule } from '@angular/core';`nimport { CommonModule } from '@angular/common';`n" +
            "import { TranslateComponent } from './translate.component';`nimport { BoxComponent } from './box.component';`n" +
            "import { demochildComponent } from './child.component';`nimport { demootherComponent } from './other.component';`n" +
            "import { demohostComponent } from './host.component';`nimport { demoheldComponent } from './held.component';`n" +
            "@NgModule({ declarations: [TranslateComponent, BoxComponent, demochildComponent, demootherComponent, demohostComponent, demoheldComponent], imports: [CommonModule], bootstrap: [demohostComponent, demoheldComponent] })`nexport class AppModule {}`n"
        'apps/shop/src/assets/locales/en.json' = '{"shop":{"title":"Shop"},"demo":{"child":{"title":"C"},"other":{"title":"O"}}}'
        'structuregate.ts.json' = '{"locales":["apps/shop/src/assets/locales"],"i18nCarriers":["translate:text"]}'
    }
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    $r = Invoke-Gate --map-query $made.Db --width 0 --sql ($script:TsOutletsSql.Replace("k.key LIKE 'demo.list.%'", "(k.key LIKE 'demo.child.%' OR k.key LIKE 'demo.other.%')"))
    Assert-Line $r 'demo.child.title | vm.kind === KindSet.Home | vm.kind === KindSet.Home | vm.kind in ["Home"]'
    Assert-Line $r 'demo.other.title | - | - | -'
}

# AN `else` RENDERS ITS TEMPLATE UNDER THE NEGATION (#115): `*ngIf="c; else elseTpl"` - the key and the component in `elseTpl` take `!(c)` on the
# way through the `else` (an `ngIfElseNot` gate in place of the `ngIf`), and the `ngIf`'s own content keeps `c`.
Test-Case 'tsrows: a key and a component in an else template take the negated condition' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/kinds.ts' = "export enum KindSet { Home = 1, Flat = 2, Life = 3 }`n"
        'apps/shop/src/translate.component.ts' = "import { Component, Input } from '@angular/core';`n" +
            "@Component({ selector: 'translate', template: '{{ text }}' })`nexport class TranslateComponent { @Input() text: string = ''; }`n"
        'apps/shop/src/child.component.ts' = "import { Component } from '@angular/core';`n" +
            "@Component({ selector: 'demo-child', template: '<translate [text]=`"\'demo.list.child\'`"></translate>' })`nexport class ChildComponent {}`n"
        'apps/shop/src/host.component.ts' = "import { Component } from '@angular/core';`nimport { KindSet } from './kinds';`n" +
            "@Component({ selector: 'demo-host', template: ``n" +
            "  <div *ngIf=`"vm.kind === KindSet.Home; else elseTpl`"><translate [text]=`"'demo.list.then'`"></translate></div>`n" +
            "  <ng-template #elseTpl><translate [text]=`"'demo.list.else'`"></translate><demo-child></demo-child></ng-template>`n`` })`n" +
            "export class HostComponent {`n  KindSet = KindSet;`n  vm: { kind: KindSet } = { kind: KindSet.Home };`n}`n"
        'apps/shop/src/app.module.ts' = "import { NgModule } from '@angular/core';`nimport { CommonModule } from '@angular/common';`n" +
            "import { TranslateComponent } from './translate.component';`nimport { ChildComponent } from './child.component';`nimport { HostComponent } from './host.component';`n" +
            "@NgModule({ declarations: [TranslateComponent, ChildComponent, HostComponent], imports: [CommonModule], bootstrap: [HostComponent] })`nexport class AppModule {}`n"
        'apps/shop/src/assets/locales/en.json' = '{"shop":{"title":"Shop"},"demo":{"list":{"then":"T","else":"E","child":"C"}}}'
        'structuregate.ts.json' = '{"locales":["apps/shop/src/assets/locales"],"i18nCarriers":["translate:text"]}'
    }
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    $r = Invoke-Gate --map-query $made.Db --width 0 --sql $script:TsOutletsSql
    Assert-Line $r 'demo.list.then | vm.kind === KindSet.Home & elseTpl | vm.kind === KindSet.Home & elseTpl | vm.kind in ["Home"]'
    Assert-Line $r 'demo.list.else | !(vm.kind === KindSet.Home) | !(vm.kind === KindSet.Home) | vm.kind not_in ["Home"]'
    Assert-Line $r 'demo.list.child | !(vm.kind === KindSet.Home) | !(vm.kind === KindSet.Home) | vm.kind not_in ["Home"]'
}

# ONE VARIABLE UNDER TWO SPELLINGS OF ONE CLASS (#117): `this.vehicle` (a setter written once in the constructor, its getter returning
# the field it copies into) and `get model()` (one plain return) both read `this.ctx.input` - so on the `else` way `vehicle.kind not_in
# [Home, Flat]` and `model.vehicle.kind in [Life]` are one dimension and intersect, the `ngIf` way (Home or Flat, and Life) is no way at
# all, and `demo.list.life` renders for Life alone. Unioned as two variables, the ways permitted every member and the key nothing.
Test-Case 'tsrows: two spellings a class proves one chain are one dimension on a way' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/kinds.ts' = "export enum KindSet { Home = 1, Flat = 2, Life = 3, Boat = 4 }`nexport interface Vehicle { kind: KindSet }`n"
        'apps/shop/src/translate.component.ts' = "import { Component, Input } from '@angular/core';`n" +
            "@Component({ selector: 'translate', template: '{{ text }}' })`nexport class TranslateComponent { @Input() text: string = ''; }`n"
        'apps/shop/src/base.ts' = "import { Directive } from '@angular/core';`nimport { KindSet, Vehicle } from './kinds';`n" +
            "@Directive()`nexport class DemoBase {`n  ctx: { input: { vehicle: Vehicle } } = { input: { vehicle: { kind: KindSet.Home } } };`n" +
            "  canShow = false;`n  private _vehicle: Vehicle;`n  get vehicle(): Vehicle { return this._vehicle; }`n  set vehicle(value: Vehicle) { this._vehicle = value; }`n" +
            "  constructor() {`n    this.vehicle = this.ctx.input.vehicle;`n    this.canShow = this.vehicle.kind === KindSet.Home || this.vehicle.kind === KindSet.Flat;`n  }`n" +
            "  get model(): { vehicle: Vehicle } { return this.ctx.input; }`n}`n"
        'apps/shop/src/host.component.ts' = "import { Component, OnInit } from '@angular/core';`nimport { KindSet } from './kinds';`nimport { DemoBase } from './base';`n" +
            "@Component({ selector: 'demo-host', template: ``n" +
            "  <div *ngIf=`"canShow; else elseTpl`"><translate [text]=`"isLife ? 'demo.list.life' : 'demo.list.other'`"></translate></div>`n" +
            "  <ng-template #elseTpl><translate [text]=`"isLife ? 'demo.list.life' : 'demo.list.other'`"></translate></ng-template>`n`` })`n" +
            "export class HostComponent extends DemoBase implements OnInit {`n  isLife = false;`n  ngOnInit(): void { this.isLife = this.model.vehicle.kind === KindSet.Life; }`n}`n"
        # A THIRD WAY spelling the enum otherwise: no one dimension is restricted on every way, so the fold reads each way's union.
        'apps/shop/src/other.component.ts' = "import { Component } from '@angular/core';`nimport { KindSet } from './kinds';`n" +
            "@Component({ selector: 'demo-other', template: '<translate *ngIf=`"kind === KindSet.Life`" [text]=`"\'demo.list.life\'`"></translate>' })`n" +
            "export class OtherComponent {`n  KindSet = KindSet;`n  kind: KindSet = KindSet.Home;`n}`n"
        'apps/shop/src/app.module.ts' = "import { NgModule } from '@angular/core';`nimport { CommonModule } from '@angular/common';`n" +
            "import { TranslateComponent } from './translate.component';`nimport { HostComponent } from './host.component';`nimport { OtherComponent } from './other.component';`n" +
            "@NgModule({ declarations: [TranslateComponent, HostComponent, OtherComponent], imports: [CommonModule], bootstrap: [HostComponent, OtherComponent] })`nexport class AppModule {}`n"
        'apps/shop/src/assets/locales/en.json' = '{"shop":{"title":"Shop"},"demo":{"list":{"life":"L","other":"O"}}}'
        'structuregate.ts.json' = '{"locales":["apps/shop/src/assets/locales"],"i18nCarriers":["translate:text"]}'
    }
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    $r = Invoke-Gate --map-query $made.Db --width 0 --sql ($script:TsOutletsSql.Replace("k.key LIKE 'demo.list.%'", "k.key = 'demo.list.life'"))
    Assert-Line $r 'demo.list.life | - | canShow & elseTpl & !(canShow) & kind === KindSet.Life | kind | model.vehicle.kind | vehicle.kind in ["Life"]'
}

}
