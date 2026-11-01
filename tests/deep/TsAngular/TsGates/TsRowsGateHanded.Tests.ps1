<#
    AN `<ng-template #ref>` HANDED TO A CHILD (#146, `rust/fbtcore/src/rows/ts/key/routes/key_outlets.rs`): the parent queries the template
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

function New-TsHandedTree([string]$Apply = 'apply(kind: Kind) { if (isSpecial(kind)) { this.slot = this.special; } }') {
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
            "  <x-holder [slot]=`"slot`"></x-holder>`n" +
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
    $made = New-TsRowsDb (New-TsHandedTree 'apply() { if (this.kind === Kind.Alpha) { this.slot = this.special; } }')
    Assert-Exit $made.Result 0
    $r = Invoke-Gate --map-query $made.Db --width 0 --sql $script:TsHandedSql
    Assert-Line $r 'demo.special.title | !!slot | this.kind === Kind.Alpha | kind in ["Alpha"]'
}

# THE QUERIED FIELD READ ANYWHERE ELSE renders the template where the map cannot follow: its contents keep their own chain.
Test-Case 'tsrows: a handed template the class also reads elsewhere keeps its own chain' {
    $made = New-TsRowsDb (New-TsHandedTree 'apply(kind: Kind) { if (isSpecial(kind)) { this.slot = this.special; } } show(at: any) { at.createEmbeddedView(this.special); }')
    Assert-Exit $made.Result 0
    $r = Invoke-Gate --map-query $made.Db --width 0 --sql $script:TsHandedSql
    Assert-Line $r 'demo.special.title | - | - | -'
}

}
