<#
    WHICH OPTION LABELS A FORM CONTROL OFFERS (#117): the keys a control is handed through an input beside its value binding - a literal
    list, a const table rebuilt into items by a helper, a list a loop renders - placed at the control's node and published as
    `control_options` (`rust/fbtcore/src/rows/ts/key/lists/key_options.rs`). Its helpers are `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '../../TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

$script:KeyOptionsFiles = @{
    # `Object.entries` is ES2017's: the default ES5 lib declares no such builtin for the checker to name.
    'apps/shop/tsconfig.app.json' = '{"compilerOptions":{"strict":true,"target":"es2020"},"include":["src/**/*.ts"]}'
    'apps/shop/src/hue-labels.ts' = "export const hueLabels = { Red: 'hue.red', Blue: 'hue.blue' };`n"
    'apps/shop/src/items.ts' = "export function toItems(table: Record<string, string>) {`n" +
        "  return Object.entries(table).map(([value, label]) => ({ value, label }));`n}`n"
    'apps/shop/src/radio.component.ts' = "import { Component, EventEmitter, Input, Output } from '@angular/core';`n" +
        "import { NgFor } from '@angular/common';`n" +
        "@Component({ selector: 'radio-list', standalone: true, imports: [NgFor], template: '<label *ngFor=`"let i of items`">{{ i.label }}</label>' })`n" +
        "export class RadioListComponent {`n" +
        "  @Input() items: { value: string; label: string }[] = [];`n" +
        "  @Input() value = '';`n" +
        "  @Output() valueChange = new EventEmitter<string>();`n" +
        "}`n"
    'apps/shop/src/widget.component.ts' = "import { Component } from '@angular/core';`n" +
        "import { hueLabels } from './hue-labels';`nimport { toItems } from './items';`nimport { RadioListComponent } from './radio.component';`n" +
        "@Component({ selector: 'app-widget', standalone: true, imports: [RadioListComponent],`n" +
        "  template: '<radio-list [items]=`"hues`" [(value)]=`"model.hue`"></radio-list><radio-list [items]=`"sizes`" [(value)]=`"model.size`"></radio-list><radio-list [items]=`"shown`"></radio-list><radio-list [items]=`"tiers`" formControlName=`"tier`"></radio-list>' })`n" +
        "export class WidgetComponent {`n" +
        "  model = { hue: '', size: '' };`n" +
        "  hues = toItems(hueLabels);`n" +
        "  sizes = [{ value: 's', label: 'size.small' }, { value: 'l', label: 'size.large' }];`n" +
        "  shown = [{ value: 'x', label: 'shown.only' }];`n" +
        "  tiers = [{ value: 't', label: 'tier.gold' }];`n" +
        "}`n"
    # THE WORKSPACE'S OWN CONFIG, with the dictionary call this tree's service is.
    'structuregate.ts.json' = '{"locales":["apps/shop/src/assets/locales"],"i18nCarriers":["money"],"gateInputs":["disabled"],' +
        '"featureChecks":["FlagService.isOn"],"featureEnum":"FlagCodes","i18nDictionaryCalls":["getTranslationDictionary"]}'
    'apps/shop/src/grade.ts' = "export enum Grade { Gold = 1, Silver = 2 }`n"
    'apps/shop/src/dictionary.service.ts' = "import { Injectable } from '@angular/core';`n@Injectable({ providedIn: 'root' })`n" +
        "export class DictionaryService { getTranslationDictionary(prefix: string): { [key: string]: string } { return { [prefix]: prefix }; } }`n"
    'apps/shop/src/enum-items.ts' = "export function enumToArray(e: object) { return Object.entries(e).map(([name, value]) => ({ name, value })); }`n" +
        "export function convertToItems(enumObject: object, translations?: { [key: string]: string }, empty?: boolean) {`n" +
        "  const out: { label: string; value: unknown }[] = [];`n" +
        "  if (empty) { out.push({ label: '---', value: null }); }`n" +
        "  out.push(...enumToArray(enumObject).map((x) => ({ label: translations?.[x.name] ?? x.name, value: x.value })));`n" +
        "  return out;`n}`n"
    'apps/shop/src/graded.component.ts' = "import { Component, OnInit } from '@angular/core';`nimport { Grade } from './grade';`n" +
        "import { DictionaryService } from './dictionary.service';`nimport { convertToItems } from './enum-items';`nimport { RadioListComponent } from './radio.component';`n" +
        "@Component({ selector: 'app-graded', standalone: true, imports: [RadioListComponent],`n" +
        "  template: '<radio-list [items]=`"grades`" [(value)]=`"model.grade`"></radio-list><radio-list [items]=`"bare`" [(value)]=`"model.bare`"></radio-list>' })`n" +
        "export class GradedComponent implements OnInit {`n" +
        "  model = { grade: 0, bare: 0 };`n" +
        "  grades: { label: string; value: unknown }[] = [];`n" +
        "  bare: { label: string; value: unknown }[] = [];`n" +
        "  constructor(private dictionary: DictionaryService) {}`n" +
        "  ngOnInit() {`n" +
        "    this.grades = convertToItems(Grade, this.dictionary.getTranslationDictionary('grade'));`n" +
        "    this.bare = convertToItems(Grade);`n" +
        "  }`n}`n"
    'apps/shop/src/view.component.ts' = "import { Component } from '@angular/core';`n" +
        "import { RadioListComponent } from './radio.component';`n" +
        "export function yesNo() { return [{ value: true, label: 'answer.yes' }, { value: false, label: 'answer.no' }]; }`n" +
        "@Component({ selector: 'app-view', standalone: true, imports: [RadioListComponent],`n" +
        "  template: '<radio-list [items]=`"view.criteria.answers`" [(value)]=`"model.answer`"></radio-list>' })`n" +
        "export class ViewComponent {`n" +
        "  model = { answer: true };`n" +
        "  view = { title: 'x', criteria: { answers: yesNo() } };`n" +
        "}`n"
    'apps/shop/src/pick.component.ts' = "import { Component, EventEmitter, Input, Output } from '@angular/core';`n" +
        "@Component({ selector: 'pick-list', standalone: true, template: '' })`n" +
        "export class PickListComponent<T> {`n  @Input() model: { ItemID: T; Label: string }[] = [];`n  @Input() value!: T;`n  @Output() valueChange = new EventEmitter<T>();`n}`n"
    'apps/shop/src/enum.service.ts' = "import { Injectable } from '@angular/core';`n@Injectable({ providedIn: 'root' })`nexport class EnumService {`n" +
        "  toItems(e: object, dict: { [key: string]: string }) {`n" +
        "    return Object.keys(e).filter((k) => isNaN(Number(k))).map((name) => ({ ItemID: (e as any)[name], Label: dict[name] }));`n  }`n" +
        "  pairs(e: object, dict: { [key: string]: string }) { return Object.entries(e).map(([name, value]) => ({ ItemID: value, Label: dict[name] })); }`n}`n"
    'apps/shop/src/ranked.component.ts' = "import { Component } from '@angular/core';`nimport { Grade } from './grade';`n" +
        "import { DictionaryService } from './dictionary.service';`nimport { EnumService } from './enum.service';`nimport { PickListComponent } from './pick.component';`n" +
        "@Component({ selector: 'app-ranked', standalone: true, imports: [PickListComponent],`n" +
        "  template: '<pick-list [(value)]=`"state.rank`" [model]=`"ranks`"></pick-list><pick-list [(value)]=`"state.pair`" [model]=`"pairs`"></pick-list><pick-list [(value)]=`"state.bare`" [model]=`"bare`"></pick-list>' })`n" +
        "export class RankedComponent {`n" +
        "  state = { rank: Grade.Gold, pair: Grade.Gold, bare: Grade.Gold };`n" +
        "  ranks = this.enums.toItems(Grade, this.dictionary.getTranslationDictionary('rank'));`n" +
        "  pairs = this.enums.pairs(Grade, this.dictionary.getTranslationDictionary('rank'));`n" +
        "  bare = this.enums.toItems(Grade, {});`n" +
        "  constructor(private enums: EnumService, private dictionary: DictionaryService) {}`n}`n"
    'apps/shop/src/assets/locales/en.json' = '{"hue":{"red":"Red","blue":"Blue"},"size":{"small":"S","large":"L"},"shown":{"only":"O"},"tier":{"gold":"G"},"grade":{"Gold":"G","Silver":"S"},"answer":{"yes":"Y","no":"N"},"rank":{"Gold":"G","Silver":"S"}}'
}

