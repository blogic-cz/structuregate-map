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
        "  template: '<radio-list [items]=`"hues`" [(value)]=`"model.hue`"></radio-list><radio-list [items]=`"sizes`" [(value)]=`"model.size`"></radio-list><radio-list [items]=`"shown`"></radio-list><radio-list [items]=`"tiers`" formControlName=`"tier`"></radio-list>' +`n" +
        "    '<radio-list [value]=`"model.hue`" (valueChange)=`"model.hue = `$event`" [items]=`"spelled`"></radio-list><radio-list [value]=`"model.hue`" (valueChange)=`"model.size = `$event`" [items]=`"crossed`"></radio-list>' })`n" +
        "export class WidgetComponent {`n" +
        "  model = { hue: '', size: '' };`n" +
        "  hues = toItems(hueLabels);`n" +
        "  sizes = [{ value: 's', label: 'size.small' }, { value: 'l', label: 'size.large' }];`n" +
        "  shown = [{ value: 'x', label: 'shown.only' }];`n" +
        "  tiers = [{ value: 't', label: 'tier.gold' }];`n" +
        "  spelled = [{ value: 'w', label: 'spelled.out' }];`n" +
        "  crossed = [{ value: 'c', label: 'crossed.over' }];`n" +
        "}`n"
    # THE WORKSPACE'S OWN CONFIG, with the dictionary call this tree's service is.
    'structuregate.ts.json' = '{"locales":["apps/shop/src/assets/locales"],"i18nCarriers":["money"],"gateInputs":["disabled"],' +
        '"featureChecks":["FlagService.isOn"],"featureEnum":"FlagCodes","i18nDictionaryCalls":["getTranslationDictionary"],"i18nCalls":["getTranslation"]}'
    'apps/shop/src/grade.ts' = "export enum Grade { Gold = 1, Silver = 2 }`n"
    'apps/shop/src/dictionary.service.ts' = "import { Injectable } from '@angular/core';`n@Injectable({ providedIn: 'root' })`n" +
        "export class DictionaryService { getTranslationDictionary(prefix: string): { [key: string]: string } { return { [prefix]: prefix }; }`n" +
        "  getTranslation(key: string): string { return key; } }`n"
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
    # AN ENUM HELPER CHAINED (#117 follow-up): a library's `for..in` list (`enumToList`), a helper spreading its `.map(..)`, a second
    # helper mapping the first - and a caller's `.filter(..)`/`.map(..)` on top of any of them.
    'apps/shop/src/ui/object.utils.ts' = "export const enumToList = <T extends object>(e: T): { name: string; value: T[keyof T] }[] => {`n" +
        "  const result: { name: string; value: T[keyof T] }[] = [];`n" +
        "  for (const item in e) {`n    result.push({ name: (e as any)[item], value: item as T[keyof T] });`n  }`n  return result;`n};`n"
    'apps/shop/src/select-items.ts' = "import { enumToList } from './ui/object.utils';`n" +
        "export const toSelect = <T extends object>(e: T, translations?: { [key: string]: string }, empty?: boolean): { label: string; value: unknown }[] => {`n" +
        "  const out: { label: string; value: unknown }[] = [];`n  if (empty) {`n    out.push({ label: '---', value: null });`n  }`n" +
        "  out.push(...enumToList(e).map((x) => ({ label: translations?.[x.name] ?? x.name, value: x.value })));`n  return out;`n};`n" +
        "export const toRadio = <T extends object>(e: T, translations?: { [key: string]: string }): { ItemID: unknown; Label: string }[] => {`n" +
        "  return toSelect(e, translations).map((x) => ({ ItemID: x.value, Label: x.label }));`n};`n"
    'apps/shop/src/tone.component.ts' = "import { Component, OnInit } from '@angular/core';`nimport { DictionaryService } from './dictionary.service';`n" +
        "import { enumToList } from './ui/object.utils';`nimport { toRadio, toSelect } from './select-items';`n" +
        "import { RadioListComponent } from './radio.component';`nimport { PickListComponent } from './pick.component';`n" +
        "export enum Tone { Warm = 'Warm', Cool = 'Cool' }`n" +
        "@Component({ selector: 'app-tone', standalone: true, imports: [RadioListComponent, PickListComponent],`n" +
        "  template: '<radio-list [items]=`"tones`" [(value)]=`"pick.tone`"></radio-list><pick-list [(value)]=`"pick.kind`" [model]=`"kinds`"></pick-list>' +`n" +
        "    '<radio-list [items]=`"months`" [(value)]=`"pick.month`"></radio-list><radio-list [items]=`"plain`" [(value)]=`"pick.plain`"></radio-list>' +`n" +
        "    '<radio-list [items]=`"rates`" [(value)]=`"pick.rate`"></radio-list><radio-list [items]=`"steps`" [(value)]=`"pick.step`"></radio-list>' +`n" +
        "    '<pick-list [(value)]=`"pick.answer`" [model]=`"criteria.answers`"></pick-list><radio-list [items]=`"rated`" [(value)]=`"pick.rated`"></radio-list>' })`n" +
        "export class ToneComponent implements OnInit {`n" +
        "  pick = { tone: '', kind: '', month: '', plain: '', rate: '', step: '', answer: '', rated: '' };`n" +
        "  tones = toSelect(Tone, this.dictionary.getTranslationDictionary('tone')).filter((x) => x.value !== Tone.Cool);`n" +
        "  plain = toSelect(Tone).filter((x) => x.value !== Tone.Cool);`n" +
        "  kinds: { ItemID: unknown; Label: string }[] = [];`n" +
        "  months: { label: string; value: string }[] = [];`n" +
        "  rates = this.rateOptions();`n" +
        "  rated = toSelect(Tone, this.dictionary.getTranslationDictionary('rate')).map((x) => ({ ...x, label: ```${x.label} %`` }));`n" +
        "  steps = this.stepOptions();`n" +
        "  criteria!: { answers: { ItemID: unknown; Label: string }[] };`n" +
        "  constructor(private dictionary: DictionaryService) {}`n" +
        "  private rateOptions() {`n    return toSelect(Tone, this.dictionary.getTranslationDictionary('rate'));`n  }`n" +
        "  private stepOptions() {`n    return [{ label: 'step.none', value: '' as unknown }].concat(toSelect(Tone, this.dictionary.getTranslationDictionary('step')).map((x) => ({ ...x, value: x.value })));`n  }`n" +
        "  ngOnInit() {`n" +
        "    this.criteria = { answers: toRadio(Tone, this.dictionary.getTranslationDictionary('answer')) };`n" +
        "    this.kinds = toRadio(Tone).filter((x) => x.ItemID !== Tone.Cool).map((x) => ({ ItemID: x.ItemID, Label: ``kind.`${x.Label}`` }));`n" +
        "    this.months = enumToList(this.dictionary.getTranslationDictionary('month')).map((x, i) => ({ label: x.name, value: String(i + 1) }));`n" +
        "  }`n}`n"
    # A LABELS TABLE THE CLASS READS THROUGH `this` (#117 follow-up): template literals over the class's own fields, up to a base's.
    'apps/shop/src/fee-base.ts' = "export abstract class FeeBase {`n  rootKey = 'fee';`n}`n"
    'apps/shop/src/fee.component.ts' = "import { Component, OnInit } from '@angular/core';`nimport { FeeBase } from './fee-base';`n" +
        "import { PickListComponent } from './pick.component';`n" +
        "@Component({ selector: 'app-fee', standalone: true, imports: [PickListComponent],`n" +
        "  template: '<pick-list [(value)]=`"fee.kind`" [model]=`"feeOptions`"></pick-list><pick-list [(value)]=`"fee.moved`" [model]=`"movedOptions`"></pick-list>' })`n" +
        "export class FeeComponent extends FeeBase implements OnInit {`n" +
        "  fee = { kind: 0, moved: 0 };`n" +
        "  feeResource = ```${this.rootKey}.plan``;`n" +
        "  moved = 'fee';`n" +
        "  labels = { plan: { prepaid: ```${this.feeResource}.prepaid``, express: ```${this.feeResource}.express`` } };`n" +
        "  feeOptions: { ItemID: number; Label: string }[] = [];`n" +
        "  movedOptions: { ItemID: number; Label: string }[] = [];`n" +
        "  ngOnInit() {`n" +
        "    this.moved = 'other';`n" +
        "    this.feeOptions = [{ ItemID: 1, Label: this.labels.plan.prepaid }, { ItemID: 2, Label: this.labels.plan.express }];`n" +
        "    this.movedOptions = [{ ItemID: 1, Label: ```${this.moved}.plan.prepaid`` }];`n" +
        "  }`n}`n"
    # A PIPE THAT RELABELS THE LIST FROM A DICTIONARY THE TEMPLATE NAMES (#117 follow-up).
    'apps/shop/src/labelled.pipe.ts' = "import { Pipe, PipeTransform } from '@angular/core';`nimport { DictionaryService } from './dictionary.service';`n" +
        "@Pipe({ name: 'labelled', standalone: true })`nexport class LabelledPipe implements PipeTransform {`n" +
        "  constructor(private dictionary: DictionaryService) {}`n" +
        "  transform(items: { label: string; value: unknown }[], path: string): { label: string; value: unknown }[] {`n" +
        "    if (items.length === 0) {`n      return [];`n    }`n" +
        "    const dict = this.dictionary.getTranslationDictionary(path);`n" +
        "    return items.map((item) => ({ ...item, label: dict[item.label] ?? '' }));`n  }`n}`n"
    'apps/shop/src/shade.component.ts' = "import { Component, Input } from '@angular/core';`nimport { Tone } from './tone.component';`nimport { toSelect } from './select-items';`n" +
        "import { LabelledPipe } from './labelled.pipe';`nimport { RadioListComponent } from './radio.component';`n" +
        "@Component({ selector: 'app-shade', standalone: true, imports: [RadioListComponent, LabelledPipe],`n" +
        "  template: '<radio-list [items]=`"shades.items | labelled : shades.path`" [(value)]=`"shade.a`"></radio-list>' +`n" +
        "    '<radio-list [items]=`"toned | labelled : \'kind\'`" [(value)]=`"shade.b`"></radio-list><radio-list [items]=`"toned | labelled : free`" [(value)]=`"shade.c`"></radio-list>' +`n" +
        "    '<radio-list [items]=`"view.hues.items | labelled : view.hues.path`" [(value)]=`"shade.d`"></radio-list>' })`n" +
        "export class ShadeComponent {`n" +
        "  @Input() free = '';`n" +
        "  shade = { a: '', b: '', c: '', d: '' };`n" +
        "  view = { hues: { items: toSelect(Tone), path: 'step' } };`n" +
        "  shades = { items: toSelect(Tone), path: 'rate' };`n" +
        "  toned = toSelect(Tone);`n" +
        "}`n"
    # A LIST THE PARENT TRANSLATES WITH A HELPER OVER ITS PARAMETER AND HANDS TO A CHILD'S `@Input` (#117 follow-up).
    'apps/shop/src/fee-options.ts' = "export const plainFees = [{ value: 'flat', label: 'Flat' }, { value: 'rate', label: 'Rate' }];`n"
    'apps/shop/src/fee-column.component.ts' = "import { Component, Input } from '@angular/core';`nimport { RadioListComponent } from './radio.component';`n" +
        "@Component({ selector: 'fee-column', standalone: true, imports: [RadioListComponent], template: '<radio-list [items]=`"fees`" [(value)]=`"chosen`"></radio-list>' })`n" +
        "export class FeeColumnComponent {`n  @Input() fees: { value: string; label: string }[] = [];`n  chosen = '';`n}`n"
    'apps/shop/src/fee-list.component.ts' = "import { Component, OnInit } from '@angular/core';`nimport { DictionaryService } from './dictionary.service';`n" +
        "import { plainFees } from './fee-options';`nimport { FeeColumnComponent } from './fee-column.component';`nimport { RadioListComponent } from './radio.component';`n" +
        "@Component({ selector: 'app-fee-list', standalone: true, imports: [FeeColumnComponent, RadioListComponent],`n" +
        "  template: '<fee-column [fees]=`"fees`"></fee-column><radio-list [items]=`"mapped`" [(value)]=`"pickMapped`"></radio-list>' })`n" +
        "export class FeeListComponent implements OnInit {`n" +
        "  fees: { value: string; label: string }[] = [];`n  mapped: { value: string; label: string }[] = [];`n  pickMapped = '';`n" +
        "  constructor(private dictionary: DictionaryService) {}`n" +
        "  ngOnInit() {`n    this.fees = this.translated(plainFees);`n" +
        "    this.mapped = plainFees.map((o) => ({ ...o, label: this.dictionary.getTranslation(``feeKind.`${o.value}``) }));`n  }`n" +
        "  private translated(options: { value: string; label: string }[]) {`n" +
        "    return options.map((option) => ({ ...option, label: this.dictionary.getTranslation(``fee.`${option.value}``) }));`n  }`n}`n"
    'apps/shop/src/assets/locales/en.json' = '{"hue":{"red":"Red","blue":"Blue"},"size":{"small":"S","large":"L"},"shown":{"only":"O"},"tier":{"gold":"G"},"grade":{"Gold":"G","Silver":"S"},"answer":{"yes":"Y","no":"N","Warm":"W","Cool":"C"},"spelled":{"out":"W"},"crossed":{"over":"C"},"rank":{"Gold":"G","Silver":"S"},"tone":{"Warm":"W","Cool":"C"},"kind":{"Warm":"W","Cool":"C"},"month":{"Jan":"J","Feb":"F"},"rate":{"Warm":"W","Cool":"C"},"step":{"none":"-","Warm":"W","Cool":"C"},"fee":{"plan":{"prepaid":"P","express":"E"},"flat":"F","rate":"R"},"feeKind":{"flat":"F","rate":"R"}}'
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
    # A TWO-WAY BINDING AS SOME ANGULAR COMPILES IT (#118): the output is `model.hue = $event`, a write of the place the input reads -
    # the same pair as `[(value)]`, matched by the place and not the text. THE CONTROL: an output storing `$event` in another place.
    $w = Invoke-Gate --map-query $made.Db --width 0 --limit 0 --sql "SELECT 'spelled=' || value || '|' || key AS s FROM control_options WHERE key IN ('spelled.out', 'crossed.over')"
    Assert-Line $w 'spelled=model.hue|spelled.out'
    Assert-NoLine $w 'crossed.over'
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

