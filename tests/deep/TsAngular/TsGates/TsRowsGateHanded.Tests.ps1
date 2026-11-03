<#
    AN `<ng-template #ref>` HANDED TO A CHILD (#146, #123 for a child relaying it, `rust/fbtcore/src/rows/ts/key/routes/key_outlets.rs`): the parent queries the template
    with `@ViewChild('ref')` into a field, assigns that field to another inside an `if`, and binds the other to a child's input that the child
    renders only with `*ngIf="!!slot" [ngTemplateOutlet]="slot"`. The contents reach the screen only where the child's outlet does, and
    only after the assignment ran: the way in to them holds the child outlet's `*ngIf`, and the condition the assignment stands under.
    Its helpers are `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '../../TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

# key | always_gates | always_branches | always_values, each gate and branch by its source text, `-` for none.
$script:TsHandedSql = "SELECT k.key || ' | ' || " +
    "COALESCE((SELECT group_concat(g.source, ' & ') FROM key_reach__always_gates a JOIN gates g ON g.id = a.value WHERE a.owner = '#' || k.rowid AND a.parent = 0), '-') || ' | ' || " +
    "COALESCE((SELECT group_concat(b.condition_source, ' & ') FROM key_reach__always_branches a JOIN branches b ON b.id = a.value WHERE a.owner = '#' || k.rowid AND a.parent = 0), '-') || ' | ' || " +
    "COALESCE((SELECT group_concat($(Get-TsFieldSql 'key_reach__always_values' 'v' 'dimension') || ' ' || $(Get-TsFieldSql 'key_reach__always_values' 'v' 'op') || ' ' || " +
    "$(Get-NodeListSql 'key_reach__always_values' 'v' 'values'), ' & ') FROM key_reach__always_values v WHERE v.owner = '#' || k.rowid AND v.parent = 0), '-') AS reach " +
    "FROM key_reach k WHERE k.key LIKE 'demo.%' ORDER BY 1"

# One line per way of each `demo.%` key: its gates, branches and own ids, each by its source text.
$script:TsHandedWays = "SELECT w.key || ' | ' || COALESCE((SELECT group_concat(src, ' & ') FROM (" +
    "SELECT g.source AS src FROM render_path__gates x JOIN gates g ON g.id = x.value WHERE x.owner = p.rowid AND x.parent = 0 " +
    "UNION ALL SELECT b.condition_source FROM render_path__branches x JOIN branches b ON b.id = x.value WHERE x.owner = p.rowid AND x.parent = 0 " +
    "UNION ALL SELECT COALESCE(g.source, b.condition_source, x.value) FROM key_reach_chains__own x LEFT JOIN gates g ON g.id = x.value " +
    "LEFT JOIN branches b ON b.id = x.value WHERE x.owner = w.chain AND x.parent = 0)), '-') AS way " +
    "FROM key_reach_ways w JOIN render_path p ON p.id = w.path WHERE w.key LIKE 'demo.%' ORDER BY 1"

function New-TsHandedTree([string]$Apply = 'apply(kind: Kind) { if (isSpecial(kind)) { this.slot = this.special; } }', [string]$Extra = '') {
    New-TsRowsWorkspace @{
        'apps/shop/src/kinds.ts' = "export enum Kind { Alpha = 1, Beta = 2, Gamma = 3 }`n" +
            "export const isSpecial = (k: Kind) => [Kind.Alpha, Kind.Beta].some(x => x === k);`n"
        'apps/shop/src/translate.component.ts' = "import { Component, Input } from '@angular/core';`n" +
            "@Component({ selector: 'translate', template: '{{ text }}' })`nexport class TranslateComponent { @Input() text: string = ''; }`n"
        'apps/shop/src/special.component.ts' = "import { Component } from '@angular/core';`n" +
            "@Component({ selector: 'x-special', template: '<translate [text]=`"\'demo.special.title\'`"></translate>' })`nexport class SpecialComponent {}`n"
        'apps/shop/src/holder.component.ts' = "import { Component, Input, TemplateRef } from '@angular/core';`n" +
            "@Component({ selector: 'x-holder', template: '<ng-container *ngIf=`"!!slot`" [ngTemplateOutlet]=`"slot`"></ng-container>' })`n" +
            "export class HolderComponent { @Input() slot: TemplateRef<any> | null = null; }`n"
        'apps/shop/src/parent.component.ts' = "import { Component, TemplateRef, ViewChild } from '@angular/core';`nimport { Kind, isSpecial } from './kinds';`n" +
            "@Component({ selector: 'x-parent', template: ``n" +
            "  <ng-template #special><x-special></x-special><translate [text]=`"'demo.parent.inside'`"></translate></ng-template>`n" +
            "  <x-holder [slot]=`"slot`"></x-holder>$Extra`n" +
            "  <translate [text]=`"'demo.parent.title'`"></translate>`n`` })`n" +
            "export class ParentComponent {`n  @ViewChild('special') special!: TemplateRef<any>;`n  slot: TemplateRef<any> | null = null;`n" +
            "  kind: Kind = Kind.Gamma;`n  $Apply`n}`n"
        'apps/shop/src/app.module.ts' = "import { NgModule } from '@angular/core';`nimport { CommonModule } from '@angular/common';`n" +
            "import { TranslateComponent } from './translate.component';`nimport { SpecialComponent } from './special.component';`n" +
            "import { HolderComponent } from './holder.component';`nimport { ParentComponent } from './parent.component';`n" +
            "@NgModule({ declarations: [TranslateComponent, SpecialComponent, HolderComponent, ParentComponent], imports: [CommonModule], bootstrap: [ParentComponent] })`nexport class AppModule {}`n"
        'apps/shop/src/assets/locales/en.json' = '{"shop":{"title":"Shop"},"demo":{"special":{"title":"S"},"parent":{"title":"P","inside":"I"}}}'
        'structuregate.ts.json' = '{"locales":["apps/shop/src/assets/locales"],"i18nCarriers":["translate:text"]}'
    }
}

