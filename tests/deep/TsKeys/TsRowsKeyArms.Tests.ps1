<#
    What `key_reach.always_values` says about a key whose condition is an ARM of an expression, and about a key
    no way in permits anything for. A key handed to a translation call inside `c ? this.t('a') : this.t('b')`
    runs only on its side of `c`; a dimension every way narrows to NOTHING is `in []` - the key
    renders for nobody - never left out, which a consumer reads as unrestricted.
#>

. (Join-Path $PSScriptRoot '..\TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

$script:TsKeyArmsSql = "SELECT k.key || ' | ' || json_extract(v.value, '$.dimension') || ' ' || " +
    "json_extract(v.value, '$.op') || ' ' || json_extract(v.value, '$.values') AS reach " +
    "FROM key_reach k, json_each(k.always_values) v WHERE k.key LIKE 'ov.%' ORDER BY 1"

# THE SHAPES OF A KEY CHOSEN IN A CONDITIONAL ARM, all through a wrapper `t` around the configured `instant`: a ternary choosing between
# two calls inside a SERVICE call's argument, under a `case` - the case alone was all `ov.e` carried - an `&&`
# in statement position, and the plain `if` that already worked.
$script:TsKeyArmsFiles = @{
    'apps/shop/src/ov.component.html' = "<b>{{ label }}</b>`n"
    'apps/shop/src/ov.component.ts' = "import { Component, Injectable } from '@angular/core';`n" +
        "export enum Brand { Rowan = 1, Oak = 2, Larch = 3, Cedar = 4, Aspen = 5 }`n" +
        "export enum Item { Cable = 1, Charger = 2, Case = 3 }`n" +
        "@Injectable({ providedIn: 'root' })`n" +
        "export class Words {`n  instant(key: string): string {`n    return key;`n  }`n}`n" +
        "@Injectable({ providedIn: 'root' })`n" +
        "export class Rows {`n  add(row: { label: string }): void {}`n}`n" +
        "@Component({ selector: 'app-ov', templateUrl: './ov.component.html', standalone: true })`n" +
        "export class OvComponent {`n" +
        "  brand: Brand = Brand.Rowan;`n  item: Item = Item.Cable;`n  label = '';`n" +
        "  constructor(private words: Words, private rows: Rows) {}`n" +
        "  t(key: string): string {`n    return this.words.instant(key);`n  }`n" +
        "  build(): void {`n" +
        "    if (this.brand === Brand.Aspen) {`n      this.rows.add({ label: this.t('ov.a') });`n    }`n" +
        "    this.brand === Brand.Cedar && this.rows.add({ label: this.t('ov.c') });`n" +
        "    switch (this.item) {`n      case Item.Case:`n" +
        "        this.rows.add({ label: this.brand === Brand.Larch ? this.t('ov.e') : this.t('ov.e2') });`n" +
        "        break;`n    }`n" +
        "  }`n" +
        "}`n"
    'apps/shop/src/assets/locales/en.json' = '{"shop":{"title":"Shop","cart":{"empty":"Empty"}},' +
        '"ov":{"a":"1","c":"2","e":"3","e2":"4"}}'
    'structuregate.ts.json' = '{"locales":["apps/shop/src/assets/locales"],"i18nCarriers":["money"],' +
        '"i18nCalls":["instant","t"],"gateInputs":["disabled"],' +
        '"featureChecks":["FlagService.isOn"],"featureEnum":"FlagCodes"}'
}

Test-Case 'tsrows: a key a translation call chooses in a conditional arm carries that arm' {
    $made = New-TsRowsDb (New-TsRowsWorkspace $script:TsKeyArmsFiles)
    $r = Invoke-Gate --map-query $made.Db --width 0 --sql $script:TsKeyArmsSql
    Assert-Exit $r 0
    Assert-Line $r 'ov.a | brand in ["Aspen"]'
    Assert-Line $r 'ov.c | brand in ["Cedar"]'
    Assert-Line $r 'ov.e | brand in ["Larch"]'
    Assert-Line $r 'ov.e | item in ["Case"]'
    Assert-Line $r 'ov.e2 | brand not_in ["Larch"]'
    Assert-Line $r 'ov.e2 | item in ["Case"]'
    # The arm is on the CALL row, as `branch` and `case` are: the i18n ref joins it.
    $c = Invoke-Gate --map-query $made.Db --width 0 --sql ("SELECT k.key || ' | ' || c.choices AS arm FROM i18n_refs k " +
        "JOIN calls c ON c.id = k.call WHERE k.key = 'ov.e2'")
    Assert-Line $c 'ov.e2 | ["!x:'
}

# A PARTIAL RUN re-reads the edited component, and the arm moves with its condition.
Test-Case 'tsrows: an edited conditional arm moves the key it chooses on a partial run' {
    $tree = New-TsRowsWorkspace $script:TsKeyArmsFiles
    Assert-Exit (New-TsRowsDb $tree).Result 0
    $ts = Join-Path $tree 'apps/shop/src/ov.component.ts'
    Set-Content -LiteralPath $ts -Value ([IO.File]::ReadAllText($ts).Replace('Brand.Larch ?', 'Brand.Oak ?'))
    $again = New-TsRowsDb $tree
    Assert-Line $again.Result 'put back'
    $r = Invoke-Gate --map-query $again.Db --width 0 --sql $script:TsKeyArmsSql
    Assert-Line $r 'ov.e | brand in ["Oak"]'
    Assert-Line $r 'ov.e2 | brand not_in ["Oak"]'
    Assert-NoLine $r 'Larch'
}

# NOTHING PERMITTED: two config lists NESTED on the only way in share no member - tooltips under an outer config
# that leaves their color out - and a key a template literal spells with a hole no member of the enum names.
# Neither key can render for anyone, and that is the answer: `in []`. v1.5.18 moved it to `unreadable_values`, the
# dimension vanished from `always_values`, and the keys read as unrestricted. The plain config keeps its `in`.
Test-Case 'tsrows: a dimension no way permits anything for is in nothing, never left out' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/visibility.directive.ts' = "import { Directive, Input, TemplateRef, ViewContainerRef } from '@angular/core';`n" +
            "export enum Colors { Red = 1, Blue = 2, Lime = 3, Teal = 4, Gray = 5, Pink = 6 }`n" +
            "export interface ColorFilter { include: Colors[]; }`n" +
            "@Directive({ selector: '[colorFilter]', standalone: true })`n" +
            "export class ColorFilterDirective {`n" +
            "  current: Colors = Colors.Gray;`n" +
            "  constructor(private templateRef: TemplateRef<any>, private viewContainer: ViewContainerRef) {}`n" +
            "  @Input() set colorFilter(input: ColorFilter) {`n" +
            "    this.viewContainer.clear();`n" +
            "    if (input.include.some((m) => m === this.current)) {`n" +
            "      this.viewContainer.createEmbeddedView(this.templateRef);`n    }`n" +
            "  }`n" +
            "}`n"
        'apps/shop/src/tips.component.html' = "<i *colorFilter=`"{include: [ids.Red]}`">{{ 'tp.red' | money }}</i>`n" +
            "<i *colorFilter=`"{include: [ids.Red, ids.Blue]}`"><b *colorFilter=`"{include: [ids.Pink]}`">" +
            "{{ 'tp.nested' | money }}</b></i>`n<i [title]=`"built`">x</i>`n"
        'apps/shop/src/tips.component.ts' = "import { Component } from '@angular/core';`n" +
            "import { Colors, ColorFilterDirective } from './visibility.directive';`n" +
            "@Component({ selector: 'app-tips', templateUrl: './tips.component.html', standalone: true, imports: [ColorFilterDirective] })`n" +
            "export class TipsComponent {`n" +
            "  ids = Colors;`n  brand: Colors = Colors.Red;`n  built = '';`n" +
            "  pick(): void {`n    this.built = ``bt.tag`${Colors[this.brand]}``;`n  }`n" +
            "}`n"
        'apps/shop/src/assets/locales/en.json' = '{"shop":{"title":"Shop","cart":{"empty":"Empty"}},' +
            '"tp":{"red":"a","nested":"b"},"bt":{"tagBlue":"c","tagTealX":"d"}}'
    }
    $made = New-TsRowsDb $tree
    $r = Invoke-Gate --map-query $made.Db --width 0 --sql ("SELECT key || ' | ' || always_values || ' | ' || " +
        "json_array_length(unreadable_values) || ' ' || coalesce(json_extract(unreadable_values, '$[0].dimension'), '-') AS reach " +
        "FROM key_reach WHERE key LIKE 'tp.%' OR key LIKE 'bt.%'")
    Assert-Exit $r 0
    Assert-Line $r '"dimension": "colorFilter.include", "op": "in", "values": [], "domain": 6}] | 0 -'
    Assert-Line $r 'tp.nested | [{"enum": "e:'
    Assert-Line $r 'bt.tagTealX | [{"enum": "e:'
    Assert-NoLine $r '| 1 '
    Assert-Line $r 'tp.red | [{"enum": "e:'
    Assert-Line $r '"values": ["Red"], "domain": 6}] | 0 -'
    Assert-Line $r '"values": ["Blue"], "domain": 6}] | 0 -'
}

