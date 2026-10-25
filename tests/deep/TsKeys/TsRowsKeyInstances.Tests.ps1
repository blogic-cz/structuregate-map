<#
    Lists of class instances a parent builds and a child's table renders, and the hand-overs around them
    (`rust/fbtcore/src/rows/ts/key/lists/key_lists.rs`, `key/fields/key_inputs.rs`, `key/fields/key_handed.rs`,
    `key/fields/key_classes.rs`, `key/fields/key_then.rs`). Its helpers are `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '..\TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

$script:InstReach = "SELECT k.key || ' | ' || k.route || ' | ' || c.name AS reach FROM key_reach k " +
    "JOIN key_reach__components j ON j.owner = '#' || k.rowid AND j.parent = 0 JOIN components c ON c.id = j.value WHERE k.key LIKE 'demo.%'"

Test-Case 'tsrows keys: a table reached through a setter input, a method handed the loop item and a returned pushed list, under one parent of two' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/tsconfig.app.json' = '{"compilerOptions":{"strict":true,"target":"ES2022"},"include":["src/**/*.ts"]}'
        'apps/shop/src/translate.service.ts' = "import { Injectable } from '@angular/core';`n" +
            "@Injectable({ providedIn: 'root' })`n" +
            "export class TranslateService {`n  getTranslation(key: string): string {`n    return key;`n  }`n}`n"
        'apps/shop/src/row-button.ts' = "import { TranslateService } from './translate.service';`n" +
            "export abstract class RowButton<T> {`n  text: string | ((row: T) => string) = '';`n  tooltip!: { text: string | ((row: T) => string) };`n}`n" +
            "export class VoidButton extends RowButton<number> {`n" +
            "  constructor(private t: TranslateService) { super(); this.tooltip = { text: (row: number) => this.t.getTranslation('demo.void.tip') }; }`n}`n" +
            "export class OpenButton extends RowButton<number> {`n" +
            "  constructor(private t: TranslateService) { super(); this.text = t.getTranslation('demo.open.label'); }`n}`n" +
            "export class MoreButton extends RowButton<number> {`n" +
            "  constructor(private t: TranslateService) { super(); this.text = t.getTranslation('demo.more.label'); }`n}`n"
        'apps/shop/src/demo-table.component.ts' = "import { Component, Input } from '@angular/core';`n" +
            "import { NgFor } from '@angular/common';`n" +
            "import { RowButton } from './row-button';`n" +
            "@Component({ selector: 'demo-table', standalone: true, imports: [NgFor], template: '<button *ngFor=`"let b of buttons`" [title]=`"tipOf(b, row)`">{{ textOf(b, row) }}</button>' })`n" +
            "export class DemoTableComponent {`n  private _buttons: RowButton<any>[] = [];`n" +
            "  @Input() set buttons(b: RowButton<any>[]) { this._buttons = b; }`n  get buttons() { return this._buttons; }`n  row: any;`n" +
            "  tipOf(button: RowButton<any>, row: any) {`n    const text = button.tooltip && button.tooltip.text;`n    return typeof text === 'function' ? text(row) : text;`n  }`n" +
            "  textOf(button: RowButton<any>, row: any) {`n    return typeof button.text === 'function' ? button.text(row) : button.text;`n  }`n}`n"
        'apps/shop/src/page.component.ts' = "import { Component, OnInit } from '@angular/core';`n" +
            "import { DemoTableComponent } from './demo-table.component';`n" +
            "import { MoreButton, OpenButton, RowButton, VoidButton } from './row-button';`n" +
            "import { TranslateService } from './translate.service';`n" +
            "@Component({ selector: 'demo-page', standalone: true, imports: [DemoTableComponent], template: '<demo-table [buttons]=`"actions`"></demo-table>' })`n" +
            "export class PageComponent implements OnInit {`n  actions: RowButton<number>[] = [];`n" +
            "  constructor(private t: TranslateService) {}`n" +
            "  ngOnInit() {`n    this.actions = this.build();`n    this.actions.push(...this.more());`n  }`n" +
            "  private build(): RowButton<number>[] {`n    const out: RowButton<number>[] = [];`n    out.push(new OpenButton(this.t));`n" +
            "    Promise.resolve(true).then((on) => {`n      if (on) { const v = new VoidButton(this.t); out.push(v); }`n    });`n    return out;`n  }`n" +
            "  private more(): RowButton<number>[] { return [new MoreButton(this.t)]; }`n}`n"
        'apps/shop/src/other.component.ts' = "import { Component } from '@angular/core';`n" +
            "import { DemoTableComponent } from './demo-table.component';`n" +
            "@Component({ selector: 'demo-other', standalone: true, imports: [DemoTableComponent], template: '<demo-table></demo-table>' })`n" +
            "export class OtherComponent {}`n"
        'apps/shop/src/assets/locales/en.json' = '{"shop":{"title":"Shop"},"demo":{"void":{"tip":"a"},"open":{"label":"b"},"more":{"label":"c"}}}'
        'structuregate.ts.json' = '{"locales":["apps/shop/src/assets/locales"],"i18nCalls":["getTranslation"]}'
    }
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    $r = Invoke-TsRowsQ $made.Db $script:InstReach
    # a method handed the loop item reads `button.tooltip.text`; the list is a callee's returned local, pushed in a callback
    Assert-Line $r 'demo.void.tip | field_binding | DemoTableComponent'
    # `button.text`, of an element pushed straight onto the returned local
    Assert-Line $r 'demo.open.label | field_binding | DemoTableComponent'
    # `this.actions.push(...this.more())`: the spread hands over what `more` returns
    Assert-Line $r 'demo.more.label | field_binding | DemoTableComponent'
    # the table has two ways in, and only the page's binds the list: the key renders on that way alone
    $p = Invoke-TsRowsQ $made.Db "SELECT 'paths ' || n_paths AS n FROM key_reach WHERE key = 'demo.open.label'"
    Assert-Line $p 'paths 1'
    $all = Invoke-TsRowsQ $made.Db "SELECT 'table paths ' || count(*) AS n FROM render_path p JOIN components c ON c.id = p.component WHERE c.name = 'DemoTableComponent'"
    Assert-Line $all 'table paths 2'
}

