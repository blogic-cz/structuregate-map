<#
    A rule that hides its element as a WHOLE (#89, `rust/fbtcore/src/cssmap/collapse.rs`): `height: 0` with `overflow: hidden`,
    `opacity: 0` with `pointer-events: none`. `style_rules.hides` lands on the zero, `class_hides` joins the class bindings to
    it, and a class gate on such a class restricts its key as a `display: none` one does. A zero alone, or written as a
    `$variable`, hides nothing. The finer combinations are the rust unit tests in `cssmap/tests.rs`. Helpers: `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '../TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

# A `$` here is SCSS, so every line is single-quoted.
$script:TsCollapseScss = @(
    '$h: 0;'
    '.collapsed-x { height: 0; overflow: hidden; }'
    '.faded-x { opacity: 0; pointer-events: none; }'
    '.short-x { height: 0; }'
    '.ghost-x { opacity: 0; }'
    '.var-x { height: $h; overflow: hidden; }'
) -join "`n"

$script:TsCollapseHtml = @(
    '<div [class.collapsed-x]="!open">a</div>'
    '<div [class.faded-x]="!open">b</div>'
    '<div [class.short-x]="!open">c</div>'
    '<div [class.ghost-x]="!open">d</div>'
    '<div [class.var-x]="!open">e</div>'
    '<span [class.collapsed-x]="kind === K.Alpha">{{ ''demo.fold.hid'' | translate }}</span>'
    '<b [class.short-x]="kind === K.Beta">{{ ''demo.fold.open'' | translate }}</b>'
) -join "`n"

function New-TsCollapseDb {
    return New-TsRowsDb (New-TsRowsWorkspace @{
        'apps/shop/src/kinds.ts' = "export enum DemoIDs { Alpha = 1, Beta = 2, Gamma = 3 }`n"
        'apps/shop/src/fold.component.ts' = "import { Component, Input } from '@angular/core';`n" +
            "import { DemoIDs } from './kinds';`n" +
            "@Component({ selector: 'demo-fold', standalone: true, templateUrl: './fold.component.html', " +
            "styleUrls: ['./fold.component.scss'] })`n" +
            "export class FoldComponent { K = DemoIDs; open = false; @Input() kind!: DemoIDs; }`n"
        'apps/shop/src/fold.component.html' = $script:TsCollapseHtml
        'apps/shop/src/fold.component.scss' = $script:TsCollapseScss
        'apps/shop/src/assets/locales/en.json' = '{"demo":{"fold":{"hid":"1","open":"2"}}}'
    }) -Shared
}

# The lens at FULL width: a row here carries a selector and a condition past the column a terminal would cut it at.
function Invoke-TsCollapseQ([string]$Path, [string]$Sql) {
    $result = Invoke-Gate --map-query $Path --width 0 --sql $Sql
    Assert-Exit $result 0
    return $result
}

Test-Case 'tsrows: a zero-height or a transparent-inert rule hides as a whole, and its class binding is a class_hides row (#89)' {
    $made = New-TsCollapseDb
    Assert-Exit $made.Result 0
    $r = Invoke-TsCollapseQ $made.Db ("SELECT 'sr=' || r.selector || '|' || r.property || '|' || r.value || '|' || r.hides || '#' AS r " +
        "FROM style_rules r JOIN files f ON f.id = r.file WHERE f.path = 'apps/shop/src/fold.component.scss'")
    # The ZERO is the deciding declaration; its partner is the condition and stays `hides = 0`.
    Assert-Line $r 'sr=.collapsed-x|height|0|1#'
    Assert-Line $r 'sr=.collapsed-x|overflow|hidden|0#'
    Assert-Line $r 'sr=.faded-x|opacity|0|1#'
    Assert-Line $r 'sr=.faded-x|pointer-events|none|0#'
    # A zero without its partner leaves the content (or the clicks) there, and a variable is not a literal.
    Assert-Line $r 'sr=.short-x|height|0|0#'
    Assert-Line $r 'sr=.ghost-x|opacity|0|0#'
    Assert-Line $r 'sr=.var-x|height|$h|0#'
    Assert-Line $r 'sr=.var-x|overflow|hidden|0#'
    $h = Invoke-TsCollapseQ $made.Db ("SELECT 'ch=' || h.class || '|' || h.condition || '|' || h.property || '|' || h.value || '|' || " +
        "h.scope || '|' || h.bare || '#' AS r FROM class_hides h")
    Assert-Line $h 'ch=collapsed-x|!open|height|0|own|1#'
    Assert-Line $h 'ch=faded-x|!open|opacity|0|own|1#'
    Assert-Line $h 'ch=collapsed-x|kind === K.Alpha|height|0|own|1#'
    foreach ($gone in @('ch=short-x', 'ch=ghost-x', 'ch=var-x', '|overflow|', '|pointer-events|')) { Assert-NoLine $h $gone }
}

# THE CLOSURE READS IT: a class gate whose class collapses its element holds on the FALSE side (#30), so the key inside
# it is restricted; one whose class is only `height: 0` restricts nothing.
Test-Case 'tsrows: a key behind a collapsing class gate is restricted in key_reach (#89)' {
    $made = New-TsCollapseDb
    Assert-Exit $made.Result 0
    $v = Invoke-TsCollapseQ $made.Db ("SELECT 'v=' || k.key || ' | ' || $(Get-TsFieldSql 'key_reach__always_values' 'v' 'dimension') || ' ' || " +
        "$(Get-TsFieldSql 'key_reach__always_values' 'v' 'op') || ' ' || $(Get-NodeListSql 'key_reach__always_values' 'v' 'values') AS r FROM key_reach k " +
        "JOIN key_reach__always_values v ON v.owner = '#' || k.rowid AND v.parent = 0 WHERE k.key LIKE 'demo.fold.%'")
    Assert-Line $v 'v=demo.fold.hid | kind not_in ["Alpha"]'
    Assert-NoLine $v 'v=demo.fold.open |'
    Assert-Line (Invoke-TsCollapseQ $made.Db "SELECT 'k=' || key || '#' AS r FROM key_reach WHERE key LIKE 'demo.fold.%'") 'k=demo.fold.open#'
}

}
