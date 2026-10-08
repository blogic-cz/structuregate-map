<#
    `import_names`: one row per name an import binds - `local` and `imported` in the source's order - and the file
    that DECLARES it, through every barrel on the way (`via`). A third of a real frontend's named imports name a
    barrel, so `imports.resolved` alone stops one hop short. Its helpers are `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '../TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

# One barrel with every shape a name comes through, and a star barrel inside it.
$script:TsImportsLib = @{
    'apps/shop/src/lib/x.ts' = "export const X = 1;`n"
    'apps/shop/src/lib/x2.ts' = "export const Q = 0;`n"
    'apps/shop/src/lib/y.ts' = "export class Y {}`n"
    'apps/shop/src/lib/w.ts' = "export enum W { A }`n"
    'apps/shop/src/lib/inner/z.ts' = "export interface Z { id: number }`n"
    'apps/shop/src/lib/inner/index.ts' = "export * from './z';`n"
    'apps/shop/src/lib/index.ts' = "export * from './x';`nexport * from './x2';`nexport { Y } from './y';`n" +
        "import { W } from './w';`nexport { W };`nexport * from './inner';`n"
    'apps/shop/src/uses-lib.ts' = "import { X, Y as Why, W, Z } from './lib';`n" +
        "export const all = [X, Why, W];`nexport type T = Z;`n"
}

function Get-TsImportNames([string]$Db) {
    Invoke-TsRowsQ $Db ("SELECT local || '|' || imported || '|' || coalesce(declared, '-') AS r FROM import_names " +
        "WHERE file = (SELECT id FROM files WHERE path = 'apps/shop/src/uses-lib.ts') ORDER BY local")
}

# EVERY SHAPE LANDS ON THE DECLARATION: a star, a named re-export (renamed on the way in), an import that the
# barrel exports again, and a star inside a star.
Test-Case 'tsrows: an imported name is traced through its barrel to the file that declares it' {
    $tree = New-TsRowsWorkspace $script:TsImportsLib
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    $r = Get-TsImportNames $made.Db
    Assert-Line $r 'X|X|apps/shop/src/lib/x.ts'
    Assert-Line $r 'Why|Y|apps/shop/src/lib/y.ts'
    Assert-Line $r 'W|W|apps/shop/src/lib/w.ts'
    Assert-Line $r 'Z|Z|apps/shop/src/lib/inner/z.ts'
    # THE ROW, not only the file: `Why` is the class `Y`.
    $id = Invoke-TsRowsQ $made.Db ("SELECT 'cls=' || c.name AS r FROM import_names n JOIN classes c ON c.id = n.declared_id " +
        "WHERE n.local = 'Why'")
    Assert-Line $id 'cls=Y'
    # AND THE BARRELS ON THE WAY, which is what a partial run re-reads the importer for.
    $via = Invoke-TsRowsQ $made.Db "SELECT 'via=' || via AS r FROM import_names WHERE local = 'Z'"
    Assert-Line $via 'apps/shop/src/lib/inner/index.ts'
    # `[from]`, not `\"from\"`: a double quote inside a native argument survives 5.1 and pwsh 7 differently.
    $ex = Invoke-TsRowsQ $made.Db ("SELECT 'ex=' || e.[from] || '>' || f.path AS r FROM exports e JOIN files f ON f.id = e.resolved_file " +
        "WHERE e.[from] = './x'")
    Assert-Line $ex 'ex=./x>apps/shop/src/lib/x.ts'
}

# A PARTIAL RUN MOVES THE DECLARATION with the file that moved it - neither the barrel nor the importer changed.
Test-Case 'tsrows: a name that moves to another file behind the same barrel is traced again' {
    $tree = New-TsRowsWorkspace $script:TsImportsLib
    Assert-Exit (New-TsRowsDb $tree).Result 0
    Set-Content -LiteralPath (Join-Path $tree 'apps/shop/src/lib/x.ts') -Value 'export const Old = 1;'
    Set-Content -LiteralPath (Join-Path $tree 'apps/shop/src/lib/x2.ts') -Value "export const X = 1;`nexport const Q = 0;"
    $again = New-TsRowsDb $tree
    Assert-Line $again.Result 'put back'
    Assert-Line (Get-TsImportNames $again.Db) 'X|X|apps/shop/src/lib/x2.ts'
}

# A NAME NO ROW ANSWERS TO - a namespace has no table - has no `declared_id` to be an edge of its own, so only
# the declaring FILE ties the importer to it. It moves; the importer must be read again all the same.
Test-Case 'tsrows: a name with no row of its own still follows its declaring file' {
    $files = $script:TsImportsLib.Clone()
    $files['apps/shop/src/lib/ns.ts'] = "export namespace N { export const v = 1; }`n"
    $files['apps/shop/src/lib/ns2.ts'] = "export const other = 2;`n"
    $files['apps/shop/src/lib/index.ts'] += "export * from './ns';`nexport * from './ns2';`n"
    $files['apps/shop/src/uses-ns.ts'] = "import { N } from './lib';`nexport const v = N.v;`n"
    $tree = New-TsRowsWorkspace $files
    Assert-Exit (New-TsRowsDb $tree).Result 0
    Set-Content -LiteralPath (Join-Path $tree 'apps/shop/src/lib/ns.ts') -Value 'export const gone = 1;'
    Set-Content -LiteralPath (Join-Path $tree 'apps/shop/src/lib/ns2.ts') -Value "export const other = 2;`nexport namespace N { export const v = 1; }"
    $again = New-TsRowsDb $tree
    Assert-Line $again.Result 'put back'
    $r = Invoke-TsRowsQ $again.Db ("SELECT 'n=' || coalesce(declared, '-') AS r FROM import_names WHERE local = 'N' " +
        "AND file = (SELECT id FROM files WHERE path = 'apps/shop/src/uses-ns.ts')")
    Assert-Line $r 'n=apps/shop/src/lib/ns2.ts'
}

# ...AND A BARREL IN THE MIDDLE, re-pointed. Only `inner/index.ts` changes; the importer names neither it nor
# the file it now points at.
Test-Case 'tsrows: a re-pointed barrel in the middle of the chain is traced again' {
    $tree = New-TsRowsWorkspace ($script:TsImportsLib + @{ 'apps/shop/src/lib/inner/z2.ts' = "export interface Z { id: string }`n" })
    Assert-Exit (New-TsRowsDb $tree).Result 0
    Set-Content -LiteralPath (Join-Path $tree 'apps/shop/src/lib/inner/index.ts') -Value "export * from './z2';"
    $again = New-TsRowsDb $tree
    Assert-Line $again.Result 'put back'
    Assert-Line (Get-TsImportNames $again.Db) 'Z|Z|apps/shop/src/lib/inner/z2.ts'
}

}
