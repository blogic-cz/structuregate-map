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

# A HOST WITH A MODULE FEDERATION REMOTE: `orders` is a second project, its federation config exposing the module
# the host's route loads. The checker types `loadRemoteModule` as `any`; only the config says which class it is.
function New-TsRemoteWorkspace([string]$RemoteName = 'demo-orders') {
    New-TsRowsWorkspace @{
        'angular.json' = '{"projects":{"shop":{"projectType":"application","root":"apps/shop","sourceRoot":"apps/shop/src",' +
            '"architect":{"build":{"options":{"tsConfig":"apps/shop/tsconfig.app.json"}}}},' +
            '"orders":{"projectType":"application","root":"apps/orders","sourceRoot":"apps/orders/src",' +
            '"architect":{"build":{"options":{"tsConfig":"apps/orders/tsconfig.app.json"}}}}}}'
        'apps/orders/tsconfig.json' = '{"references":[{"path":"./tsconfig.app.json"}]}'
        'apps/orders/tsconfig.app.json' = '{"compilerOptions":{"strict":true},"include":["src/**/*.ts"]}'
        'apps/shop/src/app.routes.ts' = "import { NgModule } from '@angular/core';`n" +
            "import { RouterModule } from '@angular/router';`n" +
            "import { loadRemoteModule } from '@angular-architects/module-federation';`n" +
            "@NgModule({ imports: [RouterModule.forRoot([`n" +
            "  { path: 'orders', loadChildren: () => loadRemoteModule('demo-orders', './Module').then((m) => m.OrdersEntryModule)" +
            ".catch(() => import('./fallback.module').then((m) => m.FallbackModule)) },`n" +
            "  { path: 'alpha', loadChildren: () => loadRemoteModule({ type: 'manifest', remoteName: 'demo-orders', exposedModule: './Module' })" +
            ".then((m) => m.OrdersEntryModule) },`n" +
            "])] })`nexport class AppModule {}`n"
        'apps/shop/src/fallback.module.ts' = "import { NgModule } from '@angular/core';`n@NgModule({})`nexport class FallbackModule {}`n"
        'apps/orders/module-federation.config.js' = (Get-TsRemoteConfig $RemoteName)
        'apps/orders/src/entry.module.ts' = "import { NgModule } from '@angular/core';`n@NgModule({})`nexport class OrdersEntryModule {}`n"
    }
}
function Get-TsRemoteConfig([string]$Name) {
    "module.exports = { name: '$Name', exposes: { './Module': 'apps/orders/src/entry.module.ts' } };`n"
}
function Get-TsRemoteRoutes([string]$Db) {
    $r = Invoke-Gate --map-query $Db --width 0 --sql ("SELECT r.path || '|' || coalesce(rc.name, 'NULL') || '|' || " +
        "coalesce(r.load_children_remote_name, 'NULL') || '|' || coalesce(mc.name, 'NULL') || '|' || " +
        "coalesce(r.load_children_recovery, 'NULL') AS r FROM routes r LEFT JOIN classes rc ON rc.id = r.load_children_remote " +
        "LEFT JOIN classes mc ON mc.id = r.load_children_module")
    Assert-Exit $r 0
    return $r
}

# A FEDERATED ROUTE LINKS TO THE REMOTE'S EXPOSED MODULE (#76), not only to its catch fallback: the config whose `name`
# is the remote, its `exposes['./Module']` the file, the class the `.then` pick names declared there. The fallback stays
# `load_children_module`, marked a recovery. Both call forms: two strings, and the `{ remoteName, exposedModule }` object.
Test-Case 'tsrows: a loadRemoteModule route links to the class its remote exposes' {
    $made = New-TsRowsDb (New-TsRemoteWorkspace)
    Assert-Exit $made.Result 0
    $r = Get-TsRemoteRoutes $made.Db
    Assert-Line $r 'orders|OrdersEntryModule|demo-orders/Module|FallbackModule|1'
    Assert-Line $r 'alpha|OrdersEntryModule|demo-orders/Module|NULL|NULL'
}

