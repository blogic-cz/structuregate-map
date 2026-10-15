<#
    A `let` local a method returns reads EVERY `=` written to it, not only its initializer
    (`rust/fbtcore/src/rows/ts/key/fields/key_follow.rs`, `Returned::enter`): the key a later write hands the
    local reaches the caller, whether the method returns the local itself or an arrow reading it.
    Its helpers are `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '..\TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

Test-Case 'tsrows keys: a let local that is written after its initializer hands those keys to the caller' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/ids.ts' = "export enum Kind { Alpha = 1, Beta = 2 }`n"
        'apps/shop/src/translate.service.ts' = "import { Injectable } from '@angular/core';`n" +
            "@Injectable({ providedIn: 'root' })`n" +
            "export class TranslateService {`n  getTranslation(key: string): string {`n    return key;`n  }`n}`n"
        'apps/shop/src/desc.service.ts' = "import { Injectable } from '@angular/core';`n" +
            "import { Kind } from './ids';`n" +
            "import { TranslateService } from './translate.service';`n" +
            "@Injectable({ providedIn: 'root' })`n" +
            "export class DescService {`n" +
            "  constructor(private translate: TranslateService) {}`n" +
            "  branchLet(kind: Kind, flag: boolean): string {`n    let second = '';`n" +
            "    if (flag) { second = this.translate.getTranslation(``demo.branch.`${Kind[kind]}.extra``); }`n" +
            "    else { second = this.translate.getTranslation(``demo.branch2.`${Kind[kind]}``); }`n" +
            "    return this.pick(() => second);`n  }`n" +
            "  private pick(get: () => string): string { return get(); }`n" +
            "  lateLet(kind: Kind): string {`n    let s = '';`n" +
            "    s = this.translate.getTranslation(``demo.late.`${Kind[kind]}``);`n    return s;`n  }`n" +
            "  unusedLet(kind: Kind): string {`n    let u = '';`n" +
            "    u = this.translate.getTranslation(``demo.unusedlet.`${Kind[kind]}``);`n    return '-';`n  }`n" +
            "}`n"
        'apps/shop/src/host.component.ts' = "import { Component, Input, OnInit } from '@angular/core';`n" +
            "import { Kind } from './ids';`n" +
            "import { DescService } from './desc.service';`n" +
            "@Component({ selector: 'demo-host', standalone: true, template: '{{ a }} {{ b }} {{ c }}' })`n" +
            "export class HostComponent implements OnInit {`n" +
            "  @Input() kind: Kind = Kind.Alpha;`n" +
            "  a!: string;`n  b!: string;`n  c!: string;`n" +
            "  constructor(private desc: DescService) {}`n" +
            "  ngOnInit() {`n    this.a = this.desc.branchLet(this.kind, true);`n" +
            "    this.b = this.desc.lateLet(this.kind);`n    this.c = this.desc.unusedLet(this.kind);`n  }`n" +
            "}`n"
        'apps/shop/src/assets/locales/en.json' = '{"shop":{"title":"Shop"},"demo":{' +
            '"branch":{"Alpha":{"extra":"x"},"Beta":{"extra":"y"}},"branch2":{"Alpha":"p","Beta":"q"},' +
            '"late":{"Alpha":"m","Beta":"n"},"unusedlet":{"Alpha":"u","Beta":"v"}}}'
        'structuregate.ts.json' = '{"locales":["apps/shop/src/assets/locales"],"i18nCalls":["getTranslation"]}'
    }
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    $r = Invoke-TsRowsQ $made.Db ("SELECT k.key || ' | ' || k.route || ' | ' || c.name AS reach FROM key_reach k " +
        "JOIN key_reach__components j ON j.owner = '#' || k.rowid AND j.parent = '#' || k.rowid || '/0' JOIN components c ON c.id = j.value WHERE k.key LIKE 'demo.%'")
    Assert-Line $r 'demo.branch.Alpha.extra | field_binding | HostComponent'
    Assert-Line $r 'demo.branch2.Alpha | field_binding | HostComponent'
    Assert-Line $r 'demo.branch2.Beta | field_binding | HostComponent'
    Assert-Line $r 'demo.late.Alpha | field_binding | HostComponent'
    # written to a local the method never returns: not reached
    Assert-NoLine $r 'demo.unusedlet.Alpha | field_binding | HostComponent'
    Assert-NoLine $r 'demo.unusedlet.Beta | field_binding | HostComponent'
}
}
