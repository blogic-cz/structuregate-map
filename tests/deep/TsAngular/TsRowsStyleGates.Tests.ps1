<#
    A `[style.x]` binding restyles its element and never hides it (#52, `gate_values::class_polarity`, as #30 for a class that
    only styles): it gives no `gate_values` row, and a key under it is not restricted. The gate stays in `gates`. Control: a class
    whose stylesheet rule hides is still the visibility condition. EXCEPT (#53) a `[style.display]`/`[style.visibility]` ternary of
    literals whose one arm hides: it restricts its keys on its showing side, and `style_hides` records every style gate.
    Helpers: `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '../TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

$script:TsStyleGatesHtml = @(
    '<em [style.color]="kind === K.Alpha && ''red''">{{ ''demo.paint.color'' | translate }}</em>'
    '<span [class.gone-x]="kind === K.Beta">{{ ''demo.paint.gone'' | translate }}</span>'
    '<div [style.display]="shown ? ''block'' : ''none''">a</div>'
    '<div [style.height.px]="h">b</div>'
    '<div [style.opacity]="o">c</div>'
    '<span [style.display]="kind === K.Alpha ? ''none'' : ''block''">{{ ''demo.paint.none'' | translate }}</span>'
    '<i [style.visibility]="kind === K.Gamma ? ''hidden'' : null">{{ ''demo.paint.hidden'' | translate }}</i>'
    '<b [style.display]="''none''">{{ ''demo.paint.never'' | translate }}</b>'
    '<b *ngIf="false">{{ ''demo.paint.offif'' | translate }}</b>'
    '<p [ngStyle]="{ display: kind === K.Beta ? ''none'' : ''block'', ''visibility'': ''hidden'', ''height.px'': h }">n</p>'
    '<p [ngStyle]="styles">m</p>'
) -join "`n"

function New-TsStyleGatesDb {
    return New-TsRowsDb (New-TsRowsWorkspace @{
        'apps/shop/src/kinds.ts' = "export enum DemoIDs { Alpha = 1, Beta = 2, Gamma = 3 }`n"
        'apps/shop/src/paint.component.ts' = "import { Component, Input } from '@angular/core';`n" +
            "import { DemoIDs } from './kinds';`n" +
            "@Component({ selector: 'demo-paint', standalone: true, templateUrl: './paint.component.html', " +
            "styleUrls: ['./paint.component.scss'] })`n" +
            "export class PaintComponent { K = DemoIDs; shown = true; h = 0; o = 1; styles = {}; @Input() kind!: DemoIDs; }`n"
        'apps/shop/src/paint.component.html' = $script:TsStyleGatesHtml
        'apps/shop/src/paint.component.scss' = '.gone-x { display: none; }'
        'apps/shop/src/assets/locales/en.json' = '{"demo":{"paint":{"color":"1","gone":"2","none":"3","hidden":"4","never":"5","offif":"6"}}}'
    }) -Shared
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
    Assert-Line $v 'v=demo.paint.gone | kind not_in ["Beta"]'
    # B2: a ternary of literals whose one arm hides is read on its showing side.
    Assert-Line $v 'v=demo.paint.none | kind not_in ["Alpha"]'
    Assert-Line $v 'v=demo.paint.hidden | kind not_in ["Gamma"]'
    Assert-NoLine $v 'v=demo.paint.color |'
    Assert-Line (Invoke-TsStyleGatesQ $made.Db "SELECT 'k=' || key || '#' AS r FROM key_reach WHERE key LIKE 'demo.paint.%'") 'k=demo.paint.color#'
    $g = Invoke-TsStyleGatesQ $made.Db "SELECT 'style=' || g.gate_kind || '#' AS r FROM gates g WHERE g.gate_kind = 'style'"
    Assert-Line $g 'style=style#'
    $n = Invoke-TsStyleGatesQ $made.Db "SELECT 'n=' || COUNT(*) || '#' AS r FROM gate_values v JOIN gates g ON g.id = v.gate WHERE g.gate_kind = 'style' AND g.name = 'color'"
    Assert-Line $n 'n=0#'
}


Test-Case 'tsrows: style_hides says which style gates hide their element and on which side (#93 A)' {
    $made = New-TsStyleGatesDb
    Assert-Exit $made.Result 0
    $r = Invoke-TsStyleGatesQ $made.Db "SELECT 'sh=' || property || '|' || coalesce(unit, '') || '|' || condition || '|' || alone || '|' || hides || '|' || coalesce([when], '') AS r FROM style_hides WHERE via = 'style' ORDER BY id"
    Assert-Line $r 'sh=color||kind === K.Alpha && ''red''|0|0|'
    Assert-Line $r 'sh=display||shown ? ''block'' : ''none''|1|1|else'
    Assert-Line $r 'sh=height|px|h|0|0|'
    Assert-Line $r 'sh=opacity||o|0|0|'
    Assert-Line $r 'sh=display||kind === K.Alpha ? ''none'' : ''block''|1|1|then'
    Assert-Line $r 'sh=visibility||kind === K.Gamma ? ''hidden'' : null|1|1|then'
    Assert-Line $r 'sh=display||''none''|1|1|always'
}


# (a) ANGULAR'S UNIT IS KEPT (#54): `[style.height.px]` is a gate named `height` that carries `unit = px`, and
# `style_hides.unit` is read from it; a binding with no unit has none.
Test-Case 'tsrows: a style gate keeps its unit, and style_hides reads it (#54)' {
    $made = New-TsStyleGatesDb
    Assert-Exit $made.Result 0
    $g = Invoke-TsStyleGatesQ $made.Db "SELECT 'gu=' || name || '|' || coalesce(unit, '') || '#' AS r FROM gates WHERE gate_kind = 'style'"
    Assert-Line $g 'gu=height|px#'
    Assert-Line $g 'gu=opacity|#'
    $r = Invoke-TsStyleGatesQ $made.Db "SELECT 'su=' || property || '|' || coalesce(unit, '') || '|' || condition AS r FROM style_hides WHERE via = 'style'"
    Assert-Line $r 'su=height|px|h'
}

# (b) A LITERAL `[ngStyle]` MAP IS ONE `style_hides` ROW PER KEY (#54), each value read as a `[style.x]` value is; no gate.
# A value that is no object literal gives no row.
Test-Case 'tsrows: style_hides has a row per literal ngStyle key, none for a non-literal value (#54)' {
    $made = New-TsStyleGatesDb
    Assert-Exit $made.Result 0
    $r = Invoke-TsStyleGatesQ $made.Db ("SELECT 'ns=' || s.via || '|' || s.property || '|' || coalesce(s.unit, '') || '|' || s.alone || '|' || " +
        "s.hides || '|' || coalesce(s.[when], '') || '|' || coalesce(s.gate, '-') || '|' || e.name || '#' AS r " +
        "FROM style_hides s JOIN expressions e ON e.id = s.expression WHERE s.via = 'ngStyle' ORDER BY s.id")
    Assert-Line $r 'ns=ngStyle|display||1|1|then|-|ngStyle#'
    Assert-Line $r 'ns=ngStyle|visibility||1|1|always|-|ngStyle#'
    Assert-Line $r 'ns=ngStyle|height|px|0|0||-|ngStyle#'
    $n = Invoke-TsStyleGatesQ $made.Db "SELECT 'nn=' || COUNT(*) || '#' AS r FROM style_hides WHERE via = 'ngStyle'"
    Assert-Line $n 'nn=3#'
    $none = Invoke-TsStyleGatesQ $made.Db ("SELECT 'nl=' || COUNT(*) || '#' AS r FROM style_hides s JOIN expressions e ON e.id = s.expression " +
        "WHERE e.source = 'styles'")
    Assert-Line $none 'nl=0#'
    $ng = Invoke-TsStyleGatesQ $made.Db "SELECT 'nx=' || COUNT(*) || '#' AS r FROM expressions WHERE name = 'ngStyle'"
    Assert-Line $ng 'nx=2#'
}

# (c) A STYLE BINDING NEVER SHOWN IS `*ngIf="false"` (#54): `[style.display]="'none'"` hides its element on every side, so the
# key under it renders on no way - the same `key_reach` row as a key under a literal false in the same template.
Test-Case 'tsrows: a key under a style that always hides renders on no way, as under *ngIf=false (#54)' {
    $made = New-TsStyleGatesDb
    Assert-Exit $made.Result 0
    # Every scalar of the row, and how many nodes each list holds: gate ids differ, the shape of the answer must not.
    $lists = @(@('roots', 'routes', 'modules', 'components', 'always_gates', 'maybe_gates', 'always_branches',
        'maybe_branches', 'always_values', 'unreadable_values') |
        ForEach-Object { "(SELECT COUNT(*) FROM key_reach__$_ c WHERE c.owner = '#' || k.rowid)" }) +
        "(SELECT COUNT(*) FROM key_reach_ways w WHERE w.key = k.key)"
    $cols = "k.n_paths || '|' || coalesce(k.min_depth, '') || '|' || coalesce(k.route, '') || '|' || k.n_refs || '|' || " +
        ($lists -join " || '|' || ")
    $r = Invoke-TsStyleGatesQ $made.Db "SELECT 'kr=' || k.key || '|' || $cols AS r FROM key_reach k WHERE k.key IN ('demo.paint.never', 'demo.paint.offif')"
    Assert-Line $r 'kr=demo.paint.never|0|'
    Assert-Line $r 'kr=demo.paint.offif|0|'
    $agree = Invoke-TsStyleGatesQ $made.Db "SELECT 'agree=' || COUNT(DISTINCT $cols) || '#' AS r FROM key_reach k WHERE k.key IN ('demo.paint.never', 'demo.paint.offif')"
    Assert-Line $agree 'agree=1#'
}

}
