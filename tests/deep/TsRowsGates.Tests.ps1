<#
    What the render closure resolves a GATE into, beyond the cases `TsRows` holds: the shapes a gate is
    written in that are not a comparison. Split from `TsRows` when that suite reached its size limit.
#>

. (Join-Path $PSScriptRoot 'TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

# A RESTRICTION WRITTEN AS A CALL is read when the called method's own body proves it tests its argument
# for membership - never by the method's name. The negated call is the other half of the rule: none of its
# arguments is active. `isPlanHidden` spells the same call and tests the opposite, so it must stay unread.
Test-Case 'tsrows: a gate calling a membership predicate over enum constants restricts that set' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/plans.component.html' = "<b *ngIf=`"isPlanActive([plans.Pro, plans.Team])`">pro</b>`n" +
            "<i *ngIf=`"!isPlanActive([plans.Free])`">paid</i>`n" +
            "<u *ngIf=`"isPlanHidden([plans.Free])`">hidden</u>`n"
        'apps/shop/src/plans.component.ts' = "import { Component } from '@angular/core';`n" +
            "export enum Plan { Free = 1, Pro = 2, Team = 3, Gold = 4 }`n" +
            "@Component({ selector: 'app-plans', templateUrl: './plans.component.html', standalone: true })`n" +
            "export class PlansComponent {`n" +
            "  plans = Plan;`n" +
            "  active: Plan[] = [];`n" +
            "  isPlanActive(ids: Plan[]): boolean {`n" +
            "    return ids.map((i) => this.active.includes(i)).some((e) => e);`n" +
            "  }`n" +
            "  isPlanHidden(ids: Plan[]): boolean {`n" +
            "    return ids.every((i) => !this.active.includes(i));`n" +
            "  }`n" +
            "}`n"
    }
    $made = New-TsRowsDb $tree
    $r = Invoke-TsRowsQ $made.Db ("SELECT g.source || ' => ' || v.dimension || ' ' || v.op || ' ' || " +
        "v.values_json AS restriction FROM gate_values v JOIN gates g ON g.id = v.gate " +
        "WHERE v.enum_name = 'Plan'")
    Assert-Line $r 'isPlanActive([plans.Pro, plans.Team]) => active in ["Pro","Team"]'
    Assert-Line $r '!isPlanActive([plans.Free]) => active not_in ["Free"]'
    $hidden = Invoke-TsRowsQ $made.Db ("SELECT count(*) AS read FROM gate_values v JOIN gates g " +
        "ON g.id = v.gate WHERE g.source LIKE 'isPlanHidden%'")
    Assert-Line $hidden '0'
}

# A PREDICATE THAT HANDS ITS ARGUMENT TO A KEYED LOOKUP proves one direction only: true means the item was
# found, false may mean it was found and switched off. So the call restricts the collection's key field,
# and its negation names the dimension with no value.
Test-Case 'tsrows: a gate calling a method that delegates to a keyed lookup restricts the key field' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/items.functions.ts' = "export enum Shade { Red = 1, Green = 2, Blue = 3, Black = 4 }`n" +
            "export interface Swatch { shade: Shade; shown: boolean; pinned: boolean; }`n" +
            "export const isShown = (key: Shade, items: Swatch[]) => {`n" +
            "  const found = items.find((item) => item.shade === key);`n" +
            "  if (!found) {`n    return false;`n  }`n" +
            "  return found.shown || found.pinned;`n" +
            "};`n"
        'apps/shop/src/items.component.html' = "<b *ngIf=`"isShown(Shade.Red)`">red</b>`n" +
            "<i *ngIf=`"!isShown(Shade.Green)`">no green</i>`n"
        'apps/shop/src/items.component.ts' = "import { Component } from '@angular/core';`n" +
            "import { Swatch, Shade, isShown } from './items.functions';`n" +
            "@Component({ selector: 'app-items', templateUrl: './items.component.html', standalone: true })`n" +
            "export class ItemsComponent {`n" +
            "  Shade = Shade;`n" +
            "  model: { swatches: Swatch[] } = { swatches: [] };`n" +
            "  isShown(item: Shade) {`n" +
            "    return isShown(item, this.model.swatches);`n" +
            "  }`n" +
            "}`n"
    }
    $made = New-TsRowsDb $tree
    $r = Invoke-TsRowsQ $made.Db ("SELECT g.source || ' => ' || v.dimension || ' ' || v.op || ' ' || " +
        "v.values_json AS restriction FROM gate_values v JOIN gates g ON g.id = v.gate " +
        "WHERE v.enum_name = 'Shade'")
    Assert-Line $r 'isShown(Shade.Red) => model.swatches.shade in ["Red"]'
    Assert-Line $r '!isShown(Shade.Green) => model.swatches.shade unknown []'
}

