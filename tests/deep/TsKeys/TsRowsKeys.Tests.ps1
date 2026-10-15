<#
    A key the code names through something OTHER than a literal at the use: a member of a const object
    (`KEYS.hint`), a `+` chain, and a locale value that is an ARRAY read whole by an i18n call. Each read as
    nothing before, so a field bound to the key reached no template, and a defined list read as missing.
#>

# In a folder of its own only because `tests/deep/` is at its file limit; the helpers stay beside the suites
# they were written for.
. (Join-Path $PSScriptRoot '..\TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

# One component naming its keys through const objects - its own file's and another file's - plus the
# negative control: a const LIST of rule objects, returned by a bound getter, holds a key without being it.
function New-TsRowsKeysConstTree {
    New-TsRowsWorkspace @{
        'apps/shop/src/keys.ts' = "export const SHARED = { hint: 'set.sharedhint' };`n"
        'apps/shop/src/settings.component.html' = "<b [title]=`"hint`">h</b><i [title]=`"sharedHint`">s</i>" +
            "<u [title]=`"tip`">t</u><s [title]=`"rules`">r</s>`n"
        'apps/shop/src/settings.component.ts' = "import { Component } from '@angular/core';`n" +
            "import { SHARED } from './keys';`n" +
            "export enum Kind { A = 1, B = 2 }`n" +
            "const KEYS = { hint: 'set.hint', guarded: 'set.guarded' };`n" +
            "const RULES = [{ Field: 'x', Message: 'set.v' }];`n" +
            "@Component({ selector: 'app-settings', templateUrl: './settings.component.html', standalone: true })`n" +
            "export class SettingsComponent {`n" +
            "  kind: Kind = Kind.A;`n" +
            "  hint = KEYS.hint;`n" +
            "  sharedHint = SHARED.hint;`n" +
            "  tip = '';`n" +
            "  pick(): void {`n" +
            "    if (this.kind === Kind.B) {`n      this.tip = KEYS.guarded;`n    }`n" +
            "  }`n" +
            "  get rules(): { Field: string; Message: string }[] {`n    return RULES;`n  }`n" +
            "}`n"
        'apps/shop/src/assets/locales/en.json' = '{"shop":{"title":"Shop","cart":{"empty":"Empty"}},' +
            '"set":{"hint":"a","sharedhint":"b","guarded":"c","v":"d"}}'
    }
}

Test-Case 'tsrows keys: a const object member bound through a field is a field binding' {
    $made = New-TsRowsDb (New-TsRowsKeysConstTree)
    $r = Invoke-TsRowsQ $made.Db "SELECT key || ' | ' || route AS reach FROM key_reach WHERE key = 'set.hint'"
    Assert-Line $r 'set.hint | field_binding'
}

# THE KEY SITS IN A FILE THAT DECLARES NO COMPONENT, so its literal reaches none: only the field it is
# read into, and the node binding that field, name the component it renders in.
Test-Case 'tsrows keys: a const object from another file reaches the component that binds its member' {
    $made = New-TsRowsDb (New-TsRowsKeysConstTree)
    $r = Invoke-TsRowsQ $made.Db ("SELECT 'hits=' || count(*) AS reach FROM key_reach k JOIN key_reach__components j ON j.owner = '#' || k.rowid AND j.parent = 0 " +
        "JOIN components c ON c.id = j.value WHERE k.key = 'set.sharedhint' AND c.name = 'SettingsComponent'")
    Assert-Line $r 'hits=1'
}

Test-Case 'tsrows keys: a const object member assigned under an if carries that branch' {
    $made = New-TsRowsDb (New-TsRowsKeysConstTree)
    $r = Invoke-TsRowsQ $made.Db ("SELECT key || ' | ' || (SELECT count(*) FROM key_reach__always_branches jn WHERE jn.owner = '#' || key_reach.rowid AND jn.parent = 0) || ' | ' || " +
        "$(Get-NodeItemListSql 'key_reach__always_values' 'key_reach.rowid' 0 'values') AS reach FROM key_reach WHERE key = 'set.guarded'")
    Assert-Line $r 'set.guarded | 1 | ["B"]'
}

# A LIST OF OBJECTS IS NOT A KEY. `RULES` holds `set.v` and the getter bound in the template returns it, but
# no value is the key - reaching into it would claim every string of every rule object.
Test-Case 'tsrows keys: a key inside a const list of objects stays a bare literal' {
    $made = New-TsRowsDb (New-TsRowsKeysConstTree)
    $r = Invoke-TsRowsQ $made.Db "SELECT key || ' | ' || route AS reach FROM key_reach WHERE key = 'set.v'"
    Assert-Line $r 'set.v | literal'
    Assert-NoLine $r 'field_binding'
}

# A locale file whose values include LISTS - of strings and of objects - read by a service method that returns
# the whole list, and a `+` chain whose tail is a parameter.
function New-TsRowsKeysLocaleTree {
    New-TsRowsWorkspace @{
        'apps/shop/src/regions.component.html' = "<b>{{ south }}</b><i>{{ plain }}</i>`n"
        'apps/shop/src/regions.component.ts' = "import { Component, Injectable } from '@angular/core';`n" +
            "@Injectable({ providedIn: 'root' })`n" +
            "export class Words {`n" +
            "  instant(key: string): string {`n    return key;`n  }`n" +
            "  dict(key: string): string[] {`n    return [key];`n  }`n" +
            "}`n" +
            "@Component({ selector: 'app-regions', templateUrl: './regions.component.html', standalone: true })`n" +
            "export class RegionsComponent {`n" +
            "  constructor(private words: Words) {}`n" +
            "  get south(): string[] {`n    return this.words.dict('regions.South');`n  }`n" +
            "  get plain(): string {`n    return this.words.instant('regions.' + 'Plain');`n  }`n" +
            "  label(kind: string): string {`n    return this.words.instant('labels.' + kind);`n  }`n" +
            "}`n"
        'apps/shop/src/assets/locales/en.json' = '{"shop":{"title":"Shop","cart":{"empty":"Empty"}},' +
            '"regions":{"North":["Alpha","Beta"],"South":["Gamma"],"West":[{"name":"Delta"}],' +
            '"Plain":"p"},"labels":{"laptop":"a"}}'
        'structuregate.ts.json' = '{"locales":["apps/shop/src/assets/locales"],"i18nCarriers":["money"],' +
            '"i18nCalls":["instant","dict"],"gateInputs":["disabled"],' +
            '"featureChecks":["FlagService.isOn"],"featureEnum":"FlagCodes"}'
    }
}

Test-Case 'tsrows keys: a locale array becomes one row per element, named by its index' {
    $made = New-TsRowsDb (New-TsRowsKeysLocaleTree)
    $r = Invoke-TsRowsQ $made.Db "SELECT key || ' | ' || text AS tr FROM translations WHERE key LIKE 'regions.%'"
    Assert-Line $r 'regions.North[0] | Alpha'
    Assert-Line $r 'regions.North[1] | Beta'
    Assert-Line $r 'regions.West[0].name | Delta'
}

# THE WHOLE LIST IS WHAT THE CALL READS, and the file defines it: no gap, and the locale that defines it.
Test-Case 'tsrows keys: an i18n call reading a whole locale array is a defined key' {
    $made = New-TsRowsDb (New-TsRowsKeysLocaleTree)
    $r = Invoke-TsRowsQ $made.Db ("SELECT key || ' | ' || coalesce(defined_in, 'none') AS ix FROM i18n_index " +
        "WHERE key IN ('regions.South', 'regions.Plain')")
    Assert-Line $r 'regions.South | ["en"]'
    Assert-Line $r 'regions.Plain | ["en"]'
    # NO KEY IN THIS TREE IS UNDEFINED, so the column is not there at all - a column no row sets is never
    # created. Asked of the schema, because `undefined_key` itself cannot be selected when it is absent.
    $gap = Invoke-TsRowsQ $made.Db "SELECT 'undefined_columns=' || count(*) AS c FROM pragma_table_info('i18n_index') WHERE name = 'undefined_key'"
    Assert-Line $gap 'undefined_columns=0'
}

Test-Case 'tsrows keys: a locale array element is referenced exactly when its array is' {
    $made = New-TsRowsDb (New-TsRowsKeysLocaleTree)
    $r = Invoke-TsRowsQ $made.Db ("SELECT key || ' | ' || referenced AS tr FROM translations " +
        "WHERE key IN ('regions.South[0]', 'regions.North[0]')")
    Assert-Line $r 'regions.South[0] | 1'
    Assert-Line $r 'regions.North[0] | 0'
}

# A `+` CHAIN WITH A HOLE is every key its pieces spell, as a template literal is: `'labels.' + kind` is no
# static key, so no i18n ref names `labels.laptop` - the chain is the only evidence it is used here.
Test-Case 'tsrows keys: a key built by a + chain reaches the component it is built in' {
    $made = New-TsRowsDb (New-TsRowsKeysLocaleTree)
    $r = Invoke-TsRowsQ $made.Db ("SELECT key || ' | ' || route || ' | ' || (SELECT count(*) FROM key_reach__components jn WHERE jn.owner = '#' || key_reach.rowid AND jn.parent = 0) AS reach " +
        "FROM key_reach WHERE key = 'labels.laptop'")
    Assert-Line $r 'labels.laptop | literal | 1'
}

# A FIELD HOLDING WHAT AN INJECTED METHOD RETURNS holds the keys that method's ONE plain return builds: `k =
# this.labels.key(this.grade)` is bound in the template, so the component renders `demo.grades.*`. A callee with
# a branch and a second return (`pick`) is not read - its branches test its parameter - and stays a literal.
# A LIST OF OBJECTS rendered by `*ngFor` (`tips`, held or returned by that call) is a site only where the loop
# variable's property is read (`t.tooltip`), carrying the loop's gate; `t.other` is never read, so it stays literal.
Test-Case 'tsrows keys: a field holding an injected method call, and a list of objects an ngFor reads, hold their keys' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/ids.ts' = "export enum Grade { Alpha = 1, Beta = 2 }`n"
        'apps/shop/src/label.service.ts' = "import { Injectable } from '@angular/core';`n" +
            "import { Grade } from './ids';`n" +
            "@Injectable({ providedIn: 'root' })`n" +
            "export class Words {`n  instant(key: string): string {`n    return key;`n  }`n}`n" +
            "@Injectable({ providedIn: 'root' })`n" +
            "export class LabelService {`n" +
            "  constructor(private words: Words) {}`n" +
            "  told(grade: Grade): string {`n    return this.words.instant(``demo.told.`${Grade[grade]}``);`n  }`n" +
            "  later() {`n    return [{ tooltip: 'demo.later.tip' }];`n  }`n" +
            "  key(grade: Grade): string {`n    return ``demo.grades.`${Grade[grade]}``;`n  }`n" +
            "  pick(grade: Grade): string {`n    if (grade === Grade.Alpha) {`n      return 'demo.pick.first';`n    }`n" +
            "    return 'demo.pick.other';`n  }`n" +
            "  tips() {`n    return [{ tooltip: 'demo.tips.alpha', other: 'demo.tips.unread' }, { tooltip: 'demo.tips.beta' }];`n  }`n" +
            "}`n"
        'apps/shop/src/badge.component.ts' = "import { Component, Input, OnInit } from '@angular/core';`n" +
            "import { Grade } from './ids';`n" +
            "import { LabelService } from './label.service';`n" +
            "@Component({ selector: 'app-badge', standalone: true,`n" +
            "  template: '<b [title]=`"k`">a</b><i [title]=`"p`">b</i>' +`n" +
            "    '<ng-container *ngFor=`"let t of tips`"><b [title]=`"t.tooltip`">x</b></ng-container>' +`n" +
            "    '<u *ngFor=`"let o of own`" [title]=`"o.label`">y</u><s [title]=`"told`">c</s>' +`n" +
            "    '<ng-container *ngFor=`"let l of later`"><b [title]=`"l.tooltip`">z</b></ng-container>' })`n" +
            "export class BadgeComponent implements OnInit {`n" +
            "  told = '';`n" +
            "  later: { tooltip: string }[];`n" +
            "  ngOnInit() {`n    this.told = this.labels.told(this.grade);`n    this.later = this.labels.later();`n  }`n" +
            "  @Input() grade: Grade = Grade.Alpha;`n" +
            "  k = this.labels.key(this.grade);`n" +
            "  p = this.labels.pick(this.grade);`n" +
            "  tips = this.labels.tips();`n" +
            "  own = [{ label: 'demo.own.first' }];`n" +
            "  constructor(private labels: LabelService) {}`n" +
            "}`n"
        'apps/shop/src/assets/locales/en.json' = '{"shop":{"title":"Shop","cart":{"empty":"Empty"}},' +
            '"demo":{"grades":{"Alpha":"a","Beta":"b"},"pick":{"first":"c","other":"d"},' +
            '"tips":{"alpha":"e","beta":"f","unread":"g"},"own":{"first":"h"},' +
            '"told":{"Alpha":"i","Beta":"j"},"later":{"tip":"k"}}}'
        'structuregate.ts.json' = '{"locales":["apps/shop/src/assets/locales"],"i18nCarriers":["money"],' +
            '"i18nCalls":["instant"],"gateInputs":["disabled"],' +
            '"featureChecks":["FlagService.isOn"],"featureEnum":"FlagCodes"}'
    }
    $made = New-TsRowsDb $tree
    $r = Invoke-TsRowsQ $made.Db ("SELECT key || ' | ' || route || ' | ' || (SELECT count(*) FROM key_reach__components jn WHERE jn.owner = '#' || key_reach.rowid AND jn.parent = 0) AS reach " +
        "FROM key_reach WHERE key LIKE 'demo.%'")
    Assert-Line $r 'demo.grades.Alpha | field_binding | 1'
    Assert-Line $r 'demo.grades.Beta | field_binding | 1'
    Assert-Line $r 'demo.pick.first | literal | 0'
    Assert-Line $r 'demo.pick.other | literal | 0'
    Assert-Line $r 'demo.tips.alpha | field_binding | 1'
    Assert-Line $r 'demo.tips.beta | field_binding | 1'
    Assert-Line $r 'demo.own.first | field_binding | 1'
    Assert-Line $r 'demo.tips.unread | literal | 0'
    # A TRANSLATE CALL THE METHOD RETURNS hands the field its key, and a list written once in `ngOnInit` is the
    # field's list.
    Assert-Line $r 'demo.told.Alpha | field_binding | 1'
    Assert-Line $r 'demo.told.Beta | field_binding | 1'
    Assert-Line $r 'demo.later.tip | field_binding | 1'
    $g = Invoke-TsRowsQ $made.Db ("SELECT key || ' | ' || (SELECT count(*) FROM key_reach__always_gates jn WHERE jn.owner = '#' || key_reach.rowid AND jn.parent = 0) AS g " +
        "FROM key_reach WHERE key = 'demo.tips.alpha'")
    Assert-Line $g 'demo.tips.alpha | 1'
}

