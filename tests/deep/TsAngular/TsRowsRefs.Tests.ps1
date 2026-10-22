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
    $made = New-TsRowsDb $tree -Shared
    Assert-Exit $made.Result 0
    $r = Invoke-TsRowsQ $made.Db ("SELECT param || ' | ' || coalesce(token,'') || ' | ' || coalesce($(Get-TsRootFieldSql 'di__resolved' 'di.id' 'name'),'') || ' | ' || coalesce(inject,0) AS r " +
        "FROM di WHERE class = (SELECT id FROM classes WHERE name = 'DemoService')")
    Assert-Line $r 'd | DEMO_TOKEN | IDemo | 0'
    Assert-Line $r 'other | Other | Other | 1'
    Assert-Line $r 'injector | Injector | Injector | 1'
    Assert-Line $r 'tok | DEMO_TOKEN | IDemo | 1'
    # `(inject(DEMO_TOKEN) as IDemo).x` is a COPY of a member of the injected object, not a dependency named `cast` (rows 78): no row carries that field's name.
    Assert-NoLine $r 'cast |'
    Assert-NoLine $r 'lazy |'
    $k = Invoke-TsRowsQ $made.Db "SELECT count(*) AS r FROM di d JOIN consts k ON k.id = d.token_id WHERE k.name = 'DEMO_TOKEN'"
    Assert-Line $k '2'
    # a token held as a property of a const object joins that const; `token` keeps the property
    Assert-Line $r 'a | DemoTokens.Alpha | IDemo | 0'
    $o = Invoke-TsRowsQ $made.Db "SELECT count(*) AS r FROM di d JOIN consts k ON k.id = d.token_id WHERE k.name = 'DemoTokens'"
    Assert-Line $o '1'
}

Test-Case 'tsrows: an export alias still joins a route and a module declaration to the class' {
    $tree = New-TsRowsWorkspace $script:TsRefsFiles
    $made = New-TsRowsDb $tree -Shared
    Assert-Exit $made.Result 0
    $route = Invoke-TsRowsQ $made.Db ("SELECT r.path || ' | ' || c.name AS r FROM routes r JOIN classes c ON c.id = r.component_id WHERE r.path = 'a'")
    Assert-Line $route 'a | AlphaComponent'
    $decl = Invoke-TsRowsQ $made.Db ("SELECT c.name AS r FROM ng_modules m JOIN ng_modules__declarations d ON d.owner = m.id AND d.parent = 0 AND d.ord = 0 JOIN classes c ON c.id = $(Get-TsFieldSql 'ng_modules__declarations' 'd' 'id') WHERE m.name = 'DemoModule'")
    Assert-Line $decl 'AlphaComponent'
    $reach = Invoke-TsRowsQ $made.Db ("SELECT m.name AS r FROM component_reach cr JOIN component_reach__areas a ON a.owner = cr.id AND a.parent = 0 AND a.ord = 0 JOIN ng_modules m ON m.id = a.value WHERE cr.name = 'AlphaComponent'")
    Assert-Line $reach 'DemoModule'
    # an IMPORT alias: the module names `DemoBeta`, which only its own `import_names` row ties to the class
    $beta = Invoke-TsRowsQ $made.Db ("SELECT 'beta=' || c.name AS r FROM ng_modules m JOIN ng_modules__declarations d ON d.owner = m.id AND d.parent = 0 AND d.ord = 1 JOIN classes c ON c.id = $(Get-TsFieldSql 'ng_modules__declarations' 'd' 'id') WHERE m.name = 'DemoModule'")
    Assert-Line $beta 'beta=BetaComponent'
}

