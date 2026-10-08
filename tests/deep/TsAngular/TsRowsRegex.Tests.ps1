<#
    The Angular half's `regexes` rows: a regex literal and a `RegExp` construction, each with its pattern
    when a literal gives it and what receives it. See `regexOf` in `src/TsRows/TsText.mjs`. Its helpers are
    `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '../TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

Test-Case 'tsrows: a regex literal and a RegExp construction are rows of the Angular half' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/words.ts' = "export const WORDS = /\s+/g;`nexport const DIGITS = new RegExp('^\\d+$', 'i');`n" +
            "export const parts = (s: string) => s.split(/,/);`n"
    }
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    $r = Invoke-TsRowsQ $made.Db ("SELECT 'rx=' || kind || '|' || api || '|' || pattern || '|' || flags || '|' || used_by " +
        "|| '|' || half FROM regexes ORDER BY line")
    Assert-Line $r 'rx=literal||\s+|g|= WORDS|typescript'
    Assert-Line $r 'rx=call|RegExp|^\d+$|i|= DIGITS|typescript'
    Assert-Line $r 'rx=literal||,||.split|typescript'
}

}
