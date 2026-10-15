<#
    More shapes a key reaches the template through (`key/fields/key_follow.rs`, `key/key_lists.rs`,
    `key/fields/key_methods.rs`): a `+=` on a returned local, an `await` and a `.then` callback of a call, a list
    copied by `slice` / a spread / a `map` that keeps its parameter, a constant ARRAY read by an unknown index, and an
    arrow PROPERTY the template calls. Each has a negative beside it: a `+=` on a local nobody returns, a `.then`
    callback that writes something else, a `map` building new elements, an array nobody indexes, an arrow no
    template calls, a child bound to a list by one parent and to anything else by another. Its helpers are `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '..\TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

function New-KeyMoreTree {
    return New-TsRowsWorkspace @{
        # `Promise.resolve` resolves to the compiler's own lib only when the target has a Promise value
        'apps/shop/tsconfig.app.json' = '{"compilerOptions":{"strict":true,"target":"ES2022"},"include":["src/**/*.ts"]}'
        'apps/shop/src/translate.service.ts' = "import { Injectable } from '@angular/core';`n" +
            "@Injectable({ providedIn: 'root' })`n" +
            "export class TranslateService {`n  getTranslation(key: string): string {`n    return key;`n  }`n}`n"
        'apps/shop/src/lists.ts' = "export function sliced(): { label: string }[] {`n  const boxes = [{ label: 'demo.list.a' }];`n  return boxes.slice();`n}`n" +
            "export function copied(): { label: string }[] {`n  const boxes = [{ label: 'demo.list.b' }];`n  return [...boxes];`n}`n" +
            "export function kept(): { label: string }[] {`n  const boxes = [{ label: 'demo.list.c' }];`n  return boxes.map((b) => ({ ...b, extra: 1 }));`n}`n" +
            "export function rebuilt(): { label: string }[] {`n  const boxes = [{ label: 'demo.list.unused' }];`n  return boxes.map((b) => ({ label: 'x' }));`n}`n" +
            "export function kidList(): { label: string }[] { return [{ label: 'demo.child.a' }]; }`n" +
            "export function idleList(): { label: string }[] { return [{ label: 'demo.child.unused' }]; }`n"
        'apps/shop/src/kids.component.ts' = "import { Component, Input } from '@angular/core';`n" +
            "import { NgFor } from '@angular/common';`n" +
            "@Component({ selector: 'demo-kids', standalone: true, imports: [NgFor], template: '<b *ngFor=`"let i of items`">{{ i.label }}</b>' })`n" +
            "export class KidsComponent { @Input() items: { label: string }[] = []; }`n" +
            "@Component({ selector: 'demo-pair', standalone: true, imports: [NgFor], template: '<u *ngFor=`"let i of items`">{{ i.label }}</u>' })`n" +
            "export class PairComponent { @Input() items: { label: string }[] = []; }`n"
        'apps/shop/src/more.service.ts' = "import { Injectable } from '@angular/core';`n" +
            "import { TranslateService } from './translate.service';`n" +
            "const ARR = ['demo.arr.a', 'demo.arr.b'];`n" +
            "const IDLE_ARR = ['demo.arr.unused'];`n" +
            "@Injectable({ providedIn: 'root' })`n" +
            "export class MoreService {`n" +
            "  constructor(private t: TranslateService) {}`n" +
            "  plus(x: boolean): string {`n    let s = this.t.getTranslation('demo.plus.a');`n    if (x) { s += this.t.getTranslation('demo.plus.b'); }`n    return s;`n  }`n" +
            "  plusIdle(): string {`n    let s = '';`n    let other = '';`n    other += this.t.getTranslation('demo.plus.unused');`n    s += 'k';`n    return s;`n  }`n" +
            "  async asyncOne(): Promise<string> { return this.t.getTranslation('demo.await.a'); }`n" +
            "  load(): Promise<string> { return Promise.resolve(this.t.getTranslation('demo.then.a')); }`n" +
            "  async loadAsync(): Promise<string> { return this.t.getTranslation('demo.then.b'); }`n" +
            "  arr(i: number): string { return this.t.getTranslation(ARR[i]); }`n" +
            "}`n"
        'apps/shop/src/more.component.ts' = "import { Component, OnInit } from '@angular/core';`n" +
            "import { NgFor } from '@angular/common';`n" +
            "import { MoreService } from './more.service';`n" +
            "import { TranslateService } from './translate.service';`n" +
            "import { sliced, copied, kept, rebuilt, kidList, idleList } from './lists';`n" +
            "import { KidsComponent, PairComponent } from './kids.component';`n" +
            "@Component({ selector: 'demo-more', standalone: true, imports: [NgFor, KidsComponent, PairComponent], template: '<demo-kids [items]=`"kids`"></demo-kids> <demo-pair [items]=`"idleKids`"></demo-pair> <demo-pair [items]=`"idleKids.slice()`"></demo-pair> {{ a }} {{ b }} {{ c }} {{ d }} {{ e }} {{ f }} {{ label() }} " +
            "<i *ngFor=`"let i of l1`">{{ i.label }}</i> <i *ngFor=`"let i of l2`">{{ i.label }}</i> <i *ngFor=`"let i of l3`">{{ i.label }}</i> <i *ngFor=`"let i of l4`">{{ i.label }}</i>' })`n" +
            "export class MoreComponent implements OnInit {`n" +
            "  a = ''; b = ''; c = ''; d = ''; e = ''; f = ''; other = '';`n" +
            "  kids = kidList(); idleKids = idleList();`n" +
            "  l1 = sliced(); l2 = copied(); l3 = kept(); l4 = rebuilt();`n" +
            "  label = () => this.t.getTranslation('demo.arrow.prop');`n" +
            "  idle = () => this.t.getTranslation('demo.arrow.idle');`n" +
            "  constructor(private svc: MoreService, private t: TranslateService) {}`n" +
            "  async ngOnInit() {`n" +
            "    this.a = this.svc.plus(true);`n" +
            "    this.b = await this.svc.asyncOne();`n" +
            "    this.svc.load().then((x) => { this.c = x; });`n" +
            "    this.svc.loadAsync().then((x) => this.d = x);`n" +
            "    this.svc.load().then((x) => { this.other = 'demo.then.neg'; });`n" +
            "    this.e = this.svc.plusIdle();`n" +
            "    this.f = this.svc.arr(1);`n" +
            "  }`n" +
            "}`n"
        'apps/shop/src/assets/locales/en.json' = '{"shop":{"title":"Shop"},"demo":{"plus":{"a":"1","b":"2","unused":"3"},"await":{"a":"1"},' +
            '"then":{"a":"1","b":"2","neg":"3"},"list":{"a":"1","b":"2","c":"3","unused":"4"},"arr":{"a":"1","b":"2","unused":"3"},"arrow":{"prop":"1","idle":"2"},"child":{"a":"1","unused":"2"}}}'
        'structuregate.ts.json' = '{"locales":["apps/shop/src/assets/locales"],"i18nCalls":["getTranslation"]}'
    }
}

