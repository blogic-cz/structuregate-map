<#
    THE ANGULAR HALF'S ENTITY KEYS (#130, `rust/fbtcore/src/rows/keys/`): a TypeScript declaration has no symbol, so its key is its file's
    path from the root and its name - a member under its class's, an enum member under its enum's, a component's row under `ng:` - and a
    fresh map of the workspace with one more file keeps every key. Its helpers are `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '../TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

Test-Case 'tsentitykeys: a TypeScript declaration is keyed by its path and name, the same after a fresh map (#130)' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/cart.service.ts' = "import { Injectable } from '@angular/core';`nexport enum Hue { Red = 1, Blue = 2 }`n" +
            "export interface Line { qty: number; }`nexport function total(lines: Line[]) { return lines.length; }`n" +
            "@Injectable({ providedIn: 'root' })`nexport class CartService {`n  count = 0;`n  load(): number { return [1, 2].map((x) => x).length; }`n}`n"
    }
    $db = Join-Path $tree 'map.sqlite'
    $gateArgs = @('--root', $tree, '--map-sqlite', $db, '--ts-node-modules', $script:TsRowsModules)
    Assert-Exit (Invoke-Gate @gateArgs) 0
    $sql = "SELECT 'k=' || key || '|' || tbl || '|' || id AS k FROM entity_keys WHERE key LIKE '%cart.service.ts%' ORDER BY key, tbl"
    $first = Invoke-Gate --map-query $db --width 0 --limit 0 --sql $sql
    Assert-Line $first 'k=ts:apps/shop/src/cart.service.ts#CartService|classes|'
    Assert-Line $first 'k=ts:apps/shop/src/cart.service.ts#CartService.load|members|'
    Assert-Line $first 'k=ts:apps/shop/src/cart.service.ts#CartService.count|members|'
    Assert-Line $first 'k=ts:apps/shop/src/cart.service.ts#Hue.Red|enums__members|'
    Assert-Line $first 'k=ts:apps/shop/src/cart.service.ts#Line|interfaces|'
    Assert-Line $first 'k=ts:apps/shop/src/cart.service.ts#total|functions|'
    Assert-Line $first 'k=ng:apps/shop/src/cart.service.ts#CartService|injectables|'
    Assert-Line $first 'k=file:apps/shop/src/cart.service.ts|files|'
    # A CALLBACK HAS NO NAME OF ITS OWN, so no key.
    Assert-NoLine $first '=>'
    # A FILE SORTING FIRST, then a fresh map: every key comes back.
    Set-Content -LiteralPath (Join-Path $tree 'apps/shop/src/a-first.ts') -Value "export const first = 1;`n" -NoNewline
    Remove-Item -LiteralPath $db -Force
    Assert-Exit (Invoke-Gate @gateArgs) 0
    $second = Invoke-Gate --map-query $db --width 0 --limit 0 --sql $sql
    $keys = { param($r) @($r.Lines | Where-Object { $_.Trim().StartsWith('k=') } | ForEach-Object { ($_.Trim() -split '\|')[0..1] -join '|' }) }
    $a = & $keys $first
    $b = & $keys $second
    Assert-Equal ($b -join "`n") ($a -join "`n") 'the keys of a fresh map'
}

}
