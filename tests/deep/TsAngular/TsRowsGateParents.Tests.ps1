<#
    A gate whose restriction lives one hop further out than `TsRowsGateThrough` reads it: a list a directive's
    setter HANDS ON to a method that tests it inside a subscription callback, and an `@Input` flag whose
    meaning is the PARENT's binding. Resolved in the
    render closure - `gate/ts/gate_directive_lists.rs` and `gate/ts/gate_input_flags.rs`.
#>

. (Join-Path $PSScriptRoot '../TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

$script:TsGateParentsSql = "SELECT g.source || ' => ' || v.enum_name || ' ' || v.dimension || ' ' || v.op || ' ' || " +
    "v.values_json AS restriction FROM gate_values v JOIN gates g ON g.id = v.gate"

# THE DIRECTIVE THAT HANDS ITS LIST ON: the setter passes its list to `evaluate`, which tests it in a nested `some` inside
# a `subscribe` callback, ORed with a side that needs the method's `level` truthy - and every call hands
# that parameter `this.level`, a field only an input no template binds writes. `checkOther` is the control:
# a second caller hands its list something else, so what the parameter holds is not known.
Test-Case 'tsrows: a list a directive hands to a method that tests it in a callback restricts the items' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/feed.ts' = "import { Injectable } from '@angular/core';`n" +
            "export enum Kind { Amber = 1, Coral = 2, Jade = 3, Other = 4, More = 5 }`n" +
            "export interface Item { kind: Kind; }`n" +
            "export interface Stream<T> { pipe(...ops: unknown[]): Stream<T>; subscribe(next: (v: T) => void): void; }`n" +
            "@Injectable({ providedIn: 'root' })`n" +
            "export class Feed {`n  select(): Stream<Item[]> { return null as unknown as Stream<Item[]>; }`n}`n" +
            "export function take(n: number): unknown { return n; }`n"
        'apps/shop/src/when-kind.directive.ts' = "import { Directive, Input, TemplateRef, ViewContainerRef } from '@angular/core';`n" +
            "import { Feed, Kind, take } from './feed';`n" +
            "@Directive({ selector: '[whenKind]', standalone: true })`n" +
            "export class WhenKindDirective {`n" +
            "  private _level = 7;`n  private level: number;`n" +
            "  constructor(private templateRef: TemplateRef<unknown>, private viewContainer: ViewContainerRef, private source: Feed) {}`n" +
            "  @Input() set whenKind(kinds: Kind[]) {`n" +
            "    this.evaluate(kinds, this.level);`n  }`n" +
            "  @Input() set whenKindLevel(level: number) {`n    this.level = level;`n  }`n" +
            "  private evaluate(kinds: Kind[], level: number): void {`n" +
            "    this.source.select().pipe(take(1)).subscribe((items) => {`n" +
            "      const matched = items.some((it) => kinds.some((k) => k === it.kind))`n" +
            "        || (level && level === this._level);`n" +
            "      this.viewContainer.clear();`n" +
            "      if (matched) {`n        this.viewContainer.createEmbeddedView(this.templateRef);`n      }`n" +
            "    });`n  }`n" +
            "}`n"
        'apps/shop/src/other-kind.directive.ts' = "import { Directive, Input, TemplateRef, ViewContainerRef } from '@angular/core';`n" +
            "import { Feed, Kind } from './feed';`n" +
            "@Directive({ selector: '[otherKind]', standalone: true })`n" +
            "export class OtherKindDirective {`n" +
            "  constructor(private templateRef: TemplateRef<unknown>, private viewContainer: ViewContainerRef, private source: Feed) {}`n" +
            "  @Input() set otherKind(types: Kind[]) {`n    this.checkOther(types);`n  }`n" +
            "  evaluate(): void {`n    this.checkOther([Kind.More]);`n  }`n" +
            "  private checkOther(types: Kind[]): void {`n" +
            "    this.source.select().subscribe((items) => {`n" +
            "      if (items.some((x) => types.some((i) => i === x.kind))) {`n" +
            "        this.viewContainer.createEmbeddedView(this.templateRef);`n      }`n" +
            "    });`n  }`n" +
            "}`n"
        'apps/shop/src/promo.component.html' = "<b *whenKind=`"[types.Amber]`">{{ 'promo.amber' | money }}</b>`n" +
            "<i *whenKind=`"[types.Coral, types.Jade]`">coral</i>`n" +
            "<u *otherKind=`"[types.Jade]`">other</u>`n"
        'apps/shop/src/promo.component.ts' = "import { Component } from '@angular/core';`n" +
            "import { Kind } from './feed';`n" +
            "import { WhenKindDirective } from './when-kind.directive';`n" +
            "import { OtherKindDirective } from './other-kind.directive';`n" +
            "@Component({ selector: 'app-promo', templateUrl: './promo.component.html', standalone: true,`n" +
            "  imports: [WhenKindDirective, OtherKindDirective] })`n" +
            "export class PromoComponent {`n  types = Kind;`n}`n"
        'apps/shop/src/assets/locales/en.json' = '{"shop":{"title":"Shop","cart":{"empty":"Empty"}},"promo":{"amber":"a"}}'
    }
    $made = New-TsRowsDb $tree
    $r = Invoke-Gate --map-query $made.Db --width 0 --sql $script:TsGateParentsSql
    Assert-Exit $r 0
    Assert-Line $r '[types.Amber] => Kind items.kind in ["Amber"]'
    Assert-Line $r '[types.Coral, types.Jade] => Kind items.kind in ["Coral","Jade"]'
    Assert-Line $r '[types.Jade] => Kind items.kind unknown []'
    $k = Invoke-Gate --map-query $made.Db --width 0 --sql ("SELECT k.key || ' | ' || json_extract(v.value, '$.dimension') || ' ' || " +
        "json_extract(v.value, '$.op') || ' ' || json_extract(v.value, '$.values') AS reach " +
        "FROM key_reach k, json_each(k.always_values) v WHERE k.key = 'promo.amber'")
    Assert-Line $k 'promo.amber | items.kind in ["Amber"]'
}