# THE TEMPLATE COPIED INTO SEVERAL FIELDS, one RELAYED by its child to a grandchild that outlets it (#146): each child is a way of its own,
# the relay's `*ngFor` row and the grandchild's outlet on it, and every way holds what its own assignment stands under.
function New-TsRelayTree {
    $outlet = '<ng-container *ngIf="!!slot" [ngTemplateOutlet]="slot" [ngTemplateOutletContext]="{ item: data }"></ng-container>'
    $holder = { param($class, $selector, $template, $extra) "import { Component, Input, TemplateRef } from '@angular/core';`n" +
        "@Component({ selector: '$selector', template: '$template' })`n" +
        "export class $class { @Input() slot: TemplateRef<any> | null = null; $extra }`n" }
    New-TsRowsWorkspace @{
        'apps/shop/src/kinds.ts' = "export enum Kind { Alpha = 1, Beta = 2, Gamma = 3 }`nexport enum Mode { One = 1, Two = 2 }`n" +
            "export const isSpecial = (k: Kind) => [Kind.Alpha, Kind.Beta].some(x => x === k);`n"
        'apps/shop/src/translate.component.ts' = "import { Component, Input } from '@angular/core';`n" +
            "@Component({ selector: 'translate', template: '{{ text }}' })`nexport class TranslateComponent { @Input() text: string = ''; }`n"
        'apps/shop/src/special.component.ts' = "import { Component, Input } from '@angular/core';`n" +
            "@Component({ selector: 'x-special', template: '<translate [text]=`"\'demo.special.title\'`"></translate>' })`nexport class SpecialComponent { @Input() item: any; }`n"
        'apps/shop/src/holder.component.ts' = (& $holder 'HolderComponent' 'x-holder-a' $outlet 'data: any = null;')
        'apps/shop/src/row.component.ts' = (& $holder 'RowComponent' 'x-row' $outlet 'data: any = null;')
        'apps/shop/src/list.component.ts' = (& $holder 'ListComponent' 'x-list' '<x-row *ngFor="let r of rows" [slot]="slot"></x-row>' 'rows: any[] = [];')
        'apps/shop/src/parent.component.ts' = "import { Component, TemplateRef, ViewChild } from '@angular/core';`nimport { Kind, Mode, isSpecial } from './kinds';`n" +
            "@Component({ selector: 'x-parent', template: ``n" +
            "  <x-holder-a [slot]=`"slotA`"></x-holder-a>`n  <x-list [slot]=`"slotB`"></x-list>`n" +
            "  <ng-template let-item=`"item`" #special><x-special [item]=`"item`"></x-special><translate [text]=`"'demo.parent.inside'`"></translate></ng-template>`n" +
            "  <translate [text]=`"'demo.parent.title'`"></translate>`n`` })`n" +
            "export class ParentComponent {`n  @ViewChild('special') special!: TemplateRef<any>;`n" +
            "  slotA: TemplateRef<any> | null = null;`n  slotB: TemplateRef<any> | null = null;`n" +
            "  model: { kind: Kind; mode: Mode; items: { on: boolean }[] } = { kind: Kind.Gamma, mode: Mode.Two, items: [] };`n" +
            "  ngAfterViewInit() { this.prepare(); }`n" +
            "  private prepare() {`n    if (isSpecial(this.model.kind)) {`n      const ready = this.model.items.some(i => i.on);`n" +
            "      if (this.model.mode === Mode.One && ready) { this.slotA = this.special; }`n      if (ready) { this.slotB = this.special; }`n    }`n  }`n}`n"
        'apps/shop/src/app.module.ts' = "import { NgModule } from '@angular/core';`nimport { CommonModule } from '@angular/common';`n" +
            "import { TranslateComponent } from './translate.component';`nimport { SpecialComponent } from './special.component';`n" +
            "import { HolderComponent } from './holder.component';`nimport { RowComponent } from './row.component';`n" +
            "import { ListComponent } from './list.component';`nimport { ParentComponent } from './parent.component';`n" +
            "@NgModule({ declarations: [TranslateComponent, SpecialComponent, HolderComponent, RowComponent, ListComponent, ParentComponent], imports: [CommonModule], bootstrap: [ParentComponent] })`nexport class AppModule {}`n"
        'apps/shop/src/assets/locales/en.json' = '{"shop":{"title":"Shop"},"demo":{"special":{"title":"S"},"parent":{"title":"P","inside":"I"}}}'
        'structuregate.ts.json' = '{"locales":["apps/shop/src/assets/locales"],"i18nCarriers":["translate:text"]}'
    }
}