# A REF INSIDE AN ELEMENT CARRIES ITS ID TOO (#75). A lazy route written into `RouterModule.forChild([...])` holds its
# module as `$module` deep in the `imports` element, and a provider object names its class as `$ref`s; only the
# top-level element got an id, so a walk of module imports by id missed every lazily loaded child.
Test-Case 'tsrows: a nested lazy module and a nested provider ref in an ng_modules list carry their id' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/demo.module.ts' = "import { NgModule } from '@angular/core';`n" +
            "import { RouterModule } from '@angular/router';`n" +
            "import { AlphaService } from './alpha.service';`n" +
            "@NgModule({ imports: [RouterModule.forChild([{ path: 'orders', loadChildren: () => import('./order.module').then(m => m.OrderModule) }])],`n" +
            "  providers: [{ provide: AlphaService, useClass: AlphaService }] })`n" +
            "export class DemoModule {}`n"
        'apps/shop/src/order.module.ts' = "import { NgModule } from '@angular/core';`n@NgModule({})`nexport class OrderModule {}`n"
        'apps/shop/src/alpha.service.ts' = "import { Injectable } from '@angular/core';`n@Injectable()`nexport class AlphaService {}`n"
    }
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    # THE SAME CLASS `routes.load_children_module` names - the id is the route's resolution, not a second one.
    $lazy = Invoke-TsRowsQ $made.Db ('SELECT ''module='' || c.name AS r FROM ng_modules m ' +
        'JOIN ng_modules__imports z ON z.owner = m.id AND z.role = ''$module'' ' +
        "JOIN classes c ON c.id = $(Get-TsFieldSql 'ng_modules__imports' 'z' 'id') " +
        "WHERE m.name = 'DemoModule' AND c.id = (SELECT load_children_module FROM routes WHERE path = 'orders')")
    Assert-Line $lazy 'module=OrderModule'
    $provided = Invoke-TsRowsQ $made.Db ('SELECT z.role || ''='' || c.name AS r FROM ng_modules m ' +
        'JOIN ng_modules__providers z ON z.owner = m.id AND z.role IN (''provide'', ''useClass'') ' +
        "JOIN classes c ON c.id = $(Get-TsFieldSql 'ng_modules__providers' 'z' 'id') WHERE m.name = 'DemoModule'")
    Assert-Line $provided 'provide=AlphaService'
    Assert-Line $provided 'useClass=AlphaService'
}

# (rows 78) AN `inject()` NESTED IN A FIELD'S INITIALIZER IS A DEPENDENCY, NOT THE FIELD (`TsDecls/TsInjects.mjs`): `x = (inject(TOKEN) as IConfig).x` copies a member of what was injected, so the class has one
# `di` row for TOKEN - `param` null, `via` the field - and none named `x`. The injected value is typed as written: the field's annotation, else the cast around the call, else the call's own type. The controls: a field that IS
# the call keeps its row under its name, and one nested use of a token a field already injects adds none.
$script:TsInjectFiles = @{
    'apps/shop/src/demo.inject.ts' = @'
import { InjectionToken, Injectable, inject } from '@angular/core';
export interface IDemo { x: number }
export interface ITranslate { t(k: string): string }
export const DEMO_TOKEN = new InjectionToken<IDemo>('demo');
export const Tokens = { Translate: new InjectionToken<unknown>('tr') };
export const LIST_TOKEN = new InjectionToken<string[]>('list');
@Injectable({ providedIn: 'root' }) export class Alpha { id = 1; }
@Injectable({ providedIn: 'root' }) export class Beta {}
@Injectable() export class Gamma {
  readonly cfgX = (inject(DEMO_TOKEN) as IDemo).x;
  readonly id = inject(Alpha).id;
  readonly again = inject(Alpha).id;
  readonly items = inject(LIST_TOKEN, { optional: true }) ?? [];
  readonly tr: ITranslate = inject(Tokens.Translate) as ITranslate;
  readonly cast = inject(Tokens.Translate) as ITranslate;
  readonly plain = inject(Beta);
}
@Injectable() export class Delta { run() { return inject(Beta); } }
export function jsonInterceptor(next: (v: unknown) => unknown) {
  const s = inject(Alpha);
  const typed = inject(Tokens.Translate) as ITranslate;
  const list = inject(LIST_TOKEN, { optional: true }) ?? [];
  return next(s);
}
export const arrowInterceptor = () => { const a: Alpha = inject(Alpha); return a; };
export const PROVIDERS = [{ provide: Tokens.Translate, useFactory: () => inject(Beta) }];
'@
}

