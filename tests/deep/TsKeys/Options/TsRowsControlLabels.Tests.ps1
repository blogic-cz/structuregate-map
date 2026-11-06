<#
    WHICH LABEL A FORM CONTROL IS SHOWN UNDER (#155): `control_labels`, one row per (control, label, key) - the control's own
    label input, else the nearest ancestor's in its template, else the host element's one render edge up, per host
    (`rust/fbtcore/src/rows/ts/key/lists/key_labels.rs`). Its helpers are `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '../../TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

$script:ControlLabelFiles = @{
    'structuregate.ts.json' = '{"locales":["apps/shop/src/assets/locales"]}'
    'apps/shop/src/assets/locales/en.json' = '{"demo":{"amountTitle":"A","amountHint":"H","withTax":"T","withTaxHint":"TH","invoiceTitle":"I","nodeTitle":"N","taxHandled":"TH2","oneWay":"OW","alphaTitle":"AT","betaTitle":"BT","relayTitle":"RT","tierA":"TA"}}'
    'apps/shop/src/parts.ts' = "import { Component, EventEmitter, Input, Output } from '@angular/core';`n" +
        "@Component({ selector: 'text-input', standalone: true, template: '' })`n" +
        "export class TextInputComponent { @Input() value = ''; @Output() valueChange = new EventEmitter<string>(); @Input() label = ''; }`n" +
        "@Component({ selector: 'checkbox', standalone: true, template: '' })`n" +
        "export class CheckboxComponent { @Input() value = false; @Output() valueChange = new EventEmitter<boolean>(); @Input() label = ''; }`n" +
        "@Component({ selector: 'hint', standalone: true, template: '<ng-content></ng-content>' })`nexport class HintComponent {}`n" +
        "@Component({ selector: 'translate', standalone: true, template: '' })`nexport class TranslateComponent { @Input() text = ''; }`n" +
        "@Component({ selector: 'form-row', standalone: true, template: '<ng-content></ng-content>' })`nexport class FormRowComponent { @Input() title = ''; }`n" +
        "@Component({ selector: 'pick', standalone: true, template: '' })`n" +
        "export class PickComponent { @Input() items: { value: string; label: string }[] = []; @Input() value = ''; @Output() valueChange = new EventEmitter<string>(); }`n"
    # THE ISSUE'S SHAPE: the amount input has no label of its own; the checkbox beside it does, and is the nearest label in the tree.
    'apps/shop/src/amount.component.ts' = "import { Component, EventEmitter, Input, OnInit, Output } from '@angular/core';`n" +
        "import { TextInputComponent, CheckboxComponent, HintComponent, TranslateComponent, PickComponent } from './parts';`n" +
        "@Component({ selector: 'amount', standalone: true, imports: [TextInputComponent, CheckboxComponent, HintComponent, TranslateComponent, PickComponent], templateUrl: './amount.component.html' })`n" +
        "export class AmountComponent implements OnInit {`n  @Input() value = 0; @Output() valueChange = new EventEmitter<number>();`n" +
        "  amount = ''; withTax = false; hintKey = ''; handled = false; oneWay = false; tier = '';`n  tiers = [{ value: 'a', label: 'demo.tierA' }];`n  ngOnInit() { this.hintKey = 'demo.amountHint'; }`n" +
        "  onHandled(v: boolean) { this.handled = v; }`n  setOneWay(v: boolean) { this.oneWay = v; }`n  onTier(v: string) { this.tier = v; }`n}`n"
    'apps/shop/src/amount.component.html' = "<text-input [(value)]=`"amount`"></text-input>`n" +
        "<hint><translate [text]=`"hintKey`"></translate></hint>`n" +
        "<checkbox [label]=`"'demo.withTax'`" [(value)]=`"withTax`"></checkbox>`n" +
        "<hint><translate [text]=`"'demo.withTaxHint'`"></translate></hint>`n" +
        "<checkbox [label]=`"'demo.taxHandled'`" [(value)]=`"handled`" (valueChange)=`"onHandled(`$event)`"></checkbox>`n" +
        "<checkbox [label]=`"'demo.oneWay'`" [value]=`"oneWay`" (valueChange)=`"setOneWay(`$event)`"></checkbox>`n" +
        "<pick [items]=`"tiers`" [(value)]=`"tier`" (valueChange)=`"onTier(`$event)`"></pick>`n"
    # TWO HOSTS, under two titles - the second a plain attribute.
    'apps/shop/src/order-form.component.ts' = "import { Component } from '@angular/core';`nimport { AmountComponent } from './amount.component';`n" +
        "import { FormRowComponent } from './parts';`n" +
        "@Component({ selector: 'order-form', standalone: true, imports: [AmountComponent, FormRowComponent], templateUrl: './order-form.component.html' })`n" +
        "export class OrderFormComponent { model = { Amount: 0 }; }`n"
    'apps/shop/src/order-form.component.html' = "<form-row [title]=`"'demo.amountTitle'`">`n  <amount [(value)]=`"model.Amount`"></amount>`n</form-row>`n"
    'apps/shop/src/invoice-form.component.ts' = "import { Component } from '@angular/core';`nimport { AmountComponent } from './amount.component';`n" +
        "import { FormRowComponent } from './parts';`n" +
        "@Component({ selector: 'invoice-form', standalone: true, imports: [AmountComponent, FormRowComponent], templateUrl: './invoice-form.component.html' })`n" +
        "export class InvoiceFormComponent { total = 0; }`n"
    'apps/shop/src/invoice-form.component.html' = "<form-row title=`"demo.invoiceTitle`"><div><amount [(value)]=`"total`"></amount></div></form-row>`n"
    # A COMPONENT RENDERED INSIDE ITSELF, with an unlabelled control and no label anywhere above: the climb ends, with no row.
    'apps/shop/src/tree-node.component.ts' = "import { Component } from '@angular/core';`nimport { NgIf } from '@angular/common';`n" +
        "import { TextInputComponent } from './parts';`n" +
        "@Component({ selector: 'tree-node', standalone: true, imports: [NgIf, TextInputComponent, TreeNodeComponent], templateUrl: './tree-node.component.html' })`n" +
        "export class TreeNodeComponent { name = ''; more = false; }`n"
    'apps/shop/src/tree-node.component.html' = "<text-input [(value)]=`"name`"></text-input><tree-node *ngIf=`"more`"></tree-node>`n"
    # A LABEL BOUND TO THE COMPONENT'S OWN `@Input`, every host filling it: a literal, a plain attribute, and a host relaying ITS input.
    'apps/shop/src/amount-wrapper.component.ts' = "import { Component, Input } from '@angular/core';`nimport { TextInputComponent, FormRowComponent } from './parts';`n" +
        "@Component({ selector: 'amount-wrapper', standalone: true, imports: [TextInputComponent, FormRowComponent], template: '<form-row [title]=`"rowTitle`"><text-input [(value)]=`"amount`"></text-input></form-row>' })`n" +
        "export class AmountWrapperComponent { @Input() rowTitle = ''; amount = ''; }`n"
    'apps/shop/src/hosts.component.ts' = "import { Component, Input } from '@angular/core';`nimport { AmountWrapperComponent } from './amount-wrapper.component';`n" +
        "@Component({ selector: 'alpha-form', standalone: true, imports: [AmountWrapperComponent], template: '<amount-wrapper [rowTitle]=`"\'demo.alphaTitle\'`"></amount-wrapper>' })`n" +
        "export class AlphaFormComponent {}`n" +
        "@Component({ selector: 'beta-form', standalone: true, imports: [AmountWrapperComponent], template: '<amount-wrapper rowTitle=`"demo.betaTitle`"></amount-wrapper>' })`n" +
        "export class BetaFormComponent {}`n" +
        "@Component({ selector: 'relay-form', standalone: true, imports: [AmountWrapperComponent], template: '<amount-wrapper [rowTitle]=`"relayTitle`"></amount-wrapper>' })`n" +
        "export class RelayFormComponent { @Input() relayTitle = ''; }`n" +
        "@Component({ selector: 'relay-host', standalone: true, imports: [RelayFormComponent], template: '<relay-form [relayTitle]=`"\'demo.relayTitle\'`"></relay-form>' })`n" +
        "export class RelayHostComponent {}`n"
}

