<#
    A branch around the call of a PRIVATE method rides into it (`rust/fbtcore/src/rows/ts/key/routes/key_called.rs`):
    a component the method creates through a factory renders under it (#56), and a key the method translates is
    reached under it (#64). Each beside its negative: a public method, a private one also called with no branch, and one also handed away.
    Its helpers are `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '..\..\TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

$script:CalledBranchQ = "SELECT k.key || ' | ' || coalesce((SELECT group_concat(b.condition_source, ' ; ') FROM json_each(k.always_branches) j " +
    "JOIN branches b ON b.id = j.value), '-') AS reach FROM key_reach k WHERE k.key LIKE 'demo.%'"
$script:CalledValueQ = "SELECT k.key || ' | ' || coalesce(json_extract(k.always_values, '$[0].dimension') || ' ' || " +
    "json_extract(k.always_values, '$[0].op') || ' ' || json_extract(k.always_values, '$[0].values'), '-') AS reach FROM key_reach k WHERE k.key LIKE 'demo.%'"

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
}