Test-Case 'tsrows keys: a +=, an await, a then callback, a copied list, an indexed array, an arrow property and a list handed to an input reach the template' {
    $tree = New-KeyMoreTree
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    $r = Invoke-TsRowsQ $made.Db ("SELECT k.key || ' | ' || k.route || ' | ' || c.name AS reach FROM key_reach k " +
        "JOIN key_reach__components j ON j.owner = '#' || k.rowid AND j.parent = 0 JOIN components c ON c.id = j.value WHERE k.key LIKE 'demo.%'")
    # the control: the first write of a returned local
    Assert-Line $r 'demo.plus.a | field_binding | MoreComponent'
    Assert-Line $r 'demo.plus.b | field_binding | MoreComponent'
    Assert-Line $r 'demo.await.a | field_binding | MoreComponent'
    Assert-Line $r 'demo.then.a | field_binding | MoreComponent'
    Assert-Line $r 'demo.then.b | field_binding | MoreComponent'
    Assert-Line $r 'demo.list.a | field_binding | MoreComponent'
    Assert-Line $r 'demo.list.b | field_binding | MoreComponent'
    Assert-Line $r 'demo.list.c | field_binding | MoreComponent'
    Assert-Line $r 'demo.arr.a | field_binding | MoreComponent'
    Assert-Line $r 'demo.arr.b | field_binding | MoreComponent'
    Assert-Line $r 'demo.arrow.prop | field_binding | MoreComponent'
    # a list the parent hands to a child's `@Input` is read in the CHILD, where its `*ngFor` is
    Assert-Line $r 'demo.child.a | field_binding | KidsComponent'
    # the negatives: a `+=` on a local nobody returns, a callback writing something else, a `map` building new
    # elements, an array nobody indexes, an arrow no template calls
    Assert-NoLine $r 'demo.plus.unused | field_binding | MoreComponent'
    Assert-NoLine $r 'demo.then.neg | field_binding | MoreComponent'
    Assert-NoLine $r 'demo.list.unused | field_binding | MoreComponent'
    Assert-NoLine $r 'demo.arr.unused | field_binding | MoreComponent'
    Assert-NoLine $r 'demo.arrow.idle | field_binding | MoreComponent'
    # each render edge hands its own list (#25): the edge binding `idleKids` shows its key, the one binding
    # `idleKids.slice()` - a call the map does not read as a list here - adds nothing and voids nothing
    Assert-Line $r 'demo.child.unused | field_binding | PairComponent'
    $p = Invoke-TsRowsQ $made.Db "SELECT 'paths ' || n_paths AS n FROM key_reach WHERE key = 'demo.child.unused'"
    Assert-Line $p 'paths 1'
}

