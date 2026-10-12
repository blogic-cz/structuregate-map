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
            "  tip(): string { return this.t.getTranslation('demo.strat.c'); }`n}`n" +
            "export interface IMark { mark(): string; }`n" +
            "export class MarkA implements IMark {`n  constructor(private t: TranslateService) {}`n" +
            "  mark(): string { return this.t.getTranslation('demo.mark.a'); }`n}`n" +
            "export class MarkB implements IMark {`n  constructor(private t: TranslateService) {}`n" +
            "  mark(): string { return this.t.getTranslation('demo.mark.b'); }`n}`n" +
            "export class MarkC implements IMark {`n  constructor(private t: TranslateService) {}`n" +
            "  mark(): string { return this.t.getTranslation('demo.mark.c'); }`n}`n"
        'apps/shop/src/button.ts' = "import { TranslateService } from './translate.service';`n" +
            "export abstract class ActionButton {`n  text = '';`n}`n" +
            "export class PaidButton extends ActionButton {`n" +
            "  constructor(t: TranslateService) { super(); this.text = t.getTranslation('demo.paid.a'); }`n}`n" +
            "export class IdleButton extends ActionButton {`n" +
            "  constructor(t: TranslateService) { super(); this.text = t.getTranslation('demo.idle.a'); }`n}`n"
        'apps/shop/src/host.component.ts' = "import { Component, OnInit } from '@angular/core';`n" +
            "import { Btn } from './btn';`n" +
            "import { IMark, IStrat, MarkA, MarkB, StratA, StratB } from './strat';`n" +
            "import { ActionButton, PaidButton } from './button';`n" +
            "import { TranslateService } from './translate.service';`n" +
            "@Component({ selector: 'demo-host', standalone: true, template: '{{ c }} {{ d }} {{ e }} {{ m.mark() }}' })`n" +
            "export class HostComponent implements OnInit {`n" +
            "  c = ''; d = ''; e = '';`n  ab!: ActionButton;`n  m!: IMark;`n  other!: IMark;`n  btn!: Btn;`n  s!: IStrat;`n  spare: IStrat | null = null;`n" +
            "  constructor(private t: TranslateService) {}`n" +
            "  ngOnInit() {`n    this.btn = new Btn(this.t);`n    this.c = this.btn.text;`n" +
            "    this.s = new StratA(this.t);`n    this.spare = new StratB(this.t);`n    this.d = this.s.tip();`n    this.ab = new PaidButton(this.t);`n    this.m = new MarkA(this.t);`n    this.other = new MarkB(this.t);`n    this.e = this.ab.text;`n  }`n}`n"
        'apps/shop/src/assets/locales/en.json' = '{"shop":{"title":"Shop"},"demo":{"btn":{"a":"a"},"strat":{"a":"b","b":"c","c":"d"},"paid":{"a":"e"},"idle":{"a":"f"},"mark":{"a":"g","b":"h","c":"i"}}}'
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
    # the template calling the interface's method itself: every implementer a `new` builds, never one nothing builds
    Assert-Line $r 'demo.mark.a | field_binding | HostComponent'
    Assert-Line $r 'demo.mark.b | field_binding | HostComponent'
    Assert-NoLine $r 'demo.mark.c | field_binding | HostComponent'
}