# A DIRECTIVE THAT RENDERS ONLY WHEN ITS CONFIG'S VALUE IS IN ITS CONFIG'S LIST restricts that value, and the
# proof is its render path: the base class copies the config into fields and returns early unless the value
# is found. An empty list hides nothing, so that occurrence restricts nothing.
Test-Case 'tsrows: a directive rendering only when its value is in its list restricts the value' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/guard.base.ts' = "import { TemplateRef, ViewContainerRef } from '@angular/core';`n" +
            "export enum Grade { A = 1, B = 2, C = 3, D = 4 }`n" +
            "export interface GuardConfig { value?: Grade; list?: Grade[]; }`n" +
            "export class GuardBase {`n" +
            "  protected _list: Grade[] | undefined;`n" +
            "  protected _value: Grade | undefined;`n" +
            "  constructor(public templateRef: TemplateRef<any>, public viewContainer: ViewContainerRef) {}`n" +
            "  protected copy(config: GuardConfig) {`n" +
            "    this._list = config.list;`n" +
            "    this._value = config.value;`n" +
            "  }`n" +
            "  protected check() {`n" +
            "    this.viewContainer.clear();`n" +
            "    if (!this._value || (this._list && this._list.length > 0 && this._list.find((x) => x === this._value) === undefined)) {`n" +
            "      return;`n" +
            "    }`n" +
            "    this.render();`n" +
            "  }`n" +
            "  protected render() {`n" +
            "    this.viewContainer.createEmbeddedView(this.templateRef);`n" +
            "  }`n" +
            "}`n"
        'apps/shop/src/guard.directive.ts' = "import { Directive, Input, TemplateRef, ViewContainerRef } from '@angular/core';`n" +
            "import { GuardBase, GuardConfig } from './guard.base';`n" +
            "@Directive({ selector: '[appGuard]', standalone: true })`n" +
            "export class GuardDirective extends GuardBase {`n" +
            "  @Input('appGuard') set config(config: GuardConfig) {`n" +
            "    if (!config) {`n      return;`n    }`n" +
            "    this.copy(config);`n" +
            "    this.check();`n" +
            "  }`n" +
            "  constructor(templateRef: TemplateRef<any>, viewContainer: ViewContainerRef) {`n" +
            "    super(templateRef, viewContainer);`n" +
            "  }`n" +
            "}`n"
        'apps/shop/src/grades.component.html' = "<b *appGuard=`"{value: current, list: [grades.A, grades.B]}`">ab</b>`n" +
            "<i *appGuard=`"{value: current, list: []}`">any</i>`n"
        'apps/shop/src/grades.component.ts' = "import { Component } from '@angular/core';`n" +
            "import { GuardDirective } from './guard.directive';`n" +
            "import { Grade } from './guard.base';`n" +
            "@Component({ selector: 'app-grades', templateUrl: './grades.component.html', standalone: true, imports: [GuardDirective] })`n" +
            "export class GradesComponent {`n" +
            "  grades = Grade;`n" +
            "  current: Grade = Grade.A;`n" +
            "}`n"
    }
    $made = New-TsRowsDb $tree
    $r = Invoke-TsRowsQ $made.Db ("SELECT g.source || ' => ' || v.dimension || ' ' || v.op || ' ' || " +
        "v.values_json AS restriction FROM gate_values v JOIN gates g ON g.id = v.gate WHERE v.enum_name = 'Grade'")
    Assert-Line $r '{value: current, list: [grades.A, grades.B]} => current in ["A","B"]'
    Assert-NoLine $r 'list: []}'
}