# A SERVICE'S KEY REACHES THE COMPONENT HOLDING ITS CALL THROUGH WHAT THE CALLEE HANDS BACK: a method returning another
# method's call (`hop` -> `inner`), a translate call under `??` or `||`, and a list of objects returned from
# both arms of an `if` (`items`) each linked nothing, and the key read as a `literal` in no component. The
# service is in a file of its own: a key in the component's file links through that file and hides the gap.
Test-Case 'tsrows keys: a service key reaches its caller through a hop, a ?? or || fallback and branched lists' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/ids.ts' = "export enum Grade { Alpha = 1, Beta = 2 }`n"
        'apps/shop/src/desc.service.ts' = "import { Injectable } from '@angular/core';`n" +
            "import { Grade } from './ids';`n" +
            "@Injectable({ providedIn: 'root' })`n" +
            "export class Words {`n  instant(key: string): string {`n    return key;`n  }`n}`n" +
            "@Injectable({ providedIn: 'root' })`n" +
            "export class DescService {`n" +
            "  constructor(private words: Words) {}`n" +
            "  hop(grade: Grade): string {`n    return this.inner(grade);`n  }`n" +
            "  private inner(grade: Grade): string {`n    return ``demo.hop.`${Grade[grade]}``;`n  }`n" +
            "  wrapped(grade: Grade): string {`n    return this.words.instant(``demo.wrapped.`${Grade[grade]}``) ?? 'x';`n  }`n" +
            "  ored(grade: Grade): string {`n    return this.words.instant(``demo.ored.`${Grade[grade]}``) || 'x';`n  }`n" +
            "  items(flag: boolean) {`n    if (flag) {`n      return [{ label: 'demo.items.a' }];`n    }`n" +
            "    return [{ label: 'demo.items.b' }];`n  }`n" +
            "}`n"
        'apps/shop/src/host.component.ts' = "import { Component, Input, OnInit } from '@angular/core';`n" +
            "import { Grade } from './ids';`n" +
            "import { DescService } from './desc.service';`n" +
            "@Component({ selector: 'app-host', standalone: true,`n" +
            "  template: '<b [title]=`"b`">b</b><i [title]=`"d`">d</i><s [title]=`"e`">e</s>' +`n" +
            "    '<u *ngFor=`"let t of items`" [title]=`"t.label`">u</u>' })`n" +
            "export class HostComponent implements OnInit {`n" +
            "  @Input() grade: Grade = Grade.Alpha;`n" +
            "  b!: string;`n  d!: string;`n  e!: string;`n  items!: { label: string }[];`n" +
            "  constructor(private desc: DescService) {}`n" +
            "  ngOnInit() {`n    this.b = this.desc.hop(this.grade);`n    this.d = this.desc.wrapped(this.grade);`n" +
            "    this.e = this.desc.ored(this.grade);`n    this.items = this.desc.items(true);`n  }`n" +
            "}`n"
        'apps/shop/src/assets/locales/en.json' = '{"shop":{"title":"Shop","cart":{"empty":"Empty"}},' +
            '"demo":{"hop":{"Alpha":"a","Beta":"b"},"wrapped":{"Alpha":"c","Beta":"d"},' +
            '"ored":{"Alpha":"e","Beta":"f"},"items":{"a":"g","b":"h"}}}'
        'structuregate.ts.json' = '{"locales":["apps/shop/src/assets/locales"],"i18nCalls":["instant"]}'
    }
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    $r = Invoke-TsRowsQ $made.Db ("SELECT k.key || ' | ' || k.route || ' | ' || c.name AS reach FROM key_reach k " +
        "JOIN key_reach__components j ON j.owner = '#' || k.rowid AND j.parent = 0 JOIN components c ON c.id = j.value WHERE k.key LIKE 'demo.%'")
    Assert-Line $r 'demo.hop.Alpha | field_binding | HostComponent'
    Assert-Line $r 'demo.hop.Beta | field_binding | HostComponent'
    Assert-Line $r 'demo.wrapped.Alpha | field_binding | HostComponent'
    Assert-Line $r 'demo.wrapped.Beta | field_binding | HostComponent'
    Assert-Line $r 'demo.ored.Alpha | field_binding | HostComponent'
    Assert-Line $r 'demo.ored.Beta | field_binding | HostComponent'
    Assert-Line $r 'demo.items.a | field_binding | HostComponent'
    Assert-Line $r 'demo.items.b | field_binding | HostComponent'
}

