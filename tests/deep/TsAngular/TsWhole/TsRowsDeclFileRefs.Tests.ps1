<#
    A TREE'S OWN `.d.ts` IS WALKED (`TsSetup/TsProgram.mjs::projectSourceFiles`): a global augmentation it declares
    (`interface Array<T> { first(): T }`) is a `type_members` row, so a call to it has the `target_id` its place names, as a
    call to any in-tree declaration does (rows 81). A library's `.d.ts` stays unwalked. Its helpers are `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '../../TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

$script:TsDeclFiles = @{
    'apps/shop/src/typings.d.ts' = "interface Array<T> {`n" +
        "  first(): T;`n" +
        "}`n" +
        "interface Date {`n" +
        "  addDays(days: number): Date;`n" +
        "}`n"
    'apps/shop/src/use-typings.ts' = "export const head = [1, 2].first();`n" +
        "export const later = new Date().addDays(1);`n"
}

Test-Case 'tsrows: a call to a member an in-tree .d.ts declares has the target_id of that member''s row' {
    $made = New-TsRowsDb (New-TsRowsWorkspace $script:TsDeclFiles)
    Assert-Exit $made.Result 0
    $rows = Invoke-TsRowsQ $made.Db ("SELECT 'call=' || c.method || ':' || coalesce(f.path, '-') || ':' || coalesce(m.line, '-') AS r " +
        "FROM calls c JOIN files cf ON cf.id = c.file LEFT JOIN type_members m ON m.id = c.target_id " +
        "LEFT JOIN files f ON f.id = m.file WHERE cf.path LIKE '%use-typings.ts' ORDER BY c.method")
    Assert-Line $rows 'call=addDays:apps/shop/src/typings.d.ts:5'
    Assert-Line $rows 'call=first:apps/shop/src/typings.d.ts:2'
    # Walked, so no longer noted as left out.
    $notes = Invoke-TsRowsQ $made.Db "SELECT 'note=' || file AS r FROM diagnostics WHERE kind = 'declaration_file_not_walked'"
    Assert-NoLine $notes 'note=apps/shop/src/typings.d.ts'
}

}
