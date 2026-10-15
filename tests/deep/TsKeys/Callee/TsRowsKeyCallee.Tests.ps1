<#
    A key a field reaches through a call into a service (`rust/fbtcore/src/rows/ts/key/fields/key_follow.rs`,
    `key/fields/key_args.rs`): the callee's own branches ride along with the caller's arguments in place of its parameters,
    and a local arrow wrapping the translate call is entered even when its name is a configured `i18nCalls` method.
    Its helpers are `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '..\..\TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

$script:CalleeQ = "SELECT k.key || ' | ' || k.route || ' | ' || $(Get-TsItemFieldSql 'key_reach__always_values' 'k.rowid' 0 'dimension') || ' ' || " +
    "$(Get-TsItemFieldSql 'key_reach__always_values' 'k.rowid' 0 'op') || ' ' || $(Get-NodeItemListSql 'key_reach__always_values' 'k.rowid' 0 'values') AS reach FROM key_reach k WHERE k.key LIKE 'demo.%'"

Test-Case 'tsrows keys: a callee branch on an enum parameter restricts the key by the argument the caller hands' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/translate.service.ts' = "import { Injectable } from '@angular/core';`n" +
            "@Injectable({ providedIn: 'root' })`n" +
            "export class TranslateService {`n  getTranslation(key: string): string {`n    return key;`n  }`n}`n"
        'apps/shop/src/hues.ts' = "export enum HueIDs { Red = 1, Green = 2, Blue = 3, Grey = 4 }`n" +
            "export const warmHues = [HueIDs.Red, HueIDs.Green];`n" +
            "export const coolHues = [HueIDs.Blue];`n"
        'apps/shop/src/desc.service.ts' = "import { Injectable } from '@angular/core';`n" +
            "import { HueIDs, warmHues, coolHues } from './hues';`n" +
            "import { TranslateService } from './translate.service';`n" +
            "@Injectable({ providedIn: 'root' })`n" +
            "export class DescService {`n  constructor(private translate: TranslateService) {}`n" +
            "  description(kind: HueIDs, name: string): string {`n" +
            "    if (warmHues.indexOf(kind) !== -1) {`n      return this.home(name);`n" +
            "    } else if (coolHues.some((e) => e === kind)) {`n      return this.translate.getTranslation(``demo.hue.cool.`${name}``);`n    }`n" +
            "    return '-';`n  }`n" +
            "  private home(name: string): string { return this.translate.getTranslation(``demo.hue.warm.`${name}``); }`n}`n"
        'apps/shop/src/tip.component.ts' = "import { Component, Input } from '@angular/core';`n" +
            "import { HueIDs } from './hues';`n" +
            "import { DescService } from './desc.service';`n" +
            "@Component({ selector: 'demo-tip', standalone: true, template: '<span [innerHTML]=`"text`"></span>' })`n" +
            "export class TipComponent {`n  @Input() item!: { kind: HueIDs; name: string };`n" +
            "  constructor(private desc: DescService) {}`n" +
            "  get text(): string { return this.desc.description(this.item.kind, this.item.name); }`n}`n"
        'apps/shop/src/assets/locales/en.json' = '{"shop":{"title":"Shop"},"demo":{"hue":{"warm":{"a":"1"},"cool":{"a":"2"}}}}'
        'structuregate.ts.json' = '{"locales":["apps/shop/src/assets/locales"],"i18nCalls":["getTranslation"]}'
    }
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    $r = Invoke-TsRowsQ $made.Db $script:CalleeQ
    # the callee's `indexOf` test on its parameter, read with the argument the getter hands it
    Assert-Line $r 'demo.hue.warm.a | field_binding | item.kind in ["Green","Red"]'
    # an `else if` holds the first test negated and its own
    Assert-Line $r 'demo.hue.cool.a | field_binding | item.kind in ["Blue"]'
}

