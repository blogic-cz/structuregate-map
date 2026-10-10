<#
    Which render edges are in the template's compilation scope, and which views keep only those. The scope
    itself is `src/TsRows/TsDerive/TsModuleScope.mjs`; its helpers are `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '../TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

# A TAG MATCHES A SELECTOR ONLY WHERE THE COMPONENT IS IN SCOPE. `renders` keeps every candidate, the
# out-of-scope one stamped so; `render_graph` is the edge Angular really instantiates. Summed over every
# candidate, a native `<header>` in a standalone component was an edge to an unrelated `header` component,
# and every walk of the graph showed it rendered on a screen it never appears on.
Test-Case 'tsrows: render_graph leaves out an element whose component is not in the template scope' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/banner.ts' = "import { Component, NgModule } from '@angular/core';`n" +
            "@Component({ selector: 'header', template: '<h1>Alpha</h1>' })`n" +
            "export class BannerComponent {}`n" +
            "@Component({ selector: 'alpha-start', template: '<header></header>' })`n" +
            "export class AlphaStartComponent {}`n" +
            "@NgModule({ declarations: [BannerComponent, AlphaStartComponent] })`n" +
            "export class AlphaModule {}`n" +
            # BESIDE THE MODULE ON PURPOSE: the declaring module is found by the class's FILE, and a standalone
            # component here once took `AlphaModule`'s scope and read as declared.
            "@Component({ selector: 'demo-panel', standalone: true, template: '<header>Title</header>' })`n" +
            "export class DemoPanelComponent {}`n"
    }
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    # BOTH CANDIDATES ARE STILL ROWS - the alternative stays visible, never silently dropped. Without these
    # two, a build that wrote no render for `<header>` at all passes the graph assertions below.
    $rows = Invoke-TsRowsQ $made.Db ("SELECT p.name || '=' || r.scope AS edge FROM renders r " +
        "JOIN classes p ON p.id = r.from_class WHERE r.to_name = 'BannerComponent'")
    Assert-Line $rows 'AlphaStartComponent=declared'
    Assert-Line $rows 'DemoPanelComponent=out_of_scope'
    $graph = Invoke-TsRowsQ $made.Db ("SELECT p.name AS parent FROM render_graph g " +
        "JOIN classes p ON p.id = g.from_class WHERE g.to_name = 'BannerComponent'")
    Assert-Line $graph 'AlphaStartComponent'
    Assert-NoLine $graph 'DemoPanelComponent'
}

}
