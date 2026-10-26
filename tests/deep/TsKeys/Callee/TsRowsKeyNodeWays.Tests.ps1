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

# (#132) A KEY THE TYPED LITERAL'S PREFIX SPELLS BUT CANNOT BUILD: `Item_${id}` over `id: Item` builds `Item_1` and `Item_2` exactly, while the reach
# also expands it to `Item_1Tooltip` - that way names the literal with `typed` 0, never NULL like a key written whole.
Test-Case 'tsrows keys: an over-expansion of a typed literal names the literal, typed 0 (#132)' {
    $made = New-TsRowsDb (New-TsRowsWorkspace @{
        'apps/shop/src/translate.pipe.ts' = "import { Pipe, PipeTransform } from '@angular/core';`n" +
            "@Pipe({ name: 'translate', standalone: true })`nexport class TranslatePipe implements PipeTransform { transform(k: string) { return k; } }`n"
        'apps/shop/src/items.component.ts' = "import { Component } from '@angular/core';`nimport { TranslatePipe } from './translate.pipe';`n" +
            "export enum Item { A = 1, B = 2 }`n" +
            "@Component({ selector: 'app-items', standalone: true, imports: [TranslatePipe], template: '<span>{{ titleOf(item) | translate }}</span>' })`n" +
            "export class ItemsComponent {`n  item = Item.A;`n  titleOf(id: Item) { return ``demo.items.Item_`${id}``; }`n}`n"
        'apps/shop/src/assets/locales/en.json' = '{"shop":{"title":"Shop"},"demo":{"items":{"Item_1":"A","Item_2":"B","Item_1Tooltip":"AT"}}}'
    }) -Shared
    Assert-Exit $made.Result 0
    $r = Invoke-TsRowsQ $made.Db ("SELECT 'w=' || w.key || '|' || coalesce(t.line, '-') || '|' || coalesce(w.typed, '-') AS r FROM key_node_ways w " +
        "LEFT JOIN template_literals t ON t.id = w.literal WHERE w.key LIKE 'demo.items.%'")
    Assert-Line $r 'w=demo.items.Item_1|7|1'
    Assert-Line $r 'w=demo.items.Item_2|7|1'
    Assert-Line $r 'w=demo.items.Item_1Tooltip|7|0'
    Assert-NoLine $r 'w=demo.items.Item_1Tooltip|-|-'
}

# (#135) ONE STEP FURTHER: a key built inside a field's initializer (`rows = [..].map(id => ({ name: t(`demo.item_${id}`) }))`) reaches `row.name`,
# and a getter that calls a service method building it (`get desc() { return this.texts.of(this.code); }`) reaches `[innerHTML]` - each way
# names the literal that built it there, never NULL like a key written whole.
Test-Case 'tsrows keys: a key reaching a node through a field or a getter names its literal (#135)' {
    $made = New-TsRowsDb (New-TsRowsWorkspace @{
        'apps/shop/src/texts.service.ts' = "import { Injectable } from '@angular/core';`n" +
            "@Injectable({ providedIn: 'root' })`nexport class I18nService { getTranslation(k: string) { return k; } }`n" +
            "@Injectable({ providedIn: 'root' })`nexport class TextsService {`n  constructor(private i18n: I18nService) {}`n" +
            "  of(code: string) { return this.i18n.getTranslation(``demo.desc.`${code}``); }`n}`n"
        'apps/shop/src/rows.component.ts' = "import { Component } from '@angular/core';`nimport { NgFor } from '@angular/common';`n" +
            "import { I18nService, TextsService } from './texts.service';`nexport enum Kind { A = 1 }`n" +
            "@Component({ selector: 'app-rows', standalone: true, imports: [NgFor], template: '<td *ngFor=`"let row of rows`">{{ row.name }}</td><span [innerHTML]=`"desc`"></span>' })`n" +
            "export class RowsComponent {`n" +
            "  rows = [Kind.A].map((id) => ({ name: this.i18n.getTranslation(``demo.item_`${id}``) }));`n" +
            "  get desc() { return this.texts.of(this.code); }`n  code = 'x';`n" +
            "  constructor(private i18n: I18nService, private texts: TextsService) {}`n}`n"
        'apps/shop/src/assets/locales/en.json' = '{"shop":{"title":"Shop"},"demo":{"item_1":"I","item_1Tooltip":"IT","desc":{"a":"A","b":"B"}}}'
        'structuregate.ts.json' = '{"locales":["apps/shop/src/assets/locales"],"i18nCalls":["getTranslation"]}'
    }) -Shared
    Assert-Exit $made.Result 0
    $r = Invoke-TsRowsQ $made.Db ("SELECT 'w=' || w.key || '|' || coalesce(nd.tag, '-') || '|' || coalesce(t.line, '-') || '|' || coalesce(w.typed, '-') AS r " +
        "FROM key_node_ways w JOIN template_nodes nd ON nd.id = w.node LEFT JOIN template_literals t ON t.id = w.literal WHERE w.key LIKE 'demo.%'")
    Assert-Line $r 'w=demo.item_1|-|7|1'
    Assert-Line $r 'w=demo.item_1Tooltip|-|7|0'
    Assert-Line $r 'w=demo.desc.a|span|7|0'
    # NO WAY READS AS A KEY WRITTEN WHOLE (literal and typed both NULL).
    $whole = @($r.Lines | Where-Object { $_.Trim().StartsWith('w=') -and $_.Trim().EndsWith('|-|-') })
    if ($whole.Count) { throw "ways with no literal:`n$($whole -join "`n")" }
}

}