# THE CONFIG IS READ ON EVERY RUN: a partial run over an edit to it alone re-links the route both ways. Nothing the
# route's own file read names the config, so a link made while that file was extracted would have kept the old answer.
Test-Case 'tsrows: a partial run follows an edit to the federation config' {
    $tree = New-TsRemoteWorkspace
    Assert-Exit (New-TsRowsDb $tree).Result 0
    $config = Join-Path $tree 'apps/orders/module-federation.config.js'
    Set-Content -LiteralPath $config -Value (Get-TsRemoteConfig 'demo-elsewhere')
    $moved = New-TsRowsDb $tree
    Assert-Exit $moved.Result 0
    Assert-Line $moved.Result 'read 1 file(s) again for 1 changed'
    $r = Get-TsRemoteRoutes $moved.Db
    Assert-Line $r 'orders|NULL|demo-orders/Module|FallbackModule|1'
    Assert-NoLine $r 'OrdersEntryModule'
    Set-Content -LiteralPath $config -Value (Get-TsRemoteConfig 'demo-orders')
    $back = New-TsRowsDb $tree
    Assert-Exit $back.Result 0
    Assert-Line $back.Result 'read 1 file(s) again for 1 changed'
    Assert-Line (Get-TsRemoteRoutes $back.Db) 'orders|OrdersEntryModule|demo-orders/Module|FallbackModule|1'
}

# A LAZY MODULE'S ROUTES STAY UNDER THEIR PARENT ON A PARTIAL RUN. The routes are joined across the lazy boundary by
# the CONTENT of the array a module's `forChild` holds and the const it names; since #75 a kept module row carries the
# ids added inside it (`component`'s `$ref`), the const read again does not, and the two stopped matching - every lazy
# root lost its `parent_route` on every partial run.
Test-Case 'tsrows: a partial run keeps a lazy module''s routes under the route that loads it' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/app.routes.ts' = "import { NgModule } from '@angular/core';`n" +
            "import { RouterModule } from '@angular/router';`n" +
            "@NgModule({ imports: [RouterModule.forRoot([`n" +
            "  { path: 'orders', loadChildren: () => import('./order.module').then((m) => m.OrderModule) },`n" +
            "])] })`nexport class AppRoutingModule {}`n"
        'apps/shop/src/order-detail.component.ts' = "import { Component } from '@angular/core';`n" +
            "@Component({ selector: 'demo-order-detail', template: '<p>order</p>' })`nexport class OrderDetailComponent {}`n"
        'apps/shop/src/order.routes.ts' = "import { OrderDetailComponent } from './order-detail.component';`n" +
            "export const orderRoutes = [{ path: 'detail', component: OrderDetailComponent }];`n"
        'apps/shop/src/order.module.ts' = "import { NgModule } from '@angular/core';`n" +
            "import { RouterModule } from '@angular/router';`n" +
            "import { OrderDetailComponent } from './order-detail.component';`n" +
            "import { orderRoutes } from './order.routes';`n" +
            "@NgModule({ declarations: [OrderDetailComponent], imports: [RouterModule.forChild(orderRoutes)] })`n" +
            "export class OrderModule {}`n"
        'apps/shop/src/note.ts' = "export const note = 1;`n"
    }
    $sql = "SELECT c.path || '<' || coalesce(p.path, 'none') AS r FROM routes c LEFT JOIN routes p ON p.id = c.parent_route " +
        "WHERE c.path = 'detail'"
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    Assert-Line (Invoke-TsRowsQ $made.Db $sql) 'detail<orders'
    # AN EDIT THAT READS NEITHER the module nor its routes again: both rows come back as they were stored.
    Add-Content -LiteralPath (Join-Path $tree 'apps/shop/src/note.ts') -Value '// demo: a remark'
    $again = New-TsRowsDb $tree
    Assert-Exit $again.Result 0
    Assert-Line $again.Result 'put back'
    $kept = Invoke-TsRowsQ $again.Db $sql
    Assert-Line $kept 'detail<orders'
    Assert-NoLine $kept 'detail<none'
}