Test-Case 'tsrows keys: a local arrow named like an i18nCalls method is entered, not read as the translate call' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/translate.service.ts' = "import { Injectable } from '@angular/core';`n" +
            "@Injectable({ providedIn: 'root' })`n" +
            "export class TranslateService {`n  getTranslation(key: string): string {`n    return key;`n  }`n}`n"
        'apps/shop/src/desc.service.ts' = "import { Injectable } from '@angular/core';`n" +
            "import { TranslateService } from './translate.service';`n" +
            "export enum HueIDs { Alpha = 1, Beta = 2 }`n" +
            "@Injectable({ providedIn: 'root' })`n" +
            "export class DescService {`n  constructor(private translate: TranslateService) {}`n" +
            "  localArrow(kind: HueIDs): string {`n" +
            "    const t = (key: string) => this.translate.getTranslation(``demo.arrow.`${key}``);`n" +
            "    if (kind === HueIDs.Alpha) { return t('first'); }`n    return t('second');`n  }`n}`n"
        'apps/shop/src/host.component.ts' = "import { Component, Input, OnInit } from '@angular/core';`n" +
            "import { DescService, HueIDs } from './desc.service';`n" +
            "@Component({ selector: 'demo-host', standalone: true, template: '{{ c }}' })`n" +
            "export class HostComponent implements OnInit {`n  @Input() kind!: HueIDs;`n  c = '';`n" +
            "  constructor(private desc: DescService) {}`n" +
            "  ngOnInit() { this.c = this.desc.localArrow(this.kind); }`n}`n"
        'apps/shop/src/assets/locales/en.json' = '{"shop":{"title":"Shop"},"demo":{"arrow":{"first":"1","second":"2"}}}'
        # `t` is a configured translate method too, as a real tree's config lists it
        'structuregate.ts.json' = '{"locales":["apps/shop/src/assets/locales"],"i18nCalls":["getTranslation","t"]}'
    }
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    $r = Invoke-TsRowsQ $made.Db "SELECT k.key || ' | ' || k.route || ' | ' || $(Get-NodeRootListSql 'key_reach__components' 'k.rowid') AS reach FROM key_reach k WHERE k.key LIKE 'demo.%'"
    Assert-Line $r 'demo.arrow.first | field_binding | ["ng:'
    Assert-Line $r 'demo.arrow.second | field_binding | ["ng:'
}

Test-Case 'tsrows keys: a method the template calls with the loop item restricts the key by the callee branch (#47)' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/translate.service.ts' = "import { Injectable } from '@angular/core';`n" +
            "@Injectable({ providedIn: 'root' })`n" +
            "export class TranslateService {`n  getTranslation(key: string): string {`n    return key;`n  }`n}`n"
        'apps/shop/src/shapes.ts' = "export enum ShapeIDs { Round = 1, Square = 2, Oval = 3, Star = 4 }`n" +
            "export const roundShapes = [ShapeIDs.Round, ShapeIDs.Square];`n" +
            "export const ovalShapes = [ShapeIDs.Oval];`n"
        'apps/shop/src/desc.service.ts' = "import { Injectable } from '@angular/core';`n" +
            "import { ShapeIDs, roundShapes, ovalShapes } from './shapes';`n" +
            "import { TranslateService } from './translate.service';`n" +
            "@Injectable({ providedIn: 'root' })`n" +
            "export class DescService {`n  constructor(private translate: TranslateService) {}`n" +
            "  description(shape: ShapeIDs, name: string): string {`n" +
            "    if (roundShapes.indexOf(shape) !== -1) {`n      return this.home(name);`n" +
            "    } else if (ovalShapes.some((e) => e === shape)) {`n      return this.translate.getTranslation(``demo.shape.oval.`${name}``);`n    }`n" +
            "    return '-';`n  }`n" +
            "  private home(name: string): string { return this.translate.getTranslation(``demo.shape.round.`${name}``); }`n}`n"
        'apps/shop/src/table.component.ts' = "import { Component, Input } from '@angular/core';`n" +
            "import { ShapeIDs } from './shapes';`n" +
            "import { DescService } from './desc.service';`n" +
            "@Component({ selector: 'demo-table', standalone: true, template: '<div *ngFor=`"let row of rows`"><span [innerHTML]=`"text(row)`"></span></div>' })`n" +
            "export class TableComponent {`n  @Input() rows!: { shape: ShapeIDs; name: string }[];`n" +
            "  constructor(private desc: DescService) {}`n" +
            "  text(row: { shape: ShapeIDs; name: string }): string { return this.desc.description(row.shape, row.name); }`n}`n"
        'apps/shop/src/assets/locales/en.json' = '{"shop":{"title":"Shop"},"demo":{"shape":{"round":{"a":"1"},"oval":{"a":"2"}}}}'
        'structuregate.ts.json' = '{"locales":["apps/shop/src/assets/locales"],"i18nCalls":["getTranslation"]}'
    }
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    $r = Invoke-TsRowsQ $made.Db $script:CalleeQ
    # the issue's repro: the template calls `text(row)`, which hands the loop item on to the service
    Assert-Line $r 'demo.shape.round.a | field_binding | row.shape in ["Round","Square"]'
    Assert-Line $r 'demo.shape.oval.a | field_binding | row.shape in ["Oval"]'
}