Test-Case 'tsrows keys: an input bound to a call or a getter whose returns are lists hands the child its elements' {
    # Object literals, not class instances: a read through a class's property would reach the keys without the binding.
    $tree = New-TsRowsWorkspace @{
        'apps/shop/tsconfig.app.json' = '{"compilerOptions":{"strict":true,"target":"ES2022"},"include":["src/**/*.ts"]}'
        'apps/shop/src/translate.service.ts' = "import { Injectable } from '@angular/core';`n" +
            "@Injectable({ providedIn: 'root' })`n" +
            "export class TranslateService {`n  getTranslation(key: string): string {`n    return key;`n  }`n}`n"
        'apps/shop/src/table.component.ts' = "import { Component, Input } from '@angular/core';`n" +
            "import { NgFor } from '@angular/common';`n" +
            "@Component({ selector: 'demo-table', standalone: true, imports: [NgFor], template: '<b *ngFor=`"let x of items`">{{ x.label }}</b>' })`n" +
            "export class TableComponent {`n  @Input() items!: { label: string }[];`n}`n"
        'apps/shop/src/page.component.ts' = "import { Component } from '@angular/core';`n" +
            "import { TableComponent } from './table.component';`n" +
            "import { TranslateService } from './translate.service';`n" +
            "@Component({ selector: 'demo-page', standalone: true, imports: [TableComponent], template: '<demo-table [items]=`"make()`"></demo-table><demo-table [items]=`"listed`"></demo-table>' })`n" +
            "export class PageComponent {`n  constructor(private t: TranslateService) {}`n" +
            "  make(): { label: string }[] { return [{ label: this.t.getTranslation('demo.a') }]; }`n" +
            "  get listed(): { label: string }[] { return [{ label: this.t.getTranslation('demo.b') }]; }`n}`n"
        'apps/shop/src/other.component.ts' = "import { Component } from '@angular/core';`n" +
            "import { TableComponent } from './table.component';`n" +
            "@Component({ selector: 'demo-other', standalone: true, imports: [TableComponent], template: '<demo-table></demo-table>' })`n" +
            "export class OtherComponent {}`n"
        'apps/shop/src/assets/locales/en.json' = '{"shop":{"title":"Shop"},"demo":{"a":"a","b":"b"}}'
        'structuregate.ts.json' = '{"locales":["apps/shop/src/assets/locales"],"i18nCalls":["getTranslation"]}'
    }
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    $r = Invoke-TsRowsQ $made.Db $script:InstReach
    Assert-Line $r 'demo.a | field_binding | TableComponent'
    Assert-Line $r 'demo.b | field_binding | TableComponent'
    # three ways into the table; a key the callee's translate call holds keeps the edge it came through, and the page
    # that only hands the list on is no site of its own (#50)
    $p = Invoke-TsRowsQ $made.Db "SELECT 'paths ' || n_paths AS n FROM key_reach WHERE key = 'demo.a'"
    Assert-Line $p 'paths 1'
}

