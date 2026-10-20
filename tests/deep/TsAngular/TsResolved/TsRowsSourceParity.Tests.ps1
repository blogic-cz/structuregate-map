<#
    THE MAP IS 1:1 WITH THE SOURCE (`TsDecls/TsBody.mjs::collectExpr`): code outside a function body - a property initializer, a
    decorator's argument, a top-level statement, a parameter default - is walked as a body, its calls and the callbacks in it rows;
    a class expression is a class; `(x as T) = v` is an assignment; a declaration with no initializer (`let t;`, a loop's
    variable, `catch (e)`) is a local. Its helpers are `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '../../TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

# Line numbers as the cases assert them.
$script:TsParityFiles = @{
    'apps/shop/src/parity.ts' = (@(
        "export const fmt = (v: number) => String(v);",                                          # 1
        "export const lift = (f: (x: number) => string) => f;",                                  # 2
        "export class Box {",                                                                    # 3
        "  static Inner = class { static A = 'a'; static B = 'b'; };",                           # 4
        "  shown = lift((v) => { const n = fmt(v); return n; });",                               # 5
        "  run(cond = () => fmt(0) !== '') {",                                                   # 6
        "    let t: string;",                                                                    # 7
        "    t = 'x';",                                                                          # 8
        "    for (const s of [1, 2]) { fmt(s); }",                                               # 9
        "    try { fmt(3); } catch (e) { fmt(4); }",                                             # 10
        "    (this.shown as any) = t;",                                                          # 11
        "    return cond();",                                                                    # 12
        "  }",                                                                                   # 13
        "}",                                                                                     # 14
        "setTimeout(() => fmt(5));",                                                             # 15
        "export const pick = (a: number[]) => (b: number) => a.includes(b);",                    # 16
        "export const swap = (xs: number[], i: number, j: number) => { [xs[i], xs[j]] = [xs[j], xs[i]]; return xs; };" # 17
    ) -join "`n") + "`n"
}

function Get-TsParity([string]$Db, [string]$Sql, [string]$Label) {
    $r = Invoke-TsRowsQ $Db ("SELECT '$Label=' || group_concat(v, ',') AS r FROM (" + $Sql + ")")
    return $r
}

Test-Case 'tsrows: code outside a function body is rows - initializers, defaults, top-level statements' {
    $made = New-TsRowsDb (New-TsRowsWorkspace $script:TsParityFiles)
    Assert-Exit $made.Result 0
    $f = "(SELECT id FROM files WHERE path = 'apps/shop/src/parity.ts')"
    # `lift(..)` in the initializer (5), `fmt` in its callback (5), in the default (6), in the body (9, 10, 10), at the top level (15).
    Assert-Line (Get-TsParity $made.Db "SELECT line || ':' || callee AS v FROM calls WHERE file = $f AND callee IN ('fmt', 'lift') ORDER BY line, col" 'calls') `
        'calls=5:lift,5:fmt,6:fmt,9:fmt,10:fmt,10:fmt,15:fmt'
    # The callbacks are functions: in the initializer (5), the default (6), the top-level call (15).
    Assert-Line (Get-TsParity $made.Db "SELECT line AS v FROM functions WHERE file = $f AND inline = 1 ORDER BY line" 'inline') 'inline=5,6,15,16'
    # The curried inner arrow returns too: an implicit return of the outer (its function) and of the inner (`a.includes(b)`).
    Assert-Line (Get-TsParity $made.Db "SELECT count(*) AS v FROM returns WHERE file = $f AND line = 16" 'curried') 'curried=2'
}

Test-Case 'tsrows: a class expression is a class, a cast target an assignment, a declaration without initializer a local' {
    $made = New-TsRowsDb (New-TsRowsWorkspace $script:TsParityFiles)
    Assert-Exit $made.Result 0
    $f = "(SELECT id FROM files WHERE path = 'apps/shop/src/parity.ts')"
    Assert-Line (Get-TsParity $made.Db ("SELECT m.name AS v FROM members m JOIN classes c ON c.id = m.class " +
        "WHERE c.file = $f AND c.name = 'Inner' ORDER BY m.name") 'inner') 'inner=A,B'
    Assert-Line (Get-TsParity $made.Db "SELECT line || ':' || target AS v FROM assignments WHERE file = $f ORDER BY line" 'writes') 'writes=8:t,11:shown,17:[xs[i], xs[j]]'
    Assert-Line (Get-TsParity $made.Db "SELECT line || ':' || name AS v FROM locals WHERE file = $f ORDER BY line, name" 'locals') 'locals=5:n,7:t,9:s,10:e'
}

}
