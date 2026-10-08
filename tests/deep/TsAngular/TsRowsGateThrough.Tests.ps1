<#
    What a gate restricts when the restriction is not in the gate's own text: a flag read through a NEGATION or
    an OR, a CALL to a function whose one return compares, a flag assigned from a SERVICE method, and a LIST a
    template hands to a structural directive. Each is resolved in the render closure -
    `rust/fbtcore/src/rows/ts/gate/ts/gate_props.rs`, `gate_calls.rs`, `gate_directive_lists.rs` and the
    disjunction in `featurewalk.rs` / `gate_collapse.rs`. Its helpers are `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '../TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

# One query for the three cases: every restriction a gate of the tree carries, by its source.
$script:TsGateThroughSql = "SELECT g.source || ' => ' || v.enum_name || ' ' || v.dimension || ' ' || v.op || ' ' || " +
    "v.values_json AS restriction FROM gate_values v JOIN gates g ON g.id = v.gate"

# THE FLAG SHAPES. `isBig` is written once in `ngOnInit`, which runs before the view is first
# read, so its NEGATION is as exact as the flag: `not_in`. `isLater` is written in a click handler that may
# never run, so `!isLater` proves nothing and stays unread. An OR is the union of what BOTH sides permit,
# and restricts nothing on a dimension one side says nothing of. A call to a method of one return - its
# parameter typed `number`, its body an OR of equalities - reads as that return, and so does a call through a
# property holding an imported arrow.
Test-Case 'tsrows: a negated flag, an or, and a call to a one-return function restrict like the flag itself' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/sizes.ts' = "export enum Size { Huge = 1, Large = 2, Small = 3, Medium = 4, Mini = 5, Tiny = 6 }`n" +
            "export const isBigOne = (id: Size) => [Size.Huge, Size.Large].includes(id);`n"
        'apps/shop/src/size.component.html' = "<b *ngIf=`"isBig`">plain</b>`n" +
            "<i *ngIf=`"!isBig`">negated</i>`n" +
            "<u *ngIf=`"isSmall || isBig`">either</u>`n" +
            "<s *ngIf=`"isBigSize(size)`">call</s>`n" +
            "<em *ngIf=`"model.edited && !isBig`">and</em>`n" +
            "<q *ngIf=`"isLarge(size)`">imported</q>`n" +
            "<a *ngIf=`"isBig || model.edited`">half</a>`n" +
            "<p *ngIf=`"!isLater`">not later</p>`n" +
            "<p *ngIf=`"isLater`">later</p>`n"
        'apps/shop/src/size.component.ts' = "import { Component, OnInit } from '@angular/core';`n" +
            "import { Size, isBigOne } from './sizes';`n" +
            "@Component({ selector: 'app-size', templateUrl: './size.component.html', standalone: true })`n" +
            "export class SizeComponent implements OnInit {`n" +
            "  size: Size = Size.Medium;`n" +
            "  model = { edited: false };`n" +
            "  isBig = false;`n" +
            "  isSmall = false;`n" +
            "  isLater = false;`n" +
            "  isLarge = isBigOne;`n" +
            "  ngOnInit() {`n" +
            "    this.isBig = this.size === Size.Huge || this.size === Size.Large;`n" +
            "    this.isSmall = this.size === Size.Small;`n" +
            "  }`n" +
            "  onClick() {`n    this.isLater = this.size === Size.Tiny;`n  }`n" +
            "  isBigSize(id: number): boolean {`n" +
            "    return id === Size.Huge || id === Size.Large;`n" +
            "  }`n" +
            "}`n"
    }
    $made = New-TsRowsDb $tree
    $r = Invoke-Gate --map-query $made.Db --width 0 --sql $script:TsGateThroughSql
    Assert-Exit $r 0
    Assert-Line $r 'isBig => Size size in ["Huge","Large"]'
    Assert-Line $r '!isBig => Size size not_in ["Huge","Large"]'
    Assert-Line $r 'isSmall || isBig => Size size in ["Huge","Large","Small"]'
    Assert-Line $r 'isBigSize(size) => Size size in ["Huge","Large"]'
    Assert-Line $r 'model.edited && !isBig => Size size not_in ["Huge","Large"]'
    Assert-Line $r 'isLarge(size) => Size size in ["Huge","Large"]'
    Assert-NoLine $r 'isBig || model.edited =>'
    Assert-Line $r 'isLater => Size size in ["Tiny"]'
    Assert-NoLine $r '!isLater =>'
}

