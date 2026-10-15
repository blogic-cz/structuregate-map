<#
    A callee's branch carried through EVERY call between the field and the key (#48,
    `rust/fbtcore/src/rows/ts/key/fields/key_follow.rs`, `key/fields/key_args.rs`): each hop's arguments are put in place
    of the next callee's parameters, outermost last, with no bound on the hops; a method reached again while its own ways
    are read (recursion) carries no condition of its own or below. Its helpers are `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '..\..\TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

$script:ChainQ = @"
SELECT k.key || ' | ' || k.route || ' | ' || k.n_paths || ' | ' || COALESCE((SELECT group_concat(v.dimension || ' ' ||
v.op || ' ' || $(Get-NodeListSql 'key_reach__always_values' 'v' 'values'), '; ') FROM key_reach__always_values v WHERE v.owner = '#' || k.rowid AND v.parent = '#' || k.rowid || '/0'), 'none') AS r
FROM key_reach k WHERE k.key LIKE 'demo.%'
"@

$script:ChainBase = @{
    'apps/shop/src/kinds.ts' = "export enum Kind { Home = 1, Flat = 2, Life = 3 }`nexport function isHome(kind: Kind): boolean { return kind === Kind.Home; }`n"
    'apps/shop/src/translate.service.ts' = "export class TranslateService { getTranslation(key: string): string { return key; } }`n"
    'apps/shop/src/label-helper.service.ts' = "import { Injectable } from '@angular/core';`nimport { TranslateService } from './translate.service';`n" +
        "@Injectable({ providedIn: 'root' })`nexport class LabelHelperService {`n  constructor(private translate: TranslateService) {}`n" +
        "  text(): { title: string } { return { title: this.translate.getTranslation('demo.a.title') }; }`n}`n"
    'structuregate.ts.json' = '{"locales":["apps/shop/src/assets/locales"],"i18nCalls":["getTranslation"]}'
}

# The facade of the issue: `sections` gathers `homeSection`, whose `if` tests a local a private accessor reads off the model.
function New-ChainFacade([string]$Gathered) {
    return "import { Injectable } from '@angular/core';`nimport { Kind, isHome } from './kinds';`n" +
        "import { LabelHelperService } from './label-helper.service';`n@Injectable({ providedIn: 'root' })`nexport class FacadeService {`n" +
        "  constructor(private helper: LabelHelperService) {}`n" +
        "  sections(model: { kind: Kind }): ({ title: string } | null)[] { return $Gathered; }`n" +
        "  homeSection(model: { kind: Kind }): { title: string } | null {`n    const kind = this.kindOf(model);`n" +
        "    if (isHome(kind)) {`n      return this.helper.text();`n    }`n    return null;`n  }`n" +
        "  private kindOf(model: { kind: Kind }): Kind { return model.kind; }`n}`n"
}

function New-ChainTree([hashtable]$Files) {
    $all = @{}
    foreach ($k in $script:ChainBase.Keys) { $all[$k] = $script:ChainBase[$k] }
    foreach ($k in $Files.Keys) { $all[$k] = $Files[$k] }
    if (-not $all.ContainsKey('apps/shop/src/assets/locales/en.json')) {
        $all['apps/shop/src/assets/locales/en.json'] = '{"demo":{"a":{"title":"T"}}}'
    }
    $made = New-TsRowsDb (New-TsRowsWorkspace $all)
    Assert-Exit $made.Result 0
    return Invoke-TsRowsQ $made.Db $script:ChainQ
}

$script:ChainRoot = "import { Component } from '@angular/core';`nimport { Kind } from './kinds';`nimport { FacadeService } from './facade.service';`n" +
    "@Component({ selector: 'demo-root', template: '<span *ngFor=`"let s of sections`">{{ s?.title }}</span>' })`n" +
    "export class RootComponent {`n  model = { kind: Kind.Life };`n  constructor(private facade: FacadeService) {}`n" +
    "  get sections() { return this.facade.sections(this.model); }`n}`n"

Test-Case 'tsrows keys: a callee branch two calls below the getter restricts the key through a filtered list' {
    $r = New-ChainTree @{
        'apps/shop/src/facade.service.ts' = New-ChainFacade '[this.homeSection(model)].filter((s) => s !== null)'
        'apps/shop/src/root.component.ts' = $script:ChainRoot
    }
    # getter -> sections -> homeSection: `isHome(kind)`, `kind` the accessor's `model.kind`, `model` the getter's `this.model`
    Assert-Line $r 'demo.a.title | field_binding | 1 | model.kind in ["Home"]'
}