# A ROUTE ARRAY REACHED FROM ANOTHER FILE IS NOT WALKED AGAIN: `forRoot(appRoutes)` in a module beside the const holds the
# same routes, and a second row per route made every lazy root claimed by TWO loaders, linked to neither (`parent_route`
# NULL, `absolute_path` without the lazy prefix). A route is one row, in the file that declares it.
Test-Case 'tsrows: a route array reached through forRoot in another file is one row per route and its lazy child links' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/home.component.ts' = @'
import { Component } from '@angular/core';
@Component({ selector: 'demo-home', template: '<p>h</p>' }) export class HomeComponent {}
@Component({ selector: 'demo-list', template: '<p>l</p>' }) export class OrderListComponent {}
'@
        'apps/shop/src/app.routes.ts' = @'
import { Routes } from '@angular/router';
import { HomeComponent } from './home.component';
export const appRoutes: Routes = [{ path: 'orders', loadChildren: () => import('./orders/orders.module').then(m => m.OrdersModule) }, { path: '', component: HomeComponent }];
'@
        'apps/shop/src/app.module.ts' = @'
import { NgModule } from '@angular/core';
import { RouterModule } from '@angular/router';
import { appRoutes } from './app.routes';
@NgModule({ imports: [RouterModule.forRoot(appRoutes, { initialNavigation: 'enabledBlocking' })] })
export class AppModule {}
'@
        'apps/shop/src/orders/orders.module.ts' = @'
import { NgModule } from '@angular/core';
import { RouterModule } from '@angular/router';
import { OrderListComponent } from '../home.component';
@NgModule({ declarations: [OrderListComponent], imports: [RouterModule.forChild([{ path: 'list', component: OrderListComponent }])] })
export class OrdersModule {}
'@
    }
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    $r = Invoke-TsRowsQ $made.Db "SELECT 'n=' || f.path || '|' || count(*) AS r FROM routes r JOIN files f ON f.id = r.file GROUP BY f.path"
    Assert-Line $r 'n=apps/shop/src/app.routes.ts|2'
    Assert-Line $r 'n=apps/shop/src/orders/orders.module.ts|1'
    Assert-NoLine $r 'app.module.ts'
    $lazy = Invoke-TsRowsQ $made.Db ("SELECT 'p=' || c.path || '<' || coalesce(p.path, 'none') || '|' || c.absolute_path AS r " +
        "FROM routes c LEFT JOIN routes p ON p.id = c.parent_route WHERE c.path = 'list'")
    Assert-Line $lazy 'p=list<orders|orders/list'
    $d = Invoke-TsRowsQ $made.Db "SELECT 'd=' || kind AS r FROM diagnostics"
    Assert-NoLine $d 'route_claimed_by_several_lazy_parents'
}

# A ROUTE'S ROW CARRIES WHAT THE ROUTER READS: `pathMatch` decides whether `''` is a prefix, `canDeactivate` and `canMatch` are
# guards beside `canActivate`, and `title` names the page. A route that declares none has them NULL / empty (the control).
Test-Case 'tsrows: a route row carries pathMatch, canDeactivate, canMatch and title' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/edit.routes.ts' = @'
import { Component } from '@angular/core';
import { Routes } from '@angular/router';
export class LeaveGuard {}
export class MatchGuard {}
export class ActGuard {}
export class LoadGuard {}
export class ChildGuard {}
@Component({ selector: 'demo-edit', template: '<p>e</p>' }) export class EditComponent {}
export const editRoutes: Routes = [
  { path: 'edit/:id', component: EditComponent, pathMatch: 'full', canDeactivate: [LeaveGuard], canMatch: [MatchGuard], title: 'Edit order',
    canActivate: [ActGuard], canLoad: [LoadGuard], canActivateChild: [ChildGuard] },
  { path: 'plain', component: EditComponent },
];
'@
    }
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    $r = Invoke-TsRowsQ $made.Db ("SELECT 'r=' || r.path || '|' || coalesce(r.path_match, '-') || '|' || coalesce(r.title, '-') || '|' || " +
        "coalesce((SELECT group_concat(x.value) FROM routes__can_deactivate x WHERE x.owner = r.id AND x.role = 'name'), '-') || '|' || " +
        "coalesce((SELECT group_concat(x.value) FROM routes__can_match x WHERE x.owner = r.id AND x.role = 'name'), '-') AS r " +
        "FROM routes r WHERE r.file IN (SELECT id FROM files WHERE path LIKE '%edit.routes.ts')")
    Assert-Line $r 'r=edit/:id|full|Edit order|LeaveGuard|MatchGuard'
    Assert-Line $r 'r=plain|-|-|-|-'
    # EVERY GUARD LIST NAMES ITS CLASSES' rows, as `can_activate_ids` did alone: the same join for each of the five lists.
    $ids = New-Object System.Collections.Generic.List[string]
    foreach ($column in 'can_activate', 'can_deactivate', 'can_match', 'can_load', 'guards') {
        $ids.Add("coalesce((SELECT group_concat(c.name) FROM routes__${column}_ids x JOIN classes c ON c.id = x.value " +
            "WHERE x.owner = r.id AND x.parent = 0), '-')")
    }
    $g = Invoke-TsRowsQ $made.Db ("SELECT 'ids=' || r.path || '|' || " + ($ids -join " || '|' || ") + " AS r FROM routes r " +
        "WHERE r.file IN (SELECT id FROM files WHERE path LIKE '%edit.routes.ts')")
    Assert-Line $g 'ids=edit/:id|ActGuard|LeaveGuard|MatchGuard|LoadGuard|ChildGuard'
    Assert-Line $g 'ids=plain|-|-|-|-|-'
}

