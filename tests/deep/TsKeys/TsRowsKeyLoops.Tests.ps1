<#
    The ELEMENTS of a list an `*ngFor` renders, when no one literal holds them all: a spread of a method's list,
    one write per arm of an `if`, a method returning a list from each arm - its element type written in place
    or NAMED (an interface, a `type` alias, a class). A directive config over the loop's item
    is read once per element (`rust/fbtcore/src/rows/ts/key/key_loops.rs`), and the elements are collected by
    `key/key_loops.rs`. Its helpers are `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '../TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

# One component per file - a key literal in the component's own file links to it and hides the gap. The
# template is the same in each; only how the class fills `cards` differs.
function New-KeyLoopsCards([string]$Name, [string]$Class, [string]$Field, [string]$Body,
        [string]$Type = '{ grade: Grade; label: string }[]', [string]$Import = '') {
    return "import { Component, OnInit } from '@angular/core';`n" + $Import +
        "import { NgFor } from '@angular/common';`n" +
        "import { Grade } from './grades';`n" +
        "import { OnlyForDirective } from './only-for.directive';`n" +
        "import { LabelComponent } from './label.component';`n" +
        "@Component({ selector: 'demo-$Name', standalone: true, imports: [NgFor, OnlyForDirective, LabelComponent], template: ```n" +
        "  <ng-container *ngFor=`"let t of cards`">`n" +
        "    <ng-container *appOnlyFor=`"{ grades: [t.grade] }`"><app-label [text]=`"t.label`"></app-label></ng-container>`n" +
        "  </ng-container>```n" +
        "})`n" +
        "export class $Class implements OnInit {`n" +
        "  isCart = false;`n" +
        "  cards: $Type$Field;`n" +
        $Body +
        "}`n"
}