Test-Case 'tsrows keys: a callee branch two calls below the getter restricts the key through a plain list' {
    $r = New-ChainTree @{
        'apps/shop/src/facade.service.ts' = New-ChainFacade '[this.homeSection(model)]'
        'apps/shop/src/root.component.ts' = $script:ChainRoot
    }
    Assert-Line $r 'demo.a.title | field_binding | 1 | model.kind in ["Home"]'
}

Test-Case 'tsrows keys: a call inside the list a getter returns carries its callee branch' {
    $r = New-ChainTree @{
        'apps/shop/src/facade.service.ts' = "import { Injectable } from '@angular/core';`nimport { Kind } from './kinds';`n" +
            "import { LabelHelperService } from './label-helper.service';`n@Injectable({ providedIn: 'root' })`nexport class FacadeService {`n" +
            "  constructor(private helper: LabelHelperService) {}`n" +
            "  homeSection(kind: Kind): { title: string } | null {`n    if (kind === Kind.Home) {`n      return this.helper.text();`n    }`n    return null;`n  }`n}`n"
        'apps/shop/src/root.component.ts' = "import { Component } from '@angular/core';`nimport { Kind } from './kinds';`nimport { FacadeService } from './facade.service';`n" +
            "@Component({ selector: 'demo-root', template: '<span *ngFor=`"let s of sections`">{{ s?.title }}</span>' })`n" +
            "export class RootComponent {`n  model = { kind: Kind.Life };`n  constructor(private facade: FacadeService) {}`n" +
            "  get sections() { return [this.facade.homeSection(this.model.kind)]; }`n}`n"
    }
    Assert-Line $r 'demo.a.title | field_binding | 1 | model.kind in ["Home"]'
}

Test-Case 'tsrows keys: a callee branch six calls below the getter restricts the key by the getter argument' {
    $r = New-ChainTree @{
        'apps/shop/src/deep.service.ts' = "import { Injectable } from '@angular/core';`nimport { Kind } from './kinds';`n" +
            "import { TranslateService } from './translate.service';`n@Injectable({ providedIn: 'root' })`nexport class DeepService {`n" +
            "  constructor(private translate: TranslateService) {}`n" +
            "  one(item: { kind: Kind }): string { return this.two(item); }`n" +
            "  two(row: { kind: Kind }): string { return this.three(row.kind, 'x'); }`n" +
            "  three(k: Kind, unused: string): string { return this.four(k); }`n" +
            "  four(kind: Kind): string { return ``[`${this.five(kind)}]``; }`n" +
            "  five(v: Kind): string { return this.six(v); }`n" +
            "  six(last: Kind): string {`n    if (last === Kind.Flat) {`n      return this.translate.getTranslation('demo.deep.a');`n    }`n    return '-';`n  }`n}`n"
        'apps/shop/src/root.component.ts' = "import { Component } from '@angular/core';`nimport { Kind } from './kinds';`nimport { DeepService } from './deep.service';`n" +
            "@Component({ selector: 'demo-root', template: '<span>{{ label }}</span>' })`n" +
            "export class RootComponent {`n  model = { kind: Kind.Life };`n  constructor(private deep: DeepService) {}`n" +
            "  get label() { return this.deep.one(this.model); }`n}`n"
        'apps/shop/src/assets/locales/en.json' = '{"demo":{"deep":{"a":"A"}}}'
    }
    Assert-Line $r 'demo.deep.a | field_binding | 1 | model.kind in ["Flat"]'
}

Test-Case 'tsrows keys: a callee branch in a recursive pair restricts nothing' {
    $r = New-ChainTree @{
        'apps/shop/src/rec.service.ts' = "import { Injectable } from '@angular/core';`nimport { Kind } from './kinds';`n" +
            "import { TranslateService } from './translate.service';`n@Injectable({ providedIn: 'root' })`nexport class RecService {`n" +
            "  constructor(private translate: TranslateService) {}`n" +
            "  first(k: Kind): string {`n    if (k === Kind.Home) {`n      return this.translate.getTranslation('demo.rec.a');`n    }`n    return this.second(k);`n  }`n" +
            "  second(k: Kind): string { return this.first(k); }`n}`n"
        'apps/shop/src/root.component.ts' = "import { Component } from '@angular/core';`nimport { Kind } from './kinds';`nimport { RecService } from './rec.service';`n" +
            "@Component({ selector: 'demo-root', template: '<span>{{ label }}</span>' })`n" +
            "export class RootComponent {`n  model = { kind: Kind.Life };`n  constructor(private rec: RecService) {}`n" +
            "  get label() { return this.rec.first(this.model.kind); }`n}`n"
        'apps/shop/src/assets/locales/en.json' = '{"demo":{"rec":{"a":"A"}}}'
    }
    # `first` is reached again while its own ways are read: no summary, so no way through it is restricted
    Assert-Line $r 'demo.rec.a | field_binding | 1 | none'
}

}
