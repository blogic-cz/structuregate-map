<#
    A reference becomes a row id (`TsRefs.mjs`): `di` carries the `@Inject(TOKEN)` token and the `inject()` calls a
    property is initialised with, and an `export { A as B }` alias still lands a module's declaration or a route's
    component on the class. Its helpers are `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '../TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

$script:TsRefsFiles = @{
    'apps/shop/src/demo.token.ts' = "import { InjectionToken, Injectable, Inject, inject, Injector } from '@angular/core';`n" +
        "export interface IDemo { x: number }`n" +
        "export const DEMO_TOKEN = new InjectionToken<IDemo>('demo');`n" +
        "export const DemoTokens = { Alpha: new InjectionToken<IDemo>('alpha') };`n" +
        "@Injectable({ providedIn: 'root' }) export class Other {}`n" +
        "@Injectable() export class DemoService {`n" +
        "  readonly other = inject(Other);`n  readonly injector = inject(Injector);`n" +
        "  readonly tok = inject(DEMO_TOKEN);`n  readonly cast = (inject(DEMO_TOKEN) as IDemo).x;`n" +
        "  lazy = () => inject(Other);`n" +
        "  constructor(@Inject(DEMO_TOKEN) private d: IDemo, private o: Other, @Inject(DemoTokens.Alpha) private a: IDemo) {}`n}`n"
    'apps/shop/src/alpha.component.ts' = "import { Component } from '@angular/core';`n" +
        "@Component({ selector: 'demo-alpha', template: '' }) class AlphaComponent {}`n" +
        "export { AlphaComponent as DemoAlpha };`n"
    'apps/shop/src/beta.component.ts' = "import { Component } from '@angular/core';`n" +
        "@Component({ selector: 'demo-beta', template: '' }) export class BetaComponent {}`n"
    'apps/shop/src/m.module.ts' = "import { NgModule } from '@angular/core';`n" +
        "import { RouterModule, Routes } from '@angular/router';`n" +
        "import { DemoAlpha } from './alpha.component';`n" +
        "import { BetaComponent as DemoBeta } from './beta.component';`n" +
        "const routes: Routes = [{ path: 'a', component: DemoAlpha }];`n" +
        "@NgModule({ declarations: [DemoAlpha, DemoBeta], imports: [RouterModule.forChild(routes)] }) export class DemoModule {}`n"
}

Test-Case 'tsrows: di carries the @Inject token and the inject() calls of property initialisers' {
    $tree = New-TsRowsWorkspace $script:TsRefsFiles
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    $r = Invoke-TsRowsQ $made.Db ("SELECT param || ' | ' || coalesce(token,'') || ' | ' || coalesce((SELECT rn.name FROM di__resolved rn WHERE rn.id = di.id || '/0'),'') || ' | ' || coalesce(inject,0) AS r " +
        "FROM di WHERE class = (SELECT id FROM classes WHERE name = 'DemoService')")
    Assert-Line $r 'd | DEMO_TOKEN | IDemo | 0'
    Assert-Line $r 'other | Other | Other | 1'
    Assert-Line $r 'injector | Injector | Injector | 1'
    Assert-Line $r 'tok | DEMO_TOKEN | IDemo | 1'
    Assert-Line $r 'cast | DEMO_TOKEN | IDemo | 1'
    Assert-NoLine $r 'lazy |'
    $k = Invoke-TsRowsQ $made.Db "SELECT count(*) AS r FROM di d JOIN consts k ON k.id = d.token_id WHERE k.name = 'DEMO_TOKEN'"
    Assert-Line $k '3'
    # a token held as a property of a const object joins that const; `token` keeps the property
    Assert-Line $r 'a | DemoTokens.Alpha | IDemo | 0'
    $o = Invoke-TsRowsQ $made.Db "SELECT count(*) AS r FROM di d JOIN consts k ON k.id = d.token_id WHERE k.name = 'DemoTokens'"
    Assert-Line $o '1'
}

Test-Case 'tsrows: an export alias still joins a route and a module declaration to the class' {
    $tree = New-TsRowsWorkspace $script:TsRefsFiles
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    $route = Invoke-TsRowsQ $made.Db ("SELECT r.path || ' | ' || c.name AS r FROM routes r JOIN classes c ON c.id = r.component_id WHERE r.path = 'a'")
    Assert-Line $route 'a | AlphaComponent'
    $decl = Invoke-TsRowsQ $made.Db ("SELECT c.name AS r FROM ng_modules m JOIN ng_modules__declarations d ON d.owner = m.id AND d.parent = m.id || '/0' AND d.ord = 0 JOIN classes c ON c.id = d.[.id] WHERE m.name = 'DemoModule'")
    Assert-Line $decl 'AlphaComponent'
    $reach = Invoke-TsRowsQ $made.Db ("SELECT m.name AS r FROM component_reach cr JOIN component_reach__areas a ON a.owner = cr.id AND a.parent = cr.id || '/0' AND a.ord = 0 JOIN ng_modules m ON m.id = a.value WHERE cr.name = 'AlphaComponent'")
    Assert-Line $reach 'DemoModule'
    # an IMPORT alias: the module names `DemoBeta`, which only its own `import_names` row ties to the class
    $beta = Invoke-TsRowsQ $made.Db ("SELECT 'beta=' || c.name AS r FROM ng_modules m JOIN ng_modules__declarations d ON d.owner = m.id AND d.parent = m.id || '/0' AND d.ord = 1 JOIN classes c ON c.id = d.[.id] WHERE m.name = 'DemoModule'")
    Assert-Line $beta 'beta=BetaComponent'
}

}
