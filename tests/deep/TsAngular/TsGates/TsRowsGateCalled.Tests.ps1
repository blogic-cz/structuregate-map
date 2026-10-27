<#
    A branch around the call of a PRIVATE method rides into it (`rust/fbtcore/src/rows/ts/key/routes/key_called.rs`):
    a component the method creates through a factory renders under it (#56), and a key the method translates is
    reached under it (#64). Each beside its negative: a public method, a private one also called with no branch, and one also handed away.
    Its helpers are `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '..\..\TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

$script:CalledBranchQ = "SELECT k.key || ' | ' || coalesce((SELECT group_concat(b.condition_source, ' ; ') FROM key_reach__always_branches j " +
    "JOIN branches b ON b.id = j.value AND j.owner = '#' || k.rowid AND j.parent = 0), '-') AS reach FROM key_reach k WHERE k.key LIKE 'demo.%'"
$script:CalledValueQ = "SELECT k.key || ' | ' || coalesce($(Get-TsItemFieldSql 'key_reach__always_values' 'k.rowid' 0 'dimension') || ' ' || " +
    "$(Get-TsItemFieldSql 'key_reach__always_values' 'k.rowid' 0 'op') || ' ' || $(Get-NodeItemListSql 'key_reach__always_values' 'k.rowid' 0 'values'), '-') AS reach FROM key_reach k WHERE k.key LIKE 'demo.%'"

Test-Case 'tsrows gates: a component a private method creates renders under the branch around that method''s call' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/kinds.ts' = "export enum Kind { Home = 1, Flat = 2, Life = 3 }`n"
        'apps/shop/src/translate.component.ts' = "import { Component, Input } from '@angular/core';`n" +
            "@Component({ selector: 'translate', standalone: true, template: '<span>{{ text }}</span>' })`n" +
            "export class TranslateComponent { @Input() text!: string; }`n"
        'apps/shop/src/component.factory.ts' = "import { Injectable, Type, ViewContainerRef } from '@angular/core';`n" +
            "@Injectable({ providedIn: 'root' })`n" +
            "export class ComponentFactory {`n" +
            "  create<T>(component: Type<T>, container: ViewContainerRef) { return container.createComponent(component); }`n}`n"
        'apps/shop/src/modal.component.ts' = "import { Component } from '@angular/core';`n" +
            "@Component({ selector: 'demo-modal', template: '<translate [text]=`"\'demo.modal.title\'`"></translate>' })`n" +
            "export class ModalComponent {}`n" +
            "@Component({ selector: 'demo-other', template: '<translate [text]=`"\'demo.other.title\'`"></translate>' })`n" +
            "export class OtherComponent {}`n"
        # `openModal` is private and called once, under the `if`; `openOther` is public, so anything may call it
        'apps/shop/src/root.component.ts' = "import { Component, ViewContainerRef } from '@angular/core';`n" +
            "import { Kind } from './kinds';`n" +
            "import { ComponentFactory } from './component.factory';`n" +
            "import { ModalComponent, OtherComponent } from './modal.component';`n" +
            "@Component({ selector: 'demo-root', template: '<button (click)=`"go()`">go</button>' })`n" +
            "export class RootComponent {`n  kind: Kind = Kind.Life;`n  flag = false;`n" +
            "  constructor(private componentFactory: ComponentFactory, private viewContainerRef: ViewContainerRef) {}`n" +
            "  go() {`n    if (this.kind === Kind.Home && !this.flag) {`n      this.openModal();`n      this.openOther();`n    }`n  }`n" +
            "  private openModal() { this.componentFactory.create(ModalComponent, this.viewContainerRef); }`n" +
            "  openOther() { this.componentFactory.create(OtherComponent, this.viewContainerRef); }`n}`n"
        'apps/shop/src/assets/locales/en.json' = '{"shop":{"title":"Shop"},"demo":{"modal":{"title":"T"},"other":{"title":"O"}}}'
        'structuregate.ts.json' = '{"locales":["apps/shop/src/assets/locales"],"i18nCarriers":["translate:text"]}'
    }
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    $r = Invoke-TsRowsQ $made.Db $script:CalledBranchQ
    Assert-Line $r 'demo.modal.title | this.kind === Kind.Home && !this.flag'
    Assert-Line $r 'demo.other.title | -'
    $v = Invoke-TsRowsQ $made.Db $script:CalledValueQ
    Assert-Line $v 'demo.modal.title | kind in ["Home"]'
    Assert-Line $v 'demo.other.title | -'
}