# A CONFIG OBJECT'S LISTS ARE RESOLVED WHATEVER THE DIRECTIVE DOES WITH THEM. The exclusion list is
# tested under a `!`, so the map cannot say whether it hides or only disables - its `op` stays `unknown` - but
# its members are a fact of the source and land in `listed_json`. An EMPTY list is listed as `[]`, and its
# `op` stays `unknown`: what an empty list means is the directive's choice. A CONDITIONAL config is read per
# branch: a member every branch lists permits the union, and one no branch writes is not a dimension at all.
Test-Case 'tsrows: a config object lists its exclusion members, and a conditional config is read per branch' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/visibility.directive.ts' = "import { Directive, Input, TemplateRef, ViewContainerRef } from '@angular/core';`n" +
            "export enum Colors { Blue = 1, Red = 2, Teal = 3, Gray = 4, Pink = 5 }`n" +
            "export interface ColorFilter { include: Colors[]; exclude: Colors[]; }`n" +
            "@Directive({ selector: '[colorFilter]', standalone: true })`n" +
            "export class ColorFilterDirective {`n" +
            "  current: Colors = Colors.Red;`n" +
            "  enabled = true;`n" +
            "  constructor(private templateRef: TemplateRef<any>, private viewContainer: ViewContainerRef) {}`n" +
            "  @Input() set colorFilter(input: ColorFilter) {`n" +
            "    this.viewContainer.clear();`n" +
            "    this.enabled = !input.exclude.some((id) => id === this.current);`n" +
            "    if (input.include.some((m) => m === this.current)) {`n" +
            "      this.viewContainer.createEmbeddedView(this.templateRef, { enabled: this.enabled });`n" +
            "    }`n" +
            "  }`n" +
            "}`n"
        'apps/shop/src/offer.component.html' = "<b *colorFilter=`"{include: [colors.Red, colors.Blue]}`">plain</b>`n" +
            "<i *colorFilter=`"{include: [], exclude: [colors.Gray, colors.Pink]}`">excluded</i>`n" +
            "<u *colorFilter=`"wide ? {include: [colors.Red]} : {include: [colors.Teal]}`">either</u>`n" +
            "<s *colorFilter=`"wide ? {include: [colors.Red]} : {exclude: [colors.Gray]}`">half</s>`n"
        'apps/shop/src/offer.component.ts' = "import { Component } from '@angular/core';`n" +
            "import { Colors, ColorFilterDirective } from './visibility.directive';`n" +
            "@Component({ selector: 'app-offer', templateUrl: './offer.component.html', standalone: true, imports: [ColorFilterDirective] })`n" +
            "export class OfferComponent {`n" +
            "  colors = Colors;`n" +
            "  wide = false;`n" +
            "}`n"
    }
    $made = New-TsRowsDb $tree
    # `--width 0`: a row here is longer than the display cuts a cell at.
    $r = Invoke-Gate --map-query $made.Db --width 0 --sql ("SELECT g.source || ' => ' || v.dimension || ' ' || " +
        "v.op || ' ' || v.values_json || ' listed ' || coalesce(v.listed_json, 'null') AS restriction " +
        "FROM gate_values v JOIN gates g ON g.id = v.gate WHERE v.enum_name = 'Colors'")
    Assert-Exit $r 0
    Assert-Line $r '{include: [colors.Red, colors.Blue]} => colorFilter.include in ["Blue","Red"] listed ["Blue","Red"]'
    Assert-Line $r 'colors.Pink]} => colorFilter.exclude unknown [] listed ["Gray","Pink"]'
    Assert-Line $r 'colors.Pink]} => colorFilter.include unknown [] listed []'
    Assert-Line $r '{include: [colors.Teal]} => colorFilter.include in ["Red","Teal"] listed ["Red","Teal"]'
    Assert-NoLine $r '{include: [colors.Teal]} => colorFilter.exclude'
    # A member ONE branch omits is unread rather than restricted: the other branch says nothing of it.
    Assert-Line $r '{exclude: [colors.Gray]} => colorFilter.include unknown [] listed ["Red"]'
    Assert-Line $r '{exclude: [colors.Gray]} => colorFilter.exclude unknown [] listed ["Gray"]'
}

