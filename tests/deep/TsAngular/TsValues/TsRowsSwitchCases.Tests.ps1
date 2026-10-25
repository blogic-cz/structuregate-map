<#
    WHAT A SWITCH TESTS AND WHAT EACH LABEL SELECTS (#127, `src/TsRows/TsDecls/TsExpr/TsStatements/TsCases.mjs`): a `switch_cases` row of the
    Angular half names the switched enum - `discriminant_type`, `discriminant_enum_id` the `enums` row - and per label `label_symbols`
    (`Enum.Member`) and `label_values` (the constant), from the checker, as the C# half's rows do. Its helpers are `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '../../TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

Test-Case 'tsrows: a switch case names the enum it tests and the member each label selects (#127)' {
    $made = New-TsRowsDb (New-TsRowsWorkspace @{
        'apps/shop/src/hue.ts' = "export enum Hue { Red = 1, Green = 2, Blue = 3 }`nexport enum Size { Small = 's', Large = 'l' }`n"
        'apps/shop/src/labels.ts' = "import { Hue as Colour, Size } from './hue';`n" +
            "export function hueLabel(h: Colour): string {`n  switch (h) {`n    case Colour.Red: return 'r';`n    case Colour.Green: return 'g';`n    default: return '?';`n  }`n}`n" +
            "export function sizeLabel(s: Size | null): string {`n  switch (s) {`n    case Size.Small:`n    case Size.Large: return 'x';`n  }`n  return '';`n}`n" +
            "export function wordLabel(w: string): number {`n  switch (w) {`n    case 'a': return 1;`n  }`n  return 0;`n}`n"
    })
    Assert-Exit $made.Result 0
    $r = Invoke-TsRowsQ $made.Db ("SELECT 'sc=' || s.line || '|' || coalesce(s.discriminant_type, '-') || '|' || coalesce(e.name, '-') || '|' || s.is_default || '|' || " +
        "coalesce((SELECT group_concat(v, ',') FROM (SELECT y.value || '=' || v.value AS v FROM switch_cases__label_symbols y JOIN switch_cases__label_values v " +
        "ON v.owner = y.owner AND v.parent = 0 AND v.ord = y.ord WHERE y.owner = s.id AND y.parent = 0 ORDER BY y.ord)), '-') AS r " +
        "FROM switch_cases s LEFT JOIN enums e ON e.id = s.discriminant_enum_id")
    # AN ALIAS IMPORT IS STILL THE ENUM: `Colour.Red` selects `Hue.Red`, value 1.
    Assert-Line $r 'sc=4|Hue|Hue|0|Hue.Red=1'
    Assert-Line $r 'sc=5|Hue|Hue|0|Hue.Green=2'
    Assert-Line $r 'sc=6|Hue|Hue|1|-'
    # A NULLABLE STRING ENUM, two labels of one group.
    Assert-Line $r 'sc=12|Size|Size|0|Size.Small=s,Size.Large=l'
    # THE CONTROL: a switch over a string names no enum; its label is a literal, no symbol.
    Assert-Line $r 'sc=18|string|-|0|=a'
}

}