Test-Case 'tsrows keys: a key a private method translates is reached under the branch around that method''s call' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/translate.service.ts' = "import { Injectable } from '@angular/core';`n" +
            "@Injectable({ providedIn: 'root' })`n" +
            "export class TranslateService {`n  getTranslation(key: string): string {`n    return key;`n  }`n}`n"
        'apps/shop/src/demo-env.ts' = "export class DemoEnv { static isHomeStatic(): boolean { return true; } }`n"
        # `homeText` is called once, under the first `if`; `lifeText` under the second and once with no branch
        'apps/shop/src/demo-price.pipe.ts' = "import { Pipe, PipeTransform } from '@angular/core';`n" +
            "import { TranslateService } from './translate.service';`n" +
            "import { DemoEnv } from './demo-env';`n" +
            "@Pipe({ name: 'demoPrice', standalone: true })`n" +
            "export class DemoPricePipe implements PipeTransform {`n" +
            "  constructor(private translate: TranslateService) {}`n" +
            "  transform(name: string): string {`n" +
            "    if (name === 'home' && DemoEnv.isHomeStatic()) {`n      return this.homeText();`n    }`n" +
            "    if (name === 'life') {`n      return this.lifeText();`n    }`n" +
            "    if (name === 'tip') {`n      return this.tipText();`n    }`n" +
            "    if (name === 'flat') {`n      return this.translate.getTranslation('demo.pipe.flat');`n    }`n" +
            "    return name ? '' : this.lifeText();`n  }`n" +
            "  private homeText(): string { return this.translate.getTranslation('demo.pipe.home'); }`n" +
            "  private lifeText(): string { return this.translate.getTranslation('demo.pipe.life'); }`n" +
            "  private tipText(): string { return this.translate.getTranslation('demo.pipe.tip'); }`n" +
            "  later(): void { setTimeout(this.tipText); }`n}`n"
        'apps/shop/src/root.component.ts' = "import { Component } from '@angular/core';`n" +
            "import { DemoPricePipe } from './demo-price.pipe';`n" +
            "@Component({ selector: 'demo-root', standalone: true, imports: [DemoPricePipe], template: '<span>{{ name | demoPrice }}</span>' })`n" +
            "export class RootComponent { name = 'home'; }`n"
        'apps/shop/src/assets/locales/en.json' = '{"shop":{"title":"Shop"},"demo":{"pipe":{"home":"H","flat":"F","life":"L","tip":"T"}}}'
        'structuregate.ts.json' = '{"locales":["apps/shop/src/assets/locales"],"i18nCalls":["getTranslation"]}'
    }
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    $r = Invoke-TsRowsQ $made.Db $script:CalledBranchQ
    Assert-Line $r "demo.pipe.flat | name === 'flat'"
    Assert-Line $r "demo.pipe.home | name === 'home' && DemoEnv.isHomeStatic()"
    # one call of `lifeText` holds no branch of its own, so no branch holds on every way
    Assert-Line $r 'demo.pipe.life | -'
    # `tipText` is also handed to `setTimeout`, a read that is no call: it runs where the map cannot see
    Assert-Line $r 'demo.pipe.tip | -'
}

