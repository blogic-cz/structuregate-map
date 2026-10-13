<#
    The PLAIN TypeScript half beside an Angular workspace: a React app or an end-to-end suite that sits in a
    folder of its own next to `web/` got no rows, because the plain half ran only where there was no workspace.
    It maps every script file OUTSIDE the workspace, and the Angular half keeps its own.

    Its helpers are its own - `-Only TsPlainBeside` runs this suite alone. The compiler cache is the one TsRows fills.
#>

function Get-TsBesideModules {
    $modules = Join-Path ([System.IO.Path]::GetTempPath()) 'sgtest-tsrows\node_modules'
    if (-not (Get-Command node -ErrorAction SilentlyContinue)) { return $null }
    if ((Test-Path (Join-Path $modules 'typescript\package.json')) -and
        (Test-Path (Join-Path $modules '@angular\compiler\package.json'))) { return $modules }
    if (-not (Get-Command npm -ErrorAction SilentlyContinue)) { return $null }
    $cache = Split-Path $modules
    [void](New-Item -ItemType Directory -Path $cache -Force)
    & npm install --no-save --silent --prefix $cache 'typescript@5' '@angular/compiler@17' '@angular/core@17' '@angular/router@17' '@ngrx/store@17' 2>&1 | Out-Null
    if (Test-Path (Join-Path $modules '@angular\compiler\package.json')) { return $modules }
    return $null
}

$script:TsBesideModules = Get-TsBesideModules
if (-not $script:TsBesideModules) { Write-Host '    (no node/npm-installed typescript + @angular/compiler - the plain-beside-workspace cases are not run)' }

function New-TsBesideTree {
    return Use-Tree @{
        'web/angular.json' = '{"projects":{"shop":{"projectType":"application","root":"apps/shop",' +
            '"sourceRoot":"apps/shop/src","architect":{"build":{"options":{"tsConfig":"apps/shop/tsconfig.app.json"}},' +
            '"test":{"options":{"tsConfig":"apps/shop/tsconfig.spec.json"}},"lint":{}}}}}'
        'web/apps/shop/tsconfig.json'     = '{"references":[{"path":"./tsconfig.app.json"}]}'
        'web/apps/shop/tsconfig.app.json' = '{"compilerOptions":{"strict":true},"include":["src/**/*.ts"]}'
        'web/apps/shop/src/main.ts'       = "export const started = true;`n"
        'admin/src/price.ts'  = "export function total(items: number[]): number {`n    let sum = 0;`n" +
                                "    for (const x of items) sum += x;`n    return sum;`n}`nexport const RATE = 10;`n"
        'admin/src/basket.ts' = "import { total as sumOf } from './price';`nimport * as P from './price';`n" +
                                "export function basket(items: number[]) {`n    return sumOf(items) * P.RATE + P.total(items);`n}`n" +
                                "export function other(sumOf: (x: number[]) => number) {`n    return sumOf([1, 2]);`n}`n"
    }
}

function Invoke-TsBesideMap([string]$Tree) {
    $run = Invoke-Gate --root $Tree --ext '.ts' --map-sqlite (Join-Path $Tree 'map.sqlite') --ts-node-modules $script:TsBesideModules
    Assert-Exit $run 0
    return $run
}

function Get-TsBesideRows([string]$Tree, [string]$Sql) {
    $r = Invoke-Gate --map-query (Join-Path $Tree 'map.sqlite') --width 0 --sql $Sql
    Assert-Exit $r 0
    return $r
}

if ($script:TsBesideModules) {

Test-Case 'tsplain beside: a script file outside the Angular workspace gets plain rows, and none inside it does' {
    $tree = New-TsBesideTree
    Invoke-TsBesideMap $tree | Out-Null
    $count = Get-TsBesideRows $tree "SELECT 'n=' || count(*) FROM files WHERE lang = 'ts'"
    Assert-Line $count 'n=2'
    $paths = Get-TsBesideRows $tree "SELECT 'p=' || path FROM files WHERE lang = 'ts'"
    Assert-Line $paths 'p=admin/src/basket.ts'
    Assert-Line $paths 'p=admin/src/price.ts'
    $calls = Get-TsBesideRows $tree ("SELECT 'call=' || func || '|' || callee || '|' || target_path || '|' || target_name " +
        "FROM calls c JOIN files f ON f.id = c.file WHERE f.lang = 'ts'")
    Assert-Line $calls 'call=basket|sumOf|admin/src/price.ts|total'
    # THE ANGULAR HALF STILL RAN over its own workspace, and the plain half did not touch it.
    $main = Get-TsBesideRows $tree "SELECT 'm=' || count(*) FROM files WHERE lang = 'typescript' AND path LIKE '%main.ts'"
    Assert-Line $main 'm=1'
    $inside = Get-TsBesideRows $tree "SELECT 'i=' || count(*) FROM files WHERE lang = 'ts' AND (path LIKE 'web/%' OR path LIKE 'apps/%')"
    Assert-Line $inside 'i=0'
}

Test-Case 'tsplain beside: the plain rows go when the last file outside the workspace goes' {
    $tree = New-TsBesideTree
    Invoke-TsBesideMap $tree | Out-Null
    Remove-Item -Recurse -Force (Join-Path $tree 'admin')
    Invoke-TsBesideMap $tree | Out-Null
    $count = Get-TsBesideRows $tree "SELECT 'n=' || count(*) FROM files WHERE lang = 'ts'"
    Assert-Line $count 'n=0'
}

Test-Case 'tsplain beside: the compiler the workspace installed is borrowed for the files outside it' {
    # THE ONLY TYPESCRIPT IS THE WORKSPACE'S: no --ts-node-modules, nothing at the root. The plain half said
    # "no typescript to borrow" beside a workspace that held one, and the files outside it got no rows.
    $tree = New-TsBesideTree
    [void](New-Item -ItemType SymbolicLink -Path (Join-Path $tree 'web/node_modules') -Target $script:TsBesideModules)
    $run = Invoke-Gate --root $tree --ext '.ts' --map-sqlite (Join-Path $tree 'map.sqlite')
    Assert-Exit $run 0
    $count = Get-TsBesideRows $tree "SELECT 'n=' || count(*) FROM files WHERE lang = 'ts'"
    Assert-Line $count 'n=2'
}

}