Test-Case 'tsrows keys: a template-called method''s own parameters read as the template''s arguments' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/translate.service.ts' = "import { Injectable } from '@angular/core';`n" +
            "@Injectable({ providedIn: 'root' })`n" +
            "export class TranslateService {`n  getTranslation(key: string): string {`n    return key;`n  }`n}`n"
        'apps/shop/src/shapes.ts' = "export enum ShapeIDs { Round = 1, Square = 2, Oval = 3, Star = 4 }`n" +
            "export const roundShapes = [ShapeIDs.Round, ShapeIDs.Square];`n" +
            "export const ovalShapes = [ShapeIDs.Oval];`n"
        'apps/shop/src/desc.service.ts' = "import { Injectable } from '@angular/core';`n" +
            "import { ShapeIDs, roundShapes, ovalShapes } from './shapes';`n" +
            "import { TranslateService } from './translate.service';`n" +
            "@Injectable({ providedIn: 'root' })`n" +
            "export class DescService {`n  constructor(private translate: TranslateService) {}`n" +
            "  description(shape: ShapeIDs, name: string): string {`n" +
            "    if (roundShapes.indexOf(shape) !== -1) {`n      return this.home(name);`n" +
            "    } else if (ovalShapes.some((e) => e === shape)) {`n      return this.translate.getTranslation(``demo.shape.oval.`${name}``);`n    }`n" +
            "    return '-';`n  }`n" +
            "  private home(name: string): string { return this.translate.getTranslation(``demo.shape.round.`${name}``); }`n}`n"
        'apps/shop/src/table.component.ts' = "import { Component, Input } from '@angular/core';`n" +
            "import { ShapeIDs } from './shapes';`n" +
            "import { DescService } from './desc.service';`n" +
            "@Component({ selector: 'demo-table', standalone: true, template: '<div *ngFor=`"let row of rows`"><i [title]=`"shapeText(row.shape)`"></i><s [title]=`"label(row)`"></s></div>' })`n" +
            "export class TableComponent {`n  @Input() rows!: { shape: ShapeIDs; name: string }[];`n" +
            "  constructor(private desc: DescService) {}`n" +
            "  shapeText(k: ShapeIDs): string { return this.desc.description(k, 'b'); }`n" +
            "  label(r: { shape: ShapeIDs; name: string }): string { return this.desc.description(r.shape, 'c'); }`n}`n"
        'apps/shop/src/assets/locales/en.json' = '{"demo":{"shape":{"oval":{"b":"1","c":"2"}}}}'
        'structuregate.ts.json' = '{"locales":["apps/shop/src/assets/locales"],"i18nCalls":["getTranslation"]}'
    }
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    $r = Invoke-TsRowsQ $made.Db $script:CalleeQ
    # `k` and `r` are the method's names: the condition reads the template's `row.shape`
    Assert-Line $r 'demo.shape.oval.b | field_binding | row.shape in ["Oval"]'
    Assert-Line $r 'demo.shape.oval.c | field_binding | row.shape in ["Oval"]'
}