Test-Case 'tsrows keys: a list of instances a parent pushes and a child loops over reaches the key through a nested property path' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/tsconfig.app.json' = '{"compilerOptions":{"strict":true,"target":"ES2022"},"include":["src/**/*.ts"]}'
        'apps/shop/src/translate.service.ts' = "import { Injectable } from '@angular/core';`n" +
            "@Injectable({ providedIn: 'root' })`n" +
            "export class TranslateService {`n  getTranslation(key: string): string {`n    return key;`n  }`n}`n"
        'apps/shop/src/row-button.ts' = "import { TranslateService } from './translate.service';`n" +
            "export interface Row { paid: boolean; }`n" +
            "export abstract class RowButton<T> {`n  text = '';`n  tooltip!: { text: (row: T) => string };`n  abstract onClick(row: T): void;`n}`n" +
            "export class PayButton extends RowButton<Row> {`n" +
            "  constructor(private t: TranslateService) { super(); this.text = 'icon.svg'; this.tooltip = { text: this.tipText }; }`n" +
            "  private tipText = (row: Row): string => { if (row.paid) { return this.t.getTranslation('demo.pay.yes'); } return this.t.getTranslation('demo.pay.no'); };`n" +
            "  onClick(row: Row): void {}`n}`n" +
            "export class SendButton extends RowButton<Row> {`n" +
            "  constructor(private t: TranslateService) { super(); this.text = t.getTranslation('demo.send.label'); this.tooltip = { text: this.tipText }; }`n" +
            "  private tipText = (row: Row): string => this.t.getTranslation('demo.send.tip');`n" +
            "  onClick(row: Row): void {}`n}`n" +
            "export class SpareButton extends RowButton<Row> {`n" +
            "  constructor(private t: TranslateService) { super(); this.text = t.getTranslation('demo.spare.label'); this.tooltip = { text: this.tipText }; }`n" +
            "  private tipText = (row: Row): string => this.t.getTranslation('demo.spare.tip');`n" +
            "  onClick(row: Row): void {}`n}`n"
        'apps/shop/src/demo-table.component.ts' = "import { Component, Input } from '@angular/core';`n" +
            "import { NgFor } from '@angular/common';`n" +
            "import { RowButton } from './row-button';`n" +
            "@Component({ selector: 'demo-table', standalone: true, imports: [NgFor], template: '<button *ngFor=`"let b of buttons`" [title]=`"b.tooltip.text(row)`">{{ b.text }}</button>' })`n" +
            "export class DemoTableComponent {`n  @Input() buttons!: RowButton<any>[];`n  row: any;`n}`n"
        'apps/shop/src/page.component.ts' = "import { Component, OnInit } from '@angular/core';`n" +
            "import { DemoTableComponent } from './demo-table.component';`n" +
            "import { PayButton, Row, RowButton, SendButton } from './row-button';`n" +
            "import { TranslateService } from './translate.service';`n" +
            "@Component({ selector: 'demo-page', standalone: true, imports: [DemoTableComponent], template: '<demo-table [buttons]=`"tableButtons`"></demo-table>' })`n" +
            "export class PageComponent implements OnInit {`n  tableButtons: RowButton<Row>[] = [];`n" +
            "  constructor(private t: TranslateService) {}`n" +
            "  ngOnInit() {`n    const buttons: RowButton<Row>[] = [];`n    buttons.push(new PayButton(this.t));`n" +
            "    Promise.resolve(1).then(() => {`n      this.tableButtons = buttons;`n      this.tableButtons.push(new SendButton(this.t));`n    });`n  }`n}`n"
        'apps/shop/src/assets/locales/en.json' = '{"shop":{"title":"Shop"},"demo":{"pay":{"yes":"a","no":"b"},"send":{"label":"c","tip":"d"},"spare":{"label":"e","tip":"f"}}}'
        'structuregate.ts.json' = '{"locales":["apps/shop/src/assets/locales"],"i18nCalls":["getTranslation"]}'
    }
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    $r = Invoke-TsRowsQ $made.Db ("SELECT k.key || ' | ' || k.route || ' | ' || c.name AS reach FROM key_reach k, " +
        "json_each(k.components) j JOIN components c ON c.id = j.value WHERE k.key LIKE 'demo.%'")
    # an arrow property the constructor puts in an object written to `tooltip`: every return of it, read where the child calls it
    Assert-Line $r 'demo.pay.yes | field_binding | DemoTableComponent'
    Assert-Line $r 'demo.pay.no | field_binding | DemoTableComponent'
    Assert-Line $r 'demo.send.tip | field_binding | DemoTableComponent'
    # a translate call the constructor writes straight to a member, read as `b.text`
    Assert-Line $r 'demo.send.label | field_binding | DemoTableComponent'
    # a class nothing builds is no element of the list
    Assert-NoLine $r 'demo.spare.tip | field_binding | DemoTableComponent'
    Assert-NoLine $r 'demo.spare.label | field_binding | DemoTableComponent'
}

