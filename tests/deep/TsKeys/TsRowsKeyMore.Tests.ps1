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
    $r = Invoke-TsRowsQ $made.Db ("SELECT k.key || ' | ' || k.route || ' | ' || c.name AS reach FROM key_reach k, " +
        "json_each(k.components) j JOIN components c ON c.id = j.value WHERE k.key LIKE 'demo.%'")
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
}