Test-Case 'tsrows: a handed template copied into several fields and relayed by a child takes each way its own gates and branches' {
    $made = New-TsRowsDb (New-TsRelayTree) -Shared
    Assert-Exit $made.Result 0
    $r = Invoke-Gate --map-query $made.Db --width 0 --sql $script:TsHandedSql
    # A COMPONENT IN IT has one render edge, holding what every way holds: the outer `if` (the two `!!slot` are two gates).
    Assert-Line $r 'demo.special.title | - | isSpecial(this.model.kind) | model.kind not_in ["Gamma"]'
    # A KEY STRAIGHT IN IT is a way per child: `x-holder-a`'s outlet, and `x-list`'s `*ngFor` relaying it to `x-row`'s outlet.
    $w = Invoke-Gate --map-query $made.Db --width 0 --sql $script:TsHandedWays
    Assert-Line $w 'demo.parent.inside | isSpecial(this.model.kind) & this.model.mode === Mode.One && ready & !!slot'
    Assert-Line $w 'demo.parent.inside | isSpecial(this.model.kind) & ready & !!slot & rows'
}

Test-Case 'tsrows: a template handed to a child through a ViewChild field takes the child outlet gate and the assignment branch' {
    $made = New-TsRowsDb (New-TsHandedTree) -Shared
    Assert-Exit $made.Result 0
    $r = Invoke-Gate --map-query $made.Db --width 0 --sql $script:TsHandedSql
    Assert-Line $r 'demo.parent.title | - | - | -'
    # THE COMPONENT IN THE TEMPLATE, and a key straight in it: the holder's `*ngIf`, and the `if` around `this.slot = this.special`.
    Assert-Line $r 'demo.special.title | !!slot | isSpecial(kind) | -'
    Assert-Line $r 'demo.parent.inside | !!slot | isSpecial(kind) | -'
}