Test-Case 'tsrows keys: an input bound to a property of a field reads the lists written to that property' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/tsconfig.app.json' = '{"compilerOptions":{"strict":true,"target":"ES2022"},"include":["src/**/*.ts"]}'
        'apps/shop/src/translate.service.ts' = "import { Injectable } from '@angular/core';`n" +
            "@Injectable({ providedIn: 'root' })`n" +
            "export class TranslateService {`n  getTranslation(key: string): string {`n    return key;`n  }`n}`n"
        'apps/shop/src/table.component.ts' = "import { Component, Input } from '@angular/core';`n" +
            "import { NgFor } from '@angular/common';`n" +
            "@Component({ selector: 'demo-table', standalone: true, imports: [NgFor], template: '<b *ngFor=`"let x of items`">{{ x.label }}</b>' })`n" +
            "export class TableComponent {`n  @Input() items!: { label: string }[];`n}`n"
        'apps/shop/src/page.component.ts' = "import { Component, OnInit } from '@angular/core';`n" +
            "import { TableComponent } from './table.component';`n" +
            "import { TranslateService } from './translate.service';`n" +
            "@Component({ selector: 'demo-page', standalone: true, imports: [TableComponent], template: '<demo-table [items]=`"cfg.list`"></demo-table><demo-table [items]=`"init.list`"></demo-table><demo-table [items]=`"mixed.list`"></demo-table>' })`n" +
            "export class PageComponent implements OnInit {`n" +
            "  cfg = { list: [] as { label: string }[] };`n" +
            "  init = { list: [{ label: this.t.getTranslation('demo.init') }] };`n" +
            "  mixed = { list: [{ label: this.t.getTranslation('demo.mixed') }] };`n" +
            "  constructor(private t: TranslateService) {}`n" +
            "  ngOnInit() {`n    this.cfg.list = [{ label: this.t.getTranslation('demo.written') }];`n    this.mixed.list = this.mixed.list.concat([]);`n  }`n}`n"
        'apps/shop/src/assets/locales/en.json' = '{"shop":{"title":"Shop"},"demo":{"init":"a","written":"b","mixed":"c"}}'
        'structuregate.ts.json' = '{"locales":["apps/shop/src/assets/locales"],"i18nCalls":["getTranslation"]}'
    }
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    $r = Invoke-TsRowsQ $made.Db $script:InstReach
    # the property's initializer, and a list the class writes to it
    Assert-Line $r 'demo.init | field_binding | TableComponent'
    Assert-Line $r 'demo.written | field_binding | TableComponent'
    # a write that is no list leaves the property unread
    Assert-NoLine $r 'demo.mixed | field_binding | TableComponent'
}

Test-Case 'tsrows keys: a template reading a base or an interface property renders what every built subtype holds' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/translate.service.ts' = "import { Injectable } from '@angular/core';`n" +
            "@Injectable({ providedIn: 'root' })`n" +
            "export class TranslateService {`n  getTranslation(key: string): string {`n    return key;`n  }`n}`n"
        'apps/shop/src/base.ts' = "import { TranslateService } from './translate.service';`n" +
            "export abstract class Base {`n  text = '';`n}`n" +
            "export class PaidB extends Base {`n  constructor(t: TranslateService) { super(); this.text = t.getTranslation('demo.paid'); }`n}`n" +
            "export class IdleB extends Base {`n  constructor(t: TranslateService) { super(); this.text = t.getTranslation('demo.idle'); }`n}`n" +
            "export interface ILabel {`n  label: string;`n}`n"
        # LabA and LabB in ONE file: `this.label` is a write a file+name key cannot tell apart, the writing class can
        'apps/shop/src/labs.ts' = "import { TranslateService } from './translate.service';`n" +
            "import { ILabel } from './base';`n" +
            "export class LabA implements ILabel {`n  label: string;`n  constructor(t: TranslateService) { this.label = t.getTranslation('demo.laba'); }`n}`n" +
            "export class LabB implements ILabel {`n  label = 'demo.labb';`n}`n"
        'apps/shop/src/host.component.ts' = "import { Component } from '@angular/core';`n" +
            "import { Base, ILabel, PaidB } from './base';`n" +
            "import { LabA } from './labs';`n" +
            "import { TranslateService } from './translate.service';`n" +
            "@Component({ selector: 'demo-host', standalone: true, template: '{{ b.text }} {{ l.label }}' })`n" +
            "export class HostComponent {`n  b: Base;`n  l: ILabel;`n" +
            "  constructor(t: TranslateService) {`n    this.b = new PaidB(t);`n    this.l = new LabA(t);`n  }`n}`n"
        'apps/shop/src/assets/locales/en.json' = '{"shop":{"title":"Shop"},"demo":{"paid":"a","idle":"b","laba":"c","labb":"d"}}'
        'structuregate.ts.json' = '{"locales":["apps/shop/src/assets/locales"],"i18nCalls":["getTranslation"]}'
    }
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    $r = Invoke-TsRowsQ $made.Db $script:InstReach
    # `{{ b.text }}` over `b: Base`: the built subclass's constructor write; a subclass nothing builds writes nothing
    Assert-Line $r 'demo.paid | field_binding | HostComponent'
    Assert-NoLine $r 'demo.idle | field_binding | HostComponent'
    # `{{ l.label }}` over `l: ILabel`: the built implementer's, never one nothing builds
    Assert-Line $r 'demo.laba | field_binding | HostComponent'
    Assert-NoLine $r 'demo.labb | field_binding | HostComponent'
}

