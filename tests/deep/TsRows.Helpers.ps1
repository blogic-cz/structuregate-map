<#
    The helpers every TypeScript-row suite shares: the compiler this half borrows, the smallest workspace it
    can answer about, and the query lens. Not a suite - `Run-Tests.ps1` globs `*.Tests.ps1` - so each suite
    that needs them dot-sources this file itself, and `-Only` still runs any of them alone.
#>

# The compiler this half BORROWS. Both packages, in one prefix, because the half checks for them together -
# a root satisfying only `typescript` would let discovery work and template parsing fail one step later.
function Get-TsRowsModules {
    if ($null -ne $script:TsRowsCache) { return $script:TsRowsCache.Path }
    $script:TsRowsCache = @{ Path = $null }
    if (-not (Get-Command node -ErrorAction SilentlyContinue)) { return $null }
    $cache = Join-Path ([System.IO.Path]::GetTempPath()) 'sgtest-tsrows'
    $modules = Join-Path $cache 'node_modules'
    if ((Test-Path (Join-Path $modules 'typescript\package.json')) -and
        (Test-Path (Join-Path $modules '@angular\compiler\package.json'))) {
        $script:TsRowsCache.Path = $modules
        return $modules
    }
    if (-not (Get-Command npm -ErrorAction SilentlyContinue)) { return $null }
    [void](New-Item -ItemType Directory -Path $cache -Force)
    # PINNED TO ONE MAJOR: the half is written against the compiler API of these versions, and
    # typescript 7 - the native port - does not expose `parseConfigFileTextToJson` at all, so
    # `typescript@latest` tested a different tool.
    # `@angular/core` as well, and not as a nicety: whether a decorator is ANGULAR's is resolved through the
    # type checker to that package, so without it every decorator in a fixture is a local one.
    # `@angular/router` and `@ngrx/store` as well: a route's lazy target and every state API are recognised
    # by the declaration the CHECKER resolved, so without the packages a fixture's routes and actions are
    # calls to nothing and the derived tables stay empty.
    & npm install --no-save --silent --prefix $cache 'typescript@5' '@angular/compiler@17' '@angular/core@17' '@angular/router@17' '@ngrx/store@17' 2>&1 | Out-Null
    if ((Test-Path (Join-Path $modules 'typescript\package.json')) -and
        (Test-Path (Join-Path $modules '@angular\compiler\package.json'))) {
        $script:TsRowsCache.Path = $modules
    }
    return $script:TsRowsCache.Path
}

$script:TsRowsModules = Get-TsRowsModules
$script:TsRowsPython = [bool]$script:Python
if (-not $script:TsRowsModules) { Write-Host '    (no node/npm-installed typescript + @angular/compiler - the typescript row cases are not run)' }
elseif (-not $script:TsRowsPython) { Write-Host '    (no python on this machine - the typescript row cases are not run)' }

