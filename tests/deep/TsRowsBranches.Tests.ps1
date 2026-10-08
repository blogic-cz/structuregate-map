<#
    A key ASSIGNED IN TYPESCRIPT carries the `if`/`else` around its write into `key_reach`. The field binds
    at one template node whose own gates say nothing about which key is in it, so without the branch every
    key the field can hold read as shown for every value of the dimension the branch tests.
#>

. (Join-Path $PSScriptRoot 'TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

# One field, four keys, each written under a different condition. The `else if` chain matters: `tips.big`
# sits in the ELSE of the `Small` test, which is `not_in Small`, and in the call to a function whose one return
# tests its argument against a list - so it is `in Big` and nothing else. The function is an ARROW CONST, whose
# `consts` row and `functions` row share one anchor. `tips.both` is also written with
# no branch at all, which is a way in with no condition, so nothing is necessary for it.
function New-TsBranchTree {
    New-TsRowsWorkspace @{
        'apps/shop/src/tips.component.html' = "<b [title]=`"tip`">tip</b>`n"
        'apps/shop/src/tips.component.ts' = "import { Component } from '@angular/core';`n" +
            "export enum TierIDs { Small = 1, Big = 2, Mid = 3, Low = 4, Top = 5 }`n" +
            "const FEATURED_TIERS = [TierIDs.Mid, TierIDs.Low];`n" +
            "export const isBigTier = (id: TierIDs): boolean => {`n" +
            "  return [TierIDs.Big].some((x) => x === id);`n" +
            "};`n" +
            "@Component({ selector: 'app-tips', templateUrl: './tips.component.html', standalone: true })`n" +
            "export class TipsComponent {`n" +
            "  tierID: TierIDs = TierIDs.Small;`n" +
            "  tip = '';`n" +
            "  pick(): void {`n" +
            "    if (this.tierID === TierIDs.Small) {`n      this.tip = 'tips.small';`n" +
            "    } else if (isBigTier(this.tierID)) {`n      this.tip = 'tips.big';`n" +
            "    } else if (FEATURED_TIERS.includes(this.tierID)) {`n      this.tip = 'tips.element';`n" +
            "    }`n" +
            "  }`n" +
            "  both(): void {`n" +
            "    if (this.tierID === TierIDs.Top) {`n      this.tip = 'tips.both';`n    }`n" +
            "  }`n" +
            "  reset(): void {`n    this.tip = 'tips.both';`n  }`n" +
            "}`n"
        'apps/shop/src/assets/locales/en.json' = '{"shop":{"title":"Shop","cart":{"empty":"Empty"}},' +
            '"tips":{"small":"a","big":"b","element":"c","both":"d"}}'
    }
}

Test-Case 'tsrows: a key assigned under an if carries the value its condition permits' {
    $made = New-TsRowsDb (New-TsBranchTree)
    $r = Invoke-TsRowsQ $made.Db ("SELECT key || ' | ' || route || ' | ' || " +
        "json_extract(always_values, '$[0].dimension') || ' ' || json_extract(always_values, '$[0].op') || ' ' || " +
        "json_extract(always_values, '$[0].values') AS reach FROM key_reach WHERE key LIKE 'tips.%'")
    Assert-Line $r 'tips.small | field_binding | tierID in ["Small"]'
    Assert-Line $r 'tips.big | field_binding | tierID in ["Big"]'
    Assert-Line $r 'tips.element | field_binding | tierID in ["Low","Mid"]'
}

# THE BRANCH IS PUBLISHED APART FROM THE GATES: `always_gates` names `gates` rows and nothing else, and a key
# written once with no condition has no branch that always holds.
Test-Case 'tsrows: a key also written with no condition needs no branch, and branches stay out of the gates' {
    $made = New-TsRowsDb (New-TsBranchTree)
    $r = Invoke-TsRowsQ $made.Db ("SELECT key || ' | ' || json_array_length(always_branches) || ' | ' || " +
        "json_array_length(maybe_branches) || ' | ' || always_gates || ' | ' || always_values AS reach " +
        "FROM key_reach WHERE key IN ('tips.small', 'tips.both')")
    Assert-Line $r 'tips.small | 1 | 1 | [] | [{'
    Assert-Line $r 'tips.both | 0 | 1 | [] | []'
}

# A `case` IS A CONDITION TOO. A group of two labels is one `in` of both - not two that intersect to nothing -
# a `default` is every value its siblings name NOT, and a switch inside an `if` joins both conditions.
Test-Case 'tsrows: a key assigned in a switch case carries the value its case permits' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/kinds.component.html' = "<b [title]=`"tip`">tip</b>`n"
        'apps/shop/src/kinds.component.ts' = "import { Component } from '@angular/core';`n" +
            "export enum Kind { Red = 1, Green = 2, Blue = 3, Black = 4, White = 5 }`n" +
            "@Component({ selector: 'app-kinds', templateUrl: './kinds.component.html', standalone: true })`n" +
            "export class KindsComponent {`n" +
            "  kind: Kind = Kind.Red;`n" +
            "  open = false;`n" +
            "  tip = '';`n" +
            "  pick(): void {`n" +
            "    switch (this.kind) {`n" +
            "      case Kind.Red:`n      case Kind.Green:`n        this.tip = 'kinds.warm';`n        break;`n" +
            "      case Kind.Blue:`n        if (this.open) {`n          this.tip = 'kinds.blue';`n        }`n        break;`n" +
            "      default:`n        this.tip = 'kinds.other';`n" +
            "    }`n" +
            "  }`n" +
            "}`n"
        'apps/shop/src/assets/locales/en.json' = '{"shop":{"title":"Shop","cart":{"empty":"Empty"}},' +
            '"kinds":{"warm":"a","blue":"b","other":"c"}}'
    }
    $made = New-TsRowsDb $tree
    $r = Invoke-TsRowsQ $made.Db ("SELECT key || ' | ' || json_array_length(always_branches) || ' | ' || " +
        "json_extract(always_values, '$[0].dimension') || ' ' || json_extract(always_values, '$[0].op') || ' ' || " +
        "json_extract(always_values, '$[0].values') AS reach FROM key_reach WHERE key LIKE 'kinds.%'")
    Assert-Line $r 'kinds.warm | 1 | kind in ["Green","Red"]'
    Assert-Line $r 'kinds.blue | 2 | kind in ["Blue"]'
    Assert-Line $r 'kinds.other | 1 | kind in ["Black","White"]'
}