# (audit) ANOTHER CLASS'S SAME-NAMED PRIVATE METHOD IS NOT A READ OF THIS ONE: the lift was refused whenever any other file spelled `openModal`,
# though the checker resolves that file's call to its own member. And a component created INSIDE A CALLBACK of the lifted method
# (`this.ready$.subscribe(() => ..)`) runs when the method does, so it renders under the same branch.
Test-Case 'tsrows gates: the lift survives a same-named method elsewhere and reaches a create in a callback' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/kinds.ts' = "export enum Kind { Home = 1, Flat = 2, Life = 3 }`n"
        'apps/shop/src/translate.component.ts' = "import { Component, Input } from '@angular/core';`n" +
            "@Component({ selector: 'translate', standalone: true, template: '<span>{{ text }}</span>' })`n" +
            "export class TranslateComponent { @Input() text!: string; }`n"
        'apps/shop/src/component.factory.ts' = "import { Injectable, Type, ViewContainerRef } from '@angular/core';`n" +
            "@Injectable({ providedIn: 'root' })`n" +
            "export class ComponentFactory {`n" +
            "  create<T>(component: Type<T>, container: ViewContainerRef) { return container.createComponent(component); }`n}`n"
        'apps/shop/src/modal.component.ts' = "import { Component } from '@angular/core';`n" +
            "@Component({ selector: 'demo-modal', template: '<translate [text]=`"\'demo.modal.title\'`"></translate>' })`n" +
            "export class ModalComponent {}`n" +
            "@Component({ selector: 'demo-confirm', template: '<translate [text]=`"\'demo.confirm.title\'`"></translate>' })`n" +
            "export class ConfirmComponent {}`n"
        'apps/shop/src/root.component.ts' = "import { Component, ViewContainerRef } from '@angular/core';`nimport { of } from 'rxjs';`n" +
            "import { Kind } from './kinds';`n" +
            "import { ComponentFactory } from './component.factory';`n" +
            "import { ModalComponent, ConfirmComponent } from './modal.component';`n" +
            "@Component({ selector: 'demo-root', template: '<button (click)=`"go()`">go</button>' })`n" +
            "export class RootComponent {`n  kind: Kind = Kind.Life;`n  sure = false;`n  ready`$ = of(true);`n" +
            "  constructor(private componentFactory: ComponentFactory, private viewContainerRef: ViewContainerRef) {}`n" +
            "  go() {`n    if (this.kind === Kind.Home) {`n      this.openModal();`n    }`n  }`n" +
            "  private openModal() {`n    this.componentFactory.create(ModalComponent, this.viewContainerRef);`n" +
            "    this.ready`$.subscribe(() => {`n      if (this.sure) {`n        this.componentFactory.create(ConfirmComponent, this.viewContainerRef);`n      }`n    });`n  }`n}`n"
        # A SIBLING with its own private `openModal`, called once with no branch: its calls resolve to its own member.
        'apps/shop/src/other.component.ts' = "import { Component } from '@angular/core';`n" +
            "@Component({ selector: 'demo-sibling', template: '<button (click)=`"go()`">go</button>' })`n" +
            "export class SiblingComponent {`n  n = 0;`n  go() { this.openModal(); }`n  private openModal() { this.n = 1; }`n}`n"
        'apps/shop/src/assets/locales/en.json' = '{"shop":{"title":"Shop"},"demo":{"modal":{"title":"T"},"confirm":{"title":"C"}}}'
        'structuregate.ts.json' = '{"locales":["apps/shop/src/assets/locales"],"i18nCarriers":["translate:text"]}'
    }
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    $v = Invoke-TsRowsQ $made.Db $script:CalledValueQ
    Assert-Line $v 'demo.modal.title | kind in ["Home"]'
    Assert-Line $v 'demo.confirm.title | kind in ["Home"]'
    $r = Invoke-TsRowsQ $made.Db $script:CalledBranchQ
    $confirm = @($r.Lines | Where-Object { $_.Contains('demo.confirm.title |') })
    if (-not ($confirm -and $confirm[0].Contains('this.sure') -and $confirm[0].Contains('this.kind === Kind.Home'))) { throw "the confirm key's branches: $confirm" }
}

# (audit) A CLASS LOOKED UP IN A `Map` OF COMPONENTS (`kinds.get(k)`) is each component the map holds: the factory site named none and the
# graph stopped there (`dynamic_render_component_not_static`), though a method returning the class from a `switch` was followed.
Test-Case 'tsrows gates: a factory creating what a Map of components looks up renders each of them' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/kinds.ts' = "export enum Kind { Home = 1, Flat = 2, Life = 3 }`n"
        'apps/shop/src/component.factory.ts' = "import { Injectable, Type, ViewContainerRef } from '@angular/core';`n" +
            "@Injectable({ providedIn: 'root' })`n" +
            "export class ComponentFactory {`n" +
            "  create<T>(component: Type<T>, container: ViewContainerRef) { return container.createComponent(component); }`n}`n"
        'apps/shop/src/details.ts' = "import { Type } from '@angular/core';`nimport { Kind } from './kinds';`n" +
            "import { HomeDetailComponent, FlatDetailComponent } from './detail.component';`n" +
            "export const detailsByKind = new Map<Kind, Type<unknown>>([[Kind.Home, HomeDetailComponent], [Kind.Flat, FlatDetailComponent]]);`n"
        'apps/shop/src/detail.component.ts' = "import { Component } from '@angular/core';`n" +
            "@Component({ selector: 'demo-home-detail', template: '<translate [text]=`"\'demo.home\'`"></translate>' })`nexport class HomeDetailComponent {}`n" +
            "@Component({ selector: 'demo-flat-detail', template: '<translate [text]=`"\'demo.flat\'`"></translate>' })`nexport class FlatDetailComponent {}`n"
        'apps/shop/src/translate.component.ts' = "import { Component, Input } from '@angular/core';`n" +
            "@Component({ selector: 'translate', standalone: true, template: '<span>{{ text }}</span>' })`n" +
            "export class TranslateComponent { @Input() text!: string; }`n"
        'apps/shop/src/root.component.ts' = "import { Component, ViewContainerRef } from '@angular/core';`n" +
            "import { Kind } from './kinds';`nimport { ComponentFactory } from './component.factory';`nimport { detailsByKind } from './details';`n" +
            "@Component({ selector: 'demo-root', template: '<button (click)=`"show()`">go</button>' })`n" +
            "export class RootComponent {`n  kind: Kind = Kind.Home;`n" +
            "  constructor(private componentFactory: ComponentFactory, private viewContainerRef: ViewContainerRef) {}`n" +
            "  show() {`n    const detail = detailsByKind.get(this.kind)!;`n    this.componentFactory.create(detail, this.viewContainerRef);`n  }`n}`n"
        'apps/shop/src/assets/locales/en.json' = '{"shop":{"title":"Shop"},"demo":{"home":"H","flat":"F"}}'
        'structuregate.ts.json' = '{"locales":["apps/shop/src/assets/locales"],"i18nCarriers":["translate:text"]}'
    }
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    $r = Invoke-TsRowsQ $made.Db "SELECT 'r=' || c.name || '>' || r.to_name AS r FROM renders r JOIN classes c ON c.id = r.from_class WHERE r.via = 'factory'"
    Assert-Line $r 'r=RootComponent>HomeDetailComponent'
    Assert-Line $r 'r=RootComponent>FlatDetailComponent'
    # (#138) EACH ENTRY RENDERS UNDER ITS OWN KEY, as a `switch` returning the class would: the lookup is a case per entry.
    $v = Invoke-TsRowsQ $made.Db $script:CalledValueQ
    Assert-Line $v 'demo.home | kind in ["Home"]'
    Assert-Line $v 'demo.flat | kind in ["Flat"]'
}