# AN ENUM HELPER UNDER A CALLER'S CHAIN (#117 follow-up): `toSelect(Tone, dict).filter(..)` keeps the helper's elements - a filter keeps a
# subset, so every key any of them holds; `.map((x) => ({ label: x.name }))` over a list is one element per element of it, `x.name` the
# element's `name` - here the dictionary's values a `for..in` helper copies (`enumToList(dict)`), every key under its prefix; a helper
# mapping another (`toRadio` over `toSelect`) and a template literal over the element (`kind.${x.Label}`). THE CONTROL: the same chain
# handed no dictionary offers nothing.
Test-Case 'tsrows: a control offers the keys of an enum helper a caller filters or maps (#117)' {
    $made = New-TsRowsDb (New-TsRowsWorkspace $script:KeyOptionsFiles) -Shared
    Assert-Exit $made.Result 0
    $r = Invoke-Gate --map-query $made.Db --width 0 --limit 0 --sql "SELECT 'tone=' || value || '|' || input || '.' || property || '|' || key AS t FROM control_options WHERE value LIKE 'pick.%'"
    Assert-Line $r 'tone=pick.tone|items.label|tone.Warm'
    Assert-Line $r 'tone=pick.tone|items.label|tone.Cool'
    Assert-Line $r 'tone=pick.month|items.label|month.Jan'
    Assert-Line $r 'tone=pick.month|items.label|month.Feb'
    Assert-Line $r 'tone=pick.kind|model.Label|kind.Warm'
    Assert-Line $r 'tone=pick.kind|model.Label|kind.Cool'
    Assert-NoLine $r 'pick.plain'
    # A METHOD RETURNING THE HELPER'S CALL is its list; `[..].concat(..)` the receiver's and the argument's; a property of a field object a
    # method writes with a call (`this.criteria = { answers: toRadio(..) }`) is that call's list.
    Assert-Line $r 'tone=pick.rate|items.label|rate.Warm'
    Assert-Line $r 'tone=pick.rate|items.label|rate.Cool'
    Assert-Line $r 'tone=pick.step|items.label|step.none'
    Assert-Line $r 'tone=pick.step|items.label|step.Warm'
    Assert-Line $r 'tone=pick.answer|model.Label|answer.Warm'
    Assert-Line $r 'tone=pick.answer|model.Label|answer.Cool'
    # A LABEL DECORATED AROUND THE ELEMENT'S OWN (`{ ...x, label: `${x.label} %` }`) shows the element's keys.
    Assert-Line $r 'tone=pick.rated|items.label|rate.Warm'
}

