<#
    What a route links to beyond its own fields: the translation keys its path is built from (`route_keys`,
    `src/TsRows/TsDerive/TsRouteKeys.mjs`). Its helpers are `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '../../TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

# A ROUTE PATH BUILT FROM TRANSLATION KEYS NAMES EACH KEY AS A ROW (#74). A translating router resolves
# `{{routes.order.detail}}` at run time; with only the path text, a consumer joined a route to the code testing the
# same key by parsing `{{ }}` out of a string. A key no locale defines keeps its row, `translation` NULL - and a key
# a route names is REFERENCED: no call or literal spells it alone, so it read as an unused translation.
Test-Case 'tsrows: each translation key of a route path is a route_keys row joined to its translation' {
    $tree = New-TsRowsWorkspace @{
        'structuregate.ts.json' = '{"locales":["apps/shop/src/assets/locales"]}'
        'apps/shop/src/assets/locales/en.json' = '{"routes":{"order":{"detail":"order"}}}'
        'apps/shop/src/order.routes.ts' = "import { Component, NgModule } from '@angular/core';`n" +
            "import { RouterModule } from '@angular/router';`n" +
            "@Component({ selector: 'demo-order-detail', template: '<p>order</p>' })`nexport class OrderDetailComponent {}`n" +
            "@NgModule({ declarations: [OrderDetailComponent], imports: [RouterModule.forRoot([`n" +
            "  { path: '{{routes.order.detail}}/:id', component: OrderDetailComponent },`n" +
            "  { path: 'list/{{ routes.order.missing }}', component: OrderDetailComponent },`n" +
            "])] })`nexport class OrderModule {}`n"
    }
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    $keys = Invoke-Gate --map-query $made.Db --width 0 --sql ("SELECT k.key || '|' || k.ord || '|' || " +
        "CASE WHEN t.key = k.key THEN 'defined' ELSE coalesce(k.translation, 'NULL') END || '|' || r.path || '|' || " +
        "(k.owner_file = r.owner_file) AS r FROM route_keys k JOIN routes r ON r.id = k.route " +
        "LEFT JOIN translations t ON t.id = k.translation")
    Assert-Exit $keys 0
    Assert-Line $keys 'routes.order.detail|0|defined|{{routes.order.detail}}/:id|1'
    Assert-Line $keys 'routes.order.missing|1|NULL|list/{{ routes.order.missing }}|1'
    $used = Invoke-TsRowsQ $made.Db "SELECT key || '=' || referenced AS r FROM translations"
    Assert-Line $used 'routes.order.detail=1'
}

}