Test-Case 'tsrows keys: a method an instance holds hands back every return it spells, and a constructor callback writes the instance' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/tsconfig.app.json' = '{"compilerOptions":{"strict":true,"target":"ES2022"},"include":["src/**/*.ts"]}'
        'apps/shop/src/translate.service.ts' = "import { Injectable } from '@angular/core';`n" +
            "@Injectable({ providedIn: 'root' })`n" +
            "export class TranslateService {`n  getTranslation(key: string): string {`n    return key;`n  }`n}`n"
        'apps/shop/src/row-button.ts' = "import { TranslateService } from './translate.service';`n" +
            "export interface Row { state: number; paid: boolean; }`n" +
            "export abstract class RowButton<T> {`n  text = '';`n  tooltip!: { text: (row: T) => string; position?: string };`n}`n" +
            "export class VoidButton extends RowButton<Row> {`n" +
            "  constructor() { super(); this.tooltip = { text: this.tipText, position: 'left' }; }`n" +
            "  private tipText(row: Row): string {`n    const gone = row.state === 1;`n" +
            "    if (gone) {`n      return 'demo.void.yes';`n    } else {`n      return 'demo.void.no';`n    }`n  }`n}`n" +
            "export class PayButton extends RowButton<Row> {`n" +
            "  constructor(private t: TranslateService) {`n    super();`n" +
            "    Promise.resolve(true).then((on) => {`n      if (on) {`n        this.tooltip = { text: this.stateTip, position: 'left' };`n      }`n    });`n  }`n" +
            "  private stateTip = (row: Row): string => {`n    let text = '';`n    switch (row.state) {`n" +
            "      case 1:`n        text = this.t.getTranslation('demo.pay.active');`n        break;`n" +
            "      case 2:`n        text = this.t.getTranslation('demo.pay.gone');`n        break;`n    }`n" +
            "    if (row.paid) {`n      return this.t.getTranslation('demo.pay.done');`n    }`n    return text;`n  };`n}`n"
        'apps/shop/src/demo-table.component.ts' = "import { Component, Input } from '@angular/core';`n" +
            "import { NgFor } from '@angular/common';`n" +
            "import { RowButton } from './row-button';`n" +
            "@Component({ selector: 'demo-table', standalone: true, imports: [NgFor], template: '<button *ngFor=`"let b of buttons`" [title]=`"b.tooltip.text(row)`">{{ b.text }}</button>' })`n" +
            "export class DemoTableComponent {`n  @Input() buttons!: RowButton<any>[];`n  row: any;`n}`n"
        'apps/shop/src/page.component.ts' = "import { Component, OnInit } from '@angular/core';`n" +
            "import { DemoTableComponent } from './demo-table.component';`n" +
            "import { PayButton, Row, RowButton, VoidButton } from './row-button';`n" +
            "import { TranslateService } from './translate.service';`n" +
            "@Component({ selector: 'demo-page', standalone: true, imports: [DemoTableComponent], template: '<demo-table [buttons]=`"tableButtons`"></demo-table>' })`n" +
            "export class PageComponent implements OnInit {`n  tableButtons: RowButton<Row>[] = [];`n" +
            "  constructor(private t: TranslateService) {}`n" +
            "  ngOnInit() {`n    const buttons: RowButton<Row>[] = [];`n    buttons.push(new VoidButton());`n" +
            "    buttons.push(new PayButton(this.t));`n    this.tableButtons = buttons;`n  }`n}`n"
        'apps/shop/src/assets/locales/en.json' = '{"shop":{"title":"Shop"},"demo":{"void":{"yes":"a","no":"b"},"pay":{"active":"c","gone":"d","done":"e"}}}'
        'structuregate.ts.json' = '{"locales":["apps/shop/src/assets/locales"],"i18nCalls":["getTranslation"]}'
    }
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    $r = Invoke-TsRowsQ $made.Db ("SELECT k.key || ' | ' || k.route || ' | ' || c.name AS reach FROM key_reach k, " +
        "json_each(k.components) j JOIN components c ON c.id = j.value WHERE k.key LIKE 'demo.%'")
    # a METHOD written to `tooltip.text`, its two returns each a key it spells: the template calls it, so either renders
    Assert-Line $r 'demo.void.yes | field_binding | DemoTableComponent'
    Assert-Line $r 'demo.void.no | field_binding | DemoTableComponent'
    # `tooltip` written in a `.then` callback inside the constructor; the returned `let` is what its `case`s assign
    Assert-Line $r 'demo.pay.active | field_binding | DemoTableComponent'
    Assert-Line $r 'demo.pay.gone | field_binding | DemoTableComponent'
    Assert-Line $r 'demo.pay.done | field_binding | DemoTableComponent'
}
}