# A KEY CHOSEN BY A TERNARY carries the condition in each arm's polarity, and a GETTER's returns are its
# writes: `mode` binds at a node, its `if` return is `in Sun`, and the `else` arm of the ternary is `not_in`.
Test-Case 'tsrows: a key chosen by a ternary or returned by a getter carries its condition' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/days.component.html' = "<b [title]=`"tip`">tip</b><i [title]=`"mode`">mode</i>`n"
        'apps/shop/src/days.component.ts' = "import { Component } from '@angular/core';`n" +
            "export enum Day { Sun = 1, Mon = 2, Tue = 3, Wed = 4 }`n" +
            "@Component({ selector: 'app-days', templateUrl: './days.component.html', standalone: true })`n" +
            "export class DaysComponent {`n" +
            "  day: Day = Day.Sun;`n" +
            "  tip = '';`n" +
            "  pick(): void {`n" +
            "    this.tip = this.day === Day.Mon ? 'days.mon' : 'days.other';`n" +
            "  }`n" +
            "  get mode(): string {`n" +
            "    if (this.day === Day.Sun) {`n      return 'days.sun';`n    }`n" +
            "    return 'days.plain';`n" +
            "  }`n" +
            "}`n"
        'apps/shop/src/assets/locales/en.json' = '{"shop":{"title":"Shop","cart":{"empty":"Empty"}},' +
            '"days":{"mon":"a","other":"b","sun":"c","plain":"d"}}'
    }
    $made = New-TsRowsDb $tree
    $r = Invoke-TsRowsQ $made.Db ("SELECT key || ' | ' || route || ' | ' || " +
        "json_extract(always_values, '$[0].dimension') || ' ' || json_extract(always_values, '$[0].op') || ' ' || " +
        "json_extract(always_values, '$[0].values') AS reach FROM key_reach WHERE key LIKE 'days.%'")
    Assert-Line $r 'days.mon | field_binding | day in ["Mon"]'
    Assert-Line $r 'days.other | field_binding | day not_in ["Mon"]'
    Assert-Line $r 'days.sun | field_binding | day in ["Sun"]'
    # The early return does not make the fallback conditional on anything the map reads: it gets no value.
    $plain = Invoke-TsRowsQ $made.Db "SELECT key || ' | ' || always_values AS reach FROM key_reach WHERE key = 'days.plain'"
    Assert-Line $plain 'days.plain | []'
}

