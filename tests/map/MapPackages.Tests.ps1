<#
    TYPESCRIPT INSTALLED IN A SUBFOLDER: `web/package.json` and `web/node_modules/typescript`, gated and
    mapped from the folder above. The compiler was resolved from `--root` alone - node looks UP from a folder, never
    down into one - so every `.ts` file was UNMAPPED, the rules never ran, and the plain deep half had no compiler.

    NO NODE_PATH ANYWHERE HERE: the compiler is reachable ONLY from `web/`, as in the tree the issue came from. The
    install is a link to the suite cache's compiler. Its helpers are its own - `-Only MapPackages` runs it alone.
#>

# `web/` with its own package.json and a compiler linked in from `$Modules` - a junction on Windows, which needs no
# privilege, a symbolic link elsewhere. 7.x's native compiler comes as `@typescript/*` packages too.
function New-MapPackagesTree([string]$Modules, [hashtable]$Files) {
    $tree = Use-Tree ($Files + @{ 'web/package.json' = '{"name":"web","private":true}' })
    $installed = Join-Path $tree 'web/node_modules'
    New-Item -ItemType Directory -Path $installed | Out-Null
    foreach ($package in 'typescript', '@typescript') {
        $source = Join-Path $Modules $package
        if (-not (Test-Path $source)) { continue }
        $kind = if ($script:OnWindows) { 'Junction' } else { 'SymbolicLink' }
        New-Item -ItemType $kind -Path (Join-Path $installed $package) -Target $source | Out-Null
    }
    return $tree
}

function Invoke-MapPackagesGate {
    param([Parameter(ValueFromRemainingArguments)][object[]]$GateArgs)
    $previous = $env:NODE_PATH
    Remove-Item Env:NODE_PATH -ErrorAction SilentlyContinue
    try { return Invoke-Gate @GateArgs }
    finally { if ($null -ne $previous) { $env:NODE_PATH = $previous } }
}

$script:MapPackagesTs = Get-TsModules
$script:MapPackagesTs5 = Get-TypeScriptPath 'typescript@5'

if ($script:MapPackagesTs) {
    Test-Case 'map: TypeScript installed in a subfolder maps the files under it, from the folder above' {
        $tree = New-MapPackagesTree $script:MapPackagesTs @{
            'web/src/a.ts' = "export const A = 1;`n"
            'web/src/b.ts' = "import { A } from './a';`nexport const B = A + 1;`n"
        }
        $out = Join-Path $tree 'm.json'
        $result = Invoke-MapPackagesGate --root $tree --ext .ts --map --map-out $out --map-check
        Assert-NoLine $result 'UNMAPPED'
        Assert-NoLine $result 'could not be resolved'
        $map = Get-Content $out -Raw | ConvertFrom-Json
        $edge = @($map.imports.PSObject.Properties | Where-Object { $_.Name -eq 'web/src/b.ts' } | ForEach-Object { $_.Value })
        Assert-Equal ($edge -contains 'web/src/a.ts') $true 'the import edge b -> a'
    }

    Test-Case 'ts: the rules run with the compiler of the subfolder a file sits in' {
        $tree = New-MapPackagesTree $script:MapPackagesTs @{ 'web/src/a.ts' = "export const A: any = 1;`n" }
        $result = Invoke-MapPackagesGate --root $tree --ts-discipline --no-gate-cache
        Assert-Exit $result 1
        Assert-Line $result 'web/src/a.ts:1'
        Assert-NoLine $result 'could not be resolved'
    }

    Test-Case 'ts: a folder with no compiler beside folders with one names its own files, and only those' {
        $tree = New-MapPackagesTree $script:MapPackagesTs @{
            'web/src/a.ts'      = "export const A = 1;`n"
            'tools/package.json' = '{"name":"tools","private":true}'
            'tools/x.ts'        = "export const X = 1;`n"
        }
        $result = Invoke-MapPackagesGate --root $tree --ts-discipline --no-gate-cache
        Assert-Exit $result 1
        Assert-Line $result 'tools/x.ts:1'
        Assert-NoLine $result 'web/src/a.ts'
    }
}

if ($script:MapPackagesTs5) {
    Test-Case 'deep: the plain TypeScript half borrows the compiler of the subfolder its files sit in' {
        $tree = New-MapPackagesTree $script:MapPackagesTs5 @{ 'web/src/a.ts' = "export function one(): number { return 1; }`n" }
        $db = Join-Path $tree 'map.sqlite'
        $run = Invoke-MapPackagesGate --root $tree --ext .ts --map-sqlite $db
        Assert-Exit $run 0
        Assert-NoLine $run 'no typescript to borrow'
        $rows = Invoke-Gate --map-query $db --sql "SELECT count(*) FROM files WHERE path = 'web/src/a.ts'"
        Assert-Line $rows '1'
    }
}