Test-Case 'tsrows keys: a list a component builds and only hands to a child input renders through that child alone (#50)' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/tsconfig.app.json' = '{"compilerOptions":{"strict":false},"include":["src/**/*.ts"]}'
        'apps/shop/src/translate.service.ts' = "import { Injectable } from '@angular/core';`n" +
            "@Injectable({ providedIn: 'root' })`n" +
            "export class TranslateService {`n  getTranslation(key: string): string {`n    return key;`n  }`n}`n"
        'apps/shop/src/kinds.ts' = "export enum Kind { Home = 1, Flat = 2, Life = 3 }`n"
        'apps/shop/src/radio.component.ts' = "import { Component, Input } from '@angular/core';`n" +
            "import { NgFor } from '@angular/common';`n" +
            "@Component({ selector: 'demo-radio', standalone: true, imports: [NgFor], template: '<label *ngFor=`"let item of model`">{{ item.Label }}</label>' })`n" +
            "export class RadioComponent { @Input() model: { Label: string }[]; }`n"
        'apps/shop/src/a.component.ts' = "import { Component } from '@angular/core';`n" +
            "import { NgIf } from '@angular/common';`n" +
            "import { Kind } from './kinds';`n" +
            "import { TranslateService } from './translate.service';`n" +
            "import { RadioComponent } from './radio.component';`n" +
            "@Component({ selector: 'demo-a', standalone: true, imports: [NgIf, RadioComponent], template: '<div *ngIf=`"vm.kind === Kind.Home`"><demo-radio [model]=`"model`"></demo-radio></div>' })`n" +
            "export class AComponent {`n  Kind = Kind;`n  vm: { kind: Kind } = { kind: Kind.Life };`n  model: { Label: string }[];`n" +
            "  constructor(private translate: TranslateService) {`n" +
            "    this.model = [{ Label: this.translate.getTranslation('demo.choice.yes') }, { Label: this.translate.getTranslation('demo.choice.no') }];`n  }`n}`n"
        'apps/shop/src/b.component.ts' = "import { Component } from '@angular/core';`n" +
            "import { RadioComponent } from './radio.component';`n" +
            "@Component({ selector: 'demo-b', standalone: true, imports: [RadioComponent], template: '<demo-radio [model]=`"other`"></demo-radio>' })`n" +
            "export class BComponent { other: { Label: string }[] = []; }`n"
        'apps/shop/src/d.component.ts' = "import { Component } from '@angular/core';`n" +
            "import { NgIf, NgFor } from '@angular/common';`n" +
            "import { Kind } from './kinds';`n" +
            "import { TranslateService } from './translate.service';`n" +
            "import { RadioComponent } from './radio.component';`n" +
            "@Component({ selector: 'demo-d', standalone: true, imports: [NgIf, NgFor, RadioComponent], template: '<i *ngFor=`"let x of model`">{{ x.Label }}</i><div *ngIf=`"vm.kind === Kind.Flat`"><demo-radio [model]=`"model`"></demo-radio></div>' })`n" +
            "export class DComponent {`n  Kind = Kind;`n  vm: { kind: Kind } = { kind: Kind.Life };`n  model: { Label: string }[];`n" +
            "  constructor(private translate: TranslateService) {`n    this.model = [{ Label: this.translate.getTranslation('demo.both.yes') }];`n  }`n}`n"
        'apps/shop/src/e.component.ts' = "import { Component } from '@angular/core';`n" +
            "import { NgIf } from '@angular/common';`n" +
            "import { Kind } from './kinds';`n" +
            "import { RadioComponent } from './radio.component';`n" +
            "@Component({ selector: 'demo-e', standalone: true, imports: [NgIf, RadioComponent], template: '<div *ngIf=`"vm.kind === Kind.Life`"><demo-radio [model]=`"model`"></demo-radio></div>' })`n" +
            "export class EComponent {`n  Kind = Kind;`n  vm: { kind: Kind } = { kind: Kind.Life };`n  model = [{ Label: 'demo.lit.yes' }];`n}`n"
        'apps/shop/src/root.component.ts' = "import { Component } from '@angular/core';`n" +
            "import { AComponent } from './a.component';`n" +
            "import { BComponent } from './b.component';`n" +
            "import { DComponent } from './d.component';`n" +
            "import { EComponent } from './e.component';`n" +
            "@Component({ selector: 'demo-root', standalone: true, imports: [AComponent, BComponent, DComponent, EComponent], template: '<demo-a></demo-a><demo-b></demo-b><demo-d></demo-d><demo-e></demo-e>' })`n" +
            "export class RootComponent {}`n"
        'apps/shop/src/assets/locales/en.json' = '{"shop":{"title":"Shop"},"demo":{"choice":{"yes":"Yes","no":"No"},"both":{"yes":"Yes"},"lit":{"yes":"Yes"}}}'
        'structuregate.ts.json' = '{"locales":["apps/shop/src/assets/locales"],"i18nCalls":["getTranslation"]}'
    }
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    $r = Invoke-TsRowsQ $made.Db ("SELECT k.key || ' | ' || k.n_paths || ' | ' || (SELECT group_concat(name, ',') FROM (SELECT c.name FROM " +
        "key_reach__components j JOIN components c ON c.id = j.value AND j.owner = '#' || k.rowid AND j.parent = 0 ORDER BY c.name)) || ' | ' || coalesce($(Get-TsItemFieldSql 'key_reach__always_values' 'k.rowid' 0 'dimension') " +
        "|| ' ' || $(Get-TsItemFieldSql 'key_reach__always_values' 'k.rowid' 0 'op') || ' ' || $(Get-NodeItemListSql 'key_reach__always_values' 'k.rowid' 0 'values'), '-') AS reach FROM key_reach k WHERE k.key LIKE 'demo.%'")
    # built by a translate call (A) or a literal (E) and displayed only through the radio under the *ngIf: the parent
    # is no site of its own, and the ungated radio path through B binds another list
    Assert-Line $r 'demo.choice.yes | 1 | RadioComponent | vm.kind in ["Home"]'
    Assert-Line $r 'demo.lit.yes | 1 | RadioComponent | vm.kind in ["Life"]'
    # a parent that displays the key itself (its own *ngFor) keeps its own, ungated path
    Assert-Line $r 'demo.both.yes | 2 | DComponent,RadioComponent | -'
    $g = Invoke-TsRowsQ $made.Db ("SELECT k.key || ' | ' || g.name || ' ' || g.source AS always FROM key_reach k JOIN key_reach__always_gates j ON j.owner = '#' || k.rowid AND j.parent = 0 " +
        "JOIN gates g ON g.id = j.value WHERE k.key LIKE 'demo.%'")
    Assert-Line $g 'demo.choice.yes | ngIf vm.kind === Kind.Home'
    Assert-NoLine $g 'demo.both.yes | ngIf'
}
Test-Case 'tsrows keys: key_reach.paths names each way - its render_path row and what the way adds to it (#53)' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/tsconfig.app.json' = '{"compilerOptions":{"strict":false},"include":["src/**/*.ts"]}'
        'apps/shop/src/translate.service.ts' = "import { Injectable } from '@angular/core';`n" +
            "@Injectable({ providedIn: 'root' })`n" +
            "export class TranslateService {`n  getTranslation(key: string): string {`n    return key;`n  }`n}`n"
        'apps/shop/src/kinds.ts' = "export enum Kind { Home = 1, Flat = 2, Life = 3 }`n"
        'apps/shop/src/radio.component.ts' = "import { Component, Input } from '@angular/core';`n" +
            "import { NgFor } from '@angular/common';`n" +
            "@Component({ selector: 'demo-radio', standalone: true, imports: [NgFor], template: '<label *ngFor=`"let item of model`">{{ item.Label }}</label>' })`n" +
            "export class RadioComponent { @Input() model: { Label: string }[]; }`n"
        'apps/shop/src/visible.directive.ts' = "import { Directive, Input } from '@angular/core';`n" +
            "import { Kind } from './kinds';`n" +
            "@Directive({ selector: '[demoVisible]', standalone: true })`n" +
            "export class VisibleDirective { @Input() demoVisible: { kinds: Kind[] }; }`n"
        'apps/shop/src/a.component.ts' = "import { Component } from '@angular/core';`n" +
            "import { Kind } from './kinds';`n" +
            "import { TranslateService } from './translate.service';`n" +
            "import { RadioComponent } from './radio.component';`n" +
            "import { VisibleDirective } from './visible.directive';`n" +
            "@Component({ selector: 'demo-a', standalone: true, imports: [VisibleDirective, RadioComponent], template: '<div *demoVisible=`"alphaConfig`"><demo-radio [model]=`"model`"></demo-radio></div><div *demoVisible=`"betaConfig`"><demo-radio [model]=`"model`"></demo-radio></div>' })`n" +
            "export class AComponent {`n  alphaConfig = { kinds: [Kind.Home] };`n  betaConfig = { kinds: [Kind.Flat] };`n  model: { Label: string }[];`n" +
            "  constructor(private translate: TranslateService) {`n    this.model = [{ Label: this.translate.getTranslation('demo.choice.yes') }];`n  }`n}`n"
        'apps/shop/src/b.component.ts' = "import { Component } from '@angular/core';`n" +
            "import { RadioComponent } from './radio.component';`n" +
            "@Component({ selector: 'demo-b', standalone: true, imports: [RadioComponent], template: '<demo-radio [model]=`"other`"></demo-radio>' })`n" +
            "export class BComponent { other: { Label: string }[] = []; }`n"
        'apps/shop/src/translate.component.ts' = "import { Component, Input } from '@angular/core';`n" +
            "@Component({ selector: 'translate', standalone: true, template: '{{ text }}' })`n" +
            "export class TranslateComponent { @Input() text: string = ''; }`n"
        'apps/shop/src/list.component.ts' = "import { Component } from '@angular/core';`n" +
            "import { NgIf, NgTemplateOutlet } from '@angular/common';`n" +
            "import { Kind } from './kinds';`n" +
            "import { TranslateComponent } from './translate.component';`n" +
            "@Component({ selector: 'demo-list', standalone: true, imports: [NgIf, NgTemplateOutlet, TranslateComponent], template: '<div *ngIf=`"vm.kind === Kind.Flat`"><ng-container *ngTemplateOutlet=`"bTpl`"></ng-container></div><ng-container [ngTemplateOutlet]=`"bTpl`"></ng-container><ng-template #bTpl><translate [text]=`"\'demo.list.b\'`"></translate></ng-template>' })`n" +
            "export class ListComponent {`n  Kind = Kind;`n  vm: { kind: Kind } = { kind: Kind.Life };`n}`n"
        'apps/shop/src/root.component.ts' = "import { Component } from '@angular/core';`n" +
            "import { AComponent } from './a.component';`n" +
            "import { BComponent } from './b.component';`n" +
            "import { ListComponent } from './list.component';`n" +
            "@Component({ selector: 'demo-root', standalone: true, imports: [AComponent, BComponent, ListComponent], template: '<demo-a></demo-a><demo-b></demo-b><demo-list></demo-list>' })`n" +
            "export class RootComponent {}`n"
        'apps/shop/src/assets/locales/en.json' = '{"demo":{"choice":{"yes":"Yes"},"list":{"b":"B"}}}'
        'structuregate.ts.json' = '{"locales":["apps/shop/src/assets/locales"],"i18nCalls":["getTranslation"]}'
    }
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    # one entry per way: n_paths of them
    $n = Invoke-TsRowsQ $made.Db "SELECT key || ' ' || n_paths || ' ' || (SELECT count(*) FROM key_reach__paths jn WHERE jn.owner = '#' || key_reach.rowid AND jn.parent = 0) AS n FROM key_reach WHERE key LIKE 'demo.%'"
    Assert-Line $n 'demo.choice.yes 2 2'
    Assert-Line $n 'demo.list.b 2 2'
    # each way: key | its render_path's components | that row's gates | what the way adds (a gate by its source, a link by its prefix)
    $w = Invoke-Gate --map-query $made.Db --width 0 --sql ("SELECT k.key || ' | ' || (SELECT group_concat(name, '>') FROM (SELECT c.name FROM render_path__hops h " +
        "JOIN components c ON c.id = h.value WHERE h.owner = p.id AND h.parent = 0 ORDER BY h.ord)) || ' | ' || coalesce((SELECT group_concat(g.source, ' & ') FROM render_path__gates pg " +
        "JOIN gates g ON g.id = pg.value WHERE pg.owner = p.id AND pg.parent = 0), '-') || ' | ' || coalesce((SELECT group_concat(coalesce(g2.source, substr(o.value, 1, instr(o.value, ':') - 1)), ' & ') " +
        "FROM key_reach__paths ol JOIN key_reach__paths o ON o.owner = ol.owner AND o.parent = ol.n LEFT JOIN gates g2 ON g2.id = o.value WHERE ol.owner = w.owner AND ol.parent = w.n AND ol.role = 'own'), '-') AS way " +
        "FROM key_reach k JOIN key_reach__paths w ON w.owner = '#' || k.rowid AND w.parent = 0 JOIN render_path p ON p.id = $(Get-TsFieldSql 'key_reach__paths' 'w' 'path') WHERE k.key LIKE 'demo.%' ORDER BY 1")
    # the two rows through A that bind the list - never the one through B, whose edge binds another list
    Assert-Line $w 'demo.choice.yes | RootComponent>AComponent>RadioComponent | alphaConfig | bind & model'
    Assert-Line $w 'demo.choice.yes | RootComponent>AComponent>RadioComponent | betaConfig | bind & model'
    Assert-NoLine $w 'demo.choice.yes | RootComponent>BComponent'
    # two outlets of one ng-template: ONE render_path row carries two ways, told apart by what each adds
    Assert-Line $w 'demo.list.b | RootComponent>ListComponent | - | vm.kind === Kind.Flat'
    Assert-Line $w 'demo.list.b | RootComponent>ListComponent | - | -'
}

