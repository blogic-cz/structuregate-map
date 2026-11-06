<#
    HOW A FORM CONTROL SHOWS EACH PROPERTY OF ITS OPTIONS (#156): `control_options.shown_as` - `option` or `hint`, read from the
    control's own template (`rust/fbtcore/src/rows/ts/key/lists/key_shown.rs`), never from the property's name. Its helpers are
    `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '../../TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

$script:ControlShownFiles = @{
    'structuregate.ts.json' = '{"locales":["apps/shop/src/assets/locales"],"hintTags":["hint"]}'
    'apps/shop/src/assets/locales/en.json' = '{"demo":{"mode":{"alpha":"A","beta":"B","note":"N","hints":{"alpha":"HA","beta":"HB"}},"tip":{"name":"TN","text":"TT"},"tierA":"TA"}}'
    'apps/shop/src/parts.ts' = "import { Component, EventEmitter, Input, Output } from '@angular/core';`nimport { NgFor, NgIf } from '@angular/common';`n" +
        "@Component({ selector: 'hint', standalone: true, template: '<ng-content></ng-content>' })`nexport class HintComponent {}`n" +
        "@Component({ selector: 'translate', standalone: true, template: '' })`nexport class TranslateComponent { @Input() text = ''; }`n" +
        # THE ISSUE'S CONTROL: the caption is the choice's text, the help sits in a `<hint>` - and the caption is a tooltip on the
        # label too, which an option's own text outranks. `Note` is read only by a `[hidden]` gate: it decides, it shows nothing.
        "@Component({ selector: 'choice', standalone: true, imports: [NgFor, NgIf, HintComponent, TranslateComponent], template: '" +
        "<label *ngFor=`"let item of items`" [title]=`"item.Caption`"><input type=`"radio`" [value]=`"item.Id`" (change)=`"valueChange.emit(item.Id)`" />" +
        "<translate [text]=`"item.Caption`"></translate><hint *ngIf=`"item.Help`"><translate [text]=`"item.Help`"></translate></hint>" +
        "<span [hidden]=`"!item.Note`"></span></label>' })`n" +
        "export class ChoiceComponent { @Input() items: any[] = []; @Input() value = false; @Output() valueChange = new EventEmitter<boolean>(); }`n" +
        # AN `@for` OVER AN ALIASED INPUT: a property handed to `[title]` is a tooltip.
        "@Component({ selector: 'tip-list', standalone: true, imports: [TranslateComponent], template: '" +
        "@for (t of options; track t.Name) { <span [title]=`"t.Tip`"><translate [text]=`"t.Name`"></translate></span> }' })`n" +
        "export class TipListComponent { @Input('choices') options: any[] = []; @Input() value = ''; @Output() valueChange = new EventEmitter<string>(); }`n" +
        # A CONTROL WHOSE TEMPLATE SHOWS NOTHING of its list (as a library's would): offered, shown as nothing.
        "@Component({ selector: 'pick', standalone: true, template: '' })`n" +
        "export class PickComponent { @Input() items: { value: string; label: string }[] = []; @Input() value = ''; @Output() valueChange = new EventEmitter<string>(); }`n"
    'apps/shop/src/order.component.ts' = "import { Component } from '@angular/core';`nimport { ChoiceComponent, TipListComponent, PickComponent } from './parts';`n" +
        "@Component({ selector: 'order', standalone: true, imports: [ChoiceComponent, TipListComponent, PickComponent], templateUrl: './order.component.html' })`n" +
        "export class OrderComponent {`n  model = { Mode: true, Tip: '', Tier: '' };`n" +
        "  modeOptions = [{ Id: true, Caption: 'demo.mode.alpha', Help: 'demo.mode.hints.alpha', Note: 'demo.mode.note' }, { Id: false, Caption: 'demo.mode.beta', Help: 'demo.mode.hints.beta', Note: 'demo.mode.note' }];`n" +
        "  tips = [{ Name: 'demo.tip.name', Tip: 'demo.tip.text' }];`n  tiers = [{ value: 'a', label: 'demo.tierA' }];`n}`n"
    'apps/shop/src/order.component.html' = "<choice [(value)]=`"model.Mode`" [items]=`"modeOptions`"></choice>`n" +
        "<tip-list [(value)]=`"model.Tip`" [choices]=`"tips`"></tip-list>`n" +
        "<pick [(value)]=`"model.Tier`" [items]=`"tiers`"></pick>`n"
}

Test-Case 'tsrows: control_options says whether the control shows a property as the option or as help, from its own template (#156)' {
    $made = New-TsRowsDb (New-TsRowsWorkspace $script:ControlShownFiles) -Shared
    Assert-Exit $made.Result 0
    $r = Invoke-Gate --map-query $made.Db --width 0 --limit 0 --sql ("SELECT 'shown=' || o.input || '|' || o.property || '|' || o.key || '|' || " +
        "coalesce(o.shown_as, '-') || '|' || coalesce(n.tag, '-') AS s FROM control_options o LEFT JOIN template_nodes n ON n.id = o.shown_node")
    Assert-Exit $r 0
    # THE CAPTION IS THE CHOICE'S OWN TEXT - its tooltip on the label notwithstanding; THE HELP IS INSIDE A `hintTags` ELEMENT.
    Assert-Line $r 'shown=items|Caption|demo.mode.alpha|option|translate'
    Assert-Line $r 'shown=items|Caption|demo.mode.beta|option|translate'
    Assert-Line $r 'shown=items|Help|demo.mode.hints.alpha|hint|translate'
    Assert-Line $r 'shown=items|Help|demo.mode.hints.beta|hint|translate'
    # A PROPERTY ONLY A GATE READS is shown as nothing.
    Assert-Line $r 'shown=items|Note|demo.mode.note|-|-'
    # AN `@for` over an aliased input: the name is the option, the property handed to `[title]` (a `hintInputs` default) a hint.
    Assert-Line $r 'shown=choices|Name|demo.tip.name|option|translate'
    Assert-Line $r 'shown=choices|Tip|demo.tip.text|hint|span'
    # A TEMPLATE SHOWING NOTHING OF THE LIST: offered, NULL.
    Assert-Line $r 'shown=items|label|demo.tierA|-|-'
}

}