# EVERY WAY THE CLASS FILLS THE LIST IS ITS ELEMENTS: a spread of a method's list, a write in each arm of an
# `if` (over an empty initializer), and a method returning a list from each arm. Each key is permitted by its
# own element's value; a write's branch rides with the elements it wrote. `plain` is the control.
Test-Case 'tsrows: a config over an ngFor item reads every element the class can fill the list with' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/grades.ts' = "export enum Grade { Alpha = 1, Beta = 2, Gamma = 3 }`n"
        'apps/shop/src/only-for.directive.ts' = "import { Directive, Input, TemplateRef, ViewContainerRef } from '@angular/core';`n" +
            "import { Grade } from './grades';`n" +
            "export interface OnlyForConfig { grades: Grade[]; }`n" +
            "@Directive({ selector: '[appOnlyFor]', standalone: true })`n" +
            "export class OnlyForDirective {`n" +
            "  current: Grade = Grade.Alpha;`n" +
            "  constructor(private templateRef: TemplateRef<any>, private viewContainer: ViewContainerRef) {}`n" +
            "  @Input() set appOnlyFor(input: OnlyForConfig) {`n" +
            "    this.viewContainer.clear();`n" +
            "    if (input.grades.some((m) => m === this.current)) {`n" +
            "      this.viewContainer.createEmbeddedView(this.templateRef);`n" +
            "    }`n" +
            "  }`n" +
            "}`n"
        'apps/shop/src/label.component.ts' = "import { Component, Input } from '@angular/core';`n" +
            "@Component({ selector: 'app-label', template: '<span>{{ text }}</span>', standalone: true })`n" +
            "export class LabelComponent { @Input() text = ''; }`n"
        'apps/shop/src/plain.component.ts' = New-KeyLoopsCards 'plain' 'PlainComponent' '' (
            "  ngOnInit() { this.cards = [{ grade: Grade.Alpha, label: 'demo.plain.alpha' }]; }`n")
        'apps/shop/src/spread.component.ts' = New-KeyLoopsCards 'spread' 'SpreadComponent' '' (
            "  ngOnInit() { this.cards = [{ grade: Grade.Alpha, label: 'demo.spread.alpha' }, ...this.more()]; }`n" +
            "  private more() { return [{ grade: Grade.Beta, label: 'demo.spread.beta' }]; }`n")
        'apps/shop/src/branch.component.ts' = New-KeyLoopsCards 'branch' 'BranchComponent' ' = []' (
            "  ngOnInit() {`n" +
            "    if (this.isCart) { this.cards = [{ grade: Grade.Alpha, label: 'demo.branch.alpha' }]; }`n" +
            "    else { this.cards = [{ grade: Grade.Beta, label: 'demo.branch.beta' }]; }`n" +
            "  }`n")
        'apps/shop/src/ret.component.ts' = New-KeyLoopsCards 'ret' 'RetComponent' '' (
            "  ngOnInit() { this.cards = this.build(); }`n" +
            "  private build() {`n" +
            "    if (this.isCart) { return [{ grade: Grade.Alpha, label: 'demo.ret.alpha' }]; }`n" +
            "    return [{ grade: Grade.Beta, label: 'demo.ret.beta' }];`n" +
            "  }`n")
        # A NAMED ELEMENT TYPE - an imported interface filled by a service, a `type` alias, a class - stamps the
        # item's property read with that type's declaration; it is still the element's own value.
        'apps/shop/src/tip.model.ts' = "import { Grade } from './grades';`n" +
            "export interface TipModel { grade: Grade; label: string; }`n" +
            "export type TipAlias = { grade: Grade; label: string };`n" +
            "export class TipClass { grade: Grade; label: string; }`n"
        'apps/shop/src/tips.service.ts' = "import { Injectable } from '@angular/core';`n" +
            "import { Grade } from './grades';`n" +
            "@Injectable({ providedIn: 'root' })`n" +
            "export class TipsService {`n" +
            "  tips() { return [{ grade: Grade.Alpha, label: 'demo.named.alpha' }, { grade: Grade.Beta, label: 'demo.named.beta' }]; }`n" +
            "}`n"
        'apps/shop/src/named.component.ts' = New-KeyLoopsCards 'named' 'NamedComponent' '' (
            "  constructor(private svc: TipsService) {}`n" +
            "  ngOnInit() { this.cards = this.svc.tips(); }`n") 'TipModel[]' (
            "import { TipModel } from './tip.model';`nimport { TipsService } from './tips.service';`n")
        'apps/shop/src/alias.component.ts' = New-KeyLoopsCards 'alias' 'AliasComponent' '' (
            "  ngOnInit() { this.cards = [{ grade: Grade.Alpha, label: 'demo.alias.alpha' }, { grade: Grade.Beta, label: 'demo.alias.beta' }]; }`n") 'TipAlias[]' (
            "import { TipAlias } from './tip.model';`n")
        'apps/shop/src/klass.component.ts' = New-KeyLoopsCards 'klass' 'KlassComponent' '' (
            "  ngOnInit() { this.cards = [{ grade: Grade.Gamma, label: 'demo.klass.gamma' }]; }`n") 'TipClass[]' (
            "import { TipClass } from './tip.model';`n")
        'apps/shop/src/assets/locales/en.json' = '{"demo":{"plain":{"alpha":"A"},"spread":{"alpha":"A","beta":"B"},' +
            '"branch":{"alpha":"A","beta":"B"},"ret":{"alpha":"A","beta":"B"},"named":{"alpha":"A","beta":"B"},' +
            '"alias":{"alpha":"A","beta":"B"},"klass":{"gamma":"C"}}}'
    }
    $made = New-TsRowsDb $tree
    $k = Invoke-Gate --map-query $made.Db --width 0 --sql ("SELECT k.key || ' | ' || json_extract(v.value, '$.dimension') || ' ' || " +
        "json_extract(v.value, '$.op') || ' ' || json_extract(v.value, '$.values') AS reach " +
        "FROM key_reach k, json_each(k.always_values) v WHERE k.key LIKE 'demo.%'")
    Assert-Exit $k 0
    Assert-Line $k 'demo.plain.alpha | appOnlyFor.grades in ["Alpha"]'
    Assert-Line $k 'demo.spread.alpha | appOnlyFor.grades in ["Alpha"]'
    Assert-Line $k 'demo.spread.beta | appOnlyFor.grades in ["Beta"]'
    Assert-Line $k 'demo.branch.alpha | appOnlyFor.grades in ["Alpha"]'
    Assert-Line $k 'demo.branch.beta | appOnlyFor.grades in ["Beta"]'
    Assert-Line $k 'demo.ret.alpha | appOnlyFor.grades in ["Alpha"]'
    Assert-Line $k 'demo.ret.beta | appOnlyFor.grades in ["Beta"]'
    Assert-Line $k 'demo.named.alpha | appOnlyFor.grades in ["Alpha"]'
    Assert-Line $k 'demo.named.beta | appOnlyFor.grades in ["Beta"]'
    Assert-Line $k 'demo.alias.alpha | appOnlyFor.grades in ["Alpha"]'
    Assert-Line $k 'demo.alias.beta | appOnlyFor.grades in ["Beta"]'
    Assert-Line $k 'demo.klass.gamma | appOnlyFor.grades in ["Gamma"]'
    # THE WRITE'S BRANCH, not the callee's: `build`'s `if` tests nothing the caller's gates share.
    $b = Invoke-Gate --map-query $made.Db --width 0 --sql ("SELECT k.key || ' | ' || coalesce((SELECT group_concat(" +
        "json_extract(k.branch_senses, '$.""' || b.value || '""'), ',') FROM json_each(k.always_branches) b " +
        "WHERE b.value NOT LIKE 'loop:%'), 'none') AS senses FROM key_reach k WHERE k.key LIKE 'demo.branch.%' OR k.key LIKE 'demo.ret.%'")
    Assert-Line $b 'demo.branch.alpha | then'
    Assert-Line $b 'demo.branch.beta | else'
    Assert-Line $b 'demo.ret.alpha | none'
    Assert-Line $b 'demo.ret.beta | none'
}
}