# A LITERAL `false` IS NO WAY IN. It names no member, so no `gate_values` row can say what it permits, and folded as
# an ordinary gate the keys behind it - its own and a child component's - read as rendered for everyone. The ways
# through it are dropped: no way left is `n_paths 0`, while a way past a real condition stays.
Test-Case 'tsrows: a key behind a literal false renders on no way' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/off.component.html' = "<i *ngIf=`"false`">{{ 'dk.off' | money }}</i>`n" +
            "<i *ngIf=`"shown`">{{ 'dk.on' | money }}</i>`n@if (false) { <b>{{ 'dk.block' | money }}</b> }`n" +
            "<app-off-child *ngIf=`"false`"></app-off-child>`n"
        'apps/shop/src/off-child.component.html' = "<i>{{ 'dk.child' | money }}</i>`n"
        'apps/shop/src/off.component.ts' = "import { Component } from '@angular/core';`n" +
            "@Component({ selector: 'app-off-child', templateUrl: './off-child.component.html', standalone: true })`n" +
            "export class OffChildComponent {}`n" +
            "@Component({ selector: 'app-off', templateUrl: './off.component.html', standalone: true, imports: [OffChildComponent] })`n" +
            "export class OffComponent {`n  shown = true;`n}`n"
        'apps/shop/src/assets/locales/en.json' = '{"shop":{"title":"Shop","cart":{"empty":"Empty"}},' +
            '"dk":{"off":"a","on":"b","block":"c","child":"d"}}'
    }
    $made = New-TsRowsDb $tree
    $r = Invoke-Gate --map-query $made.Db --width 0 --sql "SELECT key || ' | ' || n_paths AS reach FROM key_reach WHERE key LIKE 'dk.%'"
    Assert-Exit $r 0
    Assert-Line $r 'dk.off | 0'
    Assert-Line $r 'dk.block | 0'
    Assert-Line $r 'dk.child | 0'
    Assert-Line $r 'dk.on | 1'
}

}
