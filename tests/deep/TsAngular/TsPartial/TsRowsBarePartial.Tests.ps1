<#
    A partial run in a BARE workspace - angular.json and its tsconfigs, no component with a `templateUrl` - keeps every
    file's id (`TsStore.mjs::rebuiltTables`): `files` is interned, and only a walk interning a template's `.html` used to
    make it a per-file table, so without one it read as a rollup's, was handed back to no partial run, and every file
    was given a new id. A kept barrel's `exports.resolved_file` then moved and the run refused; a kept `classes` row
    named a file id that was gone. Each case compares the partial run's rows, ids replaced by what they name, with a
    fresh full map of the same edited tree. Its helpers are `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '../../TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

# The smallest workspace the Angular half maps, and nothing a template adds.
function New-TsBareWorkspace([hashtable]$Files) {
    $all = @{
        'angular.json' = '{"projects":{"shop":{"projectType":"application","root":"apps/shop",' +
            '"sourceRoot":"apps/shop/src","architect":{"build":{"options":{"tsConfig":"apps/shop/tsconfig.app.json"}},' +
            '"test":{"options":{"tsConfig":"apps/shop/tsconfig.spec.json"}},"lint":{}}}}}'
        'apps/shop/tsconfig.json' = '{"references":[{"path":"./tsconfig.app.json"}]}'
        'apps/shop/tsconfig.app.json' = '{"compilerOptions":{"strict":true},"include":["src/**/*.ts"]}'
        'apps/shop/tsconfig.spec.json' = '{"include":["src/**/*.spec.ts"]}'
        'apps/shop/src/main.ts' = "export const started = true;`n"
    }
    foreach ($key in $Files.Keys) { $all[$key] = $Files[$key] }
    return Use-Tree $all
}

