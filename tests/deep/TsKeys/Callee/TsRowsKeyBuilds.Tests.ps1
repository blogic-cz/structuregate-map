<#
    WHICH TEMPLATE LITERAL BUILDS A KEY, AND OVER WHICH ENUM (#123): `key_builds` (`rust/fbtcore/src/rows/ts/key/key_builds.rs`) - one row per
    (key, literal, hole) - names the `template_literals` row, the hole's text in that key and, for a hole the extractor typed by an enum
    (`hole_refs`: `${Hue[x]}`, or a hole TYPED by one - `enum_value`, `TsReadRefs.mjs::enumValueHole`), the enum and the member. Its helpers
    are `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '../../TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

Test-Case 'tsrows keys: an expanded key names the template literal and the enum member that build it (#123)' {
    $made = New-TsRowsDb (New-TsRowsWorkspace @{
        'apps/shop/src/rows.component.ts' = "import { Component, Injectable } from '@angular/core';`nimport { NgFor } from '@angular/common';`n" +
            "@Injectable({ providedIn: 'root' })`nexport class Words { instant(k: string) { return k; } }`n" +
            "export enum Demo { Alpha = 'Alpha', Beta = 'Beta' }`nexport enum Level { Low = 1, High = 2 }`nexport enum Hue { Red, Blue }`n" +
            "interface Row { status: Demo; level: Level; hue: Hue; free: string; }`n" +
            "@Component({ selector: 'app-rows', standalone: true, imports: [NgFor], template: '<div *ngFor=`"let r of rows`">{{ labelOf(r) }}</div>' })`n" +
            "export class RowsComponent {`n  rows: Row[] = [];`n  constructor(private words: Words) {}`n" +
            "  labelOf(r: Row): string {`n" +
            "    return this.words.instant(``demo.status.`${r.status}``) + this.words.instant(``demo.level.`${r.level}``) +`n" +
            "      this.words.instant(``demo.hue.`${Hue[r.hue]}``) + this.words.instant(``demo.free.`${r.free}``);`n  }`n}`n"
        'apps/shop/src/assets/locales/en.json' = '{"shop":{"title":"Shop"},"demo":{"status":{"Alpha":"A","Beta":"B","Gamma":"G"},' +
            '"level":{"1":"L","2":"H"},"hue":{"Red":"R","Blue":"B"},"free":{"x":"X"}}}'
        'structuregate.ts.json' = '{"locales":["apps/shop/src/assets/locales"],"i18nCalls":["instant"]}'
    }) -Shared
    Assert-Exit $made.Result 0
    $r = Invoke-TsRowsQ $made.Db ("SELECT 'b=' || b.key || '|' || b.prefix || '|' || b.hole || '|' || b.value || '|' || coalesce(e.name, '-') || '|' || coalesce(b.member, '-') || '|' || " +
        "(CASE WHEN t.id IS NULL THEN 'no-literal' ELSE 'tl' END) AS r FROM key_builds b LEFT JOIN enums e ON e.id = b.enum LEFT JOIN template_literals t ON t.id = b.literal")
    # A HOLE TYPED BY A STRING ENUM: the value is the member's value, the member its name.
    Assert-Line $r 'b=demo.status.Alpha|demo.status.|0|Alpha|Demo|Alpha|tl'
    Assert-Line $r 'b=demo.status.Beta|demo.status.|0|Beta|Demo|Beta|tl'
    # ...A NUMERIC ONE: the text is the number, the member the name that has it.
    Assert-Line $r 'b=demo.level.1|demo.level.|0|1|Level|Low|tl'
    Assert-Line $r 'b=demo.level.2|demo.level.|0|2|Level|High|tl'
    # ...A NAME LOOKUP (`Hue[r.hue]`): the member name itself.
    Assert-Line $r 'b=demo.hue.Red|demo.hue.|0|Red|Hue|Red|tl'
    # AN UNTYPED HOLE builds every key under its prefix, with no enum.
    Assert-Line $r 'b=demo.free.x|demo.free.|0|x|-|-|tl'
    # A KEY UNDER THE PREFIX THAT NO MEMBER WRITES is not this literal's: counted, a missing row prints nothing.
    $g = Invoke-TsRowsQ $made.Db "SELECT 'gamma=' || count(*) AS n FROM key_builds WHERE key = 'demo.status.Gamma'"
    Assert-Line $g 'gamma=0'
    # THE EXTRACTOR'S SIDE: the typed hole is a `hole_refs` entry of kind `enum_value`.
    $k = Invoke-TsRowsQ $made.Db ("SELECT 'kind=' || n.value AS k FROM template_literals t JOIN template_literals__hole_refs n ON n.owner = t.id AND n.role = 'kind' " +
        "JOIN template_literals__parts p ON p.owner = t.id AND p.parent = 0 AND p.ord = 0 WHERE p.value = 'demo.status.'")
    Assert-Line $k 'kind=enum_value'
}

# (#131) A HOLE TYPED BY A UNION OF LITERALS writes exactly those texts: `${group}` over `group: 'alpha' | 'beta'` builds `alpha` and `beta` keys -
# the member the literal itself, no enum - and no `gamma` key; the plain `string` hole after it stays an untyped expansion under each.
Test-Case 'tsrows keys: a hole typed by a union of literals builds only those segments (#131)' {
    $made = New-TsRowsDb (New-TsRowsWorkspace @{
        'apps/shop/src/docs.component.ts' = "import { Component, Injectable } from '@angular/core';`n" +
            "@Injectable({ providedIn: 'root' })`nexport class Words { instant(k: string) { return k; } }`n" +
            "@Component({ selector: 'app-docs', standalone: true, template: '<span>{{ label(\'alpha\', k) }}</span>' })`n" +
            "export class DocsComponent {`n  k = 'x';`n  constructor(private words: Words) {}`n" +
            "  label(group: 'alpha' | 'beta', key: string) { return this.words.instant(``demo.enums.`${group}.`${key}``); }`n}`n"
        'apps/shop/src/assets/locales/en.json' = '{"shop":{"title":"Shop"},"demo":{"enums":{"alpha":{"x":"X"},"beta":{"y":"Y"},"gamma":{"z":"Z"}}}}'
        'structuregate.ts.json' = '{"locales":["apps/shop/src/assets/locales"],"i18nCalls":["instant"]}'
    })
    Assert-Exit $made.Result 0
    $r = Invoke-TsRowsQ $made.Db ("SELECT 'b=' || key || '|' || hole || '|' || value || '|' || coalesce(enum, '-') || '|' || coalesce(member, '-') AS r FROM key_builds WHERE key LIKE 'demo.enums.%'")
    Assert-Line $r 'b=demo.enums.alpha.x|0|alpha|-|alpha'
    Assert-Line $r 'b=demo.enums.beta.y|0|beta|-|beta'
    Assert-Line $r 'b=demo.enums.alpha.x|1|x|-|-'
    $g = Invoke-TsRowsQ $made.Db "SELECT 'gamma=' || count(*) AS n FROM key_builds WHERE key LIKE 'demo.enums.gamma.%'"
    Assert-Line $g 'gamma=0'
    $k = Invoke-TsRowsQ $made.Db ("SELECT 'kind=' || n.value AS k FROM template_literals__hole_refs n WHERE n.role = 'kind'")
    Assert-Line $k 'kind=literal_union'
}

}