# One line per row: the control's component and tag, how the label was found, the key, the host element's tag and the edges climbed.
function Get-ControlLabels([string]$Db) {
    $r = Invoke-Gate --map-query $Db --width 0 --limit 0 --sql ("SELECT 'lbl=' || c.name || '|' || n.tag || '|' || l.via || '|' || l.input || '|' || coalesce(l.key, '-') || '|' || " +
        "coalesce((SELECT h.tag || '@' || hc.name FROM template_nodes h JOIN components hc ON hc.id = h.component WHERE h.id = l.host_node), '-') || '|' || l.up AS l " +
        "FROM control_labels l JOIN template_nodes n ON n.id = l.node JOIN components c ON c.id = l.component")
    Assert-Exit $r 0
    return $r
}

Test-Case 'tsrows: a control with no label of its own is shown under its host element''s label, per host (#155)' {
    $made = New-TsRowsDb (New-TsRowsWorkspace $script:ControlLabelFiles) -Shared
    Assert-Exit $made.Result 0
    $r = Get-ControlLabels $made.Db
    # THE HOST'S TITLE, one render edge up - once per place the component is rendered (a bound title and a plain attribute).
    Assert-Line $r 'lbl=AmountComponent|text-input|host|title|demo.amountTitle|amount@OrderFormComponent|1'
    Assert-Line $r 'lbl=AmountComponent|text-input|host|title|demo.invoiceTitle|amount@InvoiceFormComponent|1'
    # THE SIBLING'S LABEL IS NOT IT: the checkbox's `demo.withTax` is the nearest label in the tree, and labels only the checkbox.
    Assert-NoLine $r 'lbl=AmountComponent|text-input|wrap'
    Assert-NoLine $r 'demo.withTax|amount@'
    Assert-Line $r 'lbl=AmountComponent|checkbox|own|label|demo.withTax|-|0'
    # A WRAPPING ELEMENT'S TITLE, in the same template - through a plain `<div>` in between too.
    Assert-Line $r 'lbl=OrderFormComponent|amount|wrap|title|demo.amountTitle|-|0'
    Assert-Line $r 'lbl=InvoiceFormComponent|amount|wrap|title|demo.invoiceTitle|-|0'
    # A COMPONENT RENDERED INSIDE ITSELF ends the climb: no label above, no row, and the map was written.
    Assert-NoLine $r 'lbl=TreeNodeComponent|'
}