Test-Case 'tsrows keys: a subtype read through its base renders only on a path through a component that builds it' {
    # Each page builds ONE button kind and hands its list to its own table; both tables read `b.text` over `Base`. Every
    # built subtype's key at every read of the base made each page's button render in the other page's table too - a key a
    # role-gated page alone shows read as shown by every table.
    $table = { param($name, $sel) "import { Component, Input } from '@angular/core';`nimport { NgFor } from '@angular/common';`n" +
        "import { Base } from './base';`n" +
        "@Component({ selector: '$sel', standalone: true, imports: [NgFor], template: '<b *ngFor=`"let b of items`">{{ b.text }}</b>' })`n" +
        "export class $name {`n  @Input() items: Base[] = [];`n}`n" }
    $page = { param($name, $sel, $tableName, $tableSel, $file, $button) "import { Component } from '@angular/core';`n" +
        "import { $tableName } from './$file';`nimport { Base, $button } from './base';`nimport { TranslateService } from './translate.service';`n" +
        "@Component({ selector: '$sel', standalone: true, imports: [$tableName], template: '<$tableSel [items]=`"list`"></$tableSel>' })`n" +
        "export class $name {`n  list: Base[];`n  constructor(t: TranslateService) { this.list = [new $button(t)]; }`n}`n" }
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/translate.service.ts' = "import { Injectable } from '@angular/core';`n" +
            "@Injectable({ providedIn: 'root' })`n" +
            "export class TranslateService {`n  getTranslation(key: string): string {`n    return key;`n  }`n}`n"
        'apps/shop/src/base.ts' = "import { TranslateService } from './translate.service';`n" +
            "export abstract class Base {`n  text = '';`n}`n" +
            "export class PaidB extends Base {`n  constructor(t: TranslateService) { super(); this.text = t.getTranslation('demo.paid'); }`n}`n" +
            "export class IdleB extends Base {`n  constructor(t: TranslateService) { super(); this.text = t.getTranslation('demo.idle'); }`n}`n"
        'apps/shop/src/table-a.component.ts' = (& $table 'TableAComponent' 'demo-table-a')
        'apps/shop/src/table-b.component.ts' = (& $table 'TableBComponent' 'demo-table-b')
        'apps/shop/src/page-a.component.ts' = (& $page 'PageAComponent' 'demo-page-a' 'TableAComponent' 'demo-table-a' 'table-a.component' 'PaidB')
        'apps/shop/src/page-b.component.ts' = (& $page 'PageBComponent' 'demo-page-b' 'TableBComponent' 'demo-table-b' 'table-b.component' 'IdleB')
        'apps/shop/src/assets/locales/en.json' = '{"shop":{"title":"Shop"},"demo":{"paid":"a","idle":"b"}}'
        'structuregate.ts.json' = '{"locales":["apps/shop/src/assets/locales"],"i18nCalls":["getTranslation"]}'
    }
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    $paths = Invoke-TsRowsQ $made.Db ("SELECT 'way ' || k.key || ' ' || group_concat(c.name, '>') AS w FROM key_reach k " +
        "JOIN key_reach__paths p ON p.owner = '#' || k.rowid AND p.role = 'path' " +
        "JOIN render_path__hops h ON h.owner = p.value AND h.parent = 0 JOIN components c ON c.id = h.value " +
        "WHERE k.key IN ('demo.paid', 'demo.idle') GROUP BY k.key, p.value ORDER BY k.key")
    Assert-Line $paths 'way demo.paid PageAComponent>TableAComponent'
    Assert-NoLine $paths 'way demo.paid PageBComponent'
    Assert-Line $paths 'way demo.idle PageBComponent>TableBComponent'
    Assert-NoLine $paths 'way demo.idle PageAComponent'
}

