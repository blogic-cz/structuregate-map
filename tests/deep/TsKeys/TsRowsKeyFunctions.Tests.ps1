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
