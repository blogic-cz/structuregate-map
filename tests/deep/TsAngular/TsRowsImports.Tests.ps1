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

# A BARE `import('...')` / `require('...')` IS AN IMPORT TOO: it left only a `calls` row, so a file reached through a
# lazy chunk or a required asset had no importer and read as dead.
Test-Case 'tsrows: a dynamic import and a require leave an imports row with the file they resolve to' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/boot.ts' = "export const boot = 1;`n"
        'apps/shop/src/dyn.ts' = "declare const require: (p: string) => unknown;`nexport const go = () => import('./boot');`n" +
            "const data = require('./assets/locales/en.json');`nexport const d = data;`n"
    }
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    # `--width 0`: the line is longer than the default column and would be cut.
    $r = Invoke-Gate --map-query $made.Db --width 0 --sql ("SELECT module || ' | ' || dynamic || ' | ' || coalesce(resolved, '') AS r " +
        "FROM imports WHERE dynamic IS NOT NULL")
    Assert-Exit $r 0
    Assert-Line $r './boot | import | apps/shop/src/boot.ts'
    Assert-Line $r './assets/locales/en.json | require | apps/shop/src/assets/locales/en.json'
}

# DECLARATIONS INSIDE `declare module` / `declare global` / a namespace were skipped by the statement loop.
Test-Case 'tsrows: an interface or alias inside declare module, declare global and a namespace is a row with its module' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/boot.ts' = "export const boot = 1;`n"
        'apps/shop/src/decl.ts' = "export {};`ndeclare module './boot' { interface PluginOptionsByType { a: number } export interface Extra { b: string } }`n" +
            "declare global { interface DemoWindow { z: number } }`n" +
            "export namespace Demo { export interface Inner { y: number } export type Alias = string; }`n"
    }
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    $i = Invoke-TsRowsQ $made.Db "SELECT name || ' | ' || in_module AS r FROM interfaces WHERE in_module IS NOT NULL"
    Assert-Line $i 'PluginOptionsByType | ./boot'
    Assert-Line $i 'Extra | ./boot'
    Assert-Line $i 'DemoWindow | global'
    Assert-Line $i 'Inner | Demo'
    $a = Invoke-TsRowsQ $made.Db "SELECT name || ' | ' || in_module AS r FROM type_aliases WHERE in_module IS NOT NULL"
    Assert-Line $a 'Alias | Demo'
}

# A NAMESPACE'S CLASS, FUNCTION AND CONST ARE ROWS like a file's; an AMBIENT block's only declare, and are not.
Test-Case 'tsrows: a class, function and const inside a namespace are rows; an ambient block''s are not' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/ns.ts' = "export namespace Demo {`n  export class Order { total(): number { return 1; } }`n" +
            "  export function makeOrder(): Order { return new Order(); }`n  export const alphaLimit = 3;`n}`n" +
            "declare global { function ambientAlpha(): void; }`n"
    }
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    $r = Invoke-TsRowsQ $made.Db ("SELECT 'class ' || name AS r FROM classes WHERE name = 'Order' UNION ALL " +
        "SELECT 'function ' || name FROM functions WHERE name IN ('makeOrder', 'ambientAlpha') UNION ALL " +
        "SELECT 'const ' || name FROM consts WHERE name = 'alphaLimit'")
    Assert-Line $r 'class Order'
    Assert-Line $r 'function makeOrder'
    Assert-Line $r 'const alphaLimit'
    Assert-NoLine $r 'function ambientAlpha'
}

}
