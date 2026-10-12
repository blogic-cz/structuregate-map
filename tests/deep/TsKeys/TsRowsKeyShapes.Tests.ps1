<#
    A key reaches the template through shapes the field readers once skipped (`key/fields/key_follow.rs`,
    `Returned::through`, `key/key_lists.rs`): a constant object indexed by a variable, a list a method fills with
    `.push(..)` and returns (or a component's own field list it pushes to), a template literal's hole, a local
    holding the call and a string method chained on it. A constant never indexed and a list nobody returns stay
    unlinked. Its helpers are `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '..\TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

function New-KeyShapesTree {
    return New-TsRowsWorkspace @{
        'apps/shop/src/ids.ts' = "export enum Kind { Alpha = 1, Beta = 2 }`n"
        'apps/shop/src/map.ts' = "import { Kind } from './ids';`n" +
            "export const KIND_KEYS = { [Kind.Alpha]: 'demo.map.a', [Kind.Beta]: 'demo.map.b' };`n" +
            "export const COMP_KEYS = { [Kind.Alpha]: 'demo.map.c', [Kind.Beta]: 'demo.map.d' };`n" +
            "export const UNUSED_KEYS = { x: 'demo.map.unused' };`n"
        'apps/shop/src/translate.service.ts' = "import { Injectable } from '@angular/core';`n" +
            "@Injectable({ providedIn: 'root' })`n" +
            "export class TranslateService {`n  getTranslation(key: string): string {`n    return key;`n  }`n}`n"
        'apps/shop/src/shapes.service.ts' = "import { Injectable } from '@angular/core';`n" +
            "import { Kind } from './ids';`n" +
            "import { KIND_KEYS } from './map';`n" +
            "import { TranslateService } from './translate.service';`n" +
            "@Injectable({ providedIn: 'root' })`n" +
            "export class ShapesService {`n" +
            "  constructor(private t: TranslateService) {}`n" +
            "  dyn(kind: Kind): string { return this.t.getTranslation(KIND_KEYS[kind]); }`n" +
            "  labels(flag: boolean): string[] {`n    const out: string[] = [];`n" +
            "    out.push(this.t.getTranslation('demo.push.a'));`n" +
            "    if (flag) { out.push(this.t.getTranslation('demo.push.b')); }`n    return out;`n  }`n" +
            "  unused(): string[] {`n    const out: string[] = [];`n" +
            "    out.push(this.t.getTranslation('demo.push.unused'));`n    return ['-'];`n  }`n" +
            "  hole(): string { return ``" + '${this.t.getTranslation(' + "'demo.hole.a')}!``; }`n" +
            "  localHole(n: number): string {`n    const lbl = this.t.getTranslation('demo.hole.b');`n" +
            "    return ``demo.n`${n} (`${lbl})``;`n  }`n" +
            "  localConcat(): string {`n    const lbl = this.t.getTranslation('demo.hole.d');`n    return 'x ' + lbl;`n  }`n" +
            "  chained(): string { return this.t.getTranslation('demo.hole.c').replace('x', 'y'); }`n" +
            "}`n"
        'apps/shop/src/host.component.ts' = "import { Component, Input, OnInit } from '@angular/core';`n" +
            "import { NgFor } from '@angular/common';`n" +
            "import { Kind } from './ids';`n" +
            "import { COMP_KEYS } from './map';`n" +
            "import { ShapesService } from './shapes.service';`n" +
            "import { TranslateService } from './translate.service';`n" +
            "@Component({ selector: 'demo-host', standalone: true, imports: [NgFor], template: '{{ a }} {{ b }} {{ c }} {{ d }} {{ e }} {{ f }} {{ g }} {{ joined.join(`", `") }} <li *ngFor=`"let l of lines`">{{ l }}</li> <li *ngFor=`"let i of items`">{{ i.label }}</li>' })`n" +
            "export class HostComponent implements OnInit {`n" +
            "  @Input() kind: Kind = Kind.Alpha;`n" +
            "  a = ''; b = ''; c = ''; d = ''; e = ''; f = ''; g = '';`n" +
            "  lines: string[] = []; joined: string[] = [];`n" +
            "  items: { label: string }[] = [];`n" +
            "  constructor(private svc: ShapesService, private t: TranslateService) {}`n" +
            "  ngOnInit() {`n" +
            "    this.a = this.t.getTranslation(COMP_KEYS[this.kind]);`n" +
            "    this.b = this.svc.dyn(this.kind);`n" +
            "    this.c = this.svc.hole();`n" +
            "    this.d = this.svc.localHole(1);`n" +
            "    this.e = this.svc.localConcat();`n" +
            "    this.f = this.svc.chained();`n" +
            "    this.lines = this.svc.labels(true);`n" +
            "    this.joined = this.svc.labels(false);`n" +
            "    this.items.push({ label: this.t.getTranslation('demo.push.c') });`n" +
            "    if (this.kind === Kind.Beta) { this.items.push({ label: this.t.getTranslation('demo.push.e') }); }`n" +
            "  }`n" +
            "}`n"
        'apps/shop/src/assets/locales/en.json' = '{"shop":{"title":"Shop"},"demo":{"map":{"a":"1","b":"2","c":"5","d":"6","unused":"3"},' +
            '"push":{"a":"1","b":"2","c":"3","e":"5","unused":"4"},"hole":{"a":"1","b":"2","c":"3","d":"4"},"n":{"1":"x"}}}'
        'structuregate.ts.json' = '{"locales":["apps/shop/src/assets/locales"],"i18nCalls":["getTranslation"]}'
    }
}

