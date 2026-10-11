<#
    A component field holding a call of a FREE FUNCTION - a `function` declaration or an exported `const` arrow of a
    file that is no class - reads the keys that function returns, as a call of a method does
    (`rust/fbtcore/src/rows/ts/key/fields/key_follow.rs`, `Returned::call`): a string argument is spelled into the
    hole it names, a service method returning the call hands the keys on. Its helpers are `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '..\TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

Test-Case 'tsrows keys: a call of a free function hands the keys it returns to the field holding it' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/translate.service.ts' = "import { Injectable } from '@angular/core';`n" +
            "@Injectable({ providedIn: 'root' })`n" +
            "export class TranslateService {`n  getTranslation(key: string): string {`n    return key;`n  }`n}`n"
        'apps/shop/src/fn.ts' = "import { TranslateService } from './translate.service';`n" +
            "export function plain(t: TranslateService) { return t.getTranslation('demo.plain.a'); }`n" +
            "export const label = (n: string) => ``demo.fn.`${n}.label``;`n"
        'apps/shop/src/wrap.service.ts' = "import { Injectable } from '@angular/core';`n" +
            "import { TranslateService } from './translate.service';`n" +
            "import { plain2 } from './fn2';`n" +
            "@Injectable({ providedIn: 'root' })`n" +
            "export class WrapService {`n" +
            "  constructor(private t: TranslateService) {}`n" +
            "  viaService() { return plain2(this.t); }`n" +
            "}`n"
        'apps/shop/src/fn2.ts' = "import { TranslateService } from './translate.service';`n" +
            "export function plain2(t: TranslateService) { return t.getTranslation('demo.svc.a'); }`n"
        'apps/shop/src/host.component.ts' = "import { Component, OnInit } from '@angular/core';`n" +
            "import { TranslateService } from './translate.service';`n" +
            "import { WrapService } from './wrap.service';`n" +
            "import { plain, label } from './fn';`n" +
            "@Component({ selector: 'demo-host', standalone: true, template: '{{ a }} {{ b }} {{ s }}' })`n" +
            "export class HostComponent implements OnInit {`n" +
            "  a!: string;`n  b!: string;`n  s!: string;`n" +
            "  constructor(private t: TranslateService, private w: WrapService) {}`n" +
            "  ngOnInit() {`n    this.a = plain(this.t);`n    this.b = label('alpha');`n" +
            "    this.s = this.w.viaService();`n  }`n" +
            "}`n"
        'apps/shop/src/assets/locales/en.json' = '{"shop":{"title":"Shop"},"demo":{"plain":{"a":"a"},"svc":{"a":"b"},' +
            '"fn":{"alpha":{"label":"c"},"beta":{"label":"d"},"gamma":{"label":"e"}}}}'
        'structuregate.ts.json' = '{"locales":["apps/shop/src/assets/locales"],"i18nCalls":["getTranslation"]}'
    }
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    $r = Invoke-TsRowsQ $made.Db ("SELECT k.key || ' | ' || k.route || ' | ' || c.name AS reach FROM key_reach k, " +
        "json_each(k.components) j JOIN components c ON c.id = j.value WHERE k.key LIKE 'demo.%'")
    Assert-Line $r 'demo.plain.a | field_binding | HostComponent'
    Assert-Line $r 'demo.svc.a | field_binding | HostComponent'
    Assert-Line $r 'demo.fn.alpha.label | field_binding | HostComponent'
    # the arrow's other spelling was never handed
    Assert-NoLine $r 'demo.fn.gamma.label | field_binding | HostComponent'
}
}