# A SERVICE'S KEY REACHES THE CALLER THROUGH A HOP FROM ANY RETURN: a method whose call of another method sits under
# an `if` (`oneIf`), after an early return (`dflt`), in a `switch` (`sw`) or two hops down an arm (`both`) linked
# nothing, and the key read as a `literal` in no component. No condition is carried: the callee's branches test its
# own parameter, which no gate shares. A key a return of such a method spells itself (`demo.mine.none`, and `pick`
# above) is still not read: which of them renders is the branch's choice.
Test-Case 'tsrows keys: a service key reaches its caller through a hop from a method with several returns' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/ids.ts' = "export enum Grade { Alpha = 1, Beta = 2 }`n"
        'apps/shop/src/desc.service.ts' = "import { Injectable } from '@angular/core';`n" +
            "import { Grade } from './ids';`n" +
            "@Injectable({ providedIn: 'root' })`n" +
            "export class Words {`n  instant(key: string): string {`n    return key;`n  }`n}`n" +
            "@Injectable({ providedIn: 'root' })`n" +
            "export class DescService {`n" +
            "  constructor(private words: Words) {}`n" +
            "  oneIf(grade: Grade): string {`n    if (grade === Grade.Alpha) {`n      return this.p2(grade);`n    }`n    return 'demo.mine.none';`n  }`n" +
            "  private p2(grade: Grade): string {`n    return this.words.instant(``demo.oneif.`${Grade[grade]}``);`n  }`n" +
            "  dflt(grade: Grade): string {`n    if (grade === Grade.Alpha) {`n      return '-';`n    }`n    return this.p3(grade);`n  }`n" +
            "  private p3(grade: Grade): string {`n    return this.words.instant(``demo.dflt.`${Grade[grade]}``);`n  }`n" +
            "  sw(grade: Grade): string {`n    switch (grade) {`n      case Grade.Alpha:`n        return this.p4(grade);`n" +
            "      default:`n        return '-';`n    }`n  }`n" +
            "  private p4(grade: Grade): string {`n    return ``demo.sw.`${Grade[grade]}``;`n  }`n" +
            "  both(grade: Grade): string {`n    if (grade === Grade.Alpha) {`n      return this.deep(grade);`n    }`n" +
            "    return this.words.instant('demo.both.other');`n  }`n" +
            "  private deep(grade: Grade): string {`n    return this.p5(grade);`n  }`n" +
            "  private p5(grade: Grade): string {`n    return this.words.instant(``demo.both.`${Grade[grade]}``);`n  }`n" +
            "}`n"
        'apps/shop/src/host.component.ts' = "import { Component, Input, OnInit } from '@angular/core';`n" +
            "import { Grade } from './ids';`n" +
            "import { DescService } from './desc.service';`n" +
            "@Component({ selector: 'app-host', standalone: true,`n" +
            "  template: '<b [title]=`"b`">b</b><i [title]=`"c`">c</i><s [title]=`"d`">d</s><u [title]=`"e`">e</u>' })`n" +
            "export class HostComponent implements OnInit {`n" +
            "  @Input() grade: Grade = Grade.Alpha;`n" +
            "  b!: string;`n  c!: string;`n  d!: string;`n  e!: string;`n" +
            "  constructor(private desc: DescService) {}`n" +
            "  ngOnInit() {`n    this.b = this.desc.oneIf(this.grade);`n    this.c = this.desc.dflt(this.grade);`n" +
            "    this.d = this.desc.sw(this.grade);`n    this.e = this.desc.both(this.grade);`n  }`n" +
            "}`n"
        'apps/shop/src/assets/locales/en.json' = '{"shop":{"title":"Shop","cart":{"empty":"Empty"}},' +
            '"demo":{"oneif":{"Alpha":"a","Beta":"b"},"mine":{"none":"z"},"dflt":{"Alpha":"c","Beta":"d"},' +
            '"sw":{"Alpha":"e","Beta":"f"},"both":{"Alpha":"g","Beta":"h","other":"i"}}}'
        'structuregate.ts.json' = '{"locales":["apps/shop/src/assets/locales"],"i18nCalls":["instant"]}'
    }
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    $r = Invoke-TsRowsQ $made.Db ("SELECT k.key || ' | ' || k.route || ' | ' || c.name AS reach FROM key_reach k " +
        "JOIN key_reach__components j ON j.owner = '#' || k.rowid AND j.parent = 0 JOIN components c ON c.id = j.value WHERE k.key LIKE 'demo.%'")
    Assert-Line $r 'demo.oneif.Alpha | field_binding | HostComponent'
    Assert-Line $r 'demo.oneif.Beta | field_binding | HostComponent'
    Assert-Line $r 'demo.dflt.Alpha | field_binding | HostComponent'
    Assert-Line $r 'demo.dflt.Beta | field_binding | HostComponent'
    Assert-Line $r 'demo.sw.Alpha | field_binding | HostComponent'
    Assert-Line $r 'demo.sw.Beta | field_binding | HostComponent'
    Assert-Line $r 'demo.both.Alpha | field_binding | HostComponent'
    Assert-Line $r 'demo.both.other | field_binding | HostComponent'
    # the key `oneIf` spells in its own second return is the branch's choice, and stays unread
    Assert-NoLine $r 'demo.mine.none |'
}

