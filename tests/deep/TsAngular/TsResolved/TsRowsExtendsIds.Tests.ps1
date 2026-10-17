<#
    A base class carries the `classes` row it IS (#87, `TsDecls/TsClassRows.mjs::heritageRef`, `TsRefs.mjs`): `classes.extends`
    and each of its `type_args` hold the `id` of the declaration the checker resolved - through an import alias and a barrel,
    never by the name as written. Its helpers are `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '../../TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

# `Base` is an ALIAS of `AlphaBase` imported through a BARREL, and a second `AlphaBase` elsewhere shares the name.
$script:TsExtendsFiles = @{
    'apps/shop/src/base/alpha-base.ts' = "export class AlphaBase<T> {`n  item?: T;`n}`n"
    'apps/shop/src/base/order.ts' = "export class Order {`n  id = 1;`n}`n"
    'apps/shop/src/base/index.ts' = "export { AlphaBase } from './alpha-base';`nexport * from './order';`n"
    'apps/shop/src/decoy/alpha-base.ts' = "export class AlphaBase {}`nexport class Order {}`n"
    'apps/shop/src/beta.ts' = "import { AlphaBase as Base, Order } from './base';`n" +
        "export class Beta extends Base<Order> {}`n"
    'apps/shop/src/gamma.ts' = "import { Beta } from './beta';`nexport class Gamma extends Beta {}`n" +
        "export class Delta extends Array<string> {}`n"
}

# `<class>><base>@<base file>|<arg>@<arg file>` for each class extending another, read from the node rows.
function Get-TsExtendsIds([string]$Db) {
    $arg = "(SELECT a.name || '@' || af.path FROM classes__extends ta JOIN classes__extends ti ON ti.owner = ta.owner " +
        "AND ti.parent = ta.n JOIN classes a ON a.id = $(Get-TsFieldSql 'classes__extends' 'ti' 'id') JOIN files af ON af.id = a.file " +
        "WHERE ta.owner = c.id AND ta.parent = 0 AND ta.role = 'type_args' AND ti.ord = 0)"
    # `--width 0`: a row longer than the default width is cut with `...[+N chars]`, and the assertion reads all of it.
    $r = Invoke-Gate --map-query $Db --width 0 --sql ("SELECT 'ext=' || c.name || '>' || coalesce(b.name || '@' || bf.path, '-') || '|' || coalesce($arg, '-') AS r " +
        "FROM classes c LEFT JOIN classes b ON b.id = $(Get-TsRootFieldSql 'classes__extends' 'c.id' 'id') " +
        "LEFT JOIN files bf ON bf.id = b.file WHERE $(Get-TsRootFieldSql 'classes__extends' 'c.id' 'text') IS NOT NULL")
    Assert-Exit $r 0
    return $r
}

Test-Case 'tsrows: a base class and its type argument carry the classes row the checker resolved, through an alias and a barrel' {
    $made = New-TsRowsDb (New-TsRowsWorkspace $script:TsExtendsFiles)
    Assert-Exit $made.Result 0
    $ext = Get-TsExtendsIds $made.Db
    Assert-Line $ext 'ext=Beta>AlphaBase@apps/shop/src/base/alpha-base.ts|Order@apps/shop/src/base/order.ts'
    Assert-Line $ext 'ext=Gamma>Beta@apps/shop/src/beta.ts|-'
    # A library base has no row: `Array` is named, with no id.
    Assert-Line $ext 'ext=Delta>-|-'
    Assert-NoLine $ext 'decoy'
    # The text a consumer reads stays the name as written.
    Assert-Line (Invoke-TsRowsQ $made.Db ("SELECT 'text=' || $(Get-TsRootFieldSql 'classes__extends' 'c.id' 'text') AS r " +
        "FROM classes c WHERE c.name = 'Beta'")) 'text=Base'
}

# A PARTIAL RUN: the base's file read again keeps the id a kept subclass names, and the subclass read again resolves it anew.
Test-Case 'tsrows: a partial run keeps the extends ids' {
    $tree = New-TsRowsWorkspace $script:TsExtendsFiles
    Assert-Exit (New-TsRowsDb $tree).Result 0
    foreach ($edit in @('apps/shop/src/base/alpha-base.ts', 'apps/shop/src/beta.ts')) {
        $path = Join-Path $tree $edit
        Set-Content -LiteralPath $path -Value ((Get-Content -LiteralPath $path -Raw) + "// edited`n") -NoNewline
        $again = New-TsRowsDb $tree
        Assert-Exit $again.Result 0
        Assert-Line $again.Result 'put back'
        $ext = Get-TsExtendsIds $again.Db
        Assert-Line $ext 'ext=Beta>AlphaBase@apps/shop/src/base/alpha-base.ts|Order@apps/shop/src/base/order.ts'
        Assert-Line $ext 'ext=Gamma>Beta@apps/shop/src/beta.ts|-'
    }
}

}
