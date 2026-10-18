<#
    The BLOCK form read where the asterisk form already was: an `@switch` case restricts its switch's
    dimension as `*ngSwitchCase` does (`rust/fbtcore/src/rows/ts/gate/gate_switch.rs`), an `@for` over a list of
    objects is a loop whose item's property reads are key sites and per-element config (`key/key_loops.rs`), and
    an `@if … as` / `*ngIf … as` alias is typed from its condition (`src/TsRows/TsTpl/TsTplReads.mjs`). The pinned
    `@angular/compiler@17` parses every block used here. Its helpers are `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '../../TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

# ONE COMPONENT holding each block shape beside nothing that restricts the same enum, mapped once per case.
function New-TsBlockReadersDb {
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
        'apps/shop/src/owner.ts' = "export interface Owner { title: string; }`n"
        'apps/shop/src/blocks.component.html' = "@switch (grade) {`n" +
            "  @case (grades.Alpha) { <em>{{ 'demo.sw.alpha' | money }}</em> }`n" +
            "  @case (grades.Beta) { <em>{{ 'demo.sw.beta' | money }}</em> }`n" +
            "  @default { <s>{{ 'demo.sw.other' | money }}</s> }`n" +
            "}`n" +
            "@for (t of cards; track t.grade) {`n" +
            "  <ng-container *appOnlyFor=`"{ grades: [t.grade] }`"><app-label [text]=`"t.label`"></app-label></ng-container>`n" +
            "}`n" +
            "@if (owner; as o) { <b>{{ o.title }}</b> }`n" +
            "@if (owner$ | async; as a) { <b>{{ a.title }}</b> }`n" +
            "<i *ngIf=`"owner as p`">{{ p.title }}</i>`n" +
            "@if (owner.title === 'x'; as q) { <b>{{ q.title }}</b> }`n"
        'apps/shop/src/blocks.component.ts' = "import { Component } from '@angular/core';`n" +
            "import { AsyncPipe, NgIf } from '@angular/common';`n" +
            "import { Observable, of } from 'rxjs';`n" +
            "import { Grade } from './grades';`n" +
            "import { Owner } from './owner';`n" +
            "import { OnlyForDirective } from './only-for.directive';`n" +
            "import { LabelComponent } from './label.component';`n" +
            "@Component({ selector: 'demo-blocks', templateUrl: './blocks.component.html', standalone: true,`n" +
            "  imports: [AsyncPipe, NgIf, OnlyForDirective, LabelComponent] })`n" +
            "export class BlocksComponent {`n" +
            "  grades = Grade;`n" +
            "  grade: Grade = Grade.Alpha;`n" +
            "  cards = [{ grade: Grade.Alpha, label: 'demo.blk.alpha' }, { grade: Grade.Beta, label: 'demo.blk.beta' }];`n" +
            "  owner: Owner = { title: 't' };`n" +
            "  owner`$: Observable<Owner> = of({ title: 't' });`n" +
            "  o = 1; a = 2; p = 3; q = 4;`n" +
            "}`n"
        'apps/shop/src/assets/locales/en.json' = '{"demo":{"sw":{"alpha":"A","beta":"B","other":"O"},' +
            '"blk":{"alpha":"A","beta":"B"}}}'
    }
    $made = New-TsRowsDb $tree -Shared
    Assert-Exit $made.Result 0
    return $made.Db
}

# Each key's always_values on the `Grade` enum, `key | dimension op values`.
$script:TsBlockReadersValuesSql = "SELECT k.key || ' | ' || $(Get-TsFieldSql 'key_reach__always_values' 'v' 'dimension') || ' ' || " +
    "$(Get-TsFieldSql 'key_reach__always_values' 'v' 'op') || ' ' || $(Get-NodeListSql 'key_reach__always_values' 'v' 'values') AS reach " +
    "FROM key_reach k JOIN key_reach__always_values v ON v.owner = '#' || k.rowid AND v.parent = 0 WHERE k.key LIKE 'demo.%'"

# AN `@case` IS THE COMPARISON `@switch` WRITES, as `*ngSwitchCase` is: neither gate is a `Binary`, so without
# the switch pass a key one case alone renders read as shown for every grade. `@default` takes no value and
# restricts nothing, exactly as `ngSwitchDefault`.
Test-Case 'tsrows: an @switch case restricts the switch expression to its value' {
    $db = New-TsBlockReadersDb
    $g = Invoke-TsRowsQ $db ("SELECT g.name || ':' || g.source || ' => ' || v.dimension || ' ' || v.op || ' ' || " +
        "$(Get-NodeRootListSql 'gate_values__values_json' 'v.rowid') AS restriction FROM gate_values v JOIN gates g ON g.id = v.gate WHERE v.enum_name = 'Grade'")
    Assert-Line $g 'SwitchBlockCase:grades.Alpha => grade in ["Alpha"]'
    Assert-Line $g 'SwitchBlockCase:grades.Beta => grade in ["Beta"]'
    $k = Invoke-TsRowsQ $db $script:TsBlockReadersValuesSql
    Assert-Line $k 'demo.sw.alpha | grade in ["Alpha"]'
    Assert-Line $k 'demo.sw.beta | grade in ["Beta"]'
    Assert-NoLine $k 'demo.sw.other |'
}

# AN `@for` IS NO GATE, so the loop reader that looked for `ngForOf` gates found none: the item's `t.label` was no
# site, the class's literal was, and the directive config over `t.grade` permitted no element in particular.
Test-Case 'tsrows: an @for over a list of objects is a loop whose item reads are per-element key sites' {
    $db = New-TsBlockReadersDb
    $k = Invoke-TsRowsQ $db $script:TsBlockReadersValuesSql
    Assert-Line $k 'demo.blk.alpha | appOnlyFor.grades in ["Alpha"]'
    Assert-Line $k 'demo.blk.beta | appOnlyFor.grades in ["Beta"]'
}

