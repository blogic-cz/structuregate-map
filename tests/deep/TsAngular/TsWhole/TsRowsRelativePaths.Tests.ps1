<#
    A MAP NAMES NO MACHINE (rows 132, divergence #120, `TsPaths.mjs::relativeCells`): every path a row's structure holds - a ref's
    `file` and `also`, a string under the root - is relative to the workspace root, as `files.path` is, so two checkouts of one tree
    write one map. `files.abs` keeps the absolute path by name. Its helpers are `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '../../TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

$script:TsRelFiles = @{
    'apps/shop/src/order.ts' = "import { Item } from './cart';`n" +
        "export interface Order { first: Item; }`n" +
        "export function total(order: Order): number { return order.first.id; }`n" +
        "export const sum = total({ first: { id: 1 } });`n"
}

Test-Case 'tsrows: no structure holds an absolute path, and a ref''s file is its files.path' {
    $made = New-TsRowsDb (New-TsRowsWorkspace $script:TsRelFiles)
    Assert-Exit $made.Result 0
    $listed = Invoke-TsRowsQ $made.Db "SELECT 'node=' || name AS r FROM sqlite_master WHERE type = 'table' AND name LIKE '%\_\_%' ESCAPE '\' AND name NOT LIKE 'file\_text%' ESCAPE '\'"
    $tables = @($listed.Lines | ForEach-Object { $_.Trim() } | Where-Object { $_.StartsWith('node=') } | ForEach-Object { $_.Substring(5) })
    Assert-Equal ($tables.Count -gt 20) $true
    $union = ($tables | ForEach-Object { "SELECT '$_' AS t, value FROM `"$_`"" }) -join ' UNION ALL '
    $leaks = Invoke-TsRowsQ $made.Db ("SELECT 'abs=' || count(*) AS r FROM ($union) WHERE value IN (SELECT abs FROM files)")
    Assert-Line $leaks 'abs=0'
    # The member's type names the declaring file as `files.path` spells it.
    $ref = Invoke-TsRowsQ $made.Db ("SELECT 'ref=' || n.value AS r FROM type_members m JOIN type_members__type_ref n ON n.owner = m.id AND n.parent = 0 " +
        "AND n.role = 'file' WHERE m.name = 'first'")
    Assert-Line $ref 'ref=apps/shop/src/cart.ts'
}

}