# THE SAME DIRECTIVE WITH AN ALIASED SETTER: an aliased setter, a `level` that defaults to null, a guard
# around the callback body, and the render reached through `this.show()` called INSIDE the `subscribe` -
# the case above calls `createEmbeddedView` there directly. `ProfileComponent` has an `evaluate` of its OWN and
# fills its second parameter: another class's method, so it must not keep the directive's `level` alive.
Test-Case 'tsrows: a directive that renders through a method called in its callback restricts the items' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/api.ts' = "export enum Kind { Amber = 1, Coral = 2, Jade = 3, Other = 4, More = 5 }`n" +
            "export enum Levels { A = 1, B = 2 }`n" +
            "export interface Item { kind: Kind; }`n" +
            "export interface Stream<T> { pipe(...ops: unknown[]): Stream<T>; subscribe(next: (v: T) => void): void; }`n" +
            "export class Hub<S> { select<R>(f: (s: S) => R): Stream<R> { return null as unknown as Stream<R>; } }`n" +
            "export interface HubState { items: Item[]; }`n" +
            "export const selectors = { all: () => (s: HubState) => s.items };`n" +
            "export function take(n: number): unknown { return n; }`n"
        'apps/shop/src/when-kind.directive.ts' = "import { Directive, Input, TemplateRef, ViewContainerRef } from '@angular/core';`n" +
            "import { Levels, Kind, HubState, selectors, Hub, take } from './api';`n" +
            "@Directive({ selector: '[whenKind]', standalone: true })`n" +
            "export class WhenKindDirective {`n" +
            "  private _level = Levels.A;`n  private _kinds: Kind[] = [];`n  private shown = false;`n" +
            "  @Input('whenKind') set config(kinds: Kind[]) {`n" +
            "    this._kinds = kinds;`n    this.evaluate(kinds);`n  }`n" +
            "  @Input('whenKindLevel') set whenKindLevel(level: Levels) {`n" +
            "    this.evaluate(this._kinds, level);`n  }`n" +
            "  constructor(private templateRef: TemplateRef<unknown>, private viewContainer: ViewContainerRef,`n" +
            "    private source: Hub<HubState>) {}`n" +
            "  private evaluate(kinds: Kind[], level: Levels | null = null) {`n" +
            "    this.source.select((s) => selectors.all()(s)).pipe(take(1)).subscribe((items) => {`n" +
            "      if (!!items) {`n" +
            "        const matched = items.some((it) => kinds.some((k) => k === it.kind)) ||`n" +
            "          (level && level === this._level);`n" +
            "        if (matched && !this.shown) {`n          this.show();`n          this.shown = true;`n" +
            "        } else if (!matched && this.shown) {`n          this.hide();`n          this.shown = false;`n        }`n" +
            "      }`n    });`n  }`n" +
            "  protected hide(): void {`n    this.viewContainer.clear();`n  }`n" +
            "  protected show(): void {`n    this.viewContainer.createEmbeddedView(this.templateRef);`n  }`n" +
            "}`n"
        'apps/shop/src/profile.component.ts' = "import { Component } from '@angular/core';`n" +
            "import { Levels } from './api';`n" +
            "@Component({ selector: 'app-profile', template: '<p>p</p>', standalone: true })`n" +
            "export class ProfileComponent {`n  private seen = 0;`n" +
            "  open(): void {`n    this.evaluate(1, Levels.B);`n  }`n" +
            "  private evaluate(count: number, level: Levels | null = null): void {`n    this.seen = count + (level ?? 0);`n  }`n}`n"
        'apps/shop/src/promo.component.html' = "<b *whenKind=`"[types.Amber]`">{{ 'promo.amber' | money }}</b>`n" +
            "<i *whenKind=`"[types.Coral, types.Jade]`">coral</i>`n"
        'apps/shop/src/promo.component.ts' = "import { Component } from '@angular/core';`n" +
            "import { Kind } from './api';`n" +
            "import { WhenKindDirective } from './when-kind.directive';`n" +
            "@Component({ selector: 'app-promo', templateUrl: './promo.component.html', standalone: true,`n" +
            "  imports: [WhenKindDirective] })`n" +
            "export class PromoComponent {`n  types = Kind;`n}`n"
    }
    $made = New-TsRowsDb $tree
    $r = Invoke-Gate --map-query $made.Db --width 0 --sql $script:TsGateParentsSql
    Assert-Exit $r 0
    Assert-Line $r '[types.Amber] => Kind items.kind in ["Amber"]'
    Assert-Line $r '[types.Coral, types.Jade] => Kind items.kind in ["Coral","Jade"]'
}

