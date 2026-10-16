<#
    Which render edges are in the template's compilation scope, and which views keep only those. The scope
    itself is `src/TsRows/TsDerive/TsModuleScope.mjs`; its helpers are `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '../TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

# A TAG MATCHES A SELECTOR ONLY WHERE THE COMPONENT IS IN SCOPE. Every candidate is kept, the out-of-scope one
# stamped so and set aside in `renders_out_of_scope`; `render_graph` is the edge Angular really instantiates. Summed over every
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
    $rows = Invoke-TsRowsQ $made.Db ("SELECT p.name || '=' || r.scope AS edge FROM (SELECT from_class, scope, to_name " +
        "FROM renders UNION ALL SELECT from_class, scope, to_name FROM renders_out_of_scope) r " +
        "JOIN classes p ON p.id = r.from_class WHERE r.to_name = 'BannerComponent'")
    Assert-Line $rows 'AlphaStartComponent=declared'
    Assert-Line $rows 'DemoPanelComponent=out_of_scope'
    $graph = Invoke-TsRowsQ $made.Db ("SELECT p.name AS parent FROM render_graph g " +
        "JOIN classes p ON p.id = g.from_class WHERE g.to_name = 'BannerComponent'")
    Assert-Line $graph 'AlphaStartComponent'
    Assert-NoLine $graph 'DemoPanelComponent'
}

# TWO LAZY MODULES EACH DECLARE THEIR OWN `demo-box`, and a host in each renders `<demo-box>`. `renders` keeps all
# four candidates, the other module's box stamped `out_of_scope` (set aside since #40); the closure walks only what Angular instantiates,
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
    $edges = Invoke-TsRowsQ $made.Db ("SELECT p.name || '>' || r.to_name || '=' || r.scope AS edge FROM (SELECT from_class, " +
        "to_name, scope, tag FROM renders UNION ALL SELECT from_class, to_name, scope, tag FROM renders_out_of_scope) r " +
        "JOIN classes p ON p.id = r.from_class WHERE r.tag LIKE 'demo-%'")
    Assert-Line $edges 'HostAComponent>BBoxComponent=out_of_scope'
    Assert-Line $edges 'HostCComponent>SBoxComponent=imported'
    $sql = "SELECT k.key || ' <- ' || c.name AS reach FROM key_reach k JOIN key_reach__roots j ON j.owner = '#' || k.rowid AND j.parent = 0 " +
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

# `renders` IS WHAT ANGULAR INSTANTIATES, the same edges `render_path` walks. Two modules each declaring a
# `demo-box` gave four rows for the tag - each host to BOTH boxes - while the closure walked two, so a query over
# `renders` contradicted `render_path` (#40). The foreign candidate is set aside in `renders_out_of_scope`; a host no
# module declares (`unknown`) keeps an edge to every candidate, as `render_path` does.
Test-Case 'tsrows: renders holds only the edges the module scope allows, the rest set aside' {
    $box = { param($Class, $Key) "@Component({ selector: 'demo-box', template: '<b>{{ `"$Key`" | money }}</b>' })`nexport class $Class {}`n" }
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/a.module.ts' = "import { Component, NgModule } from '@angular/core';`n" + (& $box 'ABoxComponent' 'demo.a.title') +
            "@Component({ selector: 'demo-host-a', template: '<demo-box></demo-box>' })`nexport class HostAComponent {}`n" +
            "@NgModule({ declarations: [ABoxComponent, HostAComponent] })`nexport class AModule {}`n"
        'apps/shop/src/b.module.ts' = "import { Component, NgModule } from '@angular/core';`n" + (& $box 'BBoxComponent' 'demo.b.title') +
            "@Component({ selector: 'demo-host-b', template: '<demo-box></demo-box>' })`nexport class HostBComponent {}`n" +
            "@NgModule({ declarations: [BBoxComponent, HostBComponent] })`nexport class BModule {}`n"
        'apps/shop/src/loose.ts' = "import { Component } from '@angular/core';`n" +
            "@Component({ selector: 'demo-loose', template: '<demo-box></demo-box>' })`nexport class LooseComponent {}`n"
    }
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    $edge = "SELECT p.name || '>' || r.to_name AS edge FROM {0} r JOIN classes p ON p.id = r.from_class WHERE r.tag = 'demo-box'"
    $kept = Invoke-TsRowsQ $made.Db ($edge -f 'renders')
    Assert-Line $kept 'HostAComponent>ABoxComponent'
    Assert-Line $kept 'HostBComponent>BBoxComponent'
    Assert-NoLine $kept 'HostAComponent>BBoxComponent'
    Assert-NoLine $kept 'HostBComponent>ABoxComponent'
    Assert-Line $kept 'LooseComponent>ABoxComponent'
    Assert-Line $kept 'LooseComponent>BBoxComponent'
    # THE CANDIDATE IS NOT LOST: a build writing no foreign edge at all would pass the lines above.
    $aside = Invoke-TsRowsQ $made.Db ($edge -f 'renders_out_of_scope')
    Assert-Line $aside 'HostAComponent>BBoxComponent'
    Assert-Line $aside 'HostBComponent>ABoxComponent'
    Assert-NoLine $aside 'HostAComponent>ABoxComponent'
    # THE SAME EDGES `render_path` WALKS: every hop it takes out of a host is a `renders` row.
    $walked = Invoke-TsRowsQ $made.Db ("SELECT COUNT(*) AS foreign_hops FROM render_path p JOIN render_path__edges e ON e.owner = p.id AND e.parent = 0 " +
        "WHERE e.value NOT IN (SELECT id FROM renders)")
    Assert-Line $walked '0'
}