Test-Case 'tsrows keys: a component field nothing reads is no site of the key its initializer holds' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/kinds.ts' = "export enum Kind { Home = 1, Flat = 2, Life = 3 }`n"
        'apps/shop/src/translate.component.ts' = "import { Component, Input } from '@angular/core';`n" +
            "@Component({ selector: 'translate', standalone: true, template: '<span>{{ text }}</span>' })`n" +
            "export class TranslateComponent { @Input() text!: string; }`n"
        # A shows every key, under a kind condition
        'apps/shop/src/a.component.ts' = "import { Component, Input } from '@angular/core';`n" +
            "import { Kind } from './kinds';`n" +
            "@Component({ selector: 'demo-a', template: '<div *ngIf=`"kind === Kind.Home`">" +
            "<translate [text]=`"labelCode`"></translate><translate [text]=`"keptCode`"></translate>" +
            "<translate [text]=`"selfCode`"></translate></div>' })`n" +
            "export class AComponent { Kind = Kind; @Input() kind!: Kind;`n" +
            "  labelCode = 'demo.label'; keptCode = 'demo.kept'; selfCode = 'demo.self'; }`n"
        # B declares a field of the same key and binds only another: nothing reads `labelCode` of B
        'apps/shop/src/b.component.ts' = "import { Component } from '@angular/core';`n" +
            "@Component({ selector: 'demo-b', template: '<translate [text]=`"otherCode`"></translate>' })`n" +
            "export class BComponent { labelCode = 'demo.label'; otherCode = 'demo.other'; }`n"
        # C's own TypeScript reads its field, and D hands `this` away: both may still use the key
        'apps/shop/src/c.component.ts' = "import { Component } from '@angular/core';`n" +
            "@Component({ selector: 'demo-c', template: '<i></i>' })`n" +
            "export class CComponent { keptCode = 'demo.kept'; log(): string { return this.keptCode; } }`n"
        'apps/shop/src/d.component.ts' = "import { Component } from '@angular/core';`n" +
            "@Component({ selector: 'demo-d', template: '<i></i>' })`n" +
            "export class DComponent { selfCode = 'demo.self'; pick(k: string): string { return (this as any)[k]; } }`n"
        'apps/shop/src/root.component.ts' = "import { Component } from '@angular/core';`n" +
            "import { Kind } from './kinds';`n" +
            "@Component({ selector: 'demo-root', template: '<demo-a [kind]=`"kind`"></demo-a><demo-b></demo-b><demo-c></demo-c><demo-d></demo-d>' })`n" +
            "export class RootComponent { kind: Kind = Kind.Life; }`n"
        'apps/shop/src/assets/locales/en.json' = '{"shop":{"title":"Shop"},"demo":{"label":"L","other":"O","kept":"K","self":"S"}}'
        'structuregate.ts.json' = '{"locales":["apps/shop/src/assets/locales"],"i18nCarriers":["translate:text"]}'
    }
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    $r = Invoke-TsRowsQ $made.Db ("SELECT k.key || ' | ' || k.n_paths || ' | ' || (SELECT group_concat(c.name, ',') FROM key_reach__components j " +
        "JOIN components c ON c.id = j.value AND j.owner = '#' || k.rowid AND j.parent = 0) || ' | ' || coalesce($(Get-NodeItemListSql 'key_reach__always_values' 'k.rowid' 0 'values'), '-') AS reach FROM key_reach k WHERE k.key LIKE 'demo.%'")
    # B's field is never read, so A's `*ngIf` is the only way to the key
    Assert-Line $r 'demo.label | 1 | AComponent | ["Home"]'
    # read in C's own TypeScript, or reachable through D's `this[k]`: each keeps its ungated literal site
    Assert-Line $r 'demo.kept | 2 | AComponent,CComponent | -'
    Assert-Line $r 'demo.self | 2 | AComponent,DComponent | -'
}

