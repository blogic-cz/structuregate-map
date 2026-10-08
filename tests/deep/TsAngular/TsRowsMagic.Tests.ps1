<#
    The Angular half's literals for `--magic`: `number_literals`, and `use`/`callee`/`target` on its
    `string_literals` beside the `context` (the parent's SyntaxKind) it already wrote - see
    `src/TsRows/TsPlain/TsLiterals.mjs`. Its helpers are `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '../TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

Test-Case 'tsrows: the Angular half writes what a literal is doing, and --magic reads it' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/parse.ts' = "export const LIMIT = 40;`nexport function cell(row: string, mode: string) {`n" +
            "  if (mode === 'strict') return row.split(';')[3];`n  return row.length * 12;`n}`n"
    }
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    $n = Invoke-TsRowsQ $made.Db ("SELECT 'n=' || value || '|' || use || '|' || target || '|' || half FROM number_literals " +
        "WHERE value IN ('40', '3', '12') ORDER BY line, value")
    Assert-Line $n 'n=40|declared||typescript'
    Assert-Line $n 'n=3|index|row.split('';'')|typescript'
    Assert-Line $n 'n=12|arith||typescript'
    $s = Invoke-TsRowsQ $made.Db "SELECT 's=' || value || '|' || use || '|' || callee || '|' || context FROM string_literals WHERE value = ';'"
    Assert-Line $s 's=;|argument|split|CallExpression'
    $magic = Invoke-Gate --map-query $made.Db --magic --width 0
    Assert-Exit $magic 0
    Assert-Line $magic 'strict'
}

Test-Case 'tsrows: a body''s calls, locals and literals carry `file`, so the join every half answers finds them' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/parse.ts' = "export function cell(row: string) {`n  const parts = row.split(';');`n  return parts.length;`n}`n"
    }
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    foreach ($table in 'calls', 'locals', 'returns', 'string_literals') {
        $joined = Invoke-TsRowsQ $made.Db ("SELECT 'j=' || count(*) FROM $table t JOIN files f ON f.id = t.file " +
            "WHERE f.path LIKE '%parse.ts' AND t.file = t.owner_file")
        Assert-NoLine $joined 'j=0'
        Assert-Line $joined 'j='
    }
}

}