# A FLAG ASSIGNED FROM A SERVICE METHOD is read through the method's one return, wherever the service is:
# the service compares its own fields, so the dimensions are the service's. Written once in `ngOnInit` or as an
# initializer nothing overwrites - both are what the flag holds when the view is read - and the key under the
# gate carries the restriction into `key_reach`.
$script:TsGateThroughBeta = @{
        'apps/shop/src/ids.ts' = "export enum Size { Large = 1, Medium = 2, Small = 3, Tiny = 4 }`n" +
            "export enum Brand { North = 1, South = 2, East = 3 }`n"
        'apps/shop/src/beta.service.ts' = "import { Injectable } from '@angular/core';`n" +
            "import { Brand, Size } from './ids';`n" +
            "@Injectable({ providedIn: 'root' })`n" +
            "export class BetaService {`n" +
            "  brand: Brand = Brand.East;`n" +
            "  size: Size = Size.Large;`n" +
            "  isBetaEnabled(): boolean {`n" +
            "    return this.brand === Brand.North && [Size.Large, Size.Medium].includes(this.size);`n" +
            "  }`n" +
            "}`n"
        'apps/shop/src/beta.component.html' = "<b *ngIf=`"showNorth && model.selected`">{{ 'beta.north' | money }}</b>`n" +
            "<i *ngIf=`"showNorthNow`">now</i>`n"
        'apps/shop/src/beta.component.ts' = "import { Component, OnInit } from '@angular/core';`n" +
            "import { BetaService } from './beta.service';`n" +
            "@Component({ selector: 'app-beta', templateUrl: './beta.component.html', standalone: true })`n" +
            "export class BetaComponent implements OnInit {`n" +
            "  showNorth = false;`n" +
            "  showNorthNow = this.betaService.isBetaEnabled();`n" +
            "  model = { selected: false };`n" +
            "  constructor(private betaService: BetaService) {}`n" +
            "  ngOnInit() {`n" +
            "    this.showNorth = this.betaService.isBetaEnabled();`n" +
            "  }`n" +
            "}`n"
        'apps/shop/src/assets/locales/en.json' = '{"shop":{"title":"Shop","cart":{"empty":"Empty"}},"beta":{"north":"k"}}'
}

Test-Case 'tsrows: a flag assigned from a service method restricts what the method returns' {
    $made = New-TsRowsDb (New-TsRowsWorkspace $script:TsGateThroughBeta)
    $r = Invoke-Gate --map-query $made.Db --width 0 --sql $script:TsGateThroughSql
    Assert-Exit $r 0
    Assert-Line $r 'showNorth && model.selected => Brand brand in ["North"]'
    Assert-Line $r 'showNorth && model.selected => Size size in ["Large","Medium"]'
    Assert-Line $r 'showNorthNow => Brand brand in ["North"]'
    $k = Invoke-Gate --map-query $made.Db --width 0 --sql ("SELECT k.key || ' | ' || json_extract(v.value, '$.dimension') || ' ' || " +
        "json_extract(v.value, '$.op') || ' ' || json_extract(v.value, '$.values') AS reach " +
        "FROM key_reach k, json_each(k.always_values) v WHERE k.key = 'beta.north'")
    Assert-Line $k 'beta.north | brand in ["North"]'
    Assert-Line $k 'beta.north | size in ["Large","Medium"]'
}

# A PARTIAL RUN reads the edited service alone, and the closure - derived again from every row - moves the
# restriction of a flag in a component that did not change.
Test-Case 'tsrows: a service method edited alone moves the restriction of the flag assigned from it' {
    $tree = New-TsRowsWorkspace $script:TsGateThroughBeta
    Assert-Exit (New-TsRowsDb $tree).Result 0
    $service = Join-Path $tree 'apps/shop/src/beta.service.ts'
    Set-Content -LiteralPath $service -Value ([IO.File]::ReadAllText($service).Replace('Brand.North &&', 'Brand.South &&'))
    $again = New-TsRowsDb $tree
    Assert-Line $again.Result 'put back'
    $r = Invoke-Gate --map-query $again.Db --width 0 --sql $script:TsGateThroughSql
    Assert-Line $r 'showNorth && model.selected => Brand brand in ["South"]'
    Assert-Line $r 'showNorthNow => Brand brand in ["South"]'
    Assert-NoLine $r '["North"]'
}