Test-Case 'tsrows keys: a field only written in a method and a getter nothing reads are no sites of their keys' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/kinds.ts' = "export enum Kind { Home = 1, Flat = 2, Life = 3 }`n"
        'apps/shop/src/translate.component.ts' = "import { Component, Input } from '@angular/core';`n" +
            "@Component({ selector: 'translate', standalone: true, template: '<span>{{ text }}</span>' })`n" +
            "export class TranslateComponent { @Input() text!: string; }`n"
        'apps/shop/src/a.component.ts' = "import { Component, Input } from '@angular/core';`n" +
            "import { Kind } from './kinds';`n" +
            "@Component({ selector: 'demo-a', template: '<div *ngIf=`"kind === Kind.Home`">" +
            "<translate [text]=`"labelCode`"></translate><translate [text]=`"title`"></translate></div>' })`n" +
            "export class AComponent { Kind = Kind; @Input() kind!: Kind;`n" +
            "  labelCode = 'demo.label'; get title(): string { return 'demo.title'; } }`n"
        # B only WRITES `labelCode` (in a method ngOnInit calls) and never reads its getter `title`
        'apps/shop/src/b.component.ts' = "import { Component, Input, OnInit } from '@angular/core';`n" +
            "@Component({ selector: 'demo-b', template: '<translate [text]=`"otherCode`"></translate>' })`n" +
            "export class BComponent implements OnInit {`n  @Input() flat = false;`n" +
            "  labelCode = 'demo.label';`n  otherCode = 'demo.other';`n" +
            "  ngOnInit(): void { this.changeCodes(); }`n" +
            "  get title(): string { return 'demo.title'; }`n" +
            "  private changeCodes(): void {`n    if (this.flat) {`n      this.labelCode = 'demo.label';`n" +
            "    } else {`n      this.labelCode = 'demo.labelAlt';`n    }`n  }`n}`n"
        # C reads its getter through another getter and its written field in a method: both keep their sites
        'apps/shop/src/c.component.ts' = "import { Component, OnInit } from '@angular/core';`n" +
            "@Component({ selector: 'demo-c', template: '<translate [text]=`"shown`"></translate><i>{{ pick() }}</i>' })`n" +
            "export class CComponent implements OnInit {`n  altCode = '';`n" +
            "  ngOnInit(): void { this.altCode = 'demo.alt'; }`n" +
            "  get shown(): string { return this.keptTitle; }`n" +
            "  get keptTitle(): string { return 'demo.kept'; }`n" +
            "  pick(): string { return this.altCode; }`n}`n"
        'apps/shop/src/root.component.ts' = "import { Component } from '@angular/core';`n" +
            "import { Kind } from './kinds';`n" +
            "@Component({ selector: 'demo-root', template: '<demo-a [kind]=`"kind`"></demo-a><demo-b></demo-b><demo-c></demo-c>' })`n" +
            "export class RootComponent { kind: Kind = Kind.Life; }`n"
        'apps/shop/src/assets/locales/en.json' = '{"shop":{"title":"Shop"},"demo":{"label":"L","labelAlt":"A","other":"O","title":"T","kept":"K","alt":"V"}}'
        'structuregate.ts.json' = '{"locales":["apps/shop/src/assets/locales"],"i18nCarriers":["translate:text"]}'
    }
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    $r = Invoke-TsRowsQ $made.Db ("SELECT k.key || ' | ' || k.n_paths || ' | ' || coalesce((SELECT group_concat(c.name, ',') FROM key_reach__components j " +
        "JOIN components c ON c.id = j.value AND j.owner = '#' || k.rowid AND j.parent = 0), '-') || ' | ' || coalesce($(Get-NodeItemListSql 'key_reach__always_values' 'k.rowid' 0 'values'), '-') AS reach FROM key_reach k WHERE k.key LIKE 'demo.%'")
    # B neither reads `labelCode` nor `title`: A's `*ngIf` is the only way to either key
    Assert-Line $r 'demo.label | 1 | AComponent | ["Home"]'
    Assert-Line $r 'demo.title | 1 | AComponent | ["Home"]'
    # and the key B only writes into its unread field has no way at all
    Assert-Line $r 'demo.labelAlt | 0 | - | -'
    # a getter read through a getter the template reads, and a field a template-called method reads
    Assert-Line $r 'demo.kept | 1 | CComponent | -'
    Assert-Line $r 'demo.alt | 1 | CComponent | -'
}