# THE ASSIGNMENT'S CONDITION FOLDS AS ANY BRANCH DOES: a test of an enum field restricts the key to the member.
Test-Case 'tsrows: the condition a handed template is assigned under restricts what it renders' {
    # A HANDLER CALLING IT changes nothing: the test is of a field, not of what the call hands.
    $made = New-TsRowsDb (New-TsHandedTree 'apply() { if (this.kind === Kind.Alpha) { this.slot = this.special; } }' '<button (click)="apply()">go</button>')
    Assert-Exit $made.Result 0
    $r = Invoke-Gate --map-query $made.Db --width 0 --sql $script:TsHandedSql
    Assert-Line $r 'demo.special.title | !!slot | this.kind === Kind.Alpha | kind in ["Alpha"]'
}

# A `case` AROUND THE ASSIGNMENT HOLDS TOO (`Branches::chain_of`): the case folds to its member as a key's own `case` does (#4).
Test-Case 'tsrows: the case a handed template is assigned under restricts what it renders' {
    $made = New-TsRowsDb (New-TsHandedTree 'apply() { switch (this.kind) { case Kind.Alpha: this.slot = this.special; break; default: break; } }')
    Assert-Exit $made.Result 0
    $r = Invoke-Gate --map-query $made.Db --width 0 --sql $script:TsHandedSql
    Assert-Line $r 'demo.special.title | !!slot | - | kind in ["Alpha"]'
}

# A PARAMETER THE CONDITION TESTS IS WHAT EACH CALLER HANDS (`key_handover::through_callers`, `key_args`): `(click)="apply(kind)"` puts the
# component's `kind` where `apply`'s parameter is, so `isSpecial(kind)` restricts that field - one way per handler calling the method.
Test-Case 'tsrows: a handed template assigned under a test of a parameter takes what a template handler hands it' {
    $made = New-TsRowsDb (New-TsHandedTree -Extra '<button (click)="apply(kind)">go</button>')
    Assert-Exit $made.Result 0
    $r = Invoke-Gate --map-query $made.Db --width 0 --sql $script:TsHandedSql
    Assert-Line $r 'demo.special.title | !!slot | - | kind not_in ["Gamma"]'
}

# A STATEMENT CALL IS A CALLER TOO (#124): `this.apply(this.kind)` hands the field as a template handler would. BESIDE a handler, each
# caller is a path of its own into a component in the contents (#126, `closure::load_edges`), so both restrict it.
Test-Case 'tsrows: a handed template assigned under a test of a parameter takes what a statement call hands it' {
    foreach ($extra in '', '<button (click)="apply(kind)">go</button>') {
        $made = New-TsRowsDb (New-TsHandedTree 'ngOnInit() { this.apply(this.kind); } apply(kind: Kind) { if (isSpecial(kind)) { this.slot = this.special; } }' $extra)
        Assert-Exit $made.Result 0
        $r = Invoke-Gate --map-query $made.Db --width 0 --sql $script:TsHandedSql
        Assert-Line $r 'demo.special.title | !!slot | - | kind not_in ["Gamma"]'
    }
}

# A CALLER NO TREE STANDS FOR (a call's result), or a read of the method no call accounts for (`.bind`), may hand anything: a way
# of its own restricts nothing, so neither does the template handler's `kind` any longer.
Test-Case 'tsrows: a handed template keeps no restriction a caller the map cannot read would break' {
    foreach ($init in 'ngOnInit() { this.apply(this.pick()); } pick(): Kind { return Kind.Alpha; }', 'ngOnInit() { setTimeout(this.apply.bind(this, Kind.Alpha)); }') {
        $made = New-TsRowsDb (New-TsHandedTree "$init apply(kind: Kind) { if (isSpecial(kind)) { this.slot = this.special; } }" '<button (click)="apply(kind)">go</button>')
        Assert-Exit $made.Result 0
        $r = Invoke-Gate --map-query $made.Db --width 0 --sql $script:TsHandedSql
        Assert-Line $r 'demo.special.title | !!slot | - | -'
    }
}