# THE CONTROL AND ITS LABELS: `hues` is a const table a helper rebuilds into items (`Object.entries(t).map(([value, label]) => ..)`), `sizes` a list
# written in place; each `<radio-list>` binds its value two-way beside them. The route is ids: the bindings, the field (`holder`) and the helper (`callee`).
# THE CONTROL: a list bound where nothing binds a value offers nothing - `shown.only` still renders, through the child's loop, but in no control.
Test-Case 'tsrows: a form control names the option keys it is offered, with the route they took (#117, #118)' {
    $made = New-TsRowsDb (New-TsRowsWorkspace $script:KeyOptionsFiles) -Shared
    Assert-Exit $made.Result 0
    $r = Invoke-Gate --map-query $made.Db --width 0 --limit 0 --sql ("SELECT 'opt=' || c.name || '|' || o.value || '|' || o.input || '.' || o.property || '|' || o.key || '|' || " +
        "m.name || '|' || coalesce(f.name, '-') || '|' || (SELECT b.name FROM bindings b WHERE b.id = o.value_binding) || '|' || (SELECT b.name FROM bindings b WHERE b.id = o.input_binding) AS o " +
        "FROM control_options o JOIN components c ON c.id = o.component JOIN members m ON m.id = o.holder LEFT JOIN functions f ON f.id = o.callee")
    Assert-Exit $r 0
    Assert-Line $r 'opt=WidgetComponent|model.hue|items.label|hue.red|hues|toItems|value|items'
    Assert-Line $r 'opt=WidgetComponent|model.hue|items.label|hue.blue|hues|toItems|value|items'
    Assert-Line $r 'opt=WidgetComponent|model.size|items.label|size.small|sizes|-|value|items'
    Assert-Line $r 'opt=WidgetComponent|model.size|items.label|size.large|sizes|-|value|items'
    # A REACTIVE FORM'S CONTROL: `formControlName` names the value, written as a plain attribute.
    Assert-Line $r 'opt=WidgetComponent|tier|items.label|tier.gold|tiers|-|formControlName|items'
    # AN ENUM A HELPER LABELS FROM A DICTIONARY THE CALLER HANDS IN (rows 110): `label: translations?.[x.name] ?? x.name` over the caller's
    # `getTranslationDictionary('grade')` is every key under `grade.`. THE CONTROL: the same helper handed no dictionary labels nothing.
    $g = Invoke-Gate --map-query $made.Db --width 0 --limit 0 --sql ("SELECT 'grade=' || o.value || '|' || o.property || '|' || o.key || '|' || coalesce(f.name, '-') AS g " +
        "FROM control_options o LEFT JOIN functions f ON f.id = o.callee WHERE o.value LIKE 'model.grade' OR o.value LIKE 'model.bare'")
    Assert-Line $g 'grade=model.grade|label|grade.Gold|convertToItems'
    Assert-Line $g 'grade=model.grade|label|grade.Silver|convertToItems'
    Assert-NoLine $g 'model.bare'
    # A PROPERTY OF A VIEW MODEL'S OBJECT LITERAL (`view.criteria.answers`, as a selector's `vm.criteria.x` does): the read lands on the literal's
    # `shape_members` row, and the list is what that property is written with - here a call of `yesNo()` (`shape_values`).
    $v = Invoke-Gate --map-query $made.Db --width 0 --limit 0 --sql ("SELECT 'view=' || o.value || '|' || o.property || '|' || o.key || '|' || coalesce(f.name, '-') AS v " +
        "FROM control_options o LEFT JOIN functions f ON f.id = o.callee WHERE o.value = 'model.answer'")
    Assert-Line $v 'view=model.answer|label|answer.yes|yesNo'
    Assert-Line $v 'view=model.answer|label|answer.no|yesNo'
    # A HELPER THAT MAPS STRAIGHT OUT OF THE ENUM (#118): `Object.keys(e).filter(..).map((name) => ({ Label: dict[name] }))` returned as it is, a
    # service method called in a field initializer, the list input named `model`. THE CONTROL: the same helper handed no dictionary call offers nothing.
    $m = Invoke-Gate --map-query $made.Db --width 0 --limit 0 --sql ("SELECT 'rank=' || o.value || '|' || o.input || '.' || o.property || '|' || o.key || '|' || coalesce(f.name, m.name, '-') AS m " +
        "FROM control_options o LEFT JOIN functions f ON f.id = o.callee LEFT JOIN members m ON m.id = o.callee WHERE o.value LIKE 'state.%'")
    Assert-Line $m 'rank=state.rank|model.Label|rank.Gold|toItems'
    Assert-Line $m 'rank=state.rank|model.Label|rank.Silver|toItems'
    Assert-Line $m 'rank=state.pair|model.Label|rank.Gold|pairs'
    Assert-Line $m 'rank=state.pair|model.Label|rank.Silver|pairs'
    $bare = Invoke-Gate --map-query $made.Db --sql "SELECT 'bare=' || count(*) AS n FROM control_options WHERE value = 'state.bare'"
    Assert-Line $bare 'bare=0'
    # COUNTED, not looked for in the line above: a row with no value binding would concatenate to NULL and print as nothing.
    $none = Invoke-Gate --map-query $made.Db --sql "SELECT 'shown=' || count(*) AS n FROM control_options WHERE key = 'shown.only'"
    Assert-Line $none 'shown=0'
    Assert-NoLine $r '|hue.red|sizes'
    # THE TABLE'S KEYS RENDER THROUGH THE CHILD'S LOOP, as the written list's do: no longer a bare literal of no component.
    $k = Invoke-Gate --map-query $made.Db --width 0 --sql "SELECT 'reach=' || key || '|' || route || '|' || n_paths AS k FROM key_reach WHERE key LIKE 'hue.%' OR key = 'shown.only'"
    Assert-Line $k 'reach=hue.red|field_binding|1'
    Assert-Line $k 'reach=hue.blue|field_binding|1'
    Assert-Line $k 'reach=shown.only|field_binding|1'
}

}