# A LABELS TABLE READ THROUGH `this` (#117 follow-up): `{ Label: this.labels.plan.prepaid }` over `labels = { plan: { prepaid: `${this.feeResource}.prepaid` } }`,
# `feeResource = `${this.rootKey}.plan`` and the base class's `rootKey = 'fee'` is `fee.plan.prepaid`. THE CONTROL: a field the class also
# writes later (`this.moved = 'other'`) is no constant, so its template spells nothing.
Test-Case 'tsrows: a control offers the keys a labels table spells through the class fields (#117)' {
    $made = New-TsRowsDb (New-TsRowsWorkspace $script:KeyOptionsFiles) -Shared
    Assert-Exit $made.Result 0
    $r = Invoke-Gate --map-query $made.Db --width 0 --limit 0 --sql "SELECT 'fee=' || value || '|' || input || '.' || property || '|' || key AS t FROM control_options WHERE value LIKE 'fee.%'"
    Assert-Line $r 'fee=fee.kind|model.Label|fee.plan.prepaid'
    Assert-Line $r 'fee=fee.kind|model.Label|fee.plan.express'
    Assert-NoLine $r 'fee.moved'
}

# A PIPE REBUILDING THE BOUND LIST (#117 follow-up): `items | labelled : path` over `transform(items, path)` returning `items.map((item) => ({ ...item,
# label: dict[item.label] ?? '' }))`, `dict` a `const` of `getTranslationDictionary(path)` - the template's `path` a literal or an object literal's
# property - labels every element with every key under it. THE CONTROL: a path the template reads from an input spells nothing.
Test-Case 'tsrows: a control offers the keys a pipe relabels its list with (#117)' {
    $made = New-TsRowsDb (New-TsRowsWorkspace $script:KeyOptionsFiles) -Shared
    Assert-Exit $made.Result 0
    $r = Invoke-Gate --map-query $made.Db --width 0 --limit 0 --sql "SELECT 'shade=' || value || '|' || input || '.' || property || '|' || key AS t FROM control_options WHERE value LIKE 'shade.%'"
    Assert-Line $r 'shade=shade.a|items.label|rate.Warm'
    Assert-Line $r 'shade=shade.a|items.label|rate.Cool'
    Assert-Line $r 'shade=shade.b|items.label|kind.Warm'
    Assert-Line $r 'shade=shade.b|items.label|kind.Cool'
    # A VIEW MODEL HOLDING BOTH the list and its path, two levels down: each read of the binding is the property it lands on.
    Assert-Line $r 'shade=shade.d|items.label|step.Warm'
    Assert-NoLine $r 'shade.c'
}