# THE @Input FLAGS: `isBasic` and `isProPlan` are `@Input`s of the panel, bound by the ONE parent that renders
# it to its own fields, which `ngOnInit` writes with the comparisons. The panel's `isBasic || isProPlan` is
# then the union of the parent's products and `!isProPlan` their complement; `isLoose`, bound to `isBasic`,
# is the parent's `isBasic`. The case after it is the control: two parents that disagree, or leave it unbound.
Test-Case 'tsrows: an input flag every parent binds alike restricts what the parent compares' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/pro.ts' = "export enum PlanIDs { Basic = 1, ProSolo = 2, ProTeam = 3, Tablet = 4, Desk = 5, Laptop = 6 }`n"
        'apps/shop/src/pro-panel.component.html' = "<b *ngIf=`"isBasic || isProPlan`">{{ 'pro.both' | money }}</b>`n" +
            "<i *ngIf=`"!isProPlan`">not pro</i>`n<u *ngIf=`"isLoose`">loose</u>`n"
        'apps/shop/src/pro-panel.component.ts' = "import { Component, Input } from '@angular/core';`n" +
            "@Component({ selector: 'app-pro-panel', templateUrl: './pro-panel.component.html', standalone: true })`n" +
            "export class ProPanelComponent {`n" +
            "  @Input() isBasic = false;`n  @Input() isProPlan = false;`n  @Input() isLoose = false;`n}`n"
        'apps/shop/src/order-detail.component.html' = "<app-pro-panel [isProPlan]=`"isProPlan`" [isBasic]=`"isBasic`" [isLoose]=`"isBasic`"></app-pro-panel>`n"
        'apps/shop/src/order-detail.component.ts' = "import { Component, OnInit } from '@angular/core';`n" +
            "import { PlanIDs } from './pro';`n" +
            "import { ProPanelComponent } from './pro-panel.component';`n" +
            "@Component({ selector: 'app-order-detail', templateUrl: './order-detail.component.html', standalone: true, imports: [ProPanelComponent] })`n" +
            "export class OrderDetailComponent implements OnInit {`n" +
            "  productID: PlanIDs = PlanIDs.Basic;`n  isBasic = false;`n  isProPlan = false;`n" +
            "  ngOnInit(): void {`n" +
            "    this.isBasic = this.productID === PlanIDs.Basic;`n" +
            "    this.isProPlan = this.productID === PlanIDs.ProSolo || this.productID === PlanIDs.ProTeam;`n" +
            "  }`n}`n"
        'apps/shop/src/assets/locales/en.json' = '{"shop":{"title":"Shop","cart":{"empty":"Empty"}},"pro":{"both":"a"}}'
    }
    $made = New-TsRowsDb $tree
    $r = Invoke-Gate --map-query $made.Db --width 0 --sql $script:TsGateParentsSql
    Assert-Exit $r 0
    Assert-Line $r 'isBasic || isProPlan => PlanIDs productID in ["Basic","ProSolo","ProTeam"]'
    Assert-Line $r '!isProPlan => PlanIDs productID not_in ["ProSolo","ProTeam"]'
    Assert-Line $r 'isLoose => PlanIDs productID in ["Basic"]'
    $k = Invoke-Gate --map-query $made.Db --width 0 --sql ("SELECT k.key || ' | ' || json_extract(v.value, '$.op') || ' ' || " +
        "json_extract(v.value, '$.values') AS reach FROM key_reach k, json_each(k.always_values) v WHERE k.key = 'pro.both'")
    Assert-Line $k 'pro.both | in ["Basic","ProSolo","ProTeam"]'
}

