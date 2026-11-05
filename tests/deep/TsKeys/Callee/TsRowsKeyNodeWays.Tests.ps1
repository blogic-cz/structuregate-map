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

# (#135) A FIELD A METHOD WRITES: the initializer is only `[]`, and the rows are built where a method assigns the field
# (`this.rows = IDS.map(id => ({ name: t(..) })).filter(..)`) or pushes onto it in a loop - the literal is in that method.
Test-Case 'tsrows keys: a key reaching a node through a field a method assigns or pushes onto names its literal (#135)' {
    $made = New-TsRowsDb (New-TsRowsWorkspace @{
        'apps/shop/src/table.component.ts' = "import { Component } from '@angular/core';`nimport { NgFor } from '@angular/common';`n" +
            "export class I18nService { getTranslation(k: string) { return k; } }`nexport enum Kind { A = 1 }`n" +
            "@Component({ selector: 'app-table', standalone: true, imports: [NgFor], template: '<td *ngFor=`"let row of rows`">{{ row.name }}</td><b *ngFor=`"let m of months`">{{ m.name }}</b>' })`n" +
            "export class TableComponent {`n  rows: { name: string }[] = [];`n  months: { name: string }[] = [];`n" +
            "  constructor(private i18n: I18nService) {}`n" +
            "  build() {`n    this.rows = [Kind.A].map((id) => ({ name: this.i18n.getTranslation(``demo.item_`${id}``) })).filter((r) => !!r.name);`n  }`n" +
            "  fill(start: number) {`n    for (let n = 1; n <= 2; n++) {`n      this.months.push({ name: this.i18n.getTranslation(``demo.month.`${n + start}``) });`n    }`n  }`n}`n"
        'apps/shop/src/assets/locales/en.json' = '{"shop":{"title":"Shop"},"demo":{"item_1":"I","item_1Tooltip":"IT","month":{"1":"Jan","2":"Feb"}}}'
        'structuregate.ts.json' = '{"locales":["apps/shop/src/assets/locales"],"i18nCalls":["getTranslation"]}'
    }) -Shared
    Assert-Exit $made.Result 0
    $r = Invoke-TsRowsQ $made.Db ("SELECT 'w=' || w.key || '|' || coalesce(t.line, '-') || '|' || coalesce(w.typed, '-') AS r " +
        "FROM key_node_ways w LEFT JOIN template_literals t ON t.id = w.literal WHERE w.key LIKE 'demo.%'")
    Assert-Line $r 'w=demo.item_1|11|1'
    Assert-Line $r 'w=demo.item_1Tooltip|11|0'
    Assert-Line $r 'w=demo.month.1|15|0'
    $whole = @($r.Lines | Where-Object { $_.Trim().StartsWith('w=') -and $_.Trim().EndsWith('|-|-') })
    if ($whole.Count) { throw "ways with no literal:`n$($whole -join "`n")" }
}

# (#137) A HOLE THE TEMPLATE'S CALL SPELLS WITH A LITERAL is no wildcard: `label('time')` over `'demo.flight.' + name` builds that one key at
# that node, so the way is exact (typed 1) - the untyped hole marked it 0, an expansion, though no other key can come of that call.
Test-Case 'tsrows keys: a hole a template call fills with a literal argument makes its way exact (#137)' {
    $made = New-TsRowsDb (New-TsRowsWorkspace @{
        'apps/shop/src/flights.component.ts' = "import { Component, Injectable, Input } from '@angular/core';`n" +
            "@Injectable({ providedIn: 'root' })`nexport class I18nService { getTranslation(k: string) { return k; } }`n" +
            "@Component({ selector: 'demo-item', standalone: true, template: '<i>{{ title }}</i>' })`nexport class ItemComponent { @Input() title = ''; }`n" +
            "@Component({ selector: 'app-flights', standalone: true, imports: [ItemComponent], template: '<demo-item [title]=`"label(\'time\')`"></demo-item><demo-item [title]=`"label(\'number\')`"></demo-item>' })`n" +
            "export class FlightsComponent {`n  label(name: string) { return this.i18n.getTranslation('demo.flight.' + name); }`n  constructor(private i18n: I18nService) {}`n}`n"
        'apps/shop/src/assets/locales/en.json' = '{"shop":{"title":"Shop"},"demo":{"flight":{"time":"T","number":"N","title":"X"}}}'
        'structuregate.ts.json' = '{"locales":["apps/shop/src/assets/locales"],"i18nCalls":["getTranslation"]}'
    }) -Shared
    Assert-Exit $made.Result 0
    $r = Invoke-Gate --map-query $made.Db --width 0 --sql ("SELECT 'w=' || w.key || '|' || coalesce(w.input, '-') || '|' || coalesce(w.typed, '-') AS r FROM key_node_ways w WHERE w.key LIKE 'demo.flight.%'")
    Assert-Line $r 'w=demo.flight.time|title|1'
    Assert-Line $r 'w=demo.flight.number|title|1'
    Assert-NoLine $r 'demo.flight.title'
}

}

# (#135, re-check) A FIELD HANDED TO A CHILD'S INPUT, AND A HELPER'S RETURN PUSHED ONTO A FIELD, name their literal too. The child reads
# `item.Label` off its `@Input() model`, which the parent binds `[model]="kinds"` - the literal is in the method assigning `kinds`. `rows`
# gets `{ label: this.labelOf(code) }` pushed in `ngOnInit`, and `labelOf` returns the literal, read through the translate pipe.
Test-Case 'tsrows keys: a key reaching a node through a child input or a pushed helper return names its literal (#135)' {
    $made = New-TsRowsDb (New-TsRowsWorkspace @{
        'apps/shop/src/form.component.ts' = "import { Component, Injectable, Input, OnInit } from '@angular/core';`nimport { NgFor } from '@angular/common';`n" +
            "@Injectable({ providedIn: 'root' })`nexport class I18nService { getTranslation(k: string) { return k; } }`nexport enum Kind { A = 1, B = 2 }`n" +
            "@Component({ selector: 'app-choice', standalone: true, imports: [NgFor], template: '<div *ngFor=`"let item of model`"><span [innerHTML]=`"item.Label | money`"></span></div>' })`n" +
            "export class ChoiceComponent { @Input() model: { ItemID: number; Label: string }[] = []; }`n" +
            "@Component({ selector: 'app-form', standalone: true, imports: [ChoiceComponent], template: '<app-choice [model]=`"kinds`"></app-choice>' })`n" +
            "export class FormComponent implements OnInit {`n  kinds: { ItemID: number; Label: string }[] = [];`n  constructor(private i18n: I18nService) {}`n" +
            "  ngOnInit() {`n    this.kinds = [Kind.A, Kind.B].map((e) => ({ ItemID: e, Label: this.i18n.getTranslation('demo.kind.' + e) }));`n  }`n}`n" +
            "@Component({ selector: 'app-ids', standalone: true, imports: [NgFor], template: '<div *ngFor=`"let row of rows`">{{ row.label | money }}</div>' })`n" +
            "export class IdsComponent implements OnInit {`n  rows: { key: string; label: string }[] = [];`n" +
            "  ngOnInit() {`n    ['Alpha', 'Beta'].forEach((code) => this.rows.push({ key: code, label: this.labelOf(code) }));`n  }`n" +
            "  labelOf(code: string): string {`n    return ``demo.ids.`${code}``;`n  }`n}`n"
        'apps/shop/src/assets/locales/en.json' = '{"shop":{"title":"Shop"},"demo":{"kind":{"1":"A","2":"B"},"ids":{"Alpha":"a","Beta":"b","Gamma":"g"}}}'
        'structuregate.ts.json' = '{"locales":["apps/shop/src/assets/locales"],"i18nCalls":["getTranslation"]}'
    }) -Shared
    Assert-Exit $made.Result 0
    $r = Invoke-TsRowsQ $made.Db ("SELECT 'w=' || w.key || '|' || w.route || '|' || coalesce(t.line, '-') || '|' || coalesce(w.typed, '-') AS r " +
        "FROM key_node_ways w LEFT JOIN template_literals t ON t.id = w.literal WHERE w.key LIKE 'demo.%'")
    Assert-Line $r 'w=demo.kind.1|field_binding|13|1'
    Assert-Line $r 'w=demo.kind.2|field_binding|13|1'
    Assert-Line $r 'w=demo.ids.Alpha|field_binding|23|0'
    $whole = @($r.Lines | Where-Object { $_.Trim().StartsWith('w=') -and $_.Trim().EndsWith('|-|-') })
    if ($whole.Count) { throw "ways with no literal:`n$($whole -join "`n")" }
}