# Two components of one shape, each its own templateUrl: the count template reads `showRange` (the getter calling the
# private writer) and `floorTitle`, the floor template `rangeCode` alone. Each FILE writes `rangeCode`, so another file's
# `prop_reads` spelling the name is no read of it when every one is a write; and a getter's `return` of a ternary of
# keys holds both arms.
Test-Case 'tsrows keys: a field another file only writes and a getter returning a ternary nothing reads are no sites of their keys' {
    $class = {
        param([string]$name)
        "import { Component, Input } from '@angular/core';`n" +
        "import { Kind } from './kinds';`n" +
        "@Component({ selector: 'demo-$($name.ToLower())', templateUrl: './$($name.ToLower()).component.html' })`n" +
        "export class $($name)Component {`n  @Input() model!: { kind: Kind };`n" +
        "  @Input() a = false;`n  @Input() b = false;`n  @Input() flat = false;`n" +
        "  get showRange(): boolean {`n    this.changeCodes();`n    return this.a || this.b;`n  }`n" +
        "  rangeCode = 'demo.range';`n" +
        "  get floorTitle(): string { return this.flat ? 'demo.floors' : 'demo.houseFloors'; }`n" +
        "  private changeCodes(): void {`n    if (this.model.kind === Kind.Flat) {`n      this.rangeCode = 'demo.range';`n" +
        "    } else if (this.model.kind === Kind.Home) {`n      this.rangeCode = 'demo.houseRange';`n    }`n  }`n}`n"
    }
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/kinds.ts' = "export enum Kind { Home = 1, Flat = 2, Life = 3 }`n"
        'apps/shop/src/count.component.ts' = (& $class 'Count')
        'apps/shop/src/count.component.html' = "<div *ngIf=`"showRange`">r</div><b [title]=`"floorTitle`">f</b>`n"
        'apps/shop/src/floor.component.ts' = (& $class 'Floor')
        'apps/shop/src/floor.component.html' = "<div [title]=`"rangeCode`">r</div>`n"
        'apps/shop/src/root.component.ts' = "import { Component } from '@angular/core';`n" +
            "import { Kind } from './kinds';`n" +
            "@Component({ selector: 'demo-root', template: '<demo-count [model]=`"m`"></demo-count><demo-floor [model]=`"m`"></demo-floor>' })`n" +
            "export class RootComponent { m = { kind: Kind.Life }; }`n"
        'apps/shop/src/assets/locales/en.json' = '{"shop":{"title":"Shop"},"demo":{"range":"R","houseRange":"H","floors":"F","houseFloors":"G"}}'
        'structuregate.ts.json' = '{"locales":["apps/shop/src/assets/locales"]}'
    }
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    $r = Invoke-TsRowsQ $made.Db ("SELECT k.key || ' | ' || coalesce((SELECT group_concat(name, ',') FROM (SELECT c.name FROM key_reach__components j " +
        "JOIN components c ON c.id = j.value AND j.owner = '#' || k.rowid AND j.parent = 0 ORDER BY c.name)), '-') AS reach FROM key_reach k WHERE k.key LIKE 'demo.%'")
    # the floor template reads `rangeCode`: its initializer and both writes stay; the count one reads it nowhere
    Assert-Line $r 'demo.range | FloorComponent'
    Assert-Line $r 'demo.houseRange | FloorComponent'
    # the count template reads `floorTitle`: both arms of its return stay; the floor one reads it nowhere
    Assert-Line $r 'demo.floors | CountComponent'
    Assert-Line $r 'demo.houseFloors | CountComponent'
    # the floor's ways keep their branch: the initializer's ungated one and the `if`'s
    $w = Invoke-TsRowsQ $made.Db "SELECT key || ' ' || n_paths AS n FROM key_reach WHERE key IN ('demo.range', 'demo.houseRange')"
    Assert-Line $w 'demo.range 2'
    Assert-Line $w 'demo.houseRange 1'
}