Test-Case 'tsrows: a control with its own change handler is labelled, and a label bound to an @Input takes each host''s key (#155)' {
    $made = New-TsRowsDb (New-TsRowsWorkspace $script:ControlLabelFiles) -Shared
    Assert-Exit $made.Result 0
    $r = Get-ControlLabels $made.Db
    # `[(value)]` WITH ITS OWN `(valueChange)` handler is two outputs of one name, and still a two-way control.
    Assert-Line $r 'lbl=AmountComponent|checkbox|own|label|demo.taxHandled|-|0'
    # `[value]` WITH A `(valueChange)` HANDLER shows a value and reports its change: a control to label.
    Assert-Line $r 'lbl=AmountComponent|checkbox|own|label|demo.oneWay|-|0'
    # THE SAME PAIR IS STILL A CONTROL TO `control_options`: its list's keys are offered, the handler's output beside it notwithstanding.
    $o = Invoke-Gate --map-query $made.Db --width 0 --limit 0 --sql "SELECT 'opt=' || o.value || '|' || o.key AS o FROM control_options o"
    Assert-Exit $o 0
    Assert-Line $o 'opt=tier|demo.tierA'
    # `[title]="rowTitle"` over the component's `@Input`: one row per key a host binds it to, the host that wrote it named.
    $k = Invoke-Gate --map-query $made.Db --width 0 --limit 0 --sql ("SELECT 'key=' || l.via || '|' || coalesce(l.key, '-') || '|' || " +
        "coalesce((SELECT h.tag || '@' || hc.name FROM template_nodes h JOIN components hc ON hc.id = h.component WHERE h.id = l.key_host), '-') AS k " +
        "FROM control_labels l JOIN components c ON c.id = l.component WHERE c.name = 'AmountWrapperComponent'")
    Assert-Exit $k 0
    Assert-Line $k 'key=wrap|demo.alphaTitle|amount-wrapper@AlphaFormComponent'
    Assert-Line $k 'key=wrap|demo.betaTitle|amount-wrapper@BetaFormComponent'
    # A HOST RELAYING ITS OWN INPUT is read one render edge further up.
    Assert-Line $k 'key=wrap|demo.relayTitle|relay-form@RelayHostComponent'
    Assert-NoLine $k 'key=wrap|-|'
}

}
