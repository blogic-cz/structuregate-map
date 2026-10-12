<#
    A key a field reaches through a call into a service (`rust/fbtcore/src/rows/ts/key/fields/key_follow.rs`,
    `key/fields/key_args.rs`): the callee's own branches ride along with the caller's arguments in place of its parameters,
    and a local arrow wrapping the translate call is entered even when its name is a configured `i18nCalls` method.
    Its helpers are `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '..\TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

$script:CalleeQ = "SELECT k.key || ' | ' || k.route || ' | ' || json_extract(k.always_values, '$[0].dimension') || ' ' || " +
    "json_extract(k.always_values, '$[0].op') || ' ' || json_extract(k.always_values, '$[0].values') AS reach FROM key_reach k WHERE k.key LIKE 'demo.%'"

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
    $r = Invoke-TsRowsQ $made.Db "SELECT k.key || ' | ' || k.route || ' | ' || k.components AS reach FROM key_reach k WHERE k.key LIKE 'demo.%'"
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
    # `picked` is the getter's own `const`, read in the getter: a plain local is its own dimension, as a gate reads one
    # (only a decision or an arrow is inlined, `gate/ts/gate_tsrows.rs`); without the caller no condition held at all
    Assert-Line $r 'demo.shape.oval.a | field_binding | picked in ["Oval"]'
}
}
