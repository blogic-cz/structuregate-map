<#
    WHICH WAY A KEY REACHED EACH NODE (#126): `key_node_ways` (`rust/fbtcore/src/rows/ts/key/routes/key_node_ways.rs`) - one row per
    (key, node, route, binding, literal) - names the input the key flows through and the template literal that built it there, `typed`
    1 when every hole of that literal is enum-typed (it builds the key exactly), 0 when a hole is untyped (it builds every key its
    pieces match). Its helpers are `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '../../TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

Test-Case 'tsrows keys: each node a key reaches names its input and the literal that built it there (#126)' {
    $made = New-TsRowsDb (New-TsRowsWorkspace @{
        'apps/shop/src/translate.pipe.ts' = "import { Pipe, PipeTransform } from '@angular/core';`n" +
            "@Pipe({ name: 'translate', standalone: true })`nexport class TranslatePipe implements PipeTransform { transform(k: string) { return k; } }`n"
        'apps/shop/src/items.component.ts' = "import { Component, Input } from '@angular/core';`nimport { TranslatePipe } from './translate.pipe';`n" +
            "export enum Kind { Glass = 1 }`n" +
            "// (the pipe is its own file)`n" +
            "@Component({ selector: 'tooltip', standalone: true, template: '' })`n" +
            "export class TooltipComponent { @Input() text = ''; }`n" +
            "@Component({ selector: 'modal', standalone: true, template: '' })`n" +
            "export class ModalComponent { @Input() titleTooltipText = ''; @Input() title = ''; }`n" +
            "@Component({ selector: 'app-items', standalone: true, imports: [TranslatePipe, TooltipComponent, ModalComponent],`n" +
            "  template: '<span>{{ titleOf(n) | translate }}</span><tooltip [text]=`"tipOf(kind)`"></tooltip><modal [titleTooltipText]=`"tipKey`" [title]=`"\'demo.title\' | translate`"></modal>' })`n" +
            "export class ItemsComponent {`n" +
            "  n = 1; kind = Kind.Glass; tipKey = 'demo.tip';`n" +
            "  titleOf(n: number) { return ``demo.item_`${n}``; }`n" +
            "  tipOf(k: Kind) { return ``demo.item_`${k}Tooltip``; }`n" +
            "}`n"
        'apps/shop/src/assets/locales/en.json' = '{"shop":{"title":"Shop"},"demo":{"title":"T","tip":"P","item_1":"I","item_1Tooltip":"IT"}}'
    }) -Shared
    Assert-Exit $made.Result 0
    $r = Invoke-TsRowsQ $made.Db ("SELECT 'w=' || w.key || '|' || coalesce(nd.tag, '-') || '|' || w.route || '|' || coalesce(w.input, '-') || '|' || coalesce(t.line, '-') || '|' || " +
        "coalesce(w.typed, '-') AS r FROM key_node_ways w JOIN template_nodes nd ON nd.id = w.node LEFT JOIN template_literals t ON t.id = w.literal")
    # A KEY TWO LITERALS CAN BUILD: at the tooltip through `[text]`, built exactly by the typed literal (line 14); at the span's text (no tag,
    # no binding) only as an expansion of the untyped one (line 13) - the two nodes told apart.
    Assert-Line $r 'w=demo.item_1Tooltip|tooltip|field_binding|text|14|1'
    Assert-Line $r 'w=demo.item_1Tooltip|-|field_binding|-|13|0'
    Assert-NoLine $r 'w=demo.item_1Tooltip|-|field_binding|-|14'
    # THE INPUT A KEY FLOWS THROUGH: `tipKey` reaches the modal as its tooltip, not its title; a key written whole names no literal.
    Assert-Line $r 'w=demo.tip|modal|field_binding|titleTooltipText|-|-'
    Assert-Line $r 'w=demo.title|modal|template_string|title|-|-'
    Assert-NoLine $r '|pipe|'
}

}