function Get-TsInjectRows([string]$Db, [string]$Class) {
    return Invoke-TsRowsQ $Db ("SELECT 'param=' || coalesce(param, '-') || ' | ' || coalesce(token, '') || ' | ' || coalesce(type, '') || ' | ' || coalesce($(Get-TsRootFieldSql 'di__type_ref' 'di.id' 'name'), '') AS r " +
        "FROM di WHERE class = (SELECT id FROM classes WHERE name = '$Class')")
}

Test-Case 'tsrows: an inject() nested in a field initializer is a dependency of the class, not a di row named for the field' {
    $made = New-TsRowsDb (New-TsRowsWorkspace $script:TsInjectFiles) -Shared
    Assert-Exit $made.Result 0
    $r = Get-TsInjectRows $made.Db 'Gamma'
    Assert-NoLine $r 'param=cfgX'
    Assert-NoLine $r 'param=id '
    Assert-NoLine $r 'param=again'
    Assert-NoLine $r 'param=items'
    Assert-Line $r 'param=- | DEMO_TOKEN | IDemo | IDemo'
    Assert-Line $r 'param=- | Alpha | Alpha | Alpha'
    Assert-Line $r 'param=- | LIST_TOKEN | '
    # ONE row for the token however many fields nest it, and `via` names the field that came first.
    $n = Invoke-TsRowsQ $made.Db "SELECT 'alpha=' || count(*) || ':' || min(via) AS r FROM di WHERE token = 'Alpha' AND class = (SELECT id FROM classes WHERE name = 'Gamma')"
    Assert-Line $n 'alpha=1:id'
    # THE CONTROL: a field that is the call keeps its row under its own name.
    Assert-Line $r 'param=plain | Beta | Beta | Beta'
}

Test-Case 'tsrows: the di row of an inject() is typed by the field annotation, else the cast around the call, else the call' {
    $made = New-TsRowsDb (New-TsRowsWorkspace $script:TsInjectFiles) -Shared
    Assert-Exit $made.Result 0
    $r = Get-TsInjectRows $made.Db 'Gamma'
    Assert-Line $r 'param=tr | Tokens.Translate | ITranslate | ITranslate'
    Assert-Line $r 'param=cast | Tokens.Translate | ITranslate | ITranslate'
    # The token is still the call's argument, and the call's own type stays the answer where nothing says otherwise.
    Assert-Line $r 'param=plain | Beta | Beta | Beta'
}

Test-Case 'tsrows: an inject() in a function body is a di row of that function, a method body is none' {
    $made = New-TsRowsDb (New-TsRowsWorkspace $script:TsInjectFiles) -Shared
    Assert-Exit $made.Result 0
    Assert-Line (Invoke-TsRowsQ $made.Db "SELECT 'rows=' || count(*) AS r FROM di WHERE class IS NULL") 'rows=5'
    $r = Invoke-TsRowsQ $made.Db ("SELECT coalesce(f.name, 'inline') || ' | ' || coalesce(d.param, '-') || ' | ' || d.token || ' | ' || coalesce(d.type, '') AS r FROM di d JOIN functions f ON f.id = d.function WHERE d.class IS NULL")
    Assert-Line $r 'jsonInterceptor | s | Alpha | Alpha'
    Assert-Line $r 'jsonInterceptor | typed | Tokens.Translate | ITranslate'
    Assert-Line $r 'jsonInterceptor | - | LIST_TOKEN | '
    Assert-Line $r 'arrowInterceptor | a | Alpha | Alpha'
    Assert-Line $r 'inline | - | Beta | Beta'
    # THE CONTROL: a method body is no injection context - the class that calls inject() there has no di row for it.
    Assert-Line (Invoke-TsRowsQ $made.Db "SELECT 'delta=' || count(*) AS r FROM di WHERE class = (SELECT id FROM classes WHERE name = 'Delta')") 'delta=0'
}

}