# A FEDERATED REMOTE LOADED BY A PLAIN `import('remote/Module')`: webpack's container answers at run time, so no file resolves; the
# HOST's config lists the remote (`remotes`), the remote's config exposes the module. A specifier no host lists is a package subpath.
Test-Case 'tsrows: a plain import of a remote the host config lists resolves like loadRemoteModule' {
    $tree = New-TsRemoteWorkspace
    $files = @{
        'apps/shop/src/app.routes.ts' = @'
import { Routes } from '@angular/router';
export const appRoutes: Routes = [
  { path: 'orders', loadChildren: () => import('demo-orders/Module').then((m) => { return m.OrdersEntryModule; }) },
  { path: 'plain', loadChildren: () => import('demo-lib/Module').then((m) => m.PlainModule) },
];
'@
        'apps/shop/module-federation.config.js' = "module.exports = { name: 'shop', remotes: ['demo-orders'] };`n"
    }
    foreach ($key in $files.Keys) { Set-Content -LiteralPath (Join-Path $tree $key) -Value $files[$key] }
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    $r = Invoke-TsRowsQ $made.Db ("SELECT 'r=' || r.path || '|' || coalesce(rc.name, 'NULL') || '|' || coalesce(r.load_children_remote_name, 'NULL') AS r " +
        "FROM routes r LEFT JOIN classes rc ON rc.id = r.load_children_remote WHERE r.const_name = 'appRoutes'")
    Assert-Line $r 'r=orders|OrdersEntryModule|demo-orders/Module'
    Assert-Line $r 'r=plain|NULL|NULL'
}

# A HOST CONFIG THAT LISTS ONLY `remotes` (no `name`) STILL SAYS WHICH BARE SPECIFIERS ARE REMOTES: the config was dropped for lack of a
# `name`, so the route read as a package subpath and named nothing. The control, `demo-lib`, is listed by no host.
Test-Case 'tsrows: a host federation config with no name still lists its remotes' {
    $tree = New-TsRemoteWorkspace
    $files = @{
        'apps/shop/src/app.routes.ts' = @'
import { Routes } from '@angular/router';
export const appRoutes: Routes = [
  { path: 'orders', loadChildren: () => import('demo-orders/Module').then((m) => m.OrdersEntryModule) },
  { path: 'plain', loadChildren: () => import('demo-lib/Module').then((m) => m.PlainModule) },
];
'@
        'apps/shop/module-federation.config.js' = "module.exports = { remotes: ['demo-orders'] };`n"
    }
    foreach ($key in $files.Keys) { Set-Content -LiteralPath (Join-Path $tree $key) -Value $files[$key] }
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    $r = Invoke-TsRowsQ $made.Db ("SELECT 'r=' || r.path || '|' || coalesce(rc.name, 'NULL') || '|' || coalesce(r.load_children_remote_name, 'NULL') AS r " +
        "FROM routes r LEFT JOIN classes rc ON rc.id = r.load_children_remote WHERE r.const_name = 'appRoutes'")
    Assert-Line $r 'r=orders|OrdersEntryModule|demo-orders/Module'
    Assert-Line $r 'r=plain|NULL|NULL'
}

}