Test-Case 'tsrows keys: a field a subscribe or a Promise.resolve(x).then callback writes from its parameter holds what is handed on' {
    $tree = New-TsRowsWorkspace @{
        # Node resolution, so the fixture's own `rxjs` typings below resolve: the default for this target is Classic.
        'apps/shop/tsconfig.app.json' = '{"compilerOptions":{"target":"ES2022","moduleResolution":"node"},"include":["src/**/*.ts"]}'
        'node_modules/rxjs/package.json' = '{"name":"rxjs","version":"7.8.1","types":"index.d.ts"}'
        'node_modules/rxjs/index.d.ts' = "export interface Observer<T> { next: (value: T) => void; }`n" +
            "export type OperatorFunction<T, R> = (source: Observable<T>) => Observable<R>;`n" +
            "export declare class Observable<T> {`n  subscribe(observer?: Partial<Observer<T>> | ((value: T) => void)): void;`n  pipe<R>(...ops: OperatorFunction<any, any>[]): Observable<R>;`n}`n" +
            "export declare function of<T>(value: T): Observable<T>;`n" +
            "export declare function take<T>(n: number): OperatorFunction<T, T>;`n" +
            "export declare function map<T, R>(f: (v: T) => R): OperatorFunction<T, R>;`n"
        'apps/shop/src/translate.service.ts' = "import { Injectable } from '@angular/core';`n" +
            "@Injectable({ providedIn: 'root' })`n" +
            "export class TranslateService {`n  getTranslation(key: string): string {`n    return key;`n  }`n}`n"
        'apps/shop/src/svc.ts' = "import { Injectable } from '@angular/core';`n" +
            "import { Observable, of } from 'rxjs';`n" +
            "import { TranslateService } from './translate.service';`n" +
            "@Injectable({ providedIn: 'root' })`n" +
            "export class Svc {`n  constructor(private t: TranslateService) {}`n" +
            "  label(): string { return this.t.getTranslation('demo.resolved'); }`n" +
            "  label`$(): Observable<string> { return of(this.t.getTranslation('demo.emitted')); }`n" +
            "  next`$(): Observable<string> { return of(this.t.getTranslation('demo.next')); }`n" +
            "  piped`$(): Observable<string> { return of(this.t.getTranslation('demo.piped')); }`n" +
            "  mapped`$(): Observable<string> { return of(this.t.getTranslation('demo.mapped')); }`n}`n"
        'apps/shop/src/host.component.ts' = "import { Component, OnInit } from '@angular/core';`n" +
            "import { map, take } from 'rxjs';`n" +
            "import { Svc } from './svc';`n" +
            "@Component({ selector: 'demo-host', standalone: true, template: '{{ a }} {{ b }} {{ c }} {{ d }} {{ e }}' })`n" +
            "export class HostComponent implements OnInit {`n  a = '';`n  b = '';`n  c = '';`n  d = '';`n  e = '';`n  constructor(private svc: Svc) {}`n" +
            "  ngOnInit() {`n    Promise.resolve(this.svc.label()).then((x) => { this.a = x; });`n" +
            "    this.svc.label`$().subscribe((v) => { this.b = v; });`n" +
            "    this.svc.next`$().subscribe({ next: (v) => { this.c = v; } });`n" +
            "    this.svc.piped`$().pipe(take(1)).subscribe((v) => { this.d = v; });`n" +
            "    this.svc.mapped`$().pipe(map((x) => x.trim())).subscribe((v) => { this.e = v; });`n  }`n}`n"
        'apps/shop/src/assets/locales/en.json' = '{"shop":{"title":"Shop"},"demo":{"resolved":"a","emitted":"b","next":"c","piped":"d","mapped":"e"}}'
        'structuregate.ts.json' = '{"locales":["apps/shop/src/assets/locales"],"i18nCalls":["getTranslation"]}'
    }
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    $r = Invoke-TsRowsQ $made.Db $script:InstReach
    Assert-Line $r 'demo.resolved | field_binding | HostComponent'
    Assert-Line $r 'demo.emitted | field_binding | HostComponent'
    Assert-Line $r 'demo.next | field_binding | HostComponent'
    # `pipe(take(1))` hands the source's values on; `pipe(map(..))` makes values of its own
    Assert-Line $r 'demo.piped | field_binding | HostComponent'
    Assert-NoLine $r 'demo.mapped | field_binding | HostComponent'
}
}