if ($script:TsRowsModules -and $script:TsRowsPython) {

Test-Case 'tsrows keys: a list a free function returns is read per element where an ngFor reads the property' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/ids.ts' = "export enum Kind { Alpha = 1, Beta = 2 }`n" +
            "export interface Box { kind: Kind; label: string; tip: string; note: string }`n"
        'apps/shop/src/boxes.ts' = "import { Box, Kind } from './ids';`n" +
            "export const getBoxes = (kind: Kind, on: boolean): Box[] => {`n" +
            "  const label = (name: string) => ``demo.box.`${name}.label``;`n" +
            "  const tip = (name: string) => on ? ``demo.box.tips.`${name}`` : '';`n" +
            "  const boxes: Box[] = [`n" +
            "    { kind: Kind.Alpha, label: label('alpha'), tip: tip('alphaTip'), note: label('gamma') },`n" +
            "    { kind: Kind.Beta, label: label('beta'), tip: tip('betaTip'), note: label('delta') },`n" +
            "  ];`n" +
            "  return boxes.filter((x) => x.kind !== kind);`n};`n"
        'apps/shop/src/host.component.ts' = "import { Component, OnInit } from '@angular/core';`n" +
            "import { Box, Kind } from './ids';`n" +
            "import { getBoxes } from './boxes';`n" +
            "@Component({ selector: 'demo-host', standalone: true, template: '<li *ngFor=\`"let b of boxes\`">{{ b.label }} {{ b.tip }}</li>' })`n" +
            "export class HostComponent implements OnInit {`n" +
            "  kind = Kind.Alpha;`n  boxes: Box[] = [];`n" +
            "  ngOnInit() {`n    this.boxes = getBoxes(this.kind, true);`n  }`n}`n"
        'apps/shop/src/assets/locales/en.json' = '{"shop":{"title":"Shop"},"demo":{"box":{"alpha":{"label":"a"},"beta":{"label":"b"},' +
            '"gamma":{"label":"g"},"delta":{"label":"d"},"tips":{"alphaTip":"x","betaTip":"y"}}}}'
        'structuregate.ts.json' = '{"locales":["apps/shop/src/assets/locales"]}'
    }
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    $r = Invoke-TsRowsQ $made.Db ("SELECT k.key || ' | ' || k.route || ' | ' || c.name AS reach FROM key_reach k, " +
        "json_each(k.components) j JOIN components c ON c.id = j.value WHERE k.key LIKE 'demo.%'")
    Assert-Line $r 'demo.box.alpha.label | field_binding | HostComponent'
    Assert-Line $r 'demo.box.beta.label | field_binding | HostComponent'
    Assert-Line $r 'demo.box.tips.alphaTip | field_binding | HostComponent'
    Assert-Line $r 'demo.box.tips.betaTip | field_binding | HostComponent'
    # `note` is read by no template
    Assert-NoLine $r 'demo.box.gamma.label | field_binding | HostComponent'
    Assert-NoLine $r 'demo.box.delta.label | field_binding | HostComponent'
}
}