# AN ARROW-FUNCTION PROPERTY IS A METHOD. `isUs = (id) => [...].some((x) => x === id)` was published
# only as the member's `$fn` value: no `returns` row keyed on the member and no `params`, so the call in the
# template restricted nothing, while a method spelled with the same body did. A CONCISE body is its own return,
# implicit; a BLOCK body's `return` is one as it always was, and both need the parameter to be followed.
Test-Case 'tsrows: an arrow-function property returns like a method and its call restricts the product' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/cedar.component.html' = "<b *ngIf=`"isUs(productID)`">us</b>`n" +
            "<i *ngIf=`"!isEu(productID)`">not eu</i>`n" +
            "<u *ngIf=`"isMethod(productID)`">method</u>`n"
        'apps/shop/src/cedar.component.ts' = "import { Component } from '@angular/core';`n" +
            "export enum PlanIDs { UsA = 1, UsB = 2, EuA = 3, Other = 4 }`n" +
            "export const isOther = (id: PlanIDs) => id === PlanIDs.Other;`n" +
            "@Component({ selector: 'app-cedar', templateUrl: './cedar.component.html', standalone: true })`n" +
            "export class CedarComponent {`n" +
            "  productID: PlanIDs = PlanIDs.UsA;`n" +
            "  isUs = (productID: PlanIDs) => [PlanIDs.UsA, PlanIDs.UsB].some((x) => x === productID);`n" +
            "  isEu = (productID: PlanIDs): boolean => {`n    return [PlanIDs.EuA].includes(productID);`n  };`n" +
            "  isMethod(productID: PlanIDs) {`n" +
            "    return [PlanIDs.UsA, PlanIDs.UsB].some((x) => x === productID);`n  }`n" +
            "}`n"
    }
    $made = New-TsRowsDb $tree
    $r = Invoke-Gate --map-query $made.Db --width 0 --sql ("SELECT m.name || ' | ' || coalesce(r.implicit, 0) || ' | ' || " +
        "json_extract(r.value, '$.`$method') || ' | ' || json_extract(r.value, '$.`$receiver') || ' | ' || " +
        "json_array_length(m.params) AS ret FROM returns r JOIN members m ON m.id = r.member " +
        "WHERE m.name IN ('isUs', 'isEu', 'isMethod')")
    Assert-Exit $r 0
    Assert-Line $r 'isUs | 1 | some | [{"$enum":"PlanIDs.UsA","value":1},{"$enum":"PlanIDs.UsB","value":2}] | 1'
    Assert-Line $r 'isEu | 0 | includes | [{"$enum":"PlanIDs.EuA","value":3}] | 1'
    Assert-Line $r 'isMethod | 0 | some | [{"$enum":"PlanIDs.UsA","value":1},{"$enum":"PlanIDs.UsB","value":2}] | 1'
    # ...a function-valued `const` with a concise body is the same shape, under its `functions` row.
    $fn = Invoke-TsRowsQ $made.Db ("SELECT f.name || ' returns ' || r.source AS ret FROM returns r " +
        "JOIN functions f ON f.id = r.member WHERE f.name = 'isOther'")
    Assert-Line $fn 'isOther returns id === PlanIDs.Other'
    # ...and the call to it is the restriction its body proves, exactly as the method's is.
    $g = Invoke-TsRowsQ $made.Db ("SELECT g.source || ' => ' || v.dimension || ' ' || v.op || ' ' || " +
        "v.values_json AS restriction FROM gate_values v JOIN gates g ON g.id = v.gate WHERE v.enum_name = 'PlanIDs'")
    Assert-Line $g 'isUs(productID) => productID in ["UsA","UsB"]'
    Assert-Line $g '!isEu(productID) => productID not_in ["EuA"]'
    Assert-Line $g 'isMethod(productID) => productID in ["UsA","UsB"]'
}

# A GETTER RETURNING UNDER ONE `if` AND `false` AFTER IT is `C && E`: only one return was ever inlined, so the
# enum test guarding the only true return was lost. A getter that can return anything else after the `if` stays
# unread - its true side does not need `C`.
Test-Case 'tsrows: a getter whose only true return sits under an enum test restricts by that test' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/fern.component.html' = "<b *ngIf=`"isAlphaRemote`">a</b>`n<i *ngIf=`"isAlphaOrAny`">b</i>`n"
        'apps/shop/src/fern.component.ts' = "import { Component } from '@angular/core';`n" +
            "export enum Tone { Alpha = 1, Beta = 2, Gamma = 3, Delta = 4 }`n" +
            "@Component({ selector: 'app-fern', templateUrl: './fern.component.html', standalone: true })`n" +
            "export class FernComponent {`n" +
            "  result: { item: { tone: Tone } } | null = null;`n  mode = '';`n" +
            "  get isAlphaRemote(): boolean {`n" +
            "    if (this.result != null && this.result?.item.tone === Tone.Alpha) {`n" +
            "      return this.mode !== 'LOCAL';`n    }`n    return false;`n  }`n" +
            "  get isAlphaOrAny(): boolean {`n" +
            "    if (this.result?.item.tone === Tone.Alpha) {`n      return this.mode !== 'LOCAL';`n    }`n" +
            "    return true;`n  }`n" +
            "}`n"
    }
    $made = New-TsRowsDb $tree
    $g = Invoke-TsRowsQ $made.Db ("SELECT g.source || ' => ' || v.dimension || ' ' || v.op || ' ' || " +
        "v.values_json AS restriction FROM gate_values v JOIN gates g ON g.id = v.gate WHERE v.enum_name = 'Tone'")
    Assert-Line $g 'isAlphaRemote => '
    Assert-Line $g ' in ["Alpha"]'
    Assert-NoLine $g 'isAlphaOrAny'
}