# What the rows SAY, every id replaced by the path or name of the row it names: a row whose file id is gone reads `-`.
# A table the tree gives no row is not asked about. `-Extends`: each class's base and first type argument too.
function Get-TsBareFacts([string]$Db, [switch]$Extends) {
    $parts = [ordered]@{
        'files' = "SELECT 'file=' || f.path AS r FROM files f WHERE f.ext = 'ts'"
        'classes' = "SELECT 'cls=' || c.name || '@' || coalesce(cf.path, '-') AS r FROM classes c LEFT JOIN files cf ON cf.id = c.file"
        'exports' = ("SELECT 'ex=' || coalesce(ef.path, '-') || '>' || coalesce(rf.path, '-') AS r FROM exports e " +
            "LEFT JOIN files ef ON ef.id = e.file LEFT JOIN files rf ON rf.id = e.resolved_file WHERE e.`"from`" IS NOT NULL")
        'imports' = ("SELECT 'im=' || coalesce(mf.path, '-') || '>' || coalesce(rf.path, '-') AS r FROM imports i " +
            "LEFT JOIN files mf ON mf.id = i.file LEFT JOIN files rf ON rf.id = i.resolved_file")
        'import_names' = ("SELECT 'ib=' || coalesce(nf.path, '-') || ':' || n.local || '>' || coalesce(df.path, '-') || '#' || " +
            "coalesce(dc.name, '-') AS r FROM import_names n LEFT JOIN files nf ON nf.id = n.file " +
            "LEFT JOIN files df ON df.id = n.declared_file LEFT JOIN classes dc ON dc.id = n.declared_id")
    }
    # `--limit 0`: the map has more tables than a query prints by default.
    $names = Invoke-Gate --map-query $Db --width 0 --limit 0 --sql "SELECT 'table=' || name AS r FROM sqlite_master WHERE type = 'table'"
    Assert-Exit $names 0
    $present = @(foreach ($line in $names.Lines) { $t = $line.Trim(); if ($t.StartsWith('table=')) { $t.Substring(6) } })
    $sql = (@(foreach ($table in $parts.Keys) { if ($present -contains $table) { $parts[$table] } })) -join ' UNION ALL '
    # `--width 0`: a row longer than the default width is cut with `...[+N chars]`, and the comparison reads all of it.
    $r = Invoke-Gate --map-query $Db --width 0 --limit 0 --sql $sql
    Assert-Exit $r 0
    if (-not $Extends) { return @($r.Lines) }
    $base = ("(SELECT b.name || '@' || coalesce(bf.path, '-') FROM classes b LEFT JOIN files bf ON bf.id = b.file " +
        "WHERE b.id = $(Get-TsRootFieldSql 'classes__extends' 'c.id' 'id'))")
    $arg = ("(SELECT a.name || '@' || coalesce(af.path, '-') FROM classes__extends ta JOIN classes__extends ti ON ti.owner = ta.owner " +
        "AND ti.parent = ta.n JOIN classes a ON a.id = $(Get-TsFieldSql 'classes__extends' 'ti' 'id') " +
        "LEFT JOIN files af ON af.id = a.file WHERE ta.owner = c.id AND ta.parent = 0 AND ta.role = 'type_args' AND ti.ord = 0)")
    $e = Invoke-Gate --map-query $Db --width 0 --limit 0 --sql ("SELECT 'ext=' || c.name || '@' || coalesce(cf.path, '-') || '>' || " +
        "coalesce($base, '-') || '|' || coalesce($arg, '-') AS r FROM classes c LEFT JOIN files cf ON cf.id = c.file " +
        "WHERE $(Get-TsRootFieldSql 'classes__extends' 'c.id' 'text') IS NOT NULL")
    Assert-Exit $e 0
    return @($r.Lines) + @($e.Lines)
}

# The facts as one sorted text, so a partial run and a fresh full map compare by content.
function Get-TsBareText([string[]]$Facts) {
    $kinds = @('file=', 'cls=', 'ext=', 'ex=', 'im=', 'ib=')
    $kept = foreach ($line in $Facts) {
        $t = $line.Trim()
        foreach ($k in $kinds) { if ($t.StartsWith($k)) { $t; break } }
    }
    return ((@($kept) | Sort-Object) -join "`n")
}

# A partial run after `$Edit`, compared with a FULL map of the same edited tree in a copy of it.
function Assert-TsBarePartial([string]$Tree, [string]$Edit, [switch]$Extends) {
    $path = Join-Path $Tree $Edit
    Set-Content -LiteralPath $path -Value ((Get-Content -LiteralPath $path -Raw) + "// edited`n") -NoNewline
    $again = New-TsRowsDb $Tree
    Assert-Exit $again.Result 0
    Assert-Line $again.Result 'put back'
    $partial = Get-TsBareFacts $again.Db -Extends:$Extends
    $copy = "$Tree-fresh"
    Register-Tree $copy
    if (Test-Path $copy) { Remove-Item $copy -Recurse -Force }
    Copy-Item -LiteralPath $Tree -Destination $copy -Recurse
    Remove-Item (Join-Path $copy 'map.sqlite*') -Force -ErrorAction SilentlyContinue
    Remove-Item (Join-Path $copy '.fbt') -Recurse -Force -ErrorAction SilentlyContinue
    $full = New-TsRowsDb $copy
    Assert-Exit $full.Result 0
    Assert-NoLine $full.Result 'put back'
    Assert-Equal (Get-TsBareText $partial) (Get-TsBareText (Get-TsBareFacts $full.Db -Extends:$Extends)) "the rows after editing $Edit"
    # As lines, the shape `Assert-Line` reads.
    return [pscustomobject]@{ Lines = @($partial) }
}

Test-Case 'tsrows: a partial run in a workspace with no template keeps the file a kept barrel re-exports from' {
    $tree = New-TsBareWorkspace @{
        'apps/shop/src/base/alpha-base.ts' = "export class AlphaBase<T> {`n  item?: T;`n}`n"
        'apps/shop/src/base/index.ts' = "export * from './alpha-base';`n"
        'apps/shop/src/beta.ts' = "import { AlphaBase } from './base';`nexport const b = new AlphaBase();`n"
    }
    Assert-Exit (New-TsRowsDb $tree).Result 0
    $facts = Assert-TsBarePartial $tree 'apps/shop/src/base/alpha-base.ts'
    Assert-Line $facts 'ex=apps/shop/src/base/index.ts>apps/shop/src/base/alpha-base.ts'
    Assert-Line $facts 'ib=apps/shop/src/beta.ts:AlphaBase>apps/shop/src/base/alpha-base.ts#AlphaBase'
    Assert-Line $facts 'im=apps/shop/src/beta.ts>apps/shop/src/base/index.ts'
}

Test-Case 'tsrows: a partial run in a workspace with no template keeps every classes row and the file it names' {
    $tree = New-TsBareWorkspace @{
        'apps/shop/src/base/alpha-base.ts' = "export class AlphaBase<T> {`n  item?: T;`n}`n"
        'apps/shop/src/base/order.ts' = "export class Order {`n  id = 1;`n}`n"
        'apps/shop/src/decoy/alpha-base.ts' = "export class AlphaBase {}`nexport class Order {}`n"
        'apps/shop/src/beta.ts' = "import { AlphaBase as Base } from './base/alpha-base';`n" +
            "import { Order } from './base/order';`nexport class Beta extends Base<Order> {}`n"
        'apps/shop/src/gamma.ts' = "import { Beta } from './beta';`nexport class Gamma extends Beta {}`n" +
            "export class Delta extends Array<string> {}`n"
    }
    Assert-Exit (New-TsRowsDb $tree).Result 0
    foreach ($edit in @('apps/shop/src/base/alpha-base.ts', 'apps/shop/src/beta.ts')) {
        $facts = Assert-TsBarePartial $tree $edit -Extends
        # The classes of the files NOT read again are still there, in their own files.
        Assert-Line $facts 'cls=Order@apps/shop/src/base/order.ts'
        Assert-Line $facts 'cls=AlphaBase@apps/shop/src/decoy/alpha-base.ts'
        Assert-Line $facts 'cls=Order@apps/shop/src/decoy/alpha-base.ts'
        Assert-Line $facts 'ext=Beta@apps/shop/src/beta.ts>AlphaBase@apps/shop/src/base/alpha-base.ts|Order@apps/shop/src/base/order.ts'
        Assert-Line $facts 'ext=Gamma@apps/shop/src/gamma.ts>Beta@apps/shop/src/beta.ts|-'
        Assert-Line $facts 'ext=Delta@apps/shop/src/gamma.ts>-|-'
        Assert-NoLine $facts '@-'
    }
}

}