# A LIST A PARENT BUILDS WITH A HELPER MAPPING ITS PARAMETER, HANDED TO A CHILD'S `@Input` (#117 follow-up): `this.fees = this.translated(plainFees)`
# over `translated(options) { return options.map((option) => ({ ...option, label: t(`fee.${option.value}`) })); }` is each element of the handed list
# with its label replaced, and the child's control offers those keys through `[fees]`; a `{ ...o, label }` map in place is the same.
Test-Case 'tsrows: a control offers the keys of a list its parent translates and hands to its input (#117)' {
    $made = New-TsRowsDb (New-TsRowsWorkspace $script:KeyOptionsFiles) -Shared
    Assert-Exit $made.Result 0
    $r = Invoke-Gate --map-query $made.Db --width 0 --limit 0 --sql "SELECT 'fee=' || c.name || '|' || o.value || '|' || o.key AS t FROM control_options o JOIN components c ON c.id = o.component WHERE o.key LIKE 'fee.%' OR o.key LIKE 'feeKind.%'"
    Assert-Line $r 'fee=FeeColumnComponent|chosen|fee.flat'
    Assert-Line $r 'fee=FeeColumnComponent|chosen|fee.rate'
    Assert-Line $r 'fee=FeeListComponent|pickMapped|feeKind.flat'
    Assert-Line $r 'fee=FeeListComponent|pickMapped|feeKind.rate'
}

}