# AN ENUM MEMBER A CALLER HANDS SETTLES THE TEST (`key_args::settled`): `this.apply(Kind.Gamma)` fails `isSpecial`, so that way is
# none and the handler's `kind` alone restricts - with no handler, nothing renders the template at all, as `*ngIf="false"` (`key_dead`);
# `this.apply(Kind.Alpha)` passes it, so its way renders whatever any field holds.
Test-Case 'tsrows: an enum member a statement call hands a handed template settles the test it is assigned under' {
    $apply = 'apply(kind: Kind) { if (isSpecial(kind)) { this.slot = this.special; } }'
    $made = New-TsRowsDb (New-TsHandedTree "ngOnInit() { this.apply(Kind.Gamma); } $apply" '<button (click)="apply(kind)">go</button>')
    Assert-Exit $made.Result 0
    $r = Invoke-Gate --map-query $made.Db --width 0 --sql $script:TsHandedSql
    Assert-Line $r 'demo.parent.inside | !!slot | - | kind not_in ["Gamma"]'
    Assert-Line $r 'demo.special.title | !!slot | - | kind not_in ["Gamma"]'
    $made = New-TsRowsDb (New-TsHandedTree "ngOnInit() { this.apply(Kind.Gamma); } $apply")
    Assert-Exit $made.Result 0
    $r = Invoke-Gate --map-query $made.Db --width 0 --sql $script:TsHandedSql
    Assert-Line $r 'demo.special.title | - | - | -'
    Assert-Line $r 'demo.parent.inside | - | - | -'
    foreach ($extra in '', '<button (click)="apply(kind)">go</button>') {
        $made = New-TsRowsDb (New-TsHandedTree "ngOnInit() { this.apply(Kind.Alpha); } $apply" $extra)
        Assert-Exit $made.Result 0
        $r = Invoke-Gate --map-query $made.Db --width 0 --sql $script:TsHandedSql
        Assert-Line $r 'demo.parent.inside | !!slot | - | -'
    }
}

# A PARAMETER HANDED ON IS A HOP (`key_handover::Callers::sites`, #48's `~`): `load(k) { this.apply(k); }` puts what each caller of
# `load` hands where `apply`'s parameter is. Nothing calling `load`, or `load` writing `k` first, leaves no value to name.
Test-Case 'tsrows: a handed template assigned under a test of a parameter takes what a caller hands the method that passes it on' {
    $apply = 'apply(kind: Kind) { if (isSpecial(kind)) { this.slot = this.special; } }'
    $made = New-TsRowsDb (New-TsHandedTree "ngOnInit() { this.load(this.kind); } load(k: Kind) { this.apply(k); } $apply")
    Assert-Exit $made.Result 0
    $r = Invoke-Gate --map-query $made.Db --width 0 --sql $script:TsHandedSql
    Assert-Line $r 'demo.special.title | !!slot | - | kind not_in ["Gamma"]'
    Assert-Line $r 'demo.parent.inside | !!slot | - | kind not_in ["Gamma"]'
    $equals = 'apply(kind: Kind) { if (kind === Kind.Alpha) { this.slot = this.special; } }'
    foreach ($load in "load(k: Kind) { this.apply(k); } $apply", "load(k: Kind) { this.apply(k); } $equals", "ngOnInit() { this.load(this.kind); } load(k: Kind) { k = Kind.Alpha; this.apply(k); } $apply") {
        $made = New-TsRowsDb (New-TsHandedTree $load)
        Assert-Exit $made.Result 0
        $r = Invoke-Gate --map-query $made.Db --width 0 --sql $script:TsHandedSql
        Assert-Line $r 'demo.special.title | !!slot | - | -'
    }
}

# THE QUERIED FIELD READ ANYWHERE ELSE renders the template where the map cannot follow: its contents keep their own chain.
Test-Case 'tsrows: a handed template the class also reads elsewhere keeps its own chain' {
    $made = New-TsRowsDb (New-TsHandedTree 'apply(kind: Kind) { if (isSpecial(kind)) { this.slot = this.special; } } show(at: any) { at.createEmbeddedView(this.special); }')
    Assert-Exit $made.Result 0
    $r = Invoke-Gate --map-query $made.Db --width 0 --sql $script:TsHandedSql
    Assert-Line $r 'demo.special.title | - | - | -'
}

}
