<#
    A `[style.x]` binding restyles its element and never hides it (#52, `gate_values::class_polarity`, as #30 for a class that
    only styles): it gives no `gate_values` row, and a key under it is not restricted. The gate stays in `gates`. Control: a class
    whose stylesheet rule hides is still the visibility condition. Helpers: `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '../TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

$script:TsStyleGatesHtml = @(
    '<em [style.color]="kind === K.Alpha && ''red''">{{ ''demo.paint.color'' | translate }}</em>'
    '<span [class.gone-x]="kind === K.Beta">{{ ''demo.paint.gone'' | translate }}</span>'
) -join "`n"

function New-TsStyleGatesDb {
    return New-TsRowsDb (New-TsRowsWorkspace @{
        'apps/shop/src/kinds.ts' = "export enum DemoIDs { Alpha = 1, Beta = 2 }`n"
        'apps/shop/src/paint.component.ts' = "import { Component, Input } from '@angular/core';`n" +
            "import { DemoIDs } from './kinds';`n" +
            "@Component({ selector: 'demo-paint', standalone: true, templateUrl: './paint.component.html', " +
            "styleUrls: ['./paint.component.scss'] })`n" +
            "export class PaintComponent { K = DemoIDs; @Input() kind!: DemoIDs; }`n"
        'apps/shop/src/paint.component.html' = $script:TsStyleGatesHtml
        'apps/shop/src/paint.component.scss' = '.gone-x { display: none; }'
        'apps/shop/src/assets/locales/en.json' = '{"demo":{"paint":{"color":"1","gone":"2"}}}'
    })
}

function Invoke-TsStyleGatesQ([string]$Path, [string]$Sql) {
    $result = Invoke-Gate --map-query $Path --width 0 --sql $Sql
    Assert-Exit $result 0
    return $result
}

Test-Case 'tsrows: a style binding is no visibility condition, a hiding class is (#52)' {
    $made = New-TsStyleGatesDb
    Assert-Exit $made.Result 0
    $v = Invoke-TsStyleGatesQ $made.Db ("SELECT 'v=' || k.key || ' | ' || $(Get-TsFieldSql 'key_reach__always_values' 'v' 'dimension') || ' ' || " +
        "$(Get-TsFieldSql 'key_reach__always_values' 'v' 'op') || ' ' || $(Get-NodeListSql 'key_reach__always_values' 'v' 'values') AS r FROM key_reach k " +
        "JOIN key_reach__always_values v ON v.owner = '#' || k.rowid AND v.parent = 0 WHERE k.key LIKE 'demo.paint.%'")
    Assert-Line $v 'v=demo.paint.gone | kind in ["Alpha"]'
    # Two members only: `not_in ["Beta"]` is published as its smaller side, `in ["Alpha"]`.
    Assert-NoLine $v 'v=demo.paint.color |'
    Assert-Line (Invoke-TsStyleGatesQ $made.Db "SELECT 'k=' || key || '#' AS r FROM key_reach WHERE key LIKE 'demo.paint.%'") 'k=demo.paint.color#'
    $g = Invoke-TsStyleGatesQ $made.Db "SELECT 'style=' || g.gate_kind || '#' AS r FROM gates g WHERE g.gate_kind = 'style'"
    Assert-Line $g 'style=style#'
    $n = Invoke-TsStyleGatesQ $made.Db "SELECT 'n=' || COUNT(*) || '#' AS r FROM gate_values v JOIN gates g ON g.id = v.gate WHERE g.gate_kind = 'style'"
    Assert-Line $n 'n=0#'
}

}