Test-Case 'tsrows keys: an argument the caller holds in a const local is read in the caller' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/translate.service.ts' = "import { Injectable } from '@angular/core';`n" +
            "@Injectable({ providedIn: 'root' })`n" +
            "export class TranslateService {`n  getTranslation(key: string): string {`n    return key;`n  }`n}`n"
        'apps/shop/src/shapes.ts' = "export enum ShapeIDs { Round = 1, Square = 2, Oval = 3, Star = 4 }`n" +
            "export const ovalShapes = [ShapeIDs.Oval];`n"
        'apps/shop/src/desc.service.ts' = "import { Injectable } from '@angular/core';`n" +
            "import { ShapeIDs, ovalShapes } from './shapes';`n" +
            "import { TranslateService } from './translate.service';`n" +
            "@Injectable({ providedIn: 'root' })`n" +
            "export class DescService {`n  constructor(private translate: TranslateService) {}`n" +
            "  description(shape: ShapeIDs, name: string): string {`n" +
            "    if (ovalShapes.indexOf(shape) !== -1) {`n      return this.translate.getTranslation(``demo.shape.oval.`${name}``);`n    }`n" +
            "    return '-';`n  }`n}`n"
        'apps/shop/src/tip.component.ts' = "import { Component, Input } from '@angular/core';`n" +
            "import { ShapeIDs } from './shapes';`n" +
            "import { DescService } from './desc.service';`n" +
            "@Component({ selector: 'demo-tip', standalone: true, template: '<span [innerHTML]=`"text`"></span>' })`n" +
            "export class TipComponent {`n  @Input() item!: { shape: ShapeIDs; name: string };`n" +
            "  constructor(private desc: DescService) {}`n" +
            "  get text(): string {`n    const picked = this.item.shape;`n    return this.desc.description(picked, 'a');`n  }`n}`n"
        'apps/shop/src/assets/locales/en.json' = '{"demo":{"shape":{"oval":{"a":"1"}}}}'
        'structuregate.ts.json' = '{"locales":["apps/shop/src/assets/locales"],"i18nCalls":["getTranslation"]}'
    }
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    $r = Invoke-TsRowsQ $made.Db $script:CalleeQ
    # `picked` is the getter's own `const`: the condition reads the chain it holds
    Assert-Line $r 'demo.shape.oval.a | field_binding | item.shape in ["Oval"]'
}

Test-Case 'tsrows keys: a parameter a callee hands on to another method carries the caller''s string' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/translate.service.ts' = "import { Injectable } from '@angular/core';`n" +
            "@Injectable({ providedIn: 'root' })`n" +
            "export class TranslateService {`n  getTranslation(key: string): string {`n    return key;`n  }`n}`n"
        'apps/shop/src/desc.service.ts' = "import { Injectable } from '@angular/core';`n" +
            "import { TranslateService } from './translate.service';`n" +
            "@Injectable({ providedIn: 'root' })`n" +
            "export class DescService {`n  constructor(private translate: TranslateService) {}`n" +
            "  description(name: string): string { return this.round(name); }`n" +
            "  private round(name: string): string { return this.translate.getTranslation(``demo.shape.round.`${name}``); }`n}`n"
        'apps/shop/src/tip.component.ts' = "import { Component } from '@angular/core';`n" +
            "import { DescService } from './desc.service';`n" +
            "@Component({ selector: 'demo-tip', standalone: true, template: '<span [innerHTML]=`"text`"></span>' })`n" +
            "export class TipComponent {`n  constructor(private desc: DescService) {}`n" +
            "  get text(): string { return this.desc.description('a'); }`n}`n"
        'apps/shop/src/assets/locales/en.json' = '{"demo":{"shape":{"round":{"a":"1","b":"2"}}}}'
        'structuregate.ts.json' = '{"locales":["apps/shop/src/assets/locales"],"i18nCalls":["getTranslation"]}'
    }
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    $r = Invoke-TsRowsQ $made.Db "SELECT key || ' | ' || coalesce(route, '') AS reach FROM key_reach WHERE key LIKE 'demo.%'"
    # `round(name)` inside `description` gets the `'a'` its caller handed, not an open hole spelling every key
    Assert-Line $r 'demo.shape.round.a | field_binding'
    Assert-NoLine $r 'demo.shape.round.b | field_binding'
}
# A CHILD'S INPUT IS THE PARENT'S EXPRESSION on a way through the edge that binds it (`key/key_scopes.rs`).
$script:BoundQ = "SELECT k.key || ' | ' || $(Get-TsFieldSql 'key_reach__always_values' 'v' 'dimension') || ' ' || $(Get-TsFieldSql 'key_reach__always_values' 'v' 'op') || ' ' || " +
    "$(Get-NodeListSql 'key_reach__always_values' 'v' 'values') AS r FROM key_reach k JOIN key_reach__always_values v ON v.owner = '#' || k.rowid AND v.parent = 0 WHERE k.key LIKE 'demo.kind.%'"

