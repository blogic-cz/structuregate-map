<#
    A union type names what its members name (#88, `TsDecls/TsTypeRef.mjs::refsOfUnion`, `TsDecls/TsClassRows.mjs::resolvedOf`):
    `type_ref` (and a parameter's `resolved`) is the ONE declaration a union's members name, and `type_refs` lists them when
    they name several. Read from the node rows of `members__params` / `members__type_refs`. Its helpers are `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '../../TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

# The workspace is `strict`, so `Codes | null | undefined` stays a union for the checker.
$script:TsUnionFiles = @{
    'apps/shop/src/codes.ts' = "export class Codes { static A = 'a'; }`nexport class Alpha { id = 1; }`n" +
        "export class Order { n = 2; }`nexport enum Kind { A = 'a', B = 'b' }`n"
    'apps/shop/src/svc.ts' = "import { Codes, Alpha, Order, Kind } from './codes';`n" +
        "export class Svc {`n" +
        "  check(code: Codes | string): boolean { return !!code; }`n" +
        "  checkOnly(code: Codes): boolean { return !!code; }`n" +
        "  either(x: Order | Alpha): boolean { return !!x; }`n" +
        "  maybe(code: Codes | null | undefined): boolean { return !!code; }`n" +
        "  kind(k: Kind | undefined): boolean { return !!k; }`n" +
        "  flag(b: boolean | null): boolean { return !!b; }`n" +
        "  chosen: Order | Alpha | null = null;`n" +
        "}`n"
}

# The member `$Key` of the object `$Role` of a method's FIRST parameter (`m` the members row).
function Get-TsUnionParamSql([string]$Role, [string]$Key) {
    return "(SELECT zv.value FROM members__params zi JOIN members__params zr ON zr.owner = zi.owner AND zr.parent = zi.n " +
        "AND zr.role = '$Role' JOIN members__params zv ON zv.owner = zr.owner AND zv.parent = zr.n AND zv.role = '$Key' " +
        "WHERE zi.owner = m.id AND zi.parent = 0 AND zi.ord = 0)"
}

# A ref's `file` is the absolute path of the declaration: `codes` when it is the fixture's `codes.ts`.
function Get-TsUnionPlaceSql([string]$File) {
    return "(CASE WHEN $File LIKE '%/apps/shop/src/codes.ts' THEN 'codes' ELSE $File END)"
}

# `name@place` of every item of a list of refs, in order: `$From` names the list node `zl`, `$Where` ties it to the row `m`.
function Get-TsUnionListSql([string]$Table, [string]$From, [string]$Where) {
    return "(SELECT group_concat(zn.value || '@' || $(Get-TsUnionPlaceSql 'zf.value'), ',' ORDER BY zx.ord) FROM $From " +
        "JOIN $Table zx ON zx.owner = zl.owner AND zx.parent = zl.n " +
        "JOIN $Table zn ON zn.owner = zx.owner AND zn.parent = zx.n AND zn.role = 'name' " +
        "JOIN $Table zf ON zf.owner = zx.owner AND zf.parent = zx.n AND zf.role = 'file' WHERE $Where)"
}

function Get-TsUnionRows([string]$Db) {
    $refs = Get-TsUnionListSql 'members__params' ("members__params zi JOIN members__params zl ON zl.owner = zi.owner " +
        "AND zl.parent = zi.n AND zl.role = 'type_refs'") 'zi.owner = m.id AND zi.parent = 0 AND zi.ord = 0'
    $ref = "$(Get-TsUnionParamSql 'type_ref' 'name') || '@' || $(Get-TsUnionPlaceSql (Get-TsUnionParamSql 'type_ref' 'file'))"
    $r = Invoke-Gate --map-query $Db --width 0 --sql ("SELECT 'p=' || m.name || '|' || coalesce($(Get-TsUnionParamSql 'resolved' 'name'), '-') " +
        "|| '|' || coalesce($ref, '-') || '|' || coalesce($refs, '-') AS r FROM members m WHERE m.kind = 'method' AND m.name IN " +
        "('check', 'checkOnly', 'either', 'maybe', 'kind', 'flag')")
    Assert-Exit $r 0
    return $r
}

Test-Case 'tsrows: a union-typed parameter resolves to the in-tree type its members name (#88)' {
    $made = New-TsRowsDb (New-TsRowsWorkspace $script:TsUnionFiles)
    Assert-Exit $made.Result 0
    $p = Get-TsUnionRows $made.Db
    # The issue: `Codes | string` names Codes, as `Codes` alone does.
    Assert-Line $p 'p=check|Codes|Codes@codes|-'
    Assert-Line $p 'p=checkOnly|Codes|Codes@codes|-'
    Assert-Line $p 'p=maybe|Codes|Codes@codes|-'
    # An enum flattened by the checker into its members is named as the enum, once.
    Assert-Line $p 'p=kind|Kind|Kind@codes|-'
    # Two in-tree types are no ONE answer: `type_ref` is null and `type_refs` lists both, sorted by place.
    Assert-Line $p 'p=either|-|-|Alpha@codes,Order@codes'
    # A union naming nothing stays unresolved.
    Assert-Line $p 'p=flag|-|-|-'
    # A property's own type is resolved the same way, on `members.type_refs`.
    $own = Get-TsUnionListSql 'members__type_refs' 'members__type_refs zl' 'zl.owner = m.id AND zl.n = 0'
    Assert-Line (Invoke-Gate --map-query $made.Db --width 0 --sql ("SELECT 'own=' || coalesce($own, '-') AS r " +
        "FROM members m WHERE m.name = 'chosen'")) 'own=Alpha@codes,Order@codes'
}

}