# A KEY HANDED TO A TRANSLATION CALL runs only when the call does: a getter returning `instant('k')` inside an
# `if` produces `k` under that `if`, and a key chosen by a ternary in the argument takes its arm's condition.
Test-Case 'tsrows: a key passed to a translation call carries the branch the call sits in' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/labels.component.html' = "<b>{{ label }}</b>`n"
        'apps/shop/src/labels.component.ts' = "import { Component, Injectable } from '@angular/core';`n" +
            "export enum Plan { Solo = 1, Duo = 2, Team = 3, Crew = 4 }`n" +
            "@Injectable({ providedIn: 'root' })`n" +
            "export class Words {`n  instant(key: string): string {`n    return key;`n  }`n}`n" +
            "@Component({ selector: 'app-labels', templateUrl: './labels.component.html', standalone: true })`n" +
            "export class LabelsComponent {`n" +
            "  plan: Plan = Plan.Solo;`n" +
            "  constructor(private words: Words) {}`n" +
            "  get label(): string {`n" +
            "    if (this.plan === Plan.Duo) {`n      return this.words.instant('labels.duo');`n    }`n" +
            "    return this.words.instant(this.plan === Plan.Team ? 'labels.team' : 'labels.rest');`n" +
            "  }`n" +
            "}`n"
        'apps/shop/src/assets/locales/en.json' = '{"shop":{"title":"Shop","cart":{"empty":"Empty"}},' +
            '"labels":{"duo":"a","team":"b","rest":"c"}}'
        'structuregate.ts.json' = '{"locales":["apps/shop/src/assets/locales"],"i18nCarriers":["money"],' +
            '"i18nCalls":["instant"],"gateInputs":["disabled"],' +
            '"featureChecks":["FlagService.isOn"],"featureEnum":"FlagCodes"}'
    }
    $made = New-TsRowsDb $tree
    $r = Invoke-TsRowsQ $made.Db ("SELECT key || ' | ' || route || ' | ' || " +
        "json_extract(always_values, '$[0].dimension') || ' ' || json_extract(always_values, '$[0].op') || ' ' || " +
        "json_extract(always_values, '$[0].values') AS reach FROM key_reach WHERE key LIKE 'labels.%'")
    Assert-Line $r 'labels.duo | ts_call | plan in ["Duo"]'
    Assert-Line $r 'labels.team | ts_call | plan in ["Team"]'
    Assert-Line $r 'labels.rest | ts_call | plan not_in ["Team"]'
}

# A BARE BOOLEAN PROPERTY IS READ AS WHAT IT IS ASSIGNED. Written once, it proves its expression when true and
# nothing when false - it may never have been written - so `firms.notelm` gets no value. A getter with one
# return is its expression both ways, so `!isOak` is `not_in Oak`. A template `*ngIf` on the property reads
# the same way.
Test-Case 'tsrows: a boolean property is read as the comparison it is assigned' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/firms.component.html' = "<b [title]=`"tip`">tip</b><i *ngIf=`"isElm`">{{ 'firms.gate' | money }}</i>`n"
        'apps/shop/src/firms.component.ts' = "import { Component } from '@angular/core';`n" +
            "export enum Firm { Elm = 1, Aspen = 2, Oak = 3, Maple = 4 }`n" +
            "@Component({ selector: 'app-firms', templateUrl: './firms.component.html', standalone: true })`n" +
            "export class FirmsComponent {`n" +
            "  firm: Firm = Firm.Aspen;`n" +
            "  isElm = false;`n" +
            "  tip = '';`n" +
            "  get isOak(): boolean {`n    return this.firm === Firm.Oak;`n  }`n" +
            "  load(): void {`n    this.isElm = this.firm === Firm.Elm;`n  }`n" +
            "  pick(): void {`n" +
            "    if (this.isElm) {`n      this.tip = 'firms.elm';`n    }`n" +
            "    if (!this.isElm) {`n      this.tip = 'firms.notelm';`n    }`n" +
            "    if (!this.isOak) {`n      this.tip = 'firms.notoak';`n    }`n" +
            "  }`n" +
            "}`n"
        'apps/shop/src/assets/locales/en.json' = '{"shop":{"title":"Shop","cart":{"empty":"Empty"}},' +
            '"firms":{"elm":"a","notelm":"b","notoak":"c","gate":"d"}}'
    }
    $made = New-TsRowsDb $tree
    $r = Invoke-TsRowsQ $made.Db ("SELECT key || ' | ' || " +
        "json_extract(always_values, '$[0].dimension') || ' ' || json_extract(always_values, '$[0].op') || ' ' || " +
        "json_extract(always_values, '$[0].values') AS reach FROM key_reach WHERE key LIKE 'firms.%'")
    Assert-Line $r 'firms.elm | firm in ["Elm"]'
    Assert-Line $r 'firms.notoak | firm not_in ["Oak"]'
    Assert-Line $r 'firms.gate | firm in ["Elm"]'
    $neg = Invoke-TsRowsQ $made.Db "SELECT key || ' | ' || always_values AS reach FROM key_reach WHERE key = 'firms.notelm'"
    Assert-Line $neg 'firms.notelm | []'
}