function New-BoundKindTree([string]$ChildBody) {
    $tip = @'
import { Component, Input } from '@angular/core';
import { DemoIDs, DescService } from './svc';
@Component({ selector: 'demo-tip', standalone: true, template: '<span [innerHTML]="text"></span>' })
export class TipComponent {
  @Input() item!: { kind: DemoIDs; name: string };
  constructor(private desc: DescService) {}
__BODY__
  get text(): string { return this.desc.description(this.item.kind, this.item.name); }
}
'@
    return New-TsRowsWorkspace @{
        'apps/shop/src/svc.ts' = @'
import { Injectable } from '@angular/core';
export enum DemoIDs { Home = 1, Flat = 2, Life = 3, Car = 4 }
export const homeKinds = [DemoIDs.Home, DemoIDs.Flat];
export const lifeKinds = [DemoIDs.Life];
@Injectable({ providedIn: 'root' })
export class TranslateService { getTranslation(key: string): string { return key; } }
@Injectable({ providedIn: 'root' })
export class DescService {
  constructor(private translate: TranslateService) {}
  description(kind: DemoIDs, name: string): string {
    if (homeKinds.indexOf(kind) !== -1) {
      return this.home(name);
    } else if (lifeKinds.some((e) => e === kind)) {
      return this.translate.getTranslation(`demo.kind.life.${name}`);
    }
    return '-';
  }
  private home(name: string): string { return this.translate.getTranslation(`demo.kind.home.${name}`); }
}
@Injectable({ providedIn: 'root' })
export class CtxService { context: { kind: DemoIDs; name: string } = { kind: DemoIDs.Home, name: 'a' }; }
'@
        'apps/shop/src/tip.component.ts' = $tip.Replace('__BODY__', $ChildBody)
        'apps/shop/src/other.component.ts' = @'
import { Component } from '@angular/core';
import { DescService, CtxService } from './svc';
@Component({ selector: 'demo-other', standalone: true, template: '<span [innerHTML]="text"></span>' })
export class OtherComponent {
  constructor(private desc: DescService, private ctx: CtxService) {}
  get text(): string { return this.desc.description(this.ctx.context.kind, this.ctx.context.name); }
}
'@
        'apps/shop/src/root.component.ts' = @'
import { Component } from '@angular/core';
import { NgIf } from '@angular/common';
import { DemoIDs } from './svc';
import { TipComponent } from './tip.component';
import { OtherComponent } from './other.component';
@Component({
  selector: 'demo-root', standalone: true, imports: [NgIf, TipComponent, OtherComponent],
  template: '<div *ngIf="vm.kind !== kinds.Car"><demo-tip [item]="vm"></demo-tip></div><demo-other></demo-other>'
})
export class RootComponent {
  kinds = DemoIDs;
  vm: { kind: DemoIDs; name: string } = { kind: DemoIDs.Home, name: 'a' };
}
'@
        'apps/shop/src/assets/locales/en.json' = '{"demo":{"kind":{"home":{"a":"1"},"life":{"a":"2"}}}}'
        'structuregate.ts.json' = '{"locales":["apps/shop/src/assets/locales"],"i18nCalls":["getTranslation"]}'
    }
}

Test-Case 'tsrows keys: a child input bound to a parent read chain is that parent expression, so its gates intersect with the parent own' {
    $made = New-TsRowsDb (New-BoundKindTree '')
    Assert-Exit $made.Result 0
    $r = Invoke-Gate --map-query $made.Db --width 0 --sql $script:BoundQ
    Assert-Exit $r 0
    # `[item]="vm"` makes the child's `item.kind` the parent's `vm.kind`: {Home,Flat,Life} meets {Home,Flat}, never a union that leaves only Car out
    Assert-Line $r 'demo.kind.home.a | ctx.context.kind | vm.kind in ["Flat","Home"]'
    Assert-Line $r 'demo.kind.life.a | ctx.context.kind | vm.kind in ["Life"]'
    Assert-NoLine $r 'item.kind'
}

