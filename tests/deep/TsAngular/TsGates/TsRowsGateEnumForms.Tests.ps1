<#
    Three ways a comparison against an enum was left `unknown` though its meaning is plain. Read in
    `rust/fbtcore/src/rows/ts/gate/gate_values.rs` (`restriction_of`), `gate/ts/gate_same_enum.rs` and
    `gate/ts/gate_tsrows.rs` (`walk`):
      * the model's enum and the component's are TWO declarations with the same name, members and values -
        one enum for a comparison; a twin with a different member or value stays `unknown`;
      * a getter reads an enum through a field holding the enum OBJECT (`this.shapeRef.Round`), as a template does;
      * a NUMBER compared with an enum-typed value is the member it equals (none: `in []`); `>` stays `unknown`;
      * a private method of no parameter `if (C) { return E; } return false;` CALLED where a gate reads it is `C && E`,
        and a ternary in truth position is `(c && a) || (!c && b)` (`gate/ts/gate_props.rs`).
    Its helpers are `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '../../TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

$script:TsEnumFormsSql = "SELECT g.source || ' => ' || v.enum_name || ' ' || v.dimension || ' ' || v.op || ' ' || " +
    "v.values_json AS restriction FROM gate_values v JOIN gates g ON g.id = v.gate"

Test-Case 'tsrows: a twin enum, a getter alias field and a number each read as the member they name' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/a/tier.enum.ts' = "export enum Tier {`n  Basic = 1,`n  Plus = 2,`n}`n"
        'apps/shop/src/b/tier.enum.ts' = "export enum Tier {`n  Basic = 1,`n  Plus = 2,`n}`n"
        'apps/shop/src/a/level.enum.ts' = "export enum Level {`n  Low = 1,`n  High = 2,`n}`n"
        'apps/shop/src/b/level.enum.ts' = "export enum Level {`n  Low = 1,`n  High = 3,`n}`n"
        'apps/shop/src/a/member.ts' = "import { Tier } from './tier.enum';`nimport { Level } from './level.enum';`n" +
            "export class Member { Tier: Tier; Level: Level; }`n"
        'apps/shop/src/b/guest.ts' = "import { Member } from '../a/member';`nexport class Guest extends Member { Name: string; }`n"
        'apps/shop/src/shapes.ts' = "export enum Shape {`n  Round = 1,`n  Square = 2,`n  Flat = 3,`n}`n" +
            "export enum Sev {`n  Low = 0,`n  Mid = 1,`n  High = 2,`n  Max = 3,`n}`n" +
            "export class Card { Shape: Shape; }`nexport class Note { Sev: Sev; }`n"
        'apps/shop/src/host.component.ts' = "import { Component, Input } from '@angular/core';`n" +
            "import { Guest } from './b/guest';`nimport { Tier } from './b/tier.enum';`nimport { Level } from './b/level.enum';`n" +
            "import { Card, Shape, Note } from './shapes';`n" +
            "@Component({ selector: 'demo-host', template: ``n" +
            "  <i *ngIf=`"guest.Tier === Tier.Plus`">a</i>`n" +
            "  <i *ngIf=`"guest.Level === Level.High`">b</i>`n" +
            "  <i *ngIf=`"isRound`">c</i>`n" +
            "  <i *ngIf=`"note.Sev === 0`">d</i>`n" +
            "  <i *ngIf=`"note.Sev === 1 || note.Sev === 2`">e</i>`n" +
            "  <i *ngIf=`"note.Sev === 9`">f</i>`n" +
            "  <i *ngIf=`"note.Sev > 0`">g</i>`n" +
            "  <i *ngIf=`"note.Sev !== 0`">h</i>`` })`n" +
            "export class HostComponent {`n" +
            "  @Input() guest: Guest;`n  @Input() card: Card;`n  @Input() note: Note;`n" +
            "  Tier = Tier;`n  Level = Level;`n  shapeRef = Shape;`n" +
            "  get isRound(): boolean { return this.card.Shape === this.shapeRef.Round; }`n}`n"
    }
    $made = New-TsRowsDb $tree
    $r = Invoke-Gate --map-query $made.Db --width 0 --sql $script:TsEnumFormsSql
    Assert-Exit $r 0
    Assert-Line $r 'guest.Tier === Tier.Plus => Tier guest.Tier in ["Plus"]'
    Assert-Line $r 'guest.Level === Level.High => Level guest.Level unknown []'
    Assert-Line $r 'isRound => Shape card.Shape in ["Round"]'
    Assert-Line $r 'note.Sev === 0 => Sev note.Sev in ["Low"]'
    Assert-Line $r 'note.Sev === 1 || note.Sev === 2 => Sev note.Sev in ["High","Mid"]'
    Assert-Line $r 'note.Sev === 9 => Sev note.Sev in []'
    Assert-Line $r 'note.Sev > 0 => Sev note.Sev unknown []'
    Assert-Line $r 'note.Sev !== 0 => Sev note.Sev not_in ["Low"]'
}

