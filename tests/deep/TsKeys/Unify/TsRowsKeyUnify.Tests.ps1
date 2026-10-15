<#
    Every row keeps its own text; on a way through a render edge the bound spellings are one variable
    (`rust/fbtcore/src/rows/ts/value_folds.rs` `unify_present`, `merge_aliased`) (#33). Its helpers are `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '..\..\TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

$script:UnifyQ = "SELECT k.key || ' | ' || v.dimension || ' ' || v.op || ' ' || $(Get-NodeListSql 'key_reach__always_values' 'v' 'values') AS r " +
    "FROM key_reach k JOIN key_reach__always_values v ON v.owner = '#' || k.rowid AND v.parent = '#' || k.rowid || '/0' WHERE k.key LIKE 'demo.multi.%'"

function New-UnifyFiles([hashtable] $Extra) {
    $f = @{
        'structuregate.ts.json' = '{"locales":["apps/shop/src/assets/locales"],"i18nCalls":["getTranslation"]}'
        'apps/shop/src/shapes.ts' = 'export enum DemoIDs { Alpha = 1, Beta = 2, Gamma = 3, Delta = 4 }'
        'apps/shop/src/tip.component.ts' = @'
import { Component, Input } from '@angular/core';
import { NgIf } from '@angular/common';
import { DemoIDs } from './shapes';
@Component({ selector: 'demo-shape-tip', standalone: true, imports: [NgIf], template: `<span *ngIf="shape !== DemoIDs.Delta">{{ 'demo.multi.tip' | translate }}</span>` })
export class TipComponent {
  DemoIDs = DemoIDs;
  @Input() shape!: DemoIDs;
}
'@
        'apps/shop/src/assets/locales/en.json' = '{"demo":{"multi":{"tip":"1"}}}'
    }
    foreach ($k in $Extra.Keys) { $f[$k] = $Extra[$k] }
    $f
}

function New-UnifyParent([string] $Name, [string] $Field, [string] $Bind) {
    @"
import { Component } from '@angular/core';
import { DemoIDs } from './shapes';
import { TipComponent } from './tip.component';
@Component({ selector: 'demo-p-$Name', standalone: true, imports: [TipComponent], template: '<demo-shape-tip [shape]="$Bind"></demo-shape-tip>' })
export class Parent${Name}Component {
  $Field
}
"@
}

Test-Case 'tsrows keys: a child input bound to different variables on each way keeps its own restriction (#33)' {
    $tree = New-TsRowsWorkspace (New-UnifyFiles @{
        'apps/shop/src/parenta.component.ts' = (New-UnifyParent 'A' 'vmA = { shape: DemoIDs.Alpha };' 'vmA.shape')
        'apps/shop/src/parentb.component.ts' = (New-UnifyParent 'B' 'modelB = { inner: { shape: DemoIDs.Beta } };' 'modelB.inner.shape')
        'apps/shop/src/parentc.component.ts' = (New-UnifyParent 'C' 'cShape = DemoIDs.Gamma;' 'cShape')
    })
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    $r = Invoke-Gate --map-query $made.Db --width 0 --sql $script:UnifyQ
    Assert-Exit $r 0
    Assert-Line $r 'demo.multi.tip | shape not_in ["Delta"]'
}

Test-Case 'tsrows keys: a parent gate on the bound variable intersects with the child gate on its input (#33)' {
    $tree = New-TsRowsWorkspace (New-UnifyFiles @{
        'apps/shop/src/root.component.ts' = @'
import { Component } from '@angular/core';
import { NgIf } from '@angular/common';
import { DemoIDs } from './shapes';
import { TipComponent } from './tip.component';
@Component({ selector: 'demo-root', standalone: true, imports: [NgIf, TipComponent], template: '<div *ngIf="vm.shape !== DemoIDs.Alpha"><demo-shape-tip [shape]="vm.shape"></demo-shape-tip></div>' })
export class RootComponent {
  vm = { shape: DemoIDs.Beta };
  DemoIDs = DemoIDs;
}
'@
    })
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    $r = Invoke-Gate --map-query $made.Db --width 0 --sql $script:UnifyQ
    Assert-Exit $r 0
    Assert-Line $r 'demo.multi.tip | shape | vm.shape in ["Beta","Gamma"]'
    Assert-NoLine $r 'vm.shape not_in'
}

Test-Case 'tsrows keys: a parent gate and a bind on the bound variable keep one restriction across ways (#33)' {
    $parent = { param($Name, $Init, $Gate, $Flag)
        "import { Component } from '@angular/core';`nimport { NgIf } from '@angular/common';`nimport { DemoIDs } from './shapes';`nimport { CardComponent } from './card.component';`n" +
        "@Component({ selector: 'demo-p-$Name', standalone: true, imports: [NgIf, CardComponent], template: '<div *ngIf=`"$Gate`"><demo-shape-card [simulationInput]=`"model`" [showLoader]=`"$Flag`"></demo-shape-card></div>' })`n" +
        "export class Parent${Name}Component {`n  $Init`n  DemoIDs = DemoIDs;`n}`n" }
    $tree = New-TsRowsWorkspace (New-UnifyFiles @{
        'apps/shop/src/card.component.ts' = @'
import { Component, Input } from '@angular/core';
import { NgIf } from '@angular/common';
import { DemoIDs } from './shapes';
@Component({ selector: 'demo-shape-card', standalone: true, imports: [NgIf], template: `<b *ngIf="showLoader">{{ 'demo.multi.card' | translate }}</b>` })
export class CardComponent {
  @Input() simulationInput!: { kind: DemoIDs };
  @Input() showLoader = false;
}
'@
        'apps/shop/src/assets/locales/en.json' = '{"demo":{"multi":{"tip":"1","card":"2"}}}'
        'apps/shop/src/parentone.component.ts' = (& $parent 'One' 'model = { kind: DemoIDs.Beta };' 'model.kind !== DemoIDs.Alpha' 'model.kind === DemoIDs.Beta')
        'apps/shop/src/parenttwo.component.ts' = (& $parent 'Two' 'model = { kind: DemoIDs.Gamma }; ready = true;' 'model.kind === DemoIDs.Gamma' 'ready')
    })
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    $r = Invoke-Gate --map-query $made.Db --width 0 --sql $script:UnifyQ
    Assert-Exit $r 0
    Assert-Line $r 'demo.multi.card | model.kind in ["Beta","Gamma"]'
}
}