# The declaration each `Read` of an interpolation resolved to, `name@<owner name>.<member name>`.
function Get-TsBlockReadersTargets([string]$Db, [string]$Source) {
    return Invoke-TsRowsQ $Db ("SELECT $(Get-TsFieldSql 'expressions__ast' 'j' 'name') || '@' || coalesce(i.name || '.' || t.name, " +
        "$(Get-TsFieldSql 'expressions__ast' 'j' 'row')) AS t FROM expressions x JOIN expressions__ast j ON j.owner = x.id AND j.role = 'target' " +
        "LEFT JOIN type_members t ON t.id = $(Get-TsFieldSql 'expressions__ast' 'j' 'row') LEFT JOIN interfaces i ON i.id = t.owner " +
        "WHERE x.source = '$Source'")
}

# AN `as` ALIAS HOLDS ITS CONDITION'S VALUE: a read chain's declared type, a pipe's unwrapped one. It bound to
# nothing, so a read through it resolved nowhere. The alias still shadows the component's own member of that
# name, and a condition that TESTS something (`=== 'x'`) types nothing.
Test-Case 'tsrows: an @if or *ngIf alias is typed from the condition it holds' {
    $db = New-TsBlockReadersDb
    foreach ($alias in 'o', 'a', 'p') {
        $t = Get-TsBlockReadersTargets $db "{{ $alias.title }}"
        Assert-Line $t 'title@Owner.title'
        Assert-NoLine $t "$alias@"
    }
    $q = Get-TsBlockReadersTargets $db '{{ q.title }}'
    Assert-NoLine $q 'title@'
    Assert-NoLine $q 'q@'
}


# AN `@for` AND AN `*ngFor` OVER ONE LIST A FEATURE FILLS (`on ? [..] : []`), each with a key inside, and the
# `@for`'s `@empty` with a key of its own.
function New-TsBlockLoopDb {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/lines.component.html' = "@for (l of lines; track l) { <em>{{ 'demo.loop.inside' | money }}</em> } " +
            "@empty { <s>{{ 'demo.loop.none' | money }}</s> }`n" +
            "<u *ngFor=`"let l of lines`">{{ 'demo.ngfor.inside' | money }}</u>`n"
        'apps/shop/src/lines.component.ts' = "import { Component, OnInit } from '@angular/core';`n" +
            "import { NgFor } from '@angular/common';`n" +
            "import { FlagService, FlagCodes } from './features';`n" +
            "@Component({ selector: 'demo-lines', templateUrl: './lines.component.html', standalone: true, imports: [NgFor] })`n" +
            "export class LinesComponent implements OnInit {`n" +
            "  promoOn = false;`n" +
            "  lines: string[] = [];`n" +
            "  constructor(private readonly features: FlagService) {}`n" +
            "  ngOnInit(): void {`n" +
            "    this.promoOn = this.features.isOn(FlagCodes.IsPromoEnabled);`n" +
            "    this.lines = this.promoOn ? ['x'] : [];`n" +
            "  }`n" +
            "}`n"
        'apps/shop/src/assets/locales/en.json' = '{"demo":{"loop":{"inside":"I","none":"N"},"ngfor":{"inside":"I"}}}'
    }
    $made = New-TsRowsDb $tree -Shared
    Assert-Exit $made.Result 0
    return $made.Db
}

# AN `@for` IS A GATE, as `*ngFor`'s `ngForOf` is: an empty collection renders none of its content. Its `@empty` shows
# exactly then, so it is NOT inside the loop: the loop's sibling, under no loop gate.
Test-Case 'tsrows: an @for gates its content, and its @empty is the loop''s sibling outside that gate' {
    $db = New-TsBlockLoopDb
    $chain = "SELECT n.tag || ' <- ' || coalesce(group_concat(g.name || ':' || g.source, ' / ' ORDER BY j.ord), '(none)') AS chain " +
        "FROM template_nodes n LEFT JOIN template_nodes__gate_chain j ON j.owner = n.id AND j.parent = 0 LEFT JOIN gates g ON g.id = j.value " +
        "WHERE n.tag = '{0}' GROUP BY n.id"
    Assert-Line (Invoke-TsRowsQ $db ($chain -f 'em')) 'em <- ForLoopBlock:lines'
    Assert-Line (Invoke-TsRowsQ $db ($chain -f 's')) 's <- (none)'
    $sib = Invoke-TsRowsQ $db ("SELECT 'sibling=' || (e.parent IS f.parent) || ' depth=' || (e.depth = f.depth) AS s " +
        "FROM template_nodes e, template_nodes f WHERE e.kind = 'ForLoopBlockEmpty' AND f.kind = 'ForLoopBlock'")
    Assert-Line $sib 'sibling=1 depth=1'
}

# `[]` RENDERS NOTHING ONLY WHERE THE GATE ITERATES: a list a feature fills with `on ? [..] : []` requires the feature
# for what an `@for` renders, as for what an `*ngFor` does. The `@empty` key requires nothing.
Test-Case 'tsrows: a feature filling an @for list reaches the keys inside it as it does under *ngFor' {
    $db = New-TsBlockLoopDb
    $f = Invoke-TsRowsQ $db ("SELECT k.key || ' | ' || f.value AS feature FROM key_reach k JOIN key_reach__always_features f ON f.owner = '#' || k.rowid AND f.parent = 0 " +
        "WHERE k.key LIKE 'demo.%'")
    Assert-Line $f 'demo.loop.inside | IsPromoEnabled'
    Assert-Line $f 'demo.ngfor.inside | IsPromoEnabled'
    Assert-NoLine $f 'demo.loop.none |'
}

}