# The smallest workspace this half can answer about: one Angular CLI project whose BUILD TARGET names its
# tsconfig, over a root config that only references it. Both facts are load-bearing and both are asserted.
function New-TsRowsWorkspace([hashtable]$Extra = @{}) {
    $files = @{
        'angular.json' = '{"projects":{"shop":{"projectType":"application","root":"apps/shop",' +
            '"sourceRoot":"apps/shop/src","architect":{"build":{"options":{"tsConfig":"apps/shop/tsconfig.app.json"}},' +
            '"test":{"options":{"tsConfig":"apps/shop/tsconfig.spec.json"}},"lint":{}}}}}'
        'apps/shop/tsconfig.json' = '{"references":[{"path":"./tsconfig.app.json"}]}'
        'apps/shop/tsconfig.app.json' = '{"compilerOptions":{"strict":true},"include":["src/**/*.ts"]}'
        'apps/shop/tsconfig.spec.json' = '{"include":["src/**/*.spec.ts"]}'
        'apps/shop/src/main.ts' = "import { Cart as Basket, Item } from './cart';`n" +
            "import type { Money } from './cart';`n" +
            "import * as helpers from './helpers';`n" +
            "import shop from './shop-default';`n" +
            "export const started = true;`n"
        'apps/shop/src/cart.ts' = "export class Cart {}`nexport interface Item { id: number; }`n" +
            "export type Money = number;`n"
        'apps/shop/src/helpers.ts' = "export const help = 1;`n"
        # A quote inside a PowerShell double-quoted string is escaped with a BACKTICK. A backslash is
        # not an escape here, and the file simply stops parsing at the first one.
        'apps/shop/src/basket.component.html' = "<p class=`"intro`">basket</p>`n" +
            "<app-cart *ngIf=`"visible`" [label]=`"'shop.title' | money`"></app-cart>`n" +
            "<span [class.active]=`"isActive`">{{ count }}</span>`n" +
            "@if (visible) { <b>yes</b> }`n" +
            "<app-basket [labelAlias]=`"'x'`"></app-basket>`n"
        # `UnlessDirective` below is STRUCTURAL - it injects a TemplateRef. Without one in the
        # fixture `isStructural` is false on both sides of the registry comparison whatever the
        # reconstruction does, and a mutation that always answered false passed all 88 cases.
        'apps/shop/src/basket.component.ts' = "import { Component, Directive, Injectable, Input, NgModule, Pipe, TemplateRef, input } from '@angular/core';`n" +
            "@Component({ selector: 'app-basket', templateUrl: './basket.component.html', styleUrl: './basket.scss', standalone: true })`n" +
            "export class BasketComponent {`n" +
            "  @Input('labelAlias') label = '';`n" +
            "  @Input({ required: true }) count = 0;`n" +
            "  size = input<number>();`n" +
            "}`n" +
            "@Directive({ selector: '[appHighlight]' })`n" +
            "export class HighlightDirective {}`n" +
            "@Directive({ selector: '[appUnless]' })`n" +
            "export class UnlessDirective {`n" +
            "  constructor(private tpl: TemplateRef<unknown>) {}`n" +
            "}`n" +
            "@Pipe({ name: 'money', pure: false })`n" +
            "export class MoneyPipe {}`n" +
            "@Injectable({ providedIn: 'root' })`n" +
            "export class BasketService {}`n" +
            "const GROUP = [HighlightDirective, MoneyPipe];`n" +
            "@NgModule({ declarations: [BasketComponent], exports: [GROUP] })`n" +
            "export class BasketModule {}`n"
        'apps/shop/src/basket.scss' = ".basket { color: blue; }`n"
        # A routed module, a lazy child, and a state slice - the shapes phase 4 derives its tables from.
        'apps/shop/src/shop.routes.ts' = "import { NgModule } from '@angular/core';`n" +
            "import { RouterModule } from '@angular/router';`n" +
            "import { CartComponent } from './cart.component';`n" +
            "export const routes = [`n" +
            "  { path: 'shop', component: CartComponent, children: [`n" +
            "    { path: 'basket', component: CartComponent },`n" +
            "  ] },`n" +
            "];`n" +
            "@NgModule({ imports: [RouterModule.forRoot(routes)], declarations: [CartComponent],`n" +
            "  bootstrap: [CartComponent] })`n" +
            "export class ShopModule {}`n"
        # The actions live in their own file and the reducer imports them as a NAMESPACE, which is how this
        # shape is really written - and the only way `action_source` carries anything: a same-file reference
        # is RESOLVED by the evaluator to the action's own value, so there is no name left to record.
        'apps/shop/src/shop.actions.ts' = "import { createAction, createActionGroup } from '@ngrx/store';`n" +
            "const source = '[Shop]';`n" +
            'export const reset = createAction(`${source} reset`);' + "`n" +
            "export const events = createActionGroup({ source: 'Shop', events: { setStep: 1 } });`n"
        'apps/shop/src/shop.state.ts' = "import { createReducer, on, createSelector } from '@ngrx/store';`n" +
            "import * as shopActions from './shop.actions';`n" +
            "export const reducer = createReducer({ step: 0 }, on(shopActions.reset, (s) => ({ step: 0 })));`n" +
            "export const stepSelector = createSelector((s) => s.step, (step) => step);`n"
        'apps/shop/src/cart.component.ts' = "import { Component, Input } from '@angular/core';`n" +
            "import { Cart, Item } from './cart';`n" +
            "export abstract class Base { protected readonly baseId = 1; }`n" +
            "@Component({ selector: 'app-cart', template: '<div></div>' })`n" +
            "export class CartComponent extends Base implements Item {`n" +
            "  @Input() label = 'x';`n" +
            "  id = 7;`n" +
            "  private count?: number;`n" +
            "  static VERSION = '1.0';`n" +
            "  constructor(private readonly cart: Cart, public other: Base) { super(); }`n" +
            "  greet(name: string): string { return name; }`n" +
            "  total = 0;`n" +
            "  classify(kind: number): string {`n" +
            "    const label = 'small';`n" +
            "    if (kind > 1) { return 'big'; } else { return label; }`n" +
            "  }`n" +
            "  pick(kind: number): string {`n" +
            "    switch (kind) {`n" +
            "      case 1:`n" +
            "      case 2: return 'low';`n" +
            "      default: return 'high';`n" +
            "    }`n" +
            "  }`n" +
            "  compute(a: number, b: number, ...rest: number[]): number { return a + b; }`n" +
            "  state(): { id: number; label: string } { return { id: 1, label: 'x' }; }`n" +
            "  run(): void {`n" +
            "    const { id, label } = this.state();`n" +
            "    this.total = this.compute(1, 2, 3, 4);`n" +
            "    [1, 2].forEach((n) => n + 1);`n" +
            "    [3].forEach((n) => { const doubled = n * 2; });`n" +
            "  }`n" +
            "}`n"
        'apps/shop/src/values.ts' = "import { Category } from './labels';`n" +
            "export class FlagCodes { public static IsX = 'IsXEnabled'; }`n" +
            "export class Runtime { isOnline = false; }`n" +
            "export const COMPONENTS = [1, 2];`n" +
            "export const ALL = [...COMPONENTS, 3];`n" +
            "export const LEVEL = Category.Hardware;`n" +
            "export const FLAG = FlagCodes.IsX;`n" +
            "export const someFlag = true;`n" +
            "export const MAYBE = someFlag && { id: 1 };`n" +
            "export const ONLINE = new Runtime().isOnline;`n" +
            "export const DYNAMIC = window.location.href;`n"
        'apps/shop/src/types.ts' = "export enum Level { Low = 1, High }`n" +
            "export const enum Flag { On = 'on' }`n" +
            "export interface Row { id: number; cell?: { label: string }; }`n" +
            "export type Rows = { rows: Row[]; meta: { total: number } };`n" +
            "export type Slim = Pick<Row, 'id'>;`n" +
            "export type RowList = Row[];`n"
        'apps/shop/src/labels.ts' = "export enum Category { Hardware = 1, Software = 2 }`n" +
            "export const LABELS = {`n  [Category.Hardware]: 'laptop',`n  [Category.Software]: 'license',`n};`n" +
            "export class Greeter {`n  greet(): string {`n" +
            "    // a business rule lives here`n" +
            "    /* and a block one */`n" +
            "    return 'hello';`n  }`n}`n" +
            "export { Greeter as Welcomer };`n" +
            "export { help } from './helpers';`n"
        'apps/shop/src/shop-default.ts' = "const shop = 1;`nexport default shop;`n"
        # In the tree, in no program: `include` names src/ only. That is what `reachable` is about.
        'apps/shop/extra/dead.ts' = "export const dead = 1;`n"
        # In no program and not even TypeScript: the inventory walk is the only thing that sees it.
        'apps/shop/src/theme.scss' = ".a { color: red; }`n"
        'apps/shop/src/assets/locales/en.json' = '{"shop":{"title":"Shop","cart":{"empty":"Empty"}}}'
        'apps/shop/src/assets/locales/de.json' = '{"shop":{"title":"Shop"}}'
        # `money` is this fixture's translation carrier and `disabled` its visibility property: which pipe
        # carries a key and which DOM property counts as a gate are application knowledge, not something a
        # compiler states.
        'structuregate.ts.json' = '{"locales":["apps/shop/src/assets/locales"],"i18nCarriers":["money"],' +
            '"gateInputs":["disabled"],"featureChecks":["FlagService.isOn"],' +
            '"featureEnum":"FlagCodes"}'
        # THE TWO SHAPES THE CLOSURE RESOLVES A GATE INTO, and nothing else in this tree carries either:
        # a capability that must be GRANTED (`gate_features`) and a finite domain that is NARROWED
        # (`gate_values`). Both are declared here rather than sniffed - see `TsGateFeatures`.
        # THE FEATURE ENUM IS DECLARED OUTSIDE THE MAPPED PROGRAM, as a workspace often declares
        # one - `src/**/*.ts` is what the tsconfig includes, and this sits
        # beside it. That is not a detail: an enum the checker CAN fold is recorded as a resolved value,
        # and one it cannot stays a member reference, which is the shape the feature pass reads.
        'apps/shop/src/features.ts' = "export enum Tier { Basic = 1, Gold = 2 }`n" +
            "export class FlagCodes {`n" +
            "  static readonly IsPromoEnabled = 'IsPromoEnabled';`n" +
            "}`n" +
            "export class FlagService {`n" +
            "  isOn(code: string): boolean { return code !== undefined; }`n" +
            "}`n"
        'apps/shop/src/promo.component.html' = "<b *ngIf=`"promoVisible`">promo</b>`n" +
            "<i *ngIf=`"tier === tiers.Gold`">gold</i>`n"
        'apps/shop/src/promo.component.ts' = "import { Component } from '@angular/core';`n" +
            "import { FlagService, Tier } from './features';`n" +
            "import { FlagCodes } from './features';`n" +
            "@Component({ selector: 'app-promo', templateUrl: './promo.component.html', standalone: true })`n" +
            "export class PromoComponent {`n" +
            "  promoVisible = false;`n" +
            "  tier: Tier = Tier.Basic;`n" +
            "  tiers: typeof Tier = Tier;`n" +
            "  constructor(private readonly features: FlagService) {}`n" +
            "  ngOnInit(): void {`n" +
            "    this.promoVisible = this.features.isOn(FlagCodes.IsPromoEnabled);`n" +
            "  }`n" +
            "}`n"
    }
    foreach ($key in $Extra.Keys) { $files[$key] = $Extra[$key] }
    return Use-Tree $files
}

# The deep map over a tree, with the compiler this half has to borrow and the application knowledge it
# cannot derive. Hands back the database path.
function New-TsRowsDb([string]$Tree) {
    $db = Join-Path $Tree 'map.sqlite'
    $config = Join-Path $Tree 'structuregate.ts.json'
    $result = if (Test-Path $config) {
        Invoke-Gate --root $Tree --map-sqlite $db --ts-node-modules $script:TsRowsModules --ts-config $config
    } else {
        Invoke-Gate --root $Tree --map-sqlite $db --ts-node-modules $script:TsRowsModules
    }
    return [pscustomobject]@{ Db = $db; Result = $result }
}

# One SQL query through the exe's own lens, as rows of text.
function Invoke-TsRowsQ([string]$Path, [string]$Sql) {
    $result = Invoke-Gate --map-query $Path --sql $Sql
    Assert-Exit $result 0
    return $result
}