# A KEY A PIPE PRODUCES renders wherever a template applies the pipe: that node is its site, under the node's
# gates and the branch the key is returned under inside `transform`. The pipe's own file declares no
# component, so without this the key had no site at all. `branch_senses` spells out each branch's side.
Test-Case 'tsrows: a key returned by a pipe renders where the pipe is used, under its branch' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/kind.pipe.ts' = "import { Pipe, PipeTransform } from '@angular/core';`n" +
            "export enum Kind { Red = 1, Green = 2, Blue = 3 }`n" +
            "@Pipe({ name: 'kindLabel', standalone: true })`n" +
            "export class KindLabelPipe implements PipeTransform {`n" +
            "  transform(kind: Kind): string {`n" +
            "    if (kind === Kind.Red) {`n      return 'pipes.red';`n    }`n" +
            "    return 'pipes.other';`n" +
            "  }`n" +
            "}`n"
        'apps/shop/src/palette.component.html' = "<p *ngIf=`"open`">{{ kind | kindLabel }}</p>`n"
        'apps/shop/src/palette.component.ts' = "import { Component } from '@angular/core';`n" +
            "import { Kind, KindLabelPipe } from './kind.pipe';`n" +
            "@Component({ selector: 'app-palette', templateUrl: './palette.component.html', standalone: true, imports: [KindLabelPipe] })`n" +
            "export class PaletteComponent {`n  open = true;`n  kind: Kind = Kind.Red;`n}`n"
        'apps/shop/src/assets/locales/en.json' = '{"shop":{"title":"Shop","cart":{"empty":"Empty"}},' +
            '"pipes":{"red":"a","other":"b"}}'
    }
    $made = New-TsRowsDb $tree
    $r = Invoke-TsRowsQ $made.Db ("SELECT key || ' | ' || route || ' | ' || json_array_length(components) || ' | ' || " +
        "json_array_length(always_gates) || ' | ' || json_extract(always_values, '$[0].op') || ' ' || " +
        "json_extract(always_values, '$[0].values') AS reach FROM key_reach WHERE key = 'pipes.red'")
    Assert-Line $r 'pipes.red | pipe | 1 | 1 | in ["Red"]'
    $s = Invoke-TsRowsQ $made.Db "SELECT branch_senses AS s FROM key_reach WHERE key = 'pipes.red'"
    Assert-Line $s '": "then"'
}

# A KEY A TEMPLATE LITERAL BUILDS is every locale key its pieces spell, under the branch the template sits
# in - here `oak.some(hasTier)`, where both names are CONST LOCALS: the list is read as its members and the
# arrow is inlined, so the key needs the Oak-like tiers. A hole written `Firm[this.firm]` is the member's
# NAME, so `label.Oak.value` needs `firm in [Oak]` although no branch says so.
Test-Case 'tsrows: a key built by a template literal renders under its branch, and an enum-named hole restricts' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/bundle.component.html' = "<b>{{ label }}</b><i [title]=`"caption`">caption</i>`n"
        'apps/shop/src/bundle.component.ts' = "import { Component, Injectable } from '@angular/core';`n" +
            "export enum Tier { OakSmall = 1, OakLarge = 2, AspenBasic = 3, CedarTop = 4 }`n" +
            "export enum Firm { Oak = 1, Aspen = 2, Cedar = 3 }`n" +
            "@Injectable({ providedIn: 'root' })`n" +
            "export class Words {`n  instant(key: string): string {`n    return key;`n  }`n}`n" +
            "@Component({ selector: 'app-bundle', templateUrl: './bundle.component.html', standalone: true })`n" +
            "export class BundleComponent {`n" +
            "  tierID: Tier = Tier.OakSmall;`n" +
            "  firm: Firm = Firm.Aspen;`n" +
            "  caption = '';`n" +
            "  constructor(private words: Words) {}`n" +
            "  get label(): string {`n" +
            "    const oak = [Tier.OakSmall, Tier.OakLarge];`n" +
            "    const hasTier = (e: Tier) => e === this.tierID;`n" +
            "    if (oak.some(hasTier)) {`n" +
            "      const name = this.baseName();`n" +
            "      return this.words.instant(``plans.bundle.`${name} Pack``);`n" +
            "    }`n" +
            "    return '';`n" +
            "  }`n" +
            "  baseName(): string {`n    return 'Small';`n  }`n" +
            "  pick(): void {`n    this.caption = ``label.`${Firm[this.firm]}.value``;`n  }`n" +
            "}`n"
        'apps/shop/src/assets/locales/en.json' = '{"shop":{"title":"Shop","cart":{"empty":"Empty"}},' +
            '"plans":{"bundle":{"Small Pack":"a","Large Pack":"b"}},"label":{"Oak":{"value":"c"},"Aspen":{"value":"d"}}}'
        'structuregate.ts.json' = '{"locales":["apps/shop/src/assets/locales"],"i18nCarriers":["money"],' +
            '"i18nCalls":["instant"],"gateInputs":["disabled"],' +
            '"featureChecks":["FlagService.isOn"],"featureEnum":"FlagCodes"}'
    }
    $made = New-TsRowsDb $tree
    $r = Invoke-TsRowsQ $made.Db ("SELECT key || ' | ' || json_array_length(components) || ' | ' || " +
        "json_extract(always_values, '$[0].dimension') || ' ' || json_extract(always_values, '$[0].op') || ' ' || " +
        "json_extract(always_values, '$[0].values') AS reach FROM key_reach WHERE key LIKE 'plans.bundle.%' OR key LIKE 'label.%'")
    Assert-Line $r 'plans.bundle.Small Pack | 1 | tierID in ["OakLarge","OakSmall"]'
    Assert-Line $r 'plans.bundle.Large Pack | 1 | tierID in ["OakLarge","OakSmall"]'
    Assert-Line $r 'label.Oak.value | 1 | firm in ["Oak"]'
    Assert-Line $r 'label.Aspen.value | 1 | firm in ["Aspen"]'
}

