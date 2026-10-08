<#
    The Angular half's stylesheets: `stylesheets`, `style_rules` - one row per declaration under its
    RESOLVED selector, SCSS nesting and the wrapping `@media` included - and `class_hides`, which joins a class
    binding to the rules that hide its class. See `rust/fbtcore/src/cssmap/`. Its helpers are `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '../TsRows.Helpers.ps1')

# Line numbers matter: the cases assert them. A `$` here is SCSS, so every line is single-quoted.
$script:TsStylesLegendScss = @(
    '// a line comment, which plain CSS has no grammar for'
    '$gap: 4px;'
    '.legend-item, .chip {'
    '  margin: $gap;'
    '  &--hidden { display: none !important; }'
    '  .label { color: red; }'
    '}'
    '@media (max-width: 768px) {'
    '  .hide-mobile { display: none; }'
    '}'
    '@include respond(sm) { .item-hidden { visibility: hidden; } }'
    '@mixin hidden { .never { display: none; } }'
    '.bold.gone { display: none; }'
) -join "`n"

# A component whose stylesheet hides what its class bindings switch on, a global sheet no component names, and
# ANOTHER component's sheet hiding one of the same classes - the three scopes `class_hides` tells apart.
function New-TsStylesWorkspace([hashtable]$More = @{}) {
    $files = @{
        'apps/shop/src/legend.component.ts' = "import { Component } from '@angular/core';`n" +
            "@Component({ selector: 'app-legend', templateUrl: './legend.component.html', " +
            "styleUrls: ['./legend.component.scss'], standalone: true })`n" +
            "export class LegendComponent { off = false; small = false; shown = true; }`n"
        'apps/shop/src/legend.component.html' = "<div class=`"legend-item`" [class.legend-item--hidden]=`"off`">a</div>`n" +
            "<span [class.not-visible]=`"!shown`" [class.bold]=`"small`">b</span>`n" +
            "<p [class.hide-mobile]=`"small`" [ngClass]=`"{'hide-mobile': small}`">c</p>`n" +
            "<i [class.never]=`"off`">d</i>`n"
        'apps/shop/src/legend.component.scss' = $script:TsStylesLegendScss
        'apps/shop/src/styles.scss' = ".not-visible { visibility: hidden; }`n"
        'apps/shop/src/basket.scss' = ".basket { color: blue; }`n.not-visible { display: none; }`n"
    }
    foreach ($key in $More.Keys) { $files[$key] = $More[$key] }
    return New-TsRowsWorkspace $files
}

# The lens at FULL width: a row here is a path and a selector, past the column a terminal would cut it at.
function Invoke-TsStylesQ([string]$Path, [string]$Sql) {
    $result = Invoke-Gate --map-query $Path --width 0 --sql $Sql
    Assert-Exit $result 0
    return $result
}

$script:TsStylesRules ="SELECT 'sr=' || r.line || '|' || r.rule_line || '|' || r.selector || '|' || r.resolved || '|' || " +
    "coalesce(r.media, '-') || '|' || coalesce(r.context, '-') || '|' || r.property || '|' || r.value || '|' || " +
    "r.important || r.hides || r.live || '#' FROM style_rules r JOIN files f ON f.id = r.file " +
    "WHERE f.path = 'apps/shop/src/legend.component.scss'"