# `[hidden]` is a built-in gate: it HIDES while its condition holds, so the key under it is reached on the
# members the condition EXCLUDES. `hidden` stays in `gateInputs` on purpose - the built-in wins over the config.
Test-Case 'tsrows: a [hidden] binding gates its subtree on the negation of its condition' {
    $tpl = '<span [hidden]="kind === K.Alpha">{{ ''demo.hid.own'' | translate }}</span>' +
        '<div [hidden]="kind === K.Beta"><b>{{ ''demo.hid.child'' | translate }}</b></div>' +
        '<i *ngIf="kind === K.Gamma">{{ ''demo.hid.if'' | translate }}</i>'
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/kinds.ts' = "export enum DemoIDs { Alpha = 1, Beta = 2, Gamma = 3 }`n"
        'apps/shop/src/hid.component.ts' = "import { Component, Input } from '@angular/core';`n" +
            "import { DemoIDs } from './kinds';`n" +
            "@Component({ selector: 'demo-hid', standalone: true, template: '" + $tpl.Replace("'", "\'") + "' })`n" +
            "export class HidComponent { K = DemoIDs; @Input() kind!: DemoIDs; }`n"
        'apps/shop/src/assets/locales/en.json' = '{"demo":{"hid":{"own":"1","child":"2","if":"3"}}}'
        'structuregate.ts.json' = '{"locales":["apps/shop/src/assets/locales"],"gateInputs":["hidden"]}'
    }
    $made = New-TsRowsDb $tree
    $sql = "SELECT k.key || ' | ' || json_extract(v.value,'`$.dimension') || ' ' || json_extract(v.value,'`$.op') || ' ' || " +
        "json_extract(v.value,'`$.values') || ' | ' || (SELECT gate_kind FROM gates WHERE id = json_extract(k.always_gates,'`$[0]')) AS r " +
        "FROM key_reach k, json_each(k.always_values) v WHERE k.key LIKE 'demo.hid.%'"
    $r = Invoke-Gate --map-query $made.Db --width 0 --sql $sql
    Assert-Exit $r 0
    Assert-Line $r 'demo.hid.own | kind not_in ["Alpha"] | hidden'
    Assert-Line $r 'demo.hid.child | kind not_in ["Beta"] | hidden'
    Assert-Line $r 'demo.hid.if | kind in ["Gamma"] | structural'
}

Test-Case 'tsrows: a guarded method a field holds and a ternary of two predicates restrict as a getter does' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/kinds.ts' = "export enum Kind { Home = 1, Flat = 2, Life = 3 }`n" +
            "export const isKind = (ids: Kind[]) => (...kinds: Kind[]) => kinds.some((k) => ids.includes(k));`n" +
            "export const isHome = isKind([Kind.Home]);`nexport const isFlat = isKind([Kind.Flat]);`n" +
            "export function isLocalEnv(): boolean { return true; }`n"
        'apps/shop/src/a.component.ts' = "import { Component, Input, OnInit } from '@angular/core';`n" +
            "import { Kind, isHome, isFlat, isLocalEnv } from './kinds';`n" +
            "@Component({ selector: 'demo-a', template: '<i *ngIf=`"either`">a</i><b *ngIf=`"homeOnly`">b</b>" +
            "<u *ngIf=`"arms`">c</u><s *ngIf=`"twice`">d</s><p *ngIf=`"isFlatGift`">e</p>' })`n" +
            "export class AComponent implements OnInit {`n  @Input() modelKinds: Kind[] = [];`n  @Input() detail!: { Gift?: boolean };`n" +
            "  either = false;`n  homeOnly = false;`n  arms = false;`n  twice = false;`n" +
            "  ngOnInit() {`n" +
            "    this.either = isLocalEnv() ? this.isHomeGift() : this.isFlatGift();`n" +
            "    this.homeOnly = this.isHomeGift();`n" +
            "    this.arms = isLocalEnv() ? isHome(...this.modelKinds) : isFlat(...this.modelKinds);`n" +
            "    this.twice = this.isTwice();`n  }`n" +
            "  private isHomeGift(): boolean {`n    const isHomeSelected = isHome(...this.modelKinds);`n" +
            "    if (isHomeSelected) {`n      return !!this.detail?.Gift;`n    }`n    return false;`n  }`n" +
            "  private isFlatGift(): boolean {`n    if (isFlat(...this.modelKinds)) {`n      return !!this.detail?.Gift;`n    }`n    return false;`n  }`n" +
            "  private isTwice(): boolean {`n    if (isFlat(...this.modelKinds)) {`n      return true;`n    }`n    return !!this.detail?.Gift;`n  }`n}`n"
    }
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    $r = Invoke-TsRowsQ $made.Db $script:TsEnumFormsSql
    # each guarded method is `C && E`; the ternary's arms union while its condition restricts nothing
    Assert-Line $r 'either => Kind modelKinds in ["Flat","Home"]'
    Assert-Line $r 'homeOnly => Kind modelKinds in ["Home"]'
    Assert-Line $r 'arms => Kind modelKinds in ["Flat","Home"]'
    # a second return that is not `false`, and a method READ rather than called (a function is truthy): no row
    Assert-NoLine $r 'twice =>'
    Assert-NoLine $r 'isFlatGift =>'
}