# (rows 124) `untilDestroyed(this)` OF `@ngneat/until-destroy` IS NO HAND-OVER: it reads the destroy method alone, so a component unsubscribing
# that way still lifts its private method's call branch - the file was `this_indexed` and every such lift was refused.
Test-Case 'tsrows gates: untilDestroyed(this) does not stop a private method''s call branch from lifting' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/kinds.ts' = "export enum Kind { Home = 1, Flat = 2, Life = 3 }`n"
        'apps/shop/src/translate.component.ts' = "import { Component, Input } from '@angular/core';`n" +
            "@Component({ selector: 'translate', standalone: true, template: '<span>{{ text }}</span>' })`n" +
            "export class TranslateComponent { @Input() text!: string; }`n"
        'apps/shop/src/component.factory.ts' = "import { Injectable, Type, ViewContainerRef } from '@angular/core';`n" +
            "@Injectable({ providedIn: 'root' })`n" +
            "export class ComponentFactory {`n" +
            "  create<T>(component: Type<T>, container: ViewContainerRef) { return container.createComponent(component); }`n}`n"
        'apps/shop/src/modal.component.ts' = "import { Component } from '@angular/core';`n" +
            "@Component({ selector: 'demo-modal', template: '<translate [text]=`"\'demo.modal.title\'`"></translate>' })`n" +
            "export class ModalComponent {}`n"
        'apps/shop/src/root.component.ts' = "import { Component, ViewContainerRef } from '@angular/core';`nimport { of } from 'rxjs';`n" +
            "import { untilDestroyed } from '@ngneat/until-destroy';`n" +
            "import { Kind } from './kinds';`nimport { ComponentFactory } from './component.factory';`nimport { ModalComponent } from './modal.component';`n" +
            "@Component({ selector: 'demo-root', template: '<button (click)=`"go()`">go</button>' })`n" +
            "export class RootComponent {`n  kind: Kind = Kind.Life;`n  ready`$ = of(true);`n" +
            "  constructor(private componentFactory: ComponentFactory, private viewContainerRef: ViewContainerRef) {}`n" +
            "  go() {`n    this.ready`$.pipe(untilDestroyed(this)).subscribe();`n    if (this.kind === Kind.Home) {`n      this.openModal();`n    }`n  }`n" +
            "  private openModal() { this.componentFactory.create(ModalComponent, this.viewContainerRef); }`n}`n"
        'apps/shop/src/assets/locales/en.json' = '{"shop":{"title":"Shop"},"demo":{"modal":{"title":"T"}}}'
        'structuregate.ts.json' = '{"locales":["apps/shop/src/assets/locales"],"i18nCarriers":["translate:text"]}'
    }
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    $f = Invoke-Gate --map-query $made.Db --width 0 --sql "SELECT 'ix=' || this_indexed AS r FROM files WHERE path LIKE '%root.component.ts'"
    Assert-Line $f 'ix=0'
    $v = Invoke-TsRowsQ $made.Db $script:CalledValueQ
    Assert-Line $v 'demo.modal.title | kind in ["Home"]'
}

}
