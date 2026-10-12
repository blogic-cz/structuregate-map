<#
    Three ways a comparison against an enum was left `unknown` though its meaning is plain. Read in
    `rust/fbtcore/src/rows/ts/gate/gate_values.rs` (`restriction_of`), `gate/ts/gate_same_enum.rs` and
    `gate/ts/gate_tsrows.rs` (`walk`):
      * the model's enum and the component's are TWO declarations with the same name, members and values -
        one enum for a comparison; a twin with a different member or value stays `unknown`;
      * a getter reads an enum through a field holding the enum OBJECT (`this.shapeRef.Round`), as a template does;
      * a NUMBER compared with an enum-typed value is the member it equals (none: `in []`); `>` stays `unknown`.
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

}