Test-Case 'tsrows: an input flag two parents bind differently, or one leaves unbound, restricts nothing' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/pro.ts' = "export enum PlanIDs { Basic = 1, ProSolo = 2, ProTeam = 3, Tablet = 4 }`n"
        'apps/shop/src/pro-panel.component.html' = "<i *ngIf=`"!isProPlan`">not pro</i>`n<u *ngIf=`"isBasic`">basic</u>`n"
        'apps/shop/src/pro-panel.component.ts' = "import { Component, Input } from '@angular/core';`n" +
            "@Component({ selector: 'app-pro-panel', templateUrl: './pro-panel.component.html', standalone: true })`n" +
            "export class ProPanelComponent {`n  @Input() isBasic = false;`n  @Input() isProPlan = false;`n}`n"
        'apps/shop/src/a.component.html' = "<app-pro-panel [isProPlan]=`"isProPlan`" [isBasic]=`"isBasic`"></app-pro-panel>`n"
        'apps/shop/src/b.component.html' = "<app-pro-panel [isProPlan]=`"isProPlan`"></app-pro-panel>`n"
        'apps/shop/src/a.component.ts' = "import { Component, OnInit } from '@angular/core';`n" +
            "import { PlanIDs } from './pro';`n" +
            "import { ProPanelComponent } from './pro-panel.component';`n" +
            "@Component({ selector: 'app-a', templateUrl: './a.component.html', standalone: true, imports: [ProPanelComponent] })`n" +
            "export class AComponent implements OnInit {`n  productID: PlanIDs = PlanIDs.Basic;`n  isBasic = false;`n  isProPlan = false;`n" +
            "  ngOnInit(): void {`n    this.isBasic = this.productID === PlanIDs.Basic;`n" +
            "    this.isProPlan = this.productID === PlanIDs.ProSolo;`n  }`n}`n"
        'apps/shop/src/b.component.ts' = "import { Component, OnInit } from '@angular/core';`n" +
            "import { PlanIDs } from './pro';`n" +
            "import { ProPanelComponent } from './pro-panel.component';`n" +
            "@Component({ selector: 'app-b', templateUrl: './b.component.html', standalone: true, imports: [ProPanelComponent] })`n" +
            "export class BComponent implements OnInit {`n  productID: PlanIDs = PlanIDs.Basic;`n  isProPlan = false;`n" +
            "  ngOnInit(): void {`n    this.isProPlan = this.productID === PlanIDs.ProTeam;`n  }`n}`n"
    }
    $made = New-TsRowsDb $tree
    $r = Invoke-Gate --map-query $made.Db --width 0 --sql $script:TsGateParentsSql
    Assert-Exit $r 0
    Assert-NoLine $r '!isProPlan =>'
    Assert-NoLine $r 'isBasic =>'
}

}
