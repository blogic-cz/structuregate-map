<#
    What a reader JOINS by: one enum tested through differently written expressions folds as the enum, and the
    rows a consumer walks by id - a local's declaration, a function handed to a call, the call a chained call is
    made on - name each other instead of leaving the join to a position.
#>

. (Join-Path $PSScriptRoot '..\TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

# ONE CHILD, THREE PARENTS, ONE MEMBER: `kind === Alpha` twice and `item.kind === Alpha` once. Folded per
# dimension text each dimension was absent on some way and the key read as unrestricted; folded as the enum it
# requires `Alpha` on every way. The control is a key one parent renders under no gate at all.
Test-Case 'tsrows: a key behind one enum tested through differently written expressions carries the enum' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/kinds.ts' = "export enum DemoKind { Alpha = 1, Beta = 2, Gamma = 3 }`n"
        'apps/shop/src/parts.component.ts' = "import { Component, Input } from '@angular/core';`n" +
            "import { NgIf } from '@angular/common';`n" +
            "import { DemoKind } from './kinds';`n" +
            "import { BasketModule } from './basket.component';`n" +
            "@Component({ selector: 'demo-c', standalone: true, imports: [BasketModule], template: '<b [title]=`"\'demo.shared\' | money`"></b>' })`n" +
            "export class CComponent {}`n" +
            "@Component({ selector: 'demo-d', standalone: true, imports: [BasketModule], template: '<b [title]=`"\'demo.split\' | money`"></b>' })`n" +
            "export class DComponent {}`n" +
            "@Component({ selector: 'demo-p1', standalone: true, imports: [NgIf, CComponent, DComponent],`n" +
            "  template: '<demo-c *ngIf=`"kind === kinds.Alpha`"></demo-c><demo-d *ngIf=`"kind === kinds.Alpha`"></demo-d>' })`n" +
            "export class P1Component { kinds = DemoKind; @Input() kind: DemoKind; }`n" +
            "@Component({ selector: 'demo-p2', standalone: true, imports: [NgIf, CComponent, DComponent],`n" +
            "  template: '<demo-c *ngIf=`"item.kind === kinds.Alpha`"></demo-c><demo-d></demo-d>' })`n" +
            "export class P2Component { kinds = DemoKind; @Input() item: { kind: DemoKind }; }`n" +
            "@Component({ selector: 'demo-p3', standalone: true, imports: [NgIf, CComponent],`n" +
            "  template: '<demo-c *ngIf=`"kind === kinds.Alpha`"></demo-c>' })`n" +
            "export class P3Component { kinds = DemoKind; @Input() kind: DemoKind; }`n"
        'apps/shop/src/parts.routes.ts' = "import { P1Component, P2Component, P3Component } from './parts.component';`n" +
            "export const partRoutes = [{ path: 'p1', component: P1Component }, { path: 'p2', component: P2Component },`n" +
            "  { path: 'p3', component: P3Component }];`n"
        'apps/shop/src/assets/locales/en.json' = '{"demo":{"shared":"s","split":"t"}}'
    }
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    $r = Invoke-TsRowsQ $made.Db ("SELECT k.key || ' | ' || v.dimension || ' ' || " +
        "v.op || ' ' || $(Get-NodeListSql 'key_reach__always_values' 'v' 'values') AS reach " +
        "FROM key_reach k JOIN key_reach__always_values v ON v.owner = '#' || k.rowid AND v.parent = '#' || k.rowid || '/0' WHERE k.key LIKE 'demo.%'")
    Assert-Line $r 'demo.shared | item.kind | kind in ["Alpha"]'
    Assert-NoLine $r 'demo.split |'
}

# THE JOINS BY ID. A read of a body's `const` names where its `locals` row sits (`local`); a function handed to a
# call names its `functions` row (`$fn_id`) - an inline arrow and a `const` arrow alike, though the second's row
# sits at the const's NAME and its tree at the arrow; and a chained call names the call it is made on
# (`receiver_call`), where the start position the two share cannot tell three links apart.
Test-Case 'tsrows: a local read, a function argument and a chained call name the rows they join' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/joins.ts' = "export enum DemoKind { Alpha = 1, Beta = 2, Gamma = 3 }`n" +
            "export class Checker {`n" +
            "  isAlphaLike(kind: DemoKind): boolean {`n" +
            "    const alphaKinds = [DemoKind.Alpha, DemoKind.Beta];`n" +
            "    return alphaKinds.includes(kind);`n" +
            "  }`n" +
            "}`n" +
            "export const getTotal = (rows: number[]): number => rows.reduce((a, b) => a + b, 0);`n" +
            "export const totals = [[1]].map(getTotal);`n" +
            "export class Panel {`n" +
            "  visible = false;`n" +
            "  constructor(private flags: { isEnabled(id: number): Promise<boolean> }) {}`n" +
            "  init() {`n" +
            "    this.flags.isEnabled(1).then((on) => { this.visible = on; }).finally(() => {});`n" +
            "  }`n" +
            "}`n"
    }
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    $l = Invoke-TsRowsQ $made.Db ("SELECT 'local ' || l.name || ' ' || lc.line || ':' || " +
        "lc.col || ' row ' || l.line || ':' || l.col AS j " +
        "FROM expressions x JOIN locals l ON l.name = 'alphaKinds' " +
        "JOIN expressions__ast r1 ON r1.parent = x.id || '/0' AND r1.role = 'receiver' JOIN expressions__ast r2 ON r2.parent = r1.id AND r2.role = 'receiver' " +
        "JOIN expressions__ast lc ON lc.parent = r2.id AND lc.role = 'local' WHERE x.source = 'alphaKinds.includes(kind)'")
    Assert-Line $l 'local alphaKinds 4:11 row 4:11'
    $f = Invoke-TsRowsQ $made.Db ("SELECT 'fn ' || c.callee || ' -> ' || coalesce(f.name, 'inline') || ' ' || f.line AS j FROM calls c " +
        "JOIN calls__args a0 ON a0.owner = c.id AND a0.parent = c.id || '/0' AND a0.ord = 0 " +
        "JOIN functions f ON f.id = a0.[`$fn_id] WHERE c.callee IN ('[[1]].map', 'rows.reduce')")
    Assert-Line $f 'fn [[1]].map -> getTotal 8'
    Assert-Line $f 'fn rows.reduce -> inline 8'
    $c = Invoke-TsRowsQ $made.Db ("SELECT 'chain ' || c.method || ' on ' || coalesce(r.method, '-') AS j FROM calls c " +
        "LEFT JOIN calls r ON r.id = c.receiver_call WHERE c.method IN ('isEnabled', 'then', 'finally')")
    Assert-Line $c 'chain finally on then'
    Assert-Line $c 'chain then on isEnabled'
    Assert-Line $c 'chain isEnabled on -'
}

}
