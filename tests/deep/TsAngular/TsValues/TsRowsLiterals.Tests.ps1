<#
    A KEYWORD LITERAL IS PUBLISHED AS A LITERAL (#85, `src/TsRows/TsDecls/TsExpr/TsValue.mjs::cell`,
    `TsValueRefs.mjs::undefinedLiteral`): a whole value cell spelled `null` is `{$literal: 'null', $kind: 'NullKeyword'}` -
    a NULL cell is `return;` - the global `undefined` and `void 0` are `{$literal: 'undefined', ..}` anywhere, and a name is
    still a name: an `Identifier` value, with `$local` where a destructuring `locals` row binds it.
    Read through node rows (`<table>__value`), never JSON text. Its helpers are `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '../../TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

# The value cell of one statement row, by the member it sits in: `$literal | $kind | $expr`, `-` for a member it lacks.
function Get-TsLiteralSql([string]$Table, [string]$Where) {
    $child = "${Table}__value"
    $field = { param($role) "coalesce($(Get-NodeRootFieldSql $child 'r.id' $role), '-')" }
    return "SELECT m.name || ' | ' || $(& $field '$literal') || ' | ' || $(& $field '$kind') || ' | ' || $(& $field '$expr') || " +
        "' | ' || (SELECT count(*) FROM $child z WHERE z.owner = r.id) AS v " +
        "FROM $Table r JOIN members m ON m.id = r.member JOIN classes c ON c.id = m.class AND c.name = 'OrderService' WHERE $Where"
}

$script:TsLiteralFiles = @{
    'apps/shop/src/order.service.ts' = "import { Injectable } from '@angular/core';`n" +
        "@Injectable({ providedIn: 'root' })`n" +
        "export class OrderService {`n" +
        "  state: string | null = null;`n" +
        "  x: number | null = 1;`n" +
        "  retNull(): null { return null; }`n" +
        "  retUndef(): undefined { return undefined; }`n" +
        "  retVoid(): undefined { return void 0; }`n" +
        "  setNull(): void { this.x = null; }`n" +
        "  localUndef(): unknown { const y = undefined; return y; }`n" +
        "  letNull(): string | null { let s = null; return s; }`n" +
        "  shadow(): number { const undefined = 2; return undefined; }`n" +
        "  bare(): void { return; }`n" +
        "  retParam(a: string): string { return a; }`n" +
        "  retBinding(): string { const { state } = this.snap(); return state; }`n" +
        "  snap(): { state: string } { return { state: 'a' }; }`n" +
        "}`n"
}

Test-Case 'tsrows literals: a value cell spelled null is the null literal, never a NULL cell' {
    $made = New-TsRowsDb (New-TsRowsWorkspace $script:TsLiteralFiles)
    Assert-Exit $made.Result 0
    $r = Invoke-TsRowsQ $made.Db (Get-TsLiteralSql 'returns' "m.name IN ('retNull', 'letNull', 'bare')")
    Assert-Line $r 'retNull | null | NullKeyword | - | 3'
    # a name the evaluator follows to its `null` initializer holds the literal too
    Assert-Line $r 'letNull | null | NullKeyword | - | 3'
    # `return;` spells no value, and holds none
    Assert-Line $r 'bare | - | - | - | 0'
    $a = Invoke-TsRowsQ $made.Db (Get-TsLiteralSql 'assignments' "m.name = 'setNull'")
    Assert-Line $a 'setNull | null | NullKeyword | - | 3'
    $f = Invoke-TsRowsQ $made.Db ("SELECT r.name || ' | ' || coalesce($(Get-NodeRootFieldSql 'members__value' 'r.id' '$literal'), '-') AS v " +
        "FROM members r JOIN classes c ON c.id = r.class AND c.name = 'OrderService' WHERE r.name = 'state'")
    Assert-Line $f 'state | null'
}

Test-Case 'tsrows literals: the global undefined and void 0 are the undefined literal, a shadowing local is its value' {
    $made = New-TsRowsDb (New-TsRowsWorkspace $script:TsLiteralFiles)
    Assert-Exit $made.Result 0
    $r = Invoke-TsRowsQ $made.Db (Get-TsLiteralSql 'returns' "m.name IN ('retUndef', 'retVoid', 'localUndef', 'shadow')")
    Assert-Line $r 'retUndef | undefined | UndefinedKeyword | - | 3'
    Assert-Line $r 'retVoid | undefined | VoidExpression | - | 3'
    Assert-Line $r 'localUndef | undefined | UndefinedKeyword | - | 3'
    Assert-Line $r 'shadow | - | - | - | 0'
    $s = Invoke-TsRowsQ $made.Db "SELECT 'shadow=' || r.value AS v FROM returns r JOIN members m ON m.id = r.member WHERE m.name = 'shadow'"
    Assert-Line $s 'shadow=2'
    $l = Invoke-TsRowsQ $made.Db (Get-TsLiteralSql 'locals' "m.name = 'localUndef'")
    Assert-Line $l 'localUndef | undefined | UndefinedKeyword | - | 3'
}

Test-Case 'tsrows literals: a returned name stays an identifier, and one a destructuring binds names its locals row' {
    $made = New-TsRowsDb (New-TsRowsWorkspace $script:TsLiteralFiles)
    Assert-Exit $made.Result 0
    $r = Invoke-TsRowsQ $made.Db (Get-TsLiteralSql 'returns' "m.name IN ('retParam', 'retBinding')")
    Assert-Line $r 'retParam | - | Identifier | a |'
    Assert-Line $r 'retBinding | - | Identifier | state |'
    $j = Invoke-TsRowsQ $made.Db ("SELECT 'local=' || (CASE WHEN l.line = $(Get-NodeFieldSql 'returns__value' 'lv' 'line') " +
        "AND l.col = $(Get-NodeFieldSql 'returns__value' 'lv' 'col') THEN 'own row' ELSE 'other' END) AS v FROM returns r " +
        "JOIN members m ON m.id = r.member AND m.name = 'retBinding' " +
        "JOIN returns__value lv ON lv.owner = r.id AND lv.parent = 0 AND lv.role = '`$local' " +
        "JOIN locals l ON l.member = r.member AND l.destructured = 1")
    Assert-Line $j 'local=own row'
    $p = Invoke-TsRowsQ $made.Db ("SELECT 'param=' || count(*) AS v FROM returns r JOIN members m ON m.id = r.member AND m.name = 'retParam' " +
        "JOIN returns__value lv ON lv.owner = r.id AND lv.parent = 0 AND lv.role = '`$local'")
    Assert-Line $p 'param=0'
}
}