# A SERVICE'S KEY REACHES THE CALLER THROUGH A CALLBACK AND A LOCAL ARROW: a translated value handed back by a callback
# another method calls (`viaLambda` -> `pick(() => first)`), and a local arrow wrapping the translate call called with
# literals (`localArrow` -> `t('alpha')`) linked nothing, and the key read as a `literal` in no component. A `let`
# reassigned under an `if` (`letVar`) already linked. No condition is carried, as for a hop (#39).
Test-Case 'tsrows keys: a service key reaches its caller through a callback and a local arrow' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/ids.ts' = "export enum Kind { Alpha = 1, Beta = 2 }`n"
        'apps/shop/src/translate.service.ts' = "import { Injectable } from '@angular/core';`n" +
            "@Injectable({ providedIn: 'root' })`n" +
            "export class TranslateService {`n  getTranslation(key: string): string {`n    return key;`n  }`n}`n" +
            "@Injectable({ providedIn: 'root' })`n" +
            "export class Picker {`n  run(get: () => string): string {`n    return get();`n  }`n}`n"
        'apps/shop/src/desc.service.ts' = "import { Injectable } from '@angular/core';`n" +
            "import { Kind } from './ids';`n" +
            "import { TranslateService, Picker } from './translate.service';`n" +
            "@Injectable({ providedIn: 'root' })`n" +
            "export class DescService {`n" +
            "  constructor(private translate: TranslateService, private picker: Picker) {}`n" +
            "  letVar(kind: Kind): string {`n    const name = Kind[kind];`n" +
            "    let first = this.translate.getTranslation(``demo.letvar.`${name}``);`n" +
            "    if (kind === Kind.Beta) { first = this.translate.getTranslation(``demo.letvar.`${name}.beta``); }`n" +
            "    return first;`n  }`n" +
            "  viaLambda(kind: Kind): string {`n" +
            "    const first = this.translate.getTranslation(``demo.lambda.`${Kind[kind]}``);`n" +
            "    return this.pick(() => first);`n  }`n" +
            "  private pick(get: () => string): string { return get(); }`n" +
            "  localArrow(kind: Kind): string {`n" +
            "    const t = (key: string) => this.translate.getTranslation(``demo.arrow.`${key}``);`n" +
            "    if (kind === Kind.Alpha) { return t('alpha'); }`n" +
            "    return t('beta');`n  }`n" +
            "  armArrow(kind: Kind, flag: boolean): string {`n" +
            "    const t = (key: string) => this.translate.getTranslation(``demo.arrow.`${key}``);`n" +
            "    return flag ? t('plus') : t('extra');`n  }`n" +
            "  otherPick(kind: Kind): string {`n" +
            "    return this.picker.run(() => this.translate.getTranslation(``demo.other.`${Kind[kind]}``));`n  }`n" +
            "  twoHoles(kind: Kind): string {`n" +
            "    const t = (group: string, key: string) => this.translate.getTranslation(``demo.two.`${group}.`${key}``);`n" +
            "    return t('main', 'title');`n  }`n" +
            "  unused(kind: Kind): string {`n" +
            "    const t = (key: string) => this.translate.getTranslation(``demo.unused.`${key}``);`n" +
            "    return '-';`n  }`n" +
            "}`n"
        'apps/shop/src/host.component.ts' = "import { Component, Input, OnInit } from '@angular/core';`n" +
            "import { Kind } from './ids';`n" +
            "import { DescService } from './desc.service';`n" +
            "@Component({ selector: 'demo-host', standalone: true, template: '{{ a }} {{ b }} {{ c }} {{ d }} {{ e }} {{ f }} {{ g }}' })`n" +
            "export class HostComponent implements OnInit {`n" +
            "  @Input() kind: Kind = Kind.Alpha;`n" +
            "  a!: string;`n  b!: string;`n  c!: string;`n  d!: string;`n  e!: string;`n  f!: string;`n  g!: string;`n" +
            "  constructor(private desc: DescService) {}`n" +
            "  ngOnInit() {`n    this.a = this.desc.letVar(this.kind);`n    this.b = this.desc.viaLambda(this.kind);`n" +
            "    this.c = this.desc.localArrow(this.kind);`n    this.d = this.desc.otherPick(this.kind);`n" +
            "    this.e = this.desc.twoHoles(this.kind);`n    this.f = this.desc.unused(this.kind);`n    this.g = this.desc.armArrow(this.kind, true);`n  }`n" +
            "}`n"
        'apps/shop/src/assets/locales/en.json' = '{"shop":{"title":"Shop","cart":{"empty":"Empty"}},' +
            '"demo":{"letvar":{"Alpha":"a","Beta":{"beta":"b"}},"lambda":{"Alpha":"c","Beta":"d"},' +
            '"arrow":{"alpha":"e","beta":"f","gamma":"g","plus":"p","extra":"x"},"other":{"Alpha":"h","Beta":"i"},' +
            '"two":{"main":{"title":"j","other":"k"}},"unused":{"x":"l"}}}'
        'structuregate.ts.json' = '{"locales":["apps/shop/src/assets/locales"],"i18nCalls":["getTranslation"]}'
    }
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    $r = Invoke-TsRowsQ $made.Db ("SELECT k.key || ' | ' || k.route || ' | ' || c.name AS reach FROM key_reach k " +
        "JOIN key_reach__components j ON j.owner = '#' || k.rowid AND j.parent = 0 JOIN components c ON c.id = j.value WHERE k.key LIKE 'demo.%'")
    Assert-Line $r 'demo.letvar.Alpha | field_binding | HostComponent'
    Assert-Line $r 'demo.lambda.Alpha | field_binding | HostComponent'
    Assert-Line $r 'demo.lambda.Beta | field_binding | HostComponent'
    Assert-Line $r 'demo.arrow.alpha | field_binding | HostComponent'
    Assert-Line $r 'demo.arrow.beta | field_binding | HostComponent'
    # a ternary's arms are followed like the sides of a `??`: both keys reach the caller
    Assert-Line $r 'demo.arrow.plus | field_binding | HostComponent'
    Assert-Line $r 'demo.arrow.extra | field_binding | HostComponent'
    Assert-Line $r 'demo.other.Alpha | field_binding | HostComponent'
    Assert-Line $r 'demo.two.main.title | field_binding | HostComponent'
    # spelled by no argument the arrow is called with, or held by an arrow nothing calls: not reached
    Assert-NoLine $r 'demo.arrow.gamma |'
    Assert-NoLine $r 'demo.two.main.other |'
    Assert-NoLine $r 'demo.unused.x |'
}
}