# AN OBSERVABLE OF A SELECTOR FACTORY handed one enum member: `*ngIf="alpha$ | async"` over
# `alpha$ = this.store.select(hasItemOfTone(Tone.Alpha))`, the factory's projector testing its items for that member.
# True means an item of that member is in the store - a SET row, the true side only: `async` is null before the first
# emit, so a negated one proves nothing. A projector may hand `items.filter(...)` to a helper whose one return is
# `list.some(...)`. A projector that tests anything else, and a property written again, stay unread.
Test-Case 'tsrows: an async selector built for one enum member restricts the set it tests' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/moss.selectors.ts' = "import { createSelector } from '@ngrx/store';`n" +
            "export enum Tone { Alpha = 'Alpha', Beta = 'Beta', Gamma = 'Gamma' }`n" +
            "const selectAll = (s: { items: { tone: Tone; on: boolean }[] }) => s.items;`n" +
            "export const hasItemOfTone = (tone: Tone) => createSelector(selectAll, (items) => items.some((i) => i.tone === tone));`n" +
            "export const anyItemOn = (tone: Tone) => createSelector(selectAll, (items) => items.some((i) => i.on));`n" +
            "const anyOn = (list: { on: boolean }[] | undefined, strict: boolean) => list?.some((i) => i.on || !strict) ?? false;`n" +
            "const noneIn = (list: { on: boolean }[] | undefined) => !list?.length;`n" +
            "export const hasOnOfTone = (tone: Tone) => createSelector(selectAll, (items) => anyOn(items?.filter((i) => i.tone === tone), true));`n" +
            "export const noneOfTone = (tone: Tone) => createSelector(selectAll, (items) => noneIn(items.filter((i) => i.tone === tone)));`n" +
            "export const itemSelectors = {`n" +
            "  hasItem: (tone: Tone) => createSelector(selectAll, (items) => items.some((i) => i.tone === tone)),`n};`n"
        'apps/shop/src/moss.component.html' = "<b *ngIf=`"alpha$ | async`">a</b>`n<i *ngIf=`"beta$ | async`">b</i>`n" +
            "<u *ngIf=`"!(alpha$ | async)`">c</u>`n<s *ngIf=`"on$ | async`">d</s>`n<q *ngIf=`"moved$ | async`">e</q>`n" +
            "<em *ngIf=`"gamma$ | async`">f</em>`n<del *ngIf=`"none$ | async`">g</del>`n"
        'apps/shop/src/moss.component.ts' = "import { Component } from '@angular/core';`n" +
            "import { Store } from '@ngrx/store';`n" +
            "import { Tone, hasItemOfTone, anyItemOn, itemSelectors, hasOnOfTone, noneOfTone } from './moss.selectors';`n" +
            "@Component({ selector: 'app-moss', templateUrl: './moss.component.html', standalone: true })`n" +
            "export class MossComponent {`n" +
            "  alpha$ = this.store.select(hasItemOfTone(Tone.Alpha));`n" +
            "  beta$ = this.store.select(itemSelectors.hasItem(Tone.Beta));`n" +
            "  on$ = this.store.select(anyItemOn(Tone.Gamma));`n" +
            "  moved$ = this.store.select(hasItemOfTone(Tone.Gamma));`n" +
            "  gamma$ = this.store.select(hasOnOfTone(Tone.Gamma));`n" +
            "  none$ = this.store.select(noneOfTone(Tone.Alpha));`n" +
            "  constructor(private store: Store<{ items: { tone: Tone; on: boolean }[] }>) {}`n" +
            "  move(): void {`n    this.moved$ = this.store.select(hasItemOfTone(Tone.Beta));`n  }`n" +
            "}`n"
    }
    $made = New-TsRowsDb $tree
    $g = Invoke-TsRowsQ $made.Db ("SELECT g.source || ' => ' || v.dimension || ' ' || v.op || ' ' || " +
        "v.values_json AS restriction FROM gate_values v JOIN gates g ON g.id = v.gate WHERE v.enum_name = 'Tone'")
    Assert-Line $g 'alpha$ | async => hasItemOfTone.tone in ["Alpha"]'
    Assert-Line $g 'beta$ | async => hasItem.tone in ["Beta"]'
    Assert-NoLine $g '!('
    Assert-NoLine $g 'on$'
    Assert-NoLine $g 'moved$'
    # ...and a projector handing its filtered items to a helper that only tests them with `some`.
    Assert-Line $g 'gamma$ | async => hasOnOfTone.tone in ["Gamma"]'
    Assert-NoLine $g 'none$'
}

