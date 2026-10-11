<#
    A return that WRAPS a translate call hands its key to the field holding the call
    (`rust/fbtcore/src/rows/ts/key/fields/key_follow.rs`, `Returned::through`): an object's property value, a
    `+` chain's part, the argument of `Promise.resolve`; a list's element property is a site only where an `*ngFor`
    reads it (`key/key_loops.rs`). The argument of any other call renders nothing in the field.
    Its helpers are `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '..\TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

Test-Case 'tsrows keys: a translate call wrapped in an object, a concat, a Promise or a list reaches the field' {
    $tree = New-TsRowsWorkspace @{
        # `Promise.resolve` resolves to the compiler's own lib only when the target has a Promise value
        'apps/shop/tsconfig.app.json' = '{"compilerOptions":{"strict":true,"target":"ES2022"},"include":["src/**/*.ts"]}'
        'apps/shop/src/translate.service.ts' = "import { Injectable } from '@angular/core';`n" +
            "@Injectable({ providedIn: 'root' })`n" +
            "export class TranslateService {`n  getTranslation(key: string): string {`n    return key;`n  }`n}`n"
        'apps/shop/src/wrap.service.ts' = "import { Injectable } from '@angular/core';`n" +
            "import { TranslateService } from './translate.service';`n" +
            "@Injectable({ providedIn: 'root' })`n" +
            "export class WrapService {`n" +
            "  constructor(private t: TranslateService) {}`n" +
            "  sink(x: string): void {}`n" +
            "  objectRet() { return { title: this.t.getTranslation('demo.obj.title') }; }`n" +
            "  concatRet() { return 'x ' + this.t.getTranslation('demo.cat.part'); }`n" +
            "  promiseRet() { return Promise.resolve(this.t.getTranslation('demo.prom.x')); }`n" +
            "  listRet() { return [{ name: this.t.getTranslation('demo.list.name'), other: this.t.getTranslation('demo.list.other') }]; }`n" +
            "  argRet() { this.sink(this.t.getTranslation('demo.arg.x')); return '-'; }`n" +
            "}`n"
        'apps/shop/src/host.component.ts' = "import { Component, OnInit } from '@angular/core';`n" +
            "import { WrapService } from './wrap.service';`n" +
            "@Component({ selector: 'demo-host', standalone: true, template: '{{ o.title }} {{ c }} {{ p }} {{ a }} <li *ngFor=\`"let x of items\`">{{ x.name }}</li>' })`n" +
            "export class HostComponent implements OnInit {`n" +
            "  o: any;`n  c!: string;`n  p: any;`n  a!: string;`n  items: any[] = [];`n" +
            "  constructor(private w: WrapService) {}`n" +
            "  ngOnInit() {`n    this.o = this.w.objectRet();`n    this.c = this.w.concatRet();`n" +
            "    this.p = this.w.promiseRet();`n    this.a = this.w.argRet();`n    this.items = this.w.listRet();`n  }`n" +
            "}`n"
        'apps/shop/src/assets/locales/en.json' = '{"shop":{"title":"Shop"},"demo":{"obj":{"title":"a"},"cat":{"part":"b"},' +
            '"prom":{"x":"c"},"list":{"name":"d","other":"e"},"arg":{"x":"f"}}}'
        'structuregate.ts.json' = '{"locales":["apps/shop/src/assets/locales"],"i18nCalls":["getTranslation"]}'
    }
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    $r = Invoke-TsRowsQ $made.Db ("SELECT k.key || ' | ' || k.route || ' | ' || c.name AS reach FROM key_reach k, " +
        "json_each(k.components) j JOIN components c ON c.id = j.value WHERE k.key LIKE 'demo.%'")
    Assert-Line $r 'demo.obj.title | field_binding | HostComponent'
    Assert-Line $r 'demo.cat.part | field_binding | HostComponent'
    Assert-Line $r 'demo.prom.x | field_binding | HostComponent'
    Assert-Line $r 'demo.list.name | field_binding | HostComponent'
    # the list's other property is read by no template, and an argument of another call renders nothing in the field
    Assert-NoLine $r 'demo.list.other | field_binding | HostComponent'
    Assert-NoLine $r 'demo.arg.x | field_binding | HostComponent'
}
}
