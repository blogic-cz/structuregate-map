<#
    A TEMPLATE WRITE NAMES THE MEMBER IT WRITES (#99, `TsTpl/TsTplReads.mjs::chainOf`): `(valueChange)="f.On = $event"` gives its `Write`
    node the `target` the `Read` of `f.On` in an input binding gets. A READ NAMES WHERE IT IS (rows 134, `TsExpr/TsExprNorm.mjs::placeOf`,
    `TsTpl/TsTplExpr.mjs::placeOf`): a code read with a `target` and every template read carry their NAME's `line` and `col` in the file - the
    expression's own line is only where it starts. Its helpers are `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '../../TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

$script:TsWriteFiles = @{
    'apps/shop/src/write.component.ts' = "import { Component, EventEmitter, Input, Output } from '@angular/core';`n" +
        "export interface Flags { On: boolean; }`n" +
        "@Component({ selector: 'app-check', template: '' })`n" +
        "export class CheckComponent { @Input() value = false; @Output() valueChange = new EventEmitter<boolean>(); }`n" +
        "@Component({ selector: 'app-write', imports: [CheckComponent], template: '<app-check [value]=`"f.On`" (valueChange)=`"f.On = `$event`"></app-check><app-check (valueChange)=`"done = `$event`"></app-check>' })`n" +
        "export class WriteComponent {`n  f: Flags = { On: false };`n  done = false;`n}`n"
}

# The member each Write node of a handler targets, `name@line`.
function Get-TsWriteTargets([string]$Db) {
    return Invoke-TsRowsQ $Db ("SELECT 'write=' || n.value || '@' || l.value AS r FROM expressions x " +
        "JOIN expressions__ast w ON w.owner = x.id AND w.role = 'k' AND w.value = 'Write' " +
        "JOIN expressions__ast t ON t.owner = x.id AND t.parent = w.parent AND t.role = 'target' " +
        "JOIN expressions__ast n ON n.owner = x.id AND n.parent = t.n AND n.role = 'name' " +
        "JOIN expressions__ast l ON l.owner = x.id AND l.parent = t.n AND l.role = 'line' ORDER BY r")
}

Test-Case 'tsrows: a template write carries the target of the member it writes (#99)' {
    $made = New-TsRowsDb (New-TsRowsWorkspace $script:TsWriteFiles)
    Assert-Exit $made.Result 0
    $w = Get-TsWriteTargets $made.Db
    # `f.On = $event` writes Flags.On (line 2); `done = $event` the component's own member (line 8).
    Assert-Line $w 'write=On@2'
    Assert-Line $w 'write=done@8'
}

$script:TsPlaceFiles = @{
    'apps/shop/src/place.component.ts' = "import { Component } from '@angular/core';`n" +
        "export enum Hue { One = 1 }`n" +
        "@Component({ selector: 'app-place', templateUrl: './place.component.html' })`n" +
        "export class PlaceComponent {`n" +
        "  flag = false;`n" +
        "  run() {`n" +
        "    return [`n" +
        "      1,`n" +
        "      this.flag,`n" +
        "    ];`n" +
        "  }`n" +
        "  check() { return this.flag &&`n" +
        "    Hue.One; }`n" +
        "}`n" +
        "@Component({ selector: 'app-inline', template: '<b>{{ shown }}</b>' })`n" +
        "export class InlineComponent { shown = 1; }`n"
    'apps/shop/src/place.component.html' = "<div`n  *ngIf=`"flag`">{{`n    flag &&`n      flag }}</div>`n"
}

# Every node with a `target`: the declaration's name and where the READ is, `name=line:col`.
function Get-TsReadPlaces([string]$Db) {
    return Invoke-TsRowsQ $Db ("SELECT 'read=' || n.value || '=' || coalesce(l.value, '-') || ':' || coalesce(c.value, '-') AS r FROM expressions x " +
        "JOIN expressions__ast t ON t.owner = x.id AND t.role = 'target' " +
        "JOIN expressions__ast n ON n.owner = x.id AND n.parent = t.n AND n.role = 'name' " +
        "LEFT JOIN expressions__ast l ON l.owner = x.id AND l.parent = t.parent AND l.role = 'line' " +
        "LEFT JOIN expressions__ast c ON c.owner = x.id AND c.parent = t.parent AND c.role = 'col' ORDER BY r")
}

Test-Case 'tsrows: a read in code or a template names its own line and column (rows 134)' {
    $made = New-TsRowsDb (New-TsRowsWorkspace $script:TsPlaceFiles)
    Assert-Exit $made.Result 0
    $r = Get-TsReadPlaces $made.Db
    # CODE: `this.flag` four lines below the `return [` it is in; `Hue.One` on the line after `this.flag &&` - the enum (col 5) and its member (col 9).
    Assert-Line $r 'read=flag=9:12'
    Assert-Line $r 'read=flag=12:25'
    Assert-Line $r 'read=Hue=13:5'
    Assert-Line $r 'read=One=13:9'
    # AN EXTERNAL TEMPLATE: the `*ngIf` condition on the binding's second line, the interpolation's reads below its `{{` - where the
    # source has them, though Angular parsed the text with each whitespace run collapsed to one space.
    Assert-Line $r 'read=flag=2:10'
    Assert-Line $r 'read=flag=3:5'
    Assert-Line $r 'read=flag=4:7'
    # AN INLINE TEMPLATE: the place in the .ts, the template text's first line shifted to the quote's column.
    Assert-Line $r 'read=shown=15:55'
}

}
