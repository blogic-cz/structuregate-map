<#
    A key inside an `<ng-template #ref>` that only OUTLETS render (`[ngTemplateOutlet]="ref"`, `*ngTemplateOutlet="ref"`,
    `*ngIf="c; then ref"`): each outlet's own gate chain is a WAY in to the template's contents, so the key is
    restricted by what every outlet shares (`always_*`) and the rest is `maybe_gates` (#51). An outlet's own
    hand-over binding (`ngTemplateOutlet`, `ngIfThen`, `ngIfElse`) is no condition. A template a non-outlet also
    reads - an `else`, an event handler, a `@ViewChild` - stays as it was: the key has its own chain only. Read in
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
            # `then` renders under the condition; `else` under its negation, which a gate list cannot say.
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
    Assert-Line $r 'demo.list.e | - | - | -'
    Assert-Line $r 'demo.list.g | - | - | -'
    # Two outlets are two ways in: counted as an arm's ways are.
    $n = Invoke-Gate --map-query $made.Db --width 0 --sql ("SELECT key || ' paths ' || n_paths AS n FROM key_reach " +
        "WHERE key IN ('demo.list.b', 'demo.list.selectAll') ORDER BY 1")
    Assert-Line $n 'demo.list.b paths 2'
    Assert-Line $n 'demo.list.selectAll paths 1'
}

}
