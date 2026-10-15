<#
    A call of a method the config's `i18nDictionaryCalls` names references EVERY locale key under its prefix
    (`TsI18nCalls.mjs`, `key_routes.rs`, `key_follow::dictionary`): the code reads the result as `dict[name]`
    or hands it on, so no key is ever spelled at the call. Its helpers are `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '..\TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

function New-TsRowsDictionaryTree([string]$Config) {
    New-TsRowsWorkspace @{
        'apps/shop/src/translate.service.ts' = "import { Injectable } from '@angular/core';`n" +
            "@Injectable({ providedIn: 'root' })`n" +
            "export class TranslateService {`n" +
            "  getTranslationDictionary(prefix: string): { [k: string]: string } {`n    return {};`n  }`n}`n"
        'apps/shop/src/held.component.ts' = "import { Component } from '@angular/core';`n" +
            "import { TranslateService } from './translate.service';`n" +
            "@Component({ selector: 'demo-held', standalone: true, template: '{{ opts[""Rate""] }}' })`n" +
            "export class HeldComponent {`n" +
            "  opts = this.t.getTranslationDictionary('demo.sort.options');`n" +
            "  constructor(private t: TranslateService) {}`n}`n"
        'apps/shop/src/getter.component.ts' = "import { Component } from '@angular/core';`n" +
            "import { TranslateService } from './translate.service';`n" +
            "@Component({ selector: 'demo-getter', standalone: true, template: '{{ opts }}' })`n" +
            "export class GetterComponent {`n" +
            "  constructor(private t: TranslateService) {}`n" +
            "  get opts() { return this.t.getTranslationDictionary('demo.sort.options'); }`n}`n"
        'apps/shop/src/init.component.ts' = "import { Component, OnInit } from '@angular/core';`n" +
            "import { TranslateService } from './translate.service';`n" +
            "@Component({ selector: 'demo-init', standalone: true, template: '<p></p>' })`n" +
            "export class InitComponent implements OnInit {`n" +
            "  constructor(private t: TranslateService) {}`n" +
            "  ngOnInit() { this.t.getTranslationDictionary('demo.sort.options'); }`n}`n"
        'apps/shop/src/assets/locales/en.json' = '{"demo":{"sort":{"options":{"Rate":"r","Fees":"f"}},"other":{"x":"x"}}}'
        'structuregate.ts.json' = $Config
    }
}

Test-Case 'tsrows keys: a dictionary call references every key under its prefix, held in a field or called in a hook' {
    $tree = New-TsRowsDictionaryTree ('{"locales":["apps/shop/src/assets/locales"],' +
        '"i18nDictionaryCalls":["getTranslationDictionary"]}')
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    $r = Invoke-TsRowsQ $made.Db ("SELECT k.key || ' | ' || c.name AS reach FROM key_reach k " +
        "JOIN key_reach__components j ON j.owner = '#' || k.rowid AND j.parent = '#' || k.rowid || '/0' JOIN components c ON c.id = j.value WHERE k.key LIKE 'demo.%'")
    # held in a field the template reads, held by a getter the template reads, and called in a hook (the call
    # site's own component): each is a component the key is reached from
    foreach ($key in 'demo.sort.options.Rate', 'demo.sort.options.Fees') {
        foreach ($comp in 'HeldComponent', 'GetterComponent', 'InitComponent') { Assert-Line $r "$key | $comp" }
    }
    # a key outside the prefix is not reached by it
    Assert-NoLine $r 'demo.other.x |'
    $n = Invoke-TsRowsQ $made.Db "SELECT key || ' | ' || route AS r FROM key_reach WHERE key LIKE 'demo.%'"
    # the template position beats the call site as the key's route
    Assert-Line $n 'demo.sort.options.Rate | field_binding'
    Assert-Line $n 'demo.sort.options.Fees | field_binding'
    Assert-Line $n 'demo.other.x | none'
}

Test-Case 'tsrows keys: without the config entry the same calls reach nothing' {
    $tree = New-TsRowsDictionaryTree '{"locales":["apps/shop/src/assets/locales"]}'
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    $n = Invoke-TsRowsQ $made.Db "SELECT key || ' | ' || route AS r FROM key_reach WHERE key LIKE 'demo.%'"
    Assert-Line $n 'demo.sort.options.Rate | none'
    Assert-Line $n 'demo.sort.options.Fees | none'
    Assert-Line $n 'demo.other.x | none'
}
}
