<#
    THE SOURCE TEXT OF A FILE THE TREE LOST GOES WITH ITS ROWS (#142, `search` in `rust/fbtcore/src/rows/store.rs`): the Angular
    half replaces its rows whole and never reaches the per-file drop that clears `file_text`, so a deleted component stayed
    searchable. Its helpers are `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '../../TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

Test-Case 'gonetext: a component deleted from the workspace leaves no file_text row and no --text hit' {
    $component = "import { Component } from '@angular/core';`n" +
        "@Component({ selector: 'app-beta', template: '<p>beta-only-phrase</p>', standalone: true })`nexport class BetaComponent {}`n"
    $tree = New-TsRowsWorkspace @{ 'apps/shop/src/beta.component.ts' = $component }
    $db = Join-Path (Use-Tree @{ 'keep.txt' = "demo`n" }) 'map.sqlite'
    $map = @('--root', $tree, '--map-sqlite', $db, '--ts-node-modules', $script:TsRowsModules)
    Assert-Exit (Invoke-Gate $map) 0
    Assert-Line (Invoke-Gate --map-query $db --text 'beta-only-phrase' --limit 0) 'beta.component.ts'
    Remove-Item -LiteralPath (Join-Path $tree 'apps/shop/src/beta.component.ts')
    Assert-Exit (Invoke-Gate $map) 0
    $orphans = Invoke-TsRowsQ $db "SELECT 'orphans=' || count(*) AS n FROM file_text WHERE path NOT IN (SELECT path FROM files)"
    Assert-Line $orphans 'orphans=0'
    Assert-NoLine (Invoke-Gate --map-query $db --text 'beta-only-phrase' --limit 0) 'beta.component.ts'
}

}
