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

# TWO LAZY MODULES EACH DECLARE THEIR OWN `demo-box`, and a host in each renders `<demo-box>`. `renders` keeps all
# four candidates, the other module's box stamped `out_of_scope`; the closure walks only what Angular instantiates,
# so each box's key is reached from its own route alone, as `component_reach` already says. A third host in NO
# module (`scope = unknown`) still reaches both: nothing proves which one it renders, and an edge resolving today
# must not vanish for a scope the map cannot read. `SharedModule` exports its box to `CModule`, which imports it.
Test-Case 'tsrows: a key behind an out-of-scope render is not reached through it' {
    $box = { param($Class, $Key) "@Component({ selector: 'demo-box', template: '<b>{{ `"$Key`" | money }}</b>' })`nexport class $Class {}`n" }
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/assets/locales/en.json' = '{"demo":{"a":{"title":"A"},"b":{"title":"B"},"s":{"title":"S"}}}'
        'apps/shop/src/a.module.ts' = "import { Component, NgModule } from '@angular/core';`n" +
            "import { RouterModule } from '@angular/router';`n" + (& $box 'ABoxComponent' 'demo.a.title') +
            "@Component({ selector: 'demo-host-a', template: '<demo-box></demo-box>' })`nexport class HostAComponent {}`n" +
            "@NgModule({ declarations: [ABoxComponent, HostAComponent], imports: [RouterModule.forChild([{ path: 'a', component: HostAComponent }])] })`n" +
            "export class AModule {}`n"
        'apps/shop/src/b.module.ts' = "import { Component, NgModule } from '@angular/core';`n" +
            "import { RouterModule } from '@angular/router';`n" + (& $box 'BBoxComponent' 'demo.b.title') +
            "@Component({ selector: 'demo-host-b', template: '<demo-box></demo-box>' })`nexport class HostBComponent {}`n" +
            "@NgModule({ declarations: [BBoxComponent, HostBComponent], imports: [RouterModule.forChild([{ path: 'b', component: HostBComponent }])] })`n" +
            "export class BModule {}`n"
        'apps/shop/src/shared.module.ts' = "import { Component, NgModule } from '@angular/core';`n" +
            "@Component({ selector: 'demo-sbox', template: '<b>{{ `"demo.s.title`" | money }}</b>' })`nexport class SBoxComponent {}`n" +
            "@NgModule({ declarations: [SBoxComponent], exports: [SBoxComponent] })`nexport class SharedModule {}`n"
        'apps/shop/src/c.module.ts' = "import { Component, NgModule } from '@angular/core';`n" +
            "import { RouterModule } from '@angular/router';`n" +
            "import { SharedModule } from './shared.module';`n" +
            "@Component({ selector: 'demo-host-c', template: '<demo-sbox></demo-sbox>' })`nexport class HostCComponent {}`n" +
            "@NgModule({ declarations: [HostCComponent], imports: [SharedModule, RouterModule.forChild([{ path: 'c', component: HostCComponent }])] })`n" +
            "export class CModule {}`n"
        'apps/shop/src/loose.ts' = "import { Component } from '@angular/core';`n" +
            "@Component({ selector: 'demo-loose', template: '<demo-box></demo-box>' })`nexport class LooseComponent {}`n"
        'apps/shop/src/app.routes.ts' = "import { RouterModule } from '@angular/router';`n" +
            "import { LooseComponent } from './loose';`n" +
            "export const routing = RouterModule.forRoot([`n" +
            "  { path: 'a', loadChildren: () => import('./a.module').then((m) => m.AModule) },`n" +
            "  { path: 'b', loadChildren: () => import('./b.module').then((m) => m.BModule) },`n" +
            "  { path: 'c', loadChildren: () => import('./c.module').then((m) => m.CModule) },`n" +
            "  { path: 'loose', component: LooseComponent },`n" +
            "]);`n"
    }
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    # THE CANDIDATES STAY ROWS, stamped - without these a build writing no render for the tag passes below.
    $edges = Invoke-TsRowsQ $made.Db ("SELECT p.name || '>' || r.to_name || '=' || r.scope AS edge FROM renders r " +
        "JOIN classes p ON p.id = r.from_class WHERE r.tag LIKE 'demo-%'")
    Assert-Line $edges 'HostAComponent>BBoxComponent=out_of_scope'
    Assert-Line $edges 'HostCComponent>SBoxComponent=imported'
    $sql = "SELECT k.key || ' <- ' || c.name AS reach FROM key_reach k, json_each(k.roots) j " +
        "JOIN components m ON m.id = j.value JOIN classes c ON c.id = m.class WHERE k.key LIKE 'demo.%'"
    $r = Invoke-TsRowsQ $made.Db $sql
    Assert-Line $r 'demo.a.title <- HostAComponent'
    Assert-Line $r 'demo.b.title <- HostBComponent'
    Assert-Line $r 'demo.s.title <- HostCComponent'
    Assert-NoLine $r 'demo.a.title <- HostBComponent'
    Assert-NoLine $r 'demo.b.title <- HostAComponent'
    Assert-Line $r 'demo.a.title <- LooseComponent'
    Assert-Line $r 'demo.b.title <- LooseComponent'
}

}