# A write through ANOTHER receiver is no class's own write: `notice.infoMessage = 'k'` on an instance a factory made, in a
# component's method and in a free function, keeps its literal a site of the component under its gate, though nothing
# reads `infoMessage` by name but those two writes.
Test-Case 'tsrows keys: a key written to a member of a made instance stays a site of the component writing it' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/kinds.ts' = "export enum Step { Start = 1, Result = 2 }`n"
        'apps/shop/src/factory.ts' = "export class Factory {`n  create<T>(type: new () => T): T { return new type(); }`n}`n"
        'apps/shop/src/notice.component.ts' = "import { Component } from '@angular/core';`n" +
            "@Component({ selector: 'demo-notice', template: '<p>n</p>' })`n" +
            "export class NoticeComponent {`n  titleInfo = '';`n  infoMessage = '';`n}`n"
        'apps/shop/src/result.component.ts' = "import { Component } from '@angular/core';`n" +
            "import { Factory } from './factory';`nimport { NoticeComponent } from './notice.component';`n" +
            "@Component({ selector: 'demo-result', template: '<button (click)=`"showNotice()`">q</button>' })`n" +
            "export class ResultComponent {`n  constructor(private factory: Factory) {}`n" +
            "  showNotice(): void {`n    const notice = this.factory.create(NoticeComponent);`n" +
            "    notice.titleInfo = 'demo.notification';`n    notice.infoMessage = 'demo.notSupported';`n  }`n}`n"
        'apps/shop/src/notices.ts' = "import { Factory } from './factory';`nimport { NoticeComponent } from './notice.component';`n" +
            "export function showUnsupported(factory: Factory): void {`n  setTimeout(() => {`n" +
            "    const notice = factory.create(NoticeComponent);`n    notice.titleInfo = 'demo.notification';`n" +
            "    notice.infoMessage = 'demo.notSupported';`n  }, 10);`n}`n"
        'apps/shop/src/root.component.ts' = "import { Component } from '@angular/core';`nimport { Step } from './kinds';`n" +
            "@Component({ selector: 'demo-root', template: '<demo-result *ngIf=`"step === Step.Result`"></demo-result>' })`n" +
            "export class RootComponent { Step = Step; step: Step = Step.Start; }`n"
        'apps/shop/src/assets/locales/en.json' = '{"shop":{"title":"Shop"},"demo":{"notification":"N","notSupported":"S"}}'
        'structuregate.ts.json' = '{"locales":["apps/shop/src/assets/locales"]}'
    }
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    $r = Invoke-TsRowsQ $made.Db ("SELECT k.key || ' | ' || k.n_paths || ' | ' || coalesce((SELECT group_concat(c.name, ',') FROM key_reach__components j " +
        "JOIN components c ON c.id = j.value AND j.owner = '#' || k.rowid AND j.parent = 0), '-') || ' | ' || coalesce($(Get-NodeItemListSql 'key_reach__always_values' 'k.rowid' 0 'values'), '-') AS reach FROM key_reach k WHERE k.key LIKE 'demo.%'")
    Assert-Line $r 'demo.notSupported | 1 | ResultComponent | ["Result"]'
    Assert-Line $r 'demo.notification | 1 | ResultComponent | ["Result"]'
}
}