# A CONSTANT JOINED IN FRONT WITH `+` IS PART OF THE KEY A TEMPLATE SPELLS: `BASE + (child ? `.${name}.tipChild` :
# `.${name}.tip`)` recorded only `.` and `.tip`, so no reference row named the keys' prefix. The constant now leads
# the first piece, through parentheses and either arm of a conditional. A literal under no `+`, or after one whose
# left side is not a known string, keeps its own pieces.
Test-Case 'tsrows keys: a constant joined in front of a template literal leads its first piece' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/tips.ts' = "const BASE = 'demo.section';`n" +
            "export function applySettings(config: { tip: string }, name: string, child: boolean, lead: string) {`n" +
            "  config.tip = BASE + (child ? ``.`${name}.tipChild`` : ``.`${name}.tip``);`n" +
            "  config.tip = ``plain.`${name}.label``;`n" +
            "  config.tip = lead + ``.`${name}.loose``;`n" +
            "}`n"
    }
    $made = New-TsRowsDb $tree
    $r = Invoke-TsRowsQ $made.Db "SELECT $(Get-NodeRootListSql 'template_literals__parts' 't.id' ', ') AS parts FROM template_literals t JOIN files f ON f.id = t.file WHERE f.path LIKE '%tips.ts' ORDER BY t.line, t.col"
    Assert-Line $r '["demo.section.", ".tipChild"]'
    Assert-Line $r '["demo.section.", ".tip"]'
    Assert-Line $r '["plain.", ".label"]'
    Assert-Line $r '[".", ".loose"]'
}