# AN OUT-OF-SCOPE CANDIDATE IS NO RENDER (#77). Two lazy modules each declare a `demo-box`; only `HostAComponent`
# writes the tag, so `BBoxComponent` is reached by nothing but `HostA`'s foreign candidate. Counted as rendered, it
# read `unused = 0` with no template instantiating it. No module-less host here: `unknown` would still count.
Test-Case 'tsrows: a component only an out-of-scope candidate names is unused' {
    $box = { param($Class) "@Component({ selector: 'demo-box', template: '<b>box</b>' })`nexport class $Class {}`n" }
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/a.module.ts' = "import { Component, NgModule } from '@angular/core';`n" +
            "import { RouterModule } from '@angular/router';`n" + (& $box 'ABoxComponent') +
            "@Component({ selector: 'demo-host-a', template: '<demo-box></demo-box>' })`nexport class HostAComponent {}`n" +
            "@NgModule({ declarations: [ABoxComponent, HostAComponent], imports: [RouterModule.forChild([{ path: 'a', component: HostAComponent }])] })`n" +
            "export class AModule {}`n"
        'apps/shop/src/b.module.ts' = "import { Component, NgModule } from '@angular/core';`n" +
            "import { RouterModule } from '@angular/router';`n" + (& $box 'BBoxComponent') +
            "@Component({ selector: 'demo-host-b', template: '<p>b</p>' })`nexport class HostBComponent {}`n" +
            "@NgModule({ declarations: [BBoxComponent, HostBComponent], imports: [RouterModule.forChild([{ path: 'b', component: HostBComponent }])] })`n" +
            "export class BModule {}`n"
        'apps/shop/src/app.routes.ts' = "import { RouterModule } from '@angular/router';`n" +
            "export const routing = RouterModule.forRoot([`n" +
            "  { path: 'a', loadChildren: () => import('./a.module').then((m) => m.AModule) },`n" +
            "  { path: 'b', loadChildren: () => import('./b.module').then((m) => m.BModule) },`n" +
            "]);`n"
    }
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    # THE FOREIGN CANDIDATE IS STILL A ROW: without it the case proves nothing about how one is counted.
    $aside = Invoke-TsRowsQ $made.Db ("SELECT p.name || '>' || r.to_name AS edge FROM renders_out_of_scope r " +
        "JOIN classes p ON p.id = r.from_class")
    Assert-Line $aside 'HostAComponent>BBoxComponent'
    $r = Invoke-TsRowsQ $made.Db ("SELECT name || '=' || unused AS u FROM components " +
        "WHERE name IN ('ABoxComponent', 'BBoxComponent')")
    Assert-Line $r 'ABoxComponent=0'
    Assert-Line $r 'BBoxComponent=1'
}

}