if ($script:TsRowsModules -and $script:TsRowsPython) {

Test-Case 'tsrows: a stylesheet is a row per declaration, its SCSS nesting resolved and its @media kept' {
    $made = New-TsRowsDb (New-TsStylesWorkspace)
    Assert-Exit $made.Result 0
    $r = Invoke-TsStylesQ $made.Db $script:TsStylesRules
    Assert-Line $r 'sr=4|3|.legend-item|.legend-item|-|-|margin|$gap|001#'
    Assert-Line $r 'sr=4|3|.chip|.chip|-|-|margin|$gap|001#'
    Assert-Line $r 'sr=5|5|&--hidden|.legend-item--hidden|-|-|display|none|111#'
    Assert-Line $r 'sr=5|5|&--hidden|.chip--hidden|-|-|display|none|111#'
    Assert-Line $r 'sr=6|6|.label|.legend-item .label|-|-|color|red|001#'
    Assert-Line $r 'sr=9|9|.hide-mobile|.hide-mobile|(max-width: 768px)|-|display|none|011#'
    Assert-Line $r 'sr=11|11|.item-hidden|.item-hidden|-|@include respond(sm)|visibility|hidden|011#'
    Assert-Line $r 'sr=12|12|.never|.never|-|@mixin hidden|display|none|010#'
    Assert-Line $r 'sr=13|13|.bold.gone|.bold.gone|-|-|display|none|011#'
    Assert-Line (Invoke-TsStylesQ $made.Db ("SELECT 'n=' || count(*) || '#' FROM style_rules r JOIN files f ON f.id = r.file " +
        "WHERE f.path = 'apps/shop/src/legend.component.scss'")) 'n=10#'
    $s = Invoke-TsStylesQ $made.Db ("SELECT 'subject=' || r.subject || '|' || r.classes || '|' || r.on_element || '#' " +
        "FROM style_rules r WHERE r.resolved = '.legend-item .label'")
    Assert-Line $s 'subject=.label|["label"]|1#'
}

Test-Case 'tsrows: a class binding is joined to the rules that hide its class, with where each one applies' {
    $made = New-TsRowsDb (New-TsStylesWorkspace)
    Assert-Exit $made.Result 0
    $r = Invoke-TsStylesQ $made.Db ("SELECT 'ch=' || h.class || '|' || h.via || '|' || coalesce(g.source, h.condition) || '|' || " +
        "h.scope || '|' || f.path || '|' || h.resolved || '|' || coalesce(h.media, '-') || '|' || h.property || '|' || " +
        "h.value || '|' || h.important || h.bare || '#' FROM class_hides h JOIN files f ON f.id = h.file " +
        "LEFT JOIN gates g ON g.id = h.gate")
    Assert-Line $r 'ch=legend-item--hidden|class|off|own|apps/shop/src/legend.component.scss|.legend-item--hidden|-|display|none|11#'
    Assert-Line $r 'ch=not-visible|class|!shown|shared|apps/shop/src/styles.scss|.not-visible|-|visibility|hidden|01#'
    Assert-Line $r 'ch=not-visible|class|!shown|other|apps/shop/src/basket.scss|.not-visible|-|display|none|01#'
    Assert-Line $r 'ch=hide-mobile|class|small|own|apps/shop/src/legend.component.scss|.hide-mobile|(max-width: 768px)|display|none|01#'
    Assert-Line $r "ch=hide-mobile|ngClass|{'hide-mobile': small}|own|apps/shop/src/legend.component.scss|.hide-mobile|(max-width: 768px)|display|none|01#"
    Assert-Line $r 'ch=bold|class|small|own|apps/shop/src/legend.component.scss|.bold.gone|-|display|none|00#'
    # A MIXIN'S RULE IS NOT EMITTED WHERE IT IS WRITTEN, so it hides nothing there.
    Assert-NoLine $r 'ch=never'
    # The gate column IS a gate: every class row joins one, on the node the binding sits on.
    Assert-Line (Invoke-TsStylesQ $made.Db ("SELECT 'unjoined=' || count(*) || '#' FROM class_hides h LEFT JOIN gates g " +
        "ON g.id = h.gate AND g.node = h.node WHERE h.via = 'class' AND g.id IS NULL")) 'unjoined=0#'
}

Test-Case 'tsrows: a stylesheet the parser cannot read is a diagnostics row and has no rules' {
    $made = New-TsRowsDb (New-TsStylesWorkspace @{ 'apps/shop/src/bad.css' = ".ok { display: none }`n.bad { color: red; ]`n" })
    Assert-Exit $made.Result 0
    Assert-Line $made.Result 'could not parse 1 stylesheet(s), first apps/shop/src/bad.css:2'
    $s = Invoke-TsStylesQ $made.Db ("SELECT 'ss=' || f.path || '|' || s.syntax || '|' || coalesce(s.error_line, '-') || '|' || " +
        "s.declarations || '|' || (s.error IS NOT NULL) || '#' FROM stylesheets s JOIN files f ON f.id = s.file")
    Assert-Line $s 'ss=apps/shop/src/bad.css|css|2|0|1#'
    Assert-Line $s 'ss=apps/shop/src/legend.component.scss|scss|-|10|0#'
    $d = Invoke-TsStylesQ $made.Db "SELECT 'dg=' || kind || '|' || path || '|' || line || '#' FROM diagnostics WHERE kind = 'stylesheet_unparsed'"
    Assert-Line $d 'dg=stylesheet_unparsed|apps/shop/src/bad.css|2#'
    Assert-Line (Invoke-TsStylesQ $made.Db ("SELECT 'bad=' || count(*) || '#' FROM style_rules r JOIN files f ON f.id = r.file " +
        "WHERE f.path = 'apps/shop/src/bad.css'")) 'bad=0#'
}

# THE PARTIAL PATH WRITES DIFFERENTLY FROM THE FULL ONE: an edited stylesheet is read again, and the rows of the
# last run are replaced rather than kept beside the new ones.
Test-Case 'tsrows: an edited stylesheet is read again on a partial run, its old rows replaced' {
    $tree = New-TsStylesWorkspace @{ 'apps/shop/src/bad.css' = ".bad { color: red; ]`n" }
    Assert-Exit (New-TsRowsDb $tree).Result 0
    # Line 13 has no newline after it, so this one blank line puts the new rule on 15.
    [IO.File]::AppendAllText((Join-Path $tree 'apps/shop/src/legend.component.scss'), "`n`n.late-hidden { display: none; }`n")
    $again = New-TsRowsDb $tree
    Assert-Exit $again.Result 0
    Assert-Line $again.Result 'put back'
    $r = Invoke-TsStylesQ $again.Db $script:TsStylesRules
    Assert-Line $r 'sr=15|15|.late-hidden|.late-hidden|-|-|display|none|011#'
    Assert-Line $r 'sr=5|5|&--hidden|.legend-item--hidden|-|-|display|none|111#'
    Assert-Line (Invoke-TsStylesQ $again.Db ("SELECT 'n=' || count(*) || '|' || count(DISTINCT r.id) || '#' FROM style_rules r " +
        "JOIN files f ON f.id = r.file WHERE f.path = 'apps/shop/src/legend.component.scss'")) 'n=11|11#'
    Assert-Line (Invoke-TsStylesQ $again.Db "SELECT 'sheets=' || count(*) || '#' FROM stylesheets") 'sheets=5#'
    Assert-Line (Invoke-TsStylesQ $again.Db ("SELECT 'dg=' || count(*) || '#' FROM diagnostics " +
        "WHERE kind = 'stylesheet_unparsed'")) 'dg=1#'
}

}