# A LIST THE TEMPLATE HANDS TO A STRUCTURAL DIRECTIVE: `*whenKind="[Kind.Amber]"`
# over a directive that renders only when some item's kind is in the list it was handed. The render test
# sits in a `const` local and is ORed with a side that needs `whenKindLevel` truthy - an
# input no template here binds, so that side never holds. A list that is not the enum's constants keeps the
# dimension and states no value; `isKind` tests its list for a member of its own class.
Test-Case 'tsrows: a list handed to a structural directive restricts what the directive tests it against' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/kinds.ts' = "export enum Kind { Amber = 1, Coral = 2, Jade = 3, Other = 4, More = 5 }`n" +
            "export interface Item { kind: Kind; level: number; }`n" +
            "export const settings = { kind: Kind.Amber };`n"
        'apps/shop/src/when-kind.directive.ts' = "import { Directive, Input, TemplateRef, ViewContainerRef } from '@angular/core';`n" +
            "import { Item, Kind } from './kinds';`n" +
            "@Directive({ selector: '[whenKind]', standalone: true })`n" +
            "export class WhenKindDirective {`n" +
            "  private kinds: Kind[] = [];`n" +
            "  private level: number | undefined;`n" +
            "  private items: Item[] = [];`n" +
            "  constructor(private templateRef: TemplateRef<any>, private viewContainer: ViewContainerRef) {}`n" +
            "  @Input() set whenKind(types: Kind[]) {`n" +
            "    this.kinds = types;`n    this.sync();`n  }`n" +
            "  @Input() set whenKindLevel(level: number) {`n" +
            "    this.level = level;`n    this.sync();`n  }`n" +
            "  private sync() {`n" +
            "    this.viewContainer.clear();`n" +
            "    const matched = this.items.some((it) => this.kinds.includes(it.kind))`n" +
            "      || (!!this.level && this.items.some((it) => it.level === this.level));`n" +
            "    if (matched) {`n      this.viewContainer.createEmbeddedView(this.templateRef);`n    }`n" +
            "  }`n" +
            "}`n"
        'apps/shop/src/is-kind.directive.ts' = "import { Directive, Input, TemplateRef, ViewContainerRef } from '@angular/core';`n" +
            "import { Kind, settings } from './kinds';`n" +
            "@Directive({ selector: '[isKind]', standalone: true })`n" +
            "export class IsKindDirective {`n" +
            "  private types: Kind[] = [];`n" +
            "  private current: Kind = settings.kind;`n" +
            "  constructor(private templateRef: TemplateRef<any>, private viewContainer: ViewContainerRef) {}`n" +
            "  @Input() set isKind(types: Kind[]) {`n" +
            "    this.types = types;`n" +
            "    this.viewContainer.clear();`n" +
            "    if (this.types.includes(this.current)) {`n      this.viewContainer.createEmbeddedView(this.templateRef);`n    }`n" +
            "  }`n" +
            "}`n"
        'apps/shop/src/promo.component.html' = "<b *whenKind=`"[Kind.Amber]`">{{ 'promo.amber' | money }}</b>`n" +
            "<i *whenKind=`"[Kind.Coral, Kind.Jade]`">coral or jade</i>`n" +
            "<u *whenKind=`"chosen`">runtime</u>`n" +
            "<s *isKind=`"[Kind.Jade]`">jade here</s>`n"
        'apps/shop/src/promo.component.ts' = "import { Component } from '@angular/core';`n" +
            "import { Kind } from './kinds';`n" +
            "import { WhenKindDirective } from './when-kind.directive';`n" +
            "import { IsKindDirective } from './is-kind.directive';`n" +
            "@Component({ selector: 'app-promo', templateUrl: './promo.component.html', standalone: true,`n" +
            "  imports: [WhenKindDirective, IsKindDirective] })`n" +
            "export class PromoComponent {`n" +
            "  Kind = Kind;`n" +
            "  chosen: Kind[] = [];`n" +
            "}`n"
        'apps/shop/src/assets/locales/en.json' = '{"shop":{"title":"Shop","cart":{"empty":"Empty"}},"promo":{"amber":"a"}}'
    }
    $made = New-TsRowsDb $tree
    $r = Invoke-Gate --map-query $made.Db --width 0 --sql $script:TsGateThroughSql
    Assert-Exit $r 0
    Assert-Line $r '[Kind.Amber] => Kind items.kind in ["Amber"]'
    Assert-Line $r '[Kind.Coral, Kind.Jade] => Kind items.kind in ["Coral","Jade"]'
    Assert-Line $r 'chosen => Kind items.kind unknown []'
    Assert-Line $r '[Kind.Jade] => Kind current in ["Jade"]'
    $k = Invoke-Gate --map-query $made.Db --width 0 --sql ("SELECT k.key || ' | ' || json_extract(v.value, '$.dimension') || ' ' || " +
        "json_extract(v.value, '$.op') || ' ' || json_extract(v.value, '$.values') AS reach " +
        "FROM key_reach k, json_each(k.always_values) v WHERE k.key = 'promo.amber'")
    Assert-Line $k 'promo.amber | items.kind in ["Amber"]'
}

}
