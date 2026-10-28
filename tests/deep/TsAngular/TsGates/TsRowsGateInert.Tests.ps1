<#
    A structural directive that ALWAYS renders (`*demoVar="x"`: its setter clears and renders, on no condition) is no gate:
    a key under it keeps every other gate of its chain and loses that one, in `always_gates`, `maybe_gates` and
    `render_path.gates`. One rendering under an `if`, or taking the view back elsewhere (`clear()` in `ngOnChanges`),
    stays a gate. Its ALIAS (`*demoVar="E as v"`) read in a condition below is `E`, so a key that arm chooses keeps `E`'s restriction. Read in `rust/fbtcore/src/rows/ts/gate/ts/directive/gate_inert.rs`. Its helpers are `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '../../TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

# key | always_gates | maybe_gates, each gate by its name, `-` for none.
$script:TsInertValuesSql = "SELECT k.key || ' | ' || COALESCE((SELECT group_concat($(Get-TsFieldSql 'key_reach__always_values' 'v' 'dimension') || ' ' || " +
    "$(Get-TsFieldSql 'key_reach__always_values' 'v' 'op') || ' ' || $(Get-NodeListSql 'key_reach__always_values' 'v' 'values'), ' & ') " +
    "FROM key_reach__always_values v WHERE v.owner = '#' || k.rowid AND v.parent = 0), '-') AS reach FROM key_reach k WHERE k.key LIKE 'demo.alias.%' ORDER BY 1"

$script:TsInertSql = "SELECT k.key || ' | ' || " +
    "COALESCE((SELECT group_concat(g.name, ' & ') FROM key_reach__always_gates a JOIN gates g ON g.id = a.value WHERE a.owner = '#' || k.rowid AND a.parent = 0), '-') || ' | ' || " +
    "COALESCE((SELECT group_concat(g.name, ' & ') FROM key_reach__maybe_gates m JOIN gates g ON g.id = m.value WHERE m.owner = '#' || k.rowid AND m.parent = 0), '-') AS reach " +
    "FROM key_reach k WHERE k.key LIKE 'demo.inert.%' ORDER BY 1"

Test-Case 'tsrows: a structural directive that always renders is no gate of the keys under it' {
    $directive = { param($name, $body)
        "@Directive({ selector: '[$name]' })`n" +
        "export class $($name)Directive {`n" +
        "  private context: { `$implicit?: unknown } = {};`n" +
        "  constructor(private vcRef: ViewContainerRef, private templateRef: TemplateRef<unknown>) {}`n" +
        $body + "}`n"
    }
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/translate.component.ts' = "import { Component, Input } from '@angular/core';`n" +
            "@Component({ selector: 'translate', template: '{{ text }}' })`n" +
            "export class TranslateComponent { @Input() text: string = ''; }`n"
        'apps/shop/src/directives.ts' = "import { Directive, Input, OnChanges, TemplateRef, ViewContainerRef } from '@angular/core';`n" +
            # ALWAYS: the setter calls a method that clears and renders, on no condition.
            (& $directive 'demoVar' ("  @Input() set demoVar(v: unknown) { this.context.`$implicit = v; this.update(); }`n" +
                "  update(): void { this.vcRef.clear(); this.vcRef.createEmbeddedView(this.templateRef, this.context); }`n")) +
            # UNDER AN IF: a gate.
            (& $directive 'demoShow' ("  @Input() set demoShow(v: unknown) {`n" +
                "    this.vcRef.clear();`n    if (v) { this.vcRef.createEmbeddedView(this.templateRef); }`n  }`n")) +
            # RENDERS ALWAYS, BUT TAKES IT BACK in another hook: a gate.
            (& $directive 'demoOnce' ("  @Input() set demoOnce(v: unknown) { this.vcRef.createEmbeddedView(this.templateRef); }`n" +
                "  ngOnChanges(): void { this.vcRef.clear(); }`n"))
        'apps/shop/src/kinds.ts' = "export enum KindSet { Home = 1, Flat = 2 }`n"
        'apps/shop/src/list.component.ts' = "import { Component } from '@angular/core';`nimport { KindSet } from './kinds';`n" +
            "@Component({ selector: 'demo-list', template: ``n" +
            "  <div *ngIf=`"vm.on`"><span *demoVar=`"vm.item as item`"><translate [text]=`"'demo.inert.var'`"></translate></span></div>`n" +
            "  <span *demoShow=`"vm.on`"><translate [text]=`"'demo.inert.show'`"></translate></span>`n" +
            "  <span *demoOnce=`"vm.on`"><translate [text]=`"'demo.inert.once'`"></translate></span>`n" +
            "  <span *demoVar=`"vm.kind === KindSet.Home as home`"><translate [text]=`"home ? 'demo.alias.home' : 'demo.alias.away'`"></translate></span>`n" +
            "`` })`n" +
            "export class ListComponent { KindSet = KindSet; vm: { on: boolean; item: number; kind: KindSet } = { on: true, item: 1, kind: KindSet.Home }; }`n"
        'apps/shop/src/root.component.ts' = "import { Component, NgModule } from '@angular/core';`n" +
            "import { ListComponent } from './list.component';`n" +
            "import { TranslateComponent } from './translate.component';`n" +
            "import { demoVarDirective, demoShowDirective, demoOnceDirective } from './directives';`n" +
            "@Component({ selector: 'demo-root', template: '<demo-list></demo-list>' })`n" +
            "export class RootComponent {}`n" +
            "@NgModule({ declarations: [RootComponent, ListComponent, TranslateComponent, demoVarDirective, demoShowDirective, demoOnceDirective], bootstrap: [RootComponent] })`n" +
            "export class DemoModule {}`n"
        'apps/shop/src/assets/locales/en.json' = '{"shop":{"title":"Shop"},"demo":{"inert":{"var":"1","show":"2","once":"3"},"alias":{"home":"4","away":"5"}}}'
        'structuregate.ts.json' = '{"locales":["apps/shop/src/assets/locales"],"i18nCarriers":["translate:text"]}'
    }
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    $r = Invoke-Gate --map-query $made.Db --width 0 --sql $script:TsInertSql
    Assert-Exit $r 0
    Assert-Line $r 'demo.inert.var | ngIf | ngIf'
    Assert-Line $r 'demo.inert.show | demoShow | demoShow'
    Assert-Line $r 'demo.inert.once | demoOnce | demoOnce'
    $a = Invoke-Gate --map-query $made.Db --width 0 --sql $script:TsInertValuesSql
    Assert-Line $a 'demo.alias.home | vm.kind in ["Home"]'
    Assert-Line $a 'demo.alias.away | vm.kind in ["Flat"]'
    # The gate row stays: only the chains drop it.
    $g = Invoke-Gate --map-query $made.Db --width 0 --sql "SELECT 'gate=' || name || ' ' || gate_kind AS r FROM gates WHERE name = 'demoVar'"
    Assert-Line $g 'gate=demoVar structural'
}

}