# THE OTHER SIDE FROM INJECTED CONFIG: the directive compares its aliased `@Input` with a field initialised from
# `inject(TOKEN)`, renders through `ngOnInit` -> a method -> a private render method, and the template hands the
# constant in. `in` the member on `===`, `not_in` on `!==`, on the dimension the field's declaration names.
Test-Case 'tsrows: a directive comparing its input with an injected config value restricts that value' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/tenant.ts' = "import { InjectionToken } from '@angular/core';`n" +
            "export enum TenantIDs { Alpha = 1, Beta = 2, Gamma = 3 }`n" +
            "export interface AppConfig { tenantID: number; }`n" +
            "export const APP_CONFIG = new InjectionToken<AppConfig>('app.config');`n"
        'apps/shop/src/for-tenant.directive.ts' = "import { Directive, Input, OnInit, TemplateRef, ViewContainerRef, inject } from '@angular/core';`n" +
            "import { APP_CONFIG, AppConfig, TenantIDs } from './tenant';`n" +
            "@Directive({ selector: '[forTenant]', standalone: true })`n" +
            "export class ForTenantDirective implements OnInit {`n" +
            "  private current = (inject(APP_CONFIG) as AppConfig).tenantID;`n" +
            "  @Input('forTenant') tenantID!: TenantIDs;`n" +
            "  constructor(private tpl: TemplateRef<any>, private vc: ViewContainerRef) {}`n" +
            "  ngOnInit() {`n    this.check();`n  }`n" +
            "  check() {`n    if (this.tenantID === this.current) {`n      this.render();`n    } else {`n      this.vc.clear();`n    }`n  }`n" +
            "  private render(): void {`n    this.vc.createEmbeddedView(this.tpl);`n  }`n" +
            "}`n" +
            "@Directive({ selector: '[notForTenant]', standalone: true })`n" +
            "export class NotForTenantDirective implements OnInit {`n" +
            "  private current = (inject(APP_CONFIG) as AppConfig).tenantID;`n" +
            "  @Input('notForTenant') tenantID!: TenantIDs;`n" +
            "  constructor(private tpl: TemplateRef<any>, private vc: ViewContainerRef) {}`n" +
            "  ngOnInit() {`n    if (this.tenantID !== this.current) {`n      this.vc.createEmbeddedView(this.tpl);`n    }`n  }`n" +
            "}`n"
        'apps/shop/src/tenant.component.html' = "<b *forTenant=`"ids.Alpha`">alpha</b>`n<i *notForTenant=`"ids.Beta`">not beta</i>`n"
        'apps/shop/src/tenant.component.ts' = "import { Component } from '@angular/core';`n" +
            "import { TenantIDs } from './tenant';`n" +
            "import { ForTenantDirective, NotForTenantDirective } from './for-tenant.directive';`n" +
            "@Component({ selector: 'app-tenant', templateUrl: './tenant.component.html', standalone: true, " +
            "imports: [ForTenantDirective, NotForTenantDirective] })`n" +
            "export class TenantComponent {`n  ids = TenantIDs;`n}`n"
    }
    $made = New-TsRowsDb $tree
    $g = Invoke-TsRowsQ $made.Db ("SELECT g.name || ' | ' || coalesce(v.dimension, '-') || ' ' || coalesce(v.op, '-') || ' ' || " +
        "coalesce(v.values_json, '-') AS restriction FROM gates g LEFT JOIN gate_values v ON v.gate = g.id " +
        "WHERE g.name IN ('forTenant', 'notForTenant')")
    Assert-Line $g 'forTenant | '
    Assert-Line $g 'in ["Alpha"]'
    Assert-Line $g 'notForTenant | '
    Assert-Line $g 'not_in ["Beta"]'
}