Test-Case 'tsrows keys: a constant object indexed by a variable, a pushed list and a translate call in a hole reach the field' {
    $tree = New-KeyShapesTree
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    $r = Invoke-TsRowsQ $made.Db ("SELECT k.key || ' | ' || k.route || ' | ' || c.name AS reach FROM key_reach k, " +
        "json_each(k.components) j JOIN components c ON c.id = j.value WHERE k.key LIKE 'demo.%'")
    Assert-Line $r 'demo.map.a | field_binding | HostComponent'
    Assert-Line $r 'demo.map.b | field_binding | HostComponent'
    # the component's own call of it: its call site, `ts_call`
    Assert-Line $r 'demo.map.c | ts_call | HostComponent'
    Assert-Line $r 'demo.map.d | ts_call | HostComponent'
    Assert-Line $r 'demo.push.a | field_binding | HostComponent'
    Assert-Line $r 'demo.push.b | field_binding | HostComponent'
    Assert-Line $r 'demo.push.c | field_binding | HostComponent'
    Assert-Line $r 'demo.hole.a | field_binding | HostComponent'
    Assert-Line $r 'demo.hole.b | field_binding | HostComponent'
    Assert-Line $r 'demo.hole.c | field_binding | HostComponent'
    Assert-Line $r 'demo.hole.d | field_binding | HostComponent'
    Assert-Line $r 'demo.push.e | field_binding | HostComponent'
    # THE PUSH'S BRANCH rides with its element, a way beside the call's own branchless `ts_call` one (a returned local's pushes carry none)
    $b = Invoke-TsRowsQ $made.Db ("SELECT k.key || ' | ' || coalesce((SELECT group_concat(json_extract(k.branch_senses, '$.""' || b.value || '""'), ',') " +
        "FROM json_each(k.maybe_branches) b WHERE b.value NOT LIKE 'loop:%'), 'none') AS senses FROM key_reach k WHERE k.key LIKE 'demo.push.%'")
    Assert-Line $b 'demo.push.e | then'
    Assert-Line $b 'demo.push.c | none'
    Assert-Line $b 'demo.push.b | none'
    # a constant nobody indexes, and a list pushed to but never returned
    Assert-NoLine $r 'demo.map.unused | field_binding | HostComponent'
    Assert-NoLine $r 'demo.push.unused | field_binding | HostComponent'
}
}
