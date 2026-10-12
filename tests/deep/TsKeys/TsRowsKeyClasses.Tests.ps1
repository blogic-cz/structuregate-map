<#
    A key a CLASS INSTANCE carries reaches the template that reads it through the instance
    (`rust/fbtcore/src/rows/ts/key/fields/key_classes.rs`): a constructor's `this.<member> = ..` write is the
    member's value, and a member read on a receiver typed by a class - or by its base class or an interface - is
    the member of that class and of every subclass or implementer a `new` builds somewhere in the tree.
    A class nothing builds contributes nothing. Its helpers are `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '..\TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

function New-KeyClassesTree {
    return New-TsRowsWorkspace @{
        'apps/shop/src/translate.service.ts' = "import { Injectable } from '@angular/core';`n" +
            "@Injectable({ providedIn: 'root' })`n" +
            "export class TranslateService {`n  getTranslation(key: string): string {`n    return key;`n  }`n}`n"
        'apps/shop/src/btn.ts' = "import { TranslateService } from './translate.service';`n" +
            "export class Btn {`n  text: string;`n" +
            "  constructor(t: TranslateService) { this.text = t.getTranslation('demo.btn.a'); }`n}`n"
        'apps/shop/src/strat.ts' = "import { TranslateService } from './translate.service';`n" +
            "export interface IStrat { tip(): string; }`n" +
            "export class StratA implements IStrat {`n  constructor(private t: TranslateService) {}`n" +
            "  tip(): string { return this.t.getTranslation('demo.strat.a'); }`n}`n" +
            "export class StratB implements IStrat {`n  constructor(private t: TranslateService) {}`n" +
            "  tip(): string { return this.t.getTranslation('demo.strat.b'); }`n}`n" +
            "export class StratC implements IStrat {`n  constructor(private t: TranslateService) {}`n" +
            "  tip(): string { return this.t.getTranslation('demo.strat.c'); }`n}`n"
        'apps/shop/src/button.ts' = "import { TranslateService } from './translate.service';`n" +
            "export abstract class ActionButton {`n  text = '';`n}`n" +
            "export class PaidButton extends ActionButton {`n" +
            "  constructor(t: TranslateService) { super(); this.text = t.getTranslation('demo.paid.a'); }`n}`n" +
            "export class IdleButton extends ActionButton {`n" +
            "  constructor(t: TranslateService) { super(); this.text = t.getTranslation('demo.idle.a'); }`n}`n"
        'apps/shop/src/host.component.ts' = "import { Component, OnInit } from '@angular/core';`n" +
            "import { Btn } from './btn';`n" +
            "import { IStrat, StratA, StratB } from './strat';`n" +
            "import { ActionButton, PaidButton } from './button';`n" +
            "import { TranslateService } from './translate.service';`n" +
            "@Component({ selector: 'demo-host', standalone: true, template: '{{ c }} {{ d }} {{ e }}' })`n" +
            "export class HostComponent implements OnInit {`n" +
            "  c = ''; d = ''; e = '';`n  ab!: ActionButton;`n  btn!: Btn;`n  s!: IStrat;`n  spare: IStrat | null = null;`n" +
            "  constructor(private t: TranslateService) {}`n" +
            "  ngOnInit() {`n    this.btn = new Btn(this.t);`n    this.c = this.btn.text;`n" +
            "    this.s = new StratA(this.t);`n    this.spare = new StratB(this.t);`n    this.d = this.s.tip();`n    this.ab = new PaidButton(this.t);`n    this.e = this.ab.text;`n  }`n}`n"
        'apps/shop/src/assets/locales/en.json' = '{"shop":{"title":"Shop"},"demo":{"btn":{"a":"a"},"strat":{"a":"b","b":"c","c":"d"},"paid":{"a":"e"},"idle":{"a":"f"}}}'
        'structuregate.ts.json' = '{"locales":["apps/shop/src/assets/locales"],"i18nCalls":["getTranslation"]}'
    }
}

Test-Case 'tsrows keys: a constructor write and an interface method reach the template through the instance' {
    $tree = New-KeyClassesTree
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    $r = Invoke-TsRowsQ $made.Db ("SELECT k.key || ' | ' || k.route || ' | ' || c.name AS reach FROM key_reach k, " +
        "json_each(k.components) j JOIN components c ON c.id = j.value WHERE k.key LIKE 'demo.%'")
    # the class built with `new` holds its constructor's write
    Assert-Line $r 'demo.btn.a | field_binding | HostComponent'
    # every implementer a `new` builds is a way the interface's method answers
    Assert-Line $r 'demo.strat.a | field_binding | HostComponent'
    Assert-Line $r 'demo.strat.b | field_binding | HostComponent'
    # an implementer nothing builds contributes nothing
    Assert-NoLine $r 'demo.strat.c | field_binding | HostComponent'
    # a base class's receiver reads what a built subclass's constructor wrote to the field; a subclass nothing builds writes nothing
    Assert-Line $r 'demo.paid.a | field_binding | HostComponent'
    Assert-NoLine $r 'demo.idle.a | field_binding | HostComponent'
}
}