Test-Case 'tsrows keys: a child that writes its own input is not renamed to the parent expression' {
    $made = New-TsRowsDb (New-BoundKindTree "  ngOnInit(): void { this.item = { ...this.item }; }")
    Assert-Exit $made.Result 0
    $r = Invoke-Gate --map-query $made.Db --width 0 --sql $script:BoundQ
    Assert-Exit $r 0
    # the child's `item` is no longer what the parent bound, so the dimension keeps its own text
    Assert-NoLine $r '| vm.kind in'
    # the callee's parameter is one dimension on every way (#48), read as the caller spells it: the child's own `item.kind`
    Assert-Line $r 'demo.kind.home.a | ctx.context.kind | item.kind in ["Flat","Home"]'
}

$script:ArgQ = "SELECT k.key || ' | ' || $(Get-TsFieldSql 'key_reach__always_values' 'v' 'dimension') || ' ' || $(Get-TsFieldSql 'key_reach__always_values' 'v' 'op') || ' ' || $(Get-NodeListSql 'key_reach__always_values' 'v' 'values') AS r " +
    "FROM key_reach k JOIN key_reach__always_values v ON v.owner = '#' || k.rowid AND v.parent = 0 WHERE k.key LIKE 'demo.%'"

Test-Case 'tsrows keys: a constant bound to an input through the parent alias is no variable to rename into (#49)' {
    $tree = New-TsRowsWorkspace @{
        'structuregate.ts.json' = '{"locales":["apps/shop/src/assets/locales"],"i18nCalls":["getTranslation"]}'
        'apps/shop/src/item.component.ts' = @'
import { Component, Input } from '@angular/core';
import { DemoIDs } from './kinds';
import { PriceKeyPipe } from './price.pipe';
@Component({ selector: 'demo-item', standalone: true, imports: [PriceKeyPipe], template: '<span [title]="sum | priceKey : kind"></span>' })
export class ItemComponent {
  @Input() sum!: number;
  @Input() kind!: DemoIDs;
}
'@
        'apps/shop/src/items.component.ts' = @'
import { Component } from '@angular/core';
import { NgIf } from '@angular/common';
import { DemoIDs } from './kinds';
import { ItemComponent } from './item.component';
@Component({ selector: 'demo-items', standalone: true, imports: [NgIf, ItemComponent], template: '<demo-item *ngIf="vm.kind !== DemoIDs.Flat" [sum]="1" [kind]="DemoIDs.Home"></demo-item><demo-item *ngIf="vm.kind !== DemoIDs.Home" [sum]="2" [kind]="DemoIDs.Flat"></demo-item>' })
export class ItemsComponent {
  DemoIDs = DemoIDs;
  vm: { kind: DemoIDs } = { kind: DemoIDs.Life };
}
'@
        'apps/shop/src/kinds.ts' = @'
export enum DemoIDs { Home = 1, Flat = 2, Life = 3 }
'@
        'apps/shop/src/price.pipe.ts' = @'
import { Pipe, PipeTransform } from '@angular/core';
import { DemoIDs } from './kinds';
@Pipe({ name: 'priceKey', standalone: true })
export class PriceKeyPipe implements PipeTransform {
  transform(value: number, kind: DemoIDs): string {
    return kind === DemoIDs.Home ? 'demo.price.homeOnly' : 'demo.price.other';
  }
}
'@
        'apps/shop/src/assets/locales/en.json' = '{"demo":{"price":{"homeOnly":"Home","other":"Other"}}}'
    }
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    $r = Invoke-Gate --map-query $made.Db --width 0 --sql $script:ArgQ
    Assert-Exit $r 0
    # `[kind]="DemoIDs.Home"` renamed into the child read `DemoIDs.Home in [Home]` on one way and `DemoIDs.Flat` on the other: no common spelling, the fold widened to nothing
    Assert-Line $r 'demo.price.homeOnly | kind in ["Home"]'
    Assert-Line $r 'demo.price.other | kind not_in ["Home"]'
    Assert-NoLine $r 'DemoIDs.'
}