# A DIRECTIVE WHOSE CONSTANT ARRIVES FROM THE TEMPLATE: `*whenMode="Modes.A"`
# is a bare `Read`, and the class binds no constant - it compares the field its `@Input` filled against one of its
# own. The polarity is what the RENDER requires: under the `===` here, under the `!==` there. A value that is not
# a constant keeps the dimension and states no value.
Test-Case 'tsrows: a directive comparing the constant the template hands in restricts what it compares it with' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/modes.ts' = "export enum Modes { A = 1, B = 2, C = 3 }`n" +
            "export const settings = { mode: Modes.C };`n"
        'apps/shop/src/when-mode.directive.ts' = "import { Directive, Input, TemplateRef, ViewContainerRef } from '@angular/core';`n" +
            "import { Modes, settings } from './modes';`n" +
            "@Directive({ selector: '[whenMode]', standalone: true })`n" +
            "export class WhenModeDirective {`n" +
            "  private mode!: Modes;`n" +
            "  private activeMode = settings.mode;`n" +
            "  constructor(private templateRef: TemplateRef<any>, private viewContainer: ViewContainerRef) {}`n" +
            "  @Input() set whenMode(mode: Modes) {`n" +
            "    this.mode = mode;`n" +
            "    this.updateView();`n" +
            "  }`n" +
            "  private updateView() {`n" +
            "    if (this.mode === this.activeMode) {`n" +
            "      this.viewContainer.createEmbeddedView(this.templateRef);`n" +
            "    } else {`n" +
            "      this.viewContainer.clear();`n" +
            "    }`n" +
            "  }`n" +
            "}`n"
        'apps/shop/src/when-not-mode.directive.ts' = "import { Directive, Input, OnInit, TemplateRef, ViewContainerRef } from '@angular/core';`n" +
            "import { Modes, settings } from './modes';`n" +
            "@Directive({ selector: '[whenNotMode]', standalone: true })`n" +
            "export class WhenNotModeDirective implements OnInit {`n" +
            "  @Input() whenNotMode!: Modes;`n" +
            "  private activeMode: Modes = settings.mode;`n" +
            "  constructor(private templateRef: TemplateRef<any>, private viewContainer: ViewContainerRef) {}`n" +
            "  ngOnInit() {`n" +
            "    if (this.whenNotMode !== this.activeMode) {`n" +
            "      this.viewContainer.createEmbeddedView(this.templateRef);`n" +
            "    }`n" +
            "  }`n" +
            "}`n"
        'apps/shop/src/mode.component.html' = "<b *whenMode=`"Modes.A`">a</b>`n" +
            "<i *whenNotMode=`"Modes.B`">not b</i>`n" +
            "<u *whenMode=`"current`">runtime</u>`n"
        'apps/shop/src/mode.component.ts' = "import { Component } from '@angular/core';`n" +
            "import { Modes } from './modes';`n" +
            "import { WhenModeDirective } from './when-mode.directive';`n" +
            "import { WhenNotModeDirective } from './when-not-mode.directive';`n" +
            "@Component({ selector: 'app-mode', templateUrl: './mode.component.html', standalone: true, " +
            "imports: [WhenModeDirective, WhenNotModeDirective] })`n" +
            "export class ModeComponent {`n" +
            "  Modes = Modes;`n" +
            "  current: Modes = Modes.A;`n" +
            "}`n"
    }
    $made = New-TsRowsDb $tree
    $r = Invoke-Gate --map-query $made.Db --width 0 --sql ("SELECT g.name || ' ' || g.source || ' => ' || v.dimension || " +
        "' ' || v.op || ' ' || v.values_json AS restriction FROM gate_values v JOIN gates g ON g.id = v.gate " +
        "WHERE v.enum_name = 'Modes'")
    Assert-Exit $r 0
    Assert-Line $r 'whenMode Modes.A => activeMode in ["A"]'
    Assert-Line $r 'whenNotMode Modes.B => activeMode not_in ["B"]'
    Assert-Line $r 'whenMode current => activeMode unknown []'
}

}