# #66: the guard returns the bare optional read `this.detail?.Gift` (not `!!`) and the key sits under a carrier,
# so what is asserted is `key_reach.always_values`, through a parent that binds the list.
Test-Case 'tsrows: a key under a guarded method returning a bare optional read is reached under the guard' {
    $tpl = '<div *ngIf="homeOnly"><translate [text]="\''demo.gm.field\''"></translate></div>' +
        '<div *ngIf="viaLocal"><translate [text]="\''demo.gm.local\''"></translate></div>' +
        '<div *ngIf="either"><translate [text]="\''demo.gm.either\''"></translate></div>' +
        '<div *ngIf="isHomeGift()"><translate [text]="\''demo.gm.called\''"></translate></div>' +
        '<div *ngIf="control"><translate [text]="\''demo.gm.control\''"></translate></div>'
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/kinds.ts' = "export enum Kind { Home = 1, Flat = 2, Life = 3 }`n" +
            "export const isKind = (ids: Kind[]) => (...kinds: Kind[]) => kinds.some((k) => ids.includes(k));`n" +
            "export const isHome = isKind([Kind.Home]);`nexport const isFlat = isKind([Kind.Flat]);`n" +
            "export function isLocalEnv(): boolean { return true; }`n"
        'apps/shop/src/translate.component.ts' = "import { Component, Input } from '@angular/core';`n" +
            "@Component({ selector: 'translate', standalone: true, template: '<span>{{ text }}</span>' })`n" +
            "export class TranslateComponent { @Input() text!: string; }`n"
        'apps/shop/src/a.component.ts' = "import { Component, Input, OnInit } from '@angular/core';`n" +
            "import { Kind, isHome, isFlat, isLocalEnv } from './kinds';`n" +
            "@Component({ selector: 'demo-a', template: '" + $tpl + "' })`n" +
            "export class AComponent implements OnInit {`n  @Input() modelKinds: Kind[] = [];`n  @Input() detail!: { Gift?: boolean };`n" +
            "  homeOnly = false;`n  viaLocal = false;`n  either = false;`n  control = false;`n" +
            "  ngOnInit() {`n" +
            "    this.homeOnly = this.isHomeGift();`n" +
            "    this.viaLocal = this.isHomeLocal();`n" +
            "    this.either = isLocalEnv() ? this.isHomeGift() : this.isFlatGift();`n" +
            "    this.control = isHome(...this.modelKinds);`n  }`n" +
            "  isHomeGift(): boolean {`n    if (isHome(...this.modelKinds)) {`n      return this.detail?.Gift;`n    }`n    return false;`n  }`n" +
            "  private isHomeLocal(): boolean {`n    const sel = isHome(...this.modelKinds);`n" +
            "    if (sel) {`n      return this.detail?.Gift;`n    }`n    return false;`n  }`n" +
            "  private isFlatGift(): boolean {`n    if (isFlat(...this.modelKinds)) {`n      return this.detail?.Gift;`n    }`n    return false;`n  }`n}`n"
        'apps/shop/src/root.component.ts' = "import { Component } from '@angular/core';`n" +
            "import { Kind } from './kinds';`n" +
            "@Component({ selector: 'demo-root', template: '<demo-a [modelKinds]=`"kinds`" [detail]=`"detail`"></demo-a>' })`n" +
            "export class RootComponent { kinds: Kind[] = [Kind.Life]; detail = { Gift: true }; }`n"
        'apps/shop/src/assets/locales/en.json' = '{"demo":{"gm":{"field":"1","local":"2","either":"3","called":"4","control":"5"}}}'
        'structuregate.ts.json' = '{"locales":["apps/shop/src/assets/locales"],"i18nCarriers":["translate:text"]}'
    }
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    $sql = "SELECT k.key || ' | ' || coalesce((SELECT group_concat(json_extract(v.value,'`$.dimension') || ' ' || " +
        "json_extract(v.value,'`$.op') || ' ' || json_extract(v.value,'`$.values'), ' & ') FROM json_each(k.always_values) v), '-') AS r " +
        "FROM key_reach k WHERE k.key LIKE 'demo.gm.%'"
    $r = Invoke-Gate --map-query $made.Db --width 0 --sql $sql
    Assert-Exit $r 0
    Assert-Line $r 'demo.gm.control | modelKinds in ["Home"]'
    Assert-Line $r 'demo.gm.field | modelKinds in ["Home"]'
    Assert-Line $r 'demo.gm.local | modelKinds in ["Home"]'
    Assert-Line $r 'demo.gm.called | modelKinds in ["Home"]'
    # `modelKinds` is a SET: "holds some of Flat, Home" is not "holds no Life", so the `in` side is kept
    Assert-Line $r 'demo.gm.either | modelKinds in ["Flat","Home"]'
    $g = Invoke-TsRowsQ $made.Db $script:TsEnumFormsSql
    Assert-Line $g 'either => Kind modelKinds in ["Flat","Home"]'
    Assert-Line $g 'isHomeGift() => Kind modelKinds in ["Home"]'
}

}
