<#
    A predicate CALLED WITH A SPREAD: `isAB(...this.chosen)` over a factory whose rest parameter is tested element
    by element (`rest.some((x) => ids.includes(x))`, or a prototype method whose body proves the same) is
    `chosen.some((x) => ids.includes(x))` - a SET row on `chosen`. Read in `rust/fbtcore/src/rows/ts/gate/ts/
    gate_curried.rs` and `gate_lists.rs` (`Lists::restriction`); a set keeps the side it was written on in
    `gate/gate_collapse.rs`. Its helpers are `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '../../TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

$script:TsGateSpreadSql = "SELECT g.source || ' => ' || v.enum_name || ' ' || v.dimension || ' ' || v.op || ' ' || " +
    "v.values_json AS restriction FROM gate_values v JOIN gates g ON g.id = v.gate"

# THE ISSUE'S REPRO, then what a consumer writes beside it. A spread of an `@Input` or of a plain field is a SET
# row: "some element is in the list", and its negation "none is" - exact both ways, so `in` stays `in` even when
# the list is most of the enum ("holds some of Alpha, Beta" is not "holds no Gamma"). A prototype method proving
# the same test reads the same. `ids.includes(rest[0])` tests the first element only, and a call handing a value
# beside the spread is not the one argument the factory's test reads: both stay unread.
Test-Case 'tsrows: a predicate called with a spread of an array restricts the array as a set' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/x.component.ts' = "import { Component, Input, OnInit } from '@angular/core';`n" +
            "export enum Kind { Alpha = 1, Beta = 2, Gamma = 3 }`n" +
            "export const makeMany = (ids: Kind[]) => (...rest: Kind[]) => rest.some((x) => ids.includes(x));`n" +
            "export const isAlphaOrBeta = makeMany([Kind.Alpha, Kind.Beta]);`n" +
            "@Component({ selector: 'demo-x', template: ``n" +
            "  <translate *ngIf=`"viaSpread`" [text]=`"'demo.spread'`"></translate>`n" +
            "  <translate *ngIf=`"viaSingle`" [text]=`"'demo.single'`"></translate>`` })`n" +
            "export class XComponent implements OnInit {`n" +
            "  @Input() chosen: Kind[] = [];`n  @Input() kind: Kind;`n  viaSpread = false;`n  viaSingle = false;`n" +
            "  ngOnInit() {`n    this.viaSpread = isAlphaOrBeta(...this.chosen);`n    this.viaSingle = isAlphaOrBeta(this.kind);`n  }`n}`n"
        'apps/shop/src/array-ext.ts' = "export {};`n" +
            "declare global { interface Array<T> { intersects(other: T[]): boolean; } }`n" +
            "Array.prototype.intersects = function <T>(other: T[]): boolean { return this.some((x) => other.includes(x)); };`n"
        'apps/shop/src/preds.ts' = "import { Kind } from './x.component';`n" +
            "export const makeOver = (ids: Kind[]) => (...rest: Kind[]) => rest.intersects(ids);`n" +
            "export const isOverGamma = makeOver([Kind.Gamma]);`n" +
            "export const makeFirst = (ids: Kind[]) => (...rest: Kind[]) => ids.includes(rest[0]);`n" +
            "export const isFirstAlpha = makeFirst([Kind.Alpha]);`n"
        'apps/shop/src/y.component.ts' = "import { Component, OnInit } from '@angular/core';`n" +
            "import { Kind, isAlphaOrBeta } from './x.component';`n" +
            "import { isOverGamma, isFirstAlpha } from './preds';`n" +
            "@Component({ selector: 'demo-y', template: ``n" +
            "  <b *ngIf=`"!viaField`">n</b><i *ngIf=`"viaOver`">o</i><u *ngIf=`"viaFirst`">f</u><s *ngIf=`"viaTwo`">t</s>`` })`n" +
            "export class YComponent implements OnInit {`n" +
            "  picked: Kind[] = [];`n  kind: Kind = Kind.Alpha;`n" +
            "  viaField = false;`n  viaOver = false;`n  viaFirst = false;`n  viaTwo = false;`n" +
            "  ngOnInit() {`n    this.viaField = isAlphaOrBeta(...this.picked);`n    this.viaOver = isOverGamma(...this.picked);`n" +
            "    this.viaFirst = isFirstAlpha(...this.picked);`n    this.viaTwo = isAlphaOrBeta(this.kind, ...this.picked);`n  }`n}`n"
    }
    $made = New-TsRowsDb $tree
    $r = Invoke-Gate --map-query $made.Db --width 0 --sql $script:TsGateSpreadSql
    Assert-Exit $r 0
    Assert-Line $r 'viaSpread => Kind chosen in ["Alpha","Beta"]'
    Assert-Line $r 'viaSingle => Kind kind not_in ["Gamma"]'
    Assert-Line $r '!viaField => Kind picked not_in ["Alpha","Beta"]'
    Assert-Line $r 'viaOver => Kind picked in ["Gamma"]'
    Assert-NoLine $r 'viaFirst =>'
    Assert-NoLine $r 'viaTwo =>'
}

}