Test-Case 'tsrows keys: an argument spelled differently on each way is one dimension of the callee parameter (#48)' {
    $tree = New-TsRowsWorkspace @{
        'structuregate.ts.json' = '{"locales":["apps/shop/src/assets/locales"],"i18nCalls":["getTranslation"]}'
        'apps/shop/src/child.component.ts' = @'
import { Component } from '@angular/core';
import { DescService } from './svc';
import { CtxService } from './ctx';
@Component({ selector: 'demo-child', standalone: true, template: '<span [innerHTML]="text"></span>' })
export class ChildComponent {
  constructor(private desc: DescService, private ctx: CtxService) {}
  get text(): string { return this.desc.description(this.ctx.context.kind, this.ctx.context.name); }
}
'@
        'apps/shop/src/ctx.ts' = @'
import { Injectable } from '@angular/core';
import { DemoIDs } from './svc';
@Injectable({ providedIn: 'root' })
export class CtxService { context: { kind: DemoIDs; name: string } = { kind: DemoIDs.Home, name: 'a' }; }
'@
        'apps/shop/src/root.component.ts' = @'
import { Component, OnInit } from '@angular/core';
import { NgIf } from '@angular/common';
import { DemoIDs } from './svc';
import { CtxService } from './ctx';
import { ChildComponent } from './child.component';
@Component({ selector: 'demo-root', standalone: true, imports: [NgIf, ChildComponent], template: '<demo-child *ngIf="!isOther"></demo-child>' })
export class RootComponent implements OnInit {
  vm: { kind: DemoIDs; name: string } = { kind: DemoIDs.Home, name: 'a' };
  isOther = false;
  constructor(private ctx: CtxService) {}
  ngOnInit() {
    this.ctx.context.kind = this.vm.kind;
    this.isOther = this.vm.kind === DemoIDs.Other;
  }
}
'@
        'apps/shop/src/rootb.component.ts' = @'
import { Component } from '@angular/core';
import { ChildComponent } from './child.component';
import { TipComponent } from './tip.component';
@Component({ selector: 'demo-root-b', standalone: true, imports: [ChildComponent, TipComponent], template: '<demo-child></demo-child><demo-tip [item]="vm"></demo-tip>' })
export class RootBComponent {
  vm: { kind: number; name: string } = { kind: 1, name: 'a' };
}
'@
        'apps/shop/src/svc.ts' = @'
import { Injectable } from '@angular/core';
export enum DemoIDs { Home = 1, Flat = 2, Life = 3, Car = 4, Other = 5 }
export const homeKinds = [DemoIDs.Home, DemoIDs.Flat];
export const lifeKinds = [DemoIDs.Life];
@Injectable({ providedIn: 'root' })
export class TranslateService { getTranslation(key: string): string { return key; } }
@Injectable({ providedIn: 'root' })
export class DescService {
  constructor(private translate: TranslateService) {}
  description(kind: DemoIDs, name: string): string {
    if (homeKinds.indexOf(kind) !== -1) {
      return this.translate.getTranslation(`demo.kind.home.${name}`);
    } else if (lifeKinds.some((e) => e === kind)) {
      return this.translate.getTranslation(`demo.kind.life.${name}`);
    }
    return '-';
  }
}
'@
        'apps/shop/src/tip.component.ts' = @'
import { Component, Input } from '@angular/core';
import { DemoIDs, DescService } from './svc';
@Component({ selector: 'demo-tip', standalone: true, template: '<span [innerHTML]="text"></span>' })
export class TipComponent {
  @Input() item!: { kind: DemoIDs; name: string };
  constructor(private desc: DescService) {}
  get text(): string { return this.desc.description(this.item.kind, this.item.name); }
}
'@
        'apps/shop/src/assets/locales/en.json' = '{"demo":{"kind":{"home":{"a":"A"},"life":{"a":"B"}}}}'
    }
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    $r = Invoke-Gate --map-query $made.Db --width 0 --sql $script:ArgQ
    Assert-Exit $r 0
    # the callee's branch restricts its PARAMETER on every way, so the caller spellings are one dimension and the gate's `vm.kind not_in` does not leak in
    Assert-Line $r 'demo.kind.home.a | ctx.context.kind | vm.kind in ["Flat","Home"]'
    Assert-Line $r 'demo.kind.life.a | ctx.context.kind | vm.kind in ["Life"]'
    Assert-NoLine $r 'not_in ["Other"]'
}
}