# THE C# HALF WRITES INTO THE SAME `enums` AND `switch_cases`, and each half drops only its own:
# C# by `file`, TypeScript by `half`. A re-read of either side that deleted the other's rows would leave a map
# whose backend and frontend never join - and both sides' counts would still look plausible on their own.
Test-Case 'tsrows: C# and TypeScript rows share enums and switch_cases, and a re-read of either keeps the other' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/level.ts' = "export enum Level { Low, High }`n" +
            "export function pick(l: Level): number {`n  switch (l) {`n    case Level.Low: return 1;`n    default: return 2;`n  }`n}`n"
        'backend/Ids.cs' = "namespace Demo;`npublic enum Kinds { Base, Extra = 40 }`n" +
            "public static class Scores { public static int Of(Kinds id) { switch (id) { case Kinds.Extra: return 1; default: return 0; } } }`n"
    }
    $count = {
        param([string]$Sql)
        $r = Invoke-TsRowsQ (Join-Path $tree 'map.sqlite') $Sql
        return (@($r.Lines | Where-Object { $_ -match '^\s*\d+\s*$' })[0]).Trim()
    }
    # BY THE FILE EACH ROW HANGS OFF: the workspace holds enums and switches of its own elsewhere.
    $rows = { param([string]$Table, [string]$Path)
        & $count "SELECT count(*) FROM $Table t JOIN files f ON f.id = t.owner_file WHERE f.path = '$Path'" }
    $both = {
        "cs enums $(& $rows enums 'backend/Ids.cs') ts enums $(& $rows enums 'apps/shop/src/level.ts')" +
        " cs cases $(& $rows switch_cases 'backend/Ids.cs') ts cases $(& $rows switch_cases 'apps/shop/src/level.ts')"
    }
    $expected = 'cs enums 1 ts enums 1 cs cases 2 ts cases 2'
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    Assert-Equal (& $both) $expected 'after the first run'
    [System.IO.File]::AppendAllText((Join-Path $tree 'backend/Ids.cs'), "// edited`n")
    Assert-Exit (New-TsRowsDb $tree).Result 0
    Assert-Equal (& $both) $expected 'after the C# file was re-read'
    [System.IO.File]::AppendAllText((Join-Path $tree 'apps/shop/src/level.ts'), "// edited`n")
    Assert-Exit (New-TsRowsDb $tree).Result 0
    Assert-Equal (& $both) $expected 'after the TypeScript file was re-read'
    # A FILE THE DATABASE HAS NEVER SEEN IS A WHOLE REWRITE of the TypeScript rows (the edits above were partial,
    # and a partial run deletes only by its own files' ids) - the delete that must still spare C#'s rows.
    [System.IO.File]::WriteAllText((Join-Path $tree 'apps/shop/src/added.ts'), "export const added = 1;`n")
    $whole = New-TsRowsDb $tree
    Assert-Exit $whole.Result 0
    Assert-NoLine $whole.Result 'is replacing 1 file(s)'
    Assert-Equal (& $both) $expected 'after the whole workspace was re-read'
}

}