if ($script:TsRowsModules -and $script:TsRowsPython) {

Test-Case 'tsrows keys: a local arrow spells its hole by its calls, a ternary of strings by each arm' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/translate.service.ts' = "import { Injectable } from '@angular/core';`n" +
            "@Injectable({ providedIn: 'root' })`n" +
            "export class TranslateService {`n  getTranslation(key: string): string {`n    return key;`n  }`n}`n"
        'apps/shop/src/host.component.ts' = "import { Component, Input, OnInit } from '@angular/core';`n" +
            "import { TranslateService } from './translate.service';`n" +
            "@Component({ selector: 'demo-host', standalone: true, template: '{{ a }} {{ b }}' })`n" +
            "export class HostComponent implements OnInit {`n" +
            "  @Input() eco = false;`n  a!: string;`n  b!: string;`n" +
            "  constructor(private ts: TranslateService) {}`n" +
            "  ngOnInit() {`n    this.a = this.plain();`n    this.b = this.tern();`n  }`n" +
            "  plain(): string { const k = (n: string) => this.ts.getTranslation(``p.`${n}``); return k('lit'); }`n" +
            "  tern(): string { const k = (n: string) => this.ts.getTranslation(``q.`${n}``); return k(this.eco ? 'x' : 'y'); }`n" +
            "}`n"
        'apps/shop/src/assets/locales/en.json' = '{"shop":{"title":"Shop"},"p":{"lit":"1","other":"2","more":"3"},' +
            '"q":{"x":"1","y":"2","z":"3","w":"4"}}'
        'structuregate.ts.json' = '{"locales":["apps/shop/src/assets/locales"],"i18nCalls":["getTranslation"]}'
    }
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    $r = Invoke-TsRowsQ $made.Db ("SELECT k.key || ' | ' || k.route || ' | ' || c.name AS reach FROM key_reach k, " +
        "json_each(k.components) j JOIN components c ON c.id = j.value WHERE k.key LIKE 'p.%' OR k.key LIKE 'q.%'")
    Assert-Line $r 'p.lit | field_binding | HostComponent'
    Assert-Line $r 'q.x | field_binding | HostComponent'
    Assert-Line $r 'q.y | field_binding | HostComponent'
    # the arrow's own template is no wildcard: its calls spell it, and no call spells these
    Assert-NoLine $r 'p.other |'
    Assert-NoLine $r 'p.more |'
    Assert-NoLine $r 'q.z |'
    Assert-NoLine $r 'q.w |'
}
}

if ($script:TsRowsModules -and $script:TsRowsPython) {

Test-Case 'tsrows keys: a local arrow handed a string enum member spells its value, one handed a variable stays a wildcard' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/ids.ts' = "export enum Codes { IsAlpha = 'IsAlpha', IsBeta = 'IsBeta', IsGamma = 'IsGamma' }`n"
        'apps/shop/src/host.component.ts' = "import { Component, OnInit } from '@angular/core';`n" +
            "import { Codes } from './ids';`n" +
            "@Component({ selector: 'demo-host', standalone: true, template: '<p>x</p>' })`n" +
            "export class HostComponent implements OnInit {`n" +
            "  modules: any[] = [];`n  code = Codes.IsGamma;`n" +
            "  ngOnInit() {`n    this.initModules();`n    this.initOthers();`n  }`n" +
            "  private initModules() {`n" +
            "    const getLabel = (k: string) => ``demo.mod.`${k}``;`n" +
            "    this.modules.push({ label: getLabel(Codes.IsAlpha), inner: [{ label: getLabel(Codes.IsBeta) }] });`n  }`n" +
            "  private initOthers() {`n" +
            "    const pick = (k: string) => ``demo.var.`${k}``;`n" +
            "    this.modules.push({ label: pick(this.code) });`n  }`n" +
            "}`n"
        'apps/shop/src/assets/locales/en.json' = '{"shop":{"title":"Shop"},"demo":{"mod":{"IsAlpha":"a","IsBeta":"b","IsGamma":"c"},' +
            '"var":{"one":"1","two":"2"}}}'
        'structuregate.ts.json' = '{"locales":["apps/shop/src/assets/locales"]}'
    }
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    $r = Invoke-TsRowsQ $made.Db ("SELECT k.key || ' | ' || k.route || ' | ' || c.name AS reach FROM key_reach k, " +
        "json_each(k.components) j JOIN components c ON c.id = j.value WHERE k.key LIKE 'demo.%'")
    # pushed into a list no template reads, so only the arrow's own site reaches the host.
    # every call spells its hole, by the enum member's value
    Assert-Line $r 'demo.mod.IsAlpha | literal | HostComponent'
    Assert-Line $r 'demo.mod.IsBeta | literal | HostComponent'
    Assert-NoLine $r 'demo.mod.IsGamma |'
    # a variable spells nothing: the arrow's own template stays a wildcard, and the keys stay referenced
    Assert-Line $r 'demo.var.one | literal | HostComponent'
    Assert-Line $r 'demo.var.two | literal | HostComponent'
}
}
