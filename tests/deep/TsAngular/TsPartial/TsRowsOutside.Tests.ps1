<#
    A FILE NO BUILD PROGRAM REACHES IS PARSED (#120, `src/TsRows/TsSetup/TsOutside.mjs`): after every real program, the `.ts`
    files left are walked through one more program per project, under that project's tsconfig - full rows, `reachable` 0, the
    project the one whose config it borrowed (no `projects` row of its own), the `ts_not_in_any_program` diagnostic kept. A partial run that moves a file in or out of a
    program gives what a full map of the same tree gives. Its helpers are `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '../../TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

$script:OutsideFiles = @{
    'angular.json' = '{"projects":{"shop":{"projectType":"application","root":"apps/shop","sourceRoot":"apps/shop/src",' +
        '"architect":{"build":{"options":{"tsConfig":"apps/shop/tsconfig.app.json"}}}}}}'
    'apps/shop/tsconfig.json' = '{"references":[{"path":"./tsconfig.app.json"}]}'
    'apps/shop/tsconfig.app.json' = '{"compilerOptions":{"strict":true},"files":["src/main.ts"]}'
    'apps/shop/src/main.ts' = "import { UsedDto } from './dtos/used.model';`nexport const x: UsedDto = { id: 1 };`n"
    'apps/shop/src/dtos/used.model.ts' = "export class UsedDto { id = 0; }`n"
    'apps/shop/src/dtos/orphan.model.ts' = "import { UsedDto } from './used.model';`nexport class OrphanDto extends UsedDto { note = ''; }`n"
    'apps/shop/src/dtos/orphan.d.ts' = "export declare class Typed { n: number; }`n"
}

# What the rows say about each `.ts` file, ids replaced by names: reachable, project, the classes it declares, each one's base row.
function Get-TsOutsideFacts([string]$Db) {
    $r = Invoke-TsRowsQ $Db ("SELECT 'f=' || f.path || '|' || coalesce(f.reachable, '-') || '|' || coalesce(p.name, '-') || '|' || " +
        "coalesce((SELECT group_concat(c.name || '<' || coalesce(b.name, '-'), ',') FROM classes c LEFT JOIN classes__extends x ON x.owner = c.id AND x.parent = 0 AND x.role = 'id' LEFT JOIN classes b ON b.id = x.value WHERE c.file = f.id), '-') AS r " +
        "FROM files f LEFT JOIN projects p ON p.id = f.project WHERE f.ext = 'ts' ORDER BY f.path")
    return @($r.Lines | Where-Object { $_.Trim().StartsWith('f=') } | ForEach-Object { $_.Trim() })
}

Test-Case 'tsrows: a file no build program reaches is parsed, unreachable, under its project (#120)' {
    $made = New-TsRowsDb (Use-Tree $script:OutsideFiles)
    Assert-Exit $made.Result 0
    $facts = Get-TsOutsideFacts $made.Db
    # THE CHECKER ANSWERS THERE TOO: the base class is the in-program file's row.
    Assert-Equal ($facts -contains 'f=apps/shop/src/dtos/orphan.model.ts|0|shop|OrphanDto<UsedDto') $true ($facts -join "`n")
    Assert-Equal ($facts -contains 'f=apps/shop/src/dtos/used.model.ts|1|shop|UsedDto<-') $true ($facts -join "`n")
    # A DECLARATION FILE IS NOT WALKED, in a program or out of one.
    Assert-Equal ($facts -contains 'f=apps/shop/src/dtos/orphan.d.ts|0|-|-') $true ($facts -join "`n")
    $d = Invoke-TsRowsQ $made.Db "SELECT kind || ' ' || file AS problem FROM diagnostics"
    Assert-Line $d 'ts_not_in_any_program apps/shop/src/dtos/orphan.model.ts'
}

Test-Case 'tsrows: a partial run that moves a file into a program and out again maps what a full run does (#120)' {
    $tree = Use-Tree $script:OutsideFiles
    # THE DATABASES BESIDE THE TREE, NOT IN IT: a run's `.last-run.jsonl` in the tree is a new file the next run reads everything for.
    $dbs = Use-Tree @{ 'keep.txt' = '' }
    $db = Join-Path $dbs 'map.sqlite'
    $map = { param($out) Invoke-Gate --root $tree --map-sqlite $out --ts-node-modules $script:TsRowsModules }
    Assert-Exit (& $map $db) 0
    $main = Join-Path $tree 'apps/shop/src/main.ts'
    $states = @(
        "import { UsedDto } from './dtos/used.model';`nimport { OrphanDto } from './dtos/orphan.model';`nexport const x: UsedDto = { id: 1 };`nexport const y = new OrphanDto();`n",
        "import { UsedDto } from './dtos/used.model';`nexport const x: UsedDto = { id: 1 };`n")
    $n = 0
    foreach ($text in $states) {
        Set-Content -LiteralPath $main -Value $text -NoNewline
        $partial = & $map $db
        Assert-Exit $partial 0
        Assert-Line $partial 'is reading 1 file(s)'
        $n += 1
        $fresh = Join-Path $dbs "fresh$n.sqlite"
        Assert-Exit (& $map $fresh) 0
        Assert-Equal ((Get-TsOutsideFacts $db) -join "`n") ((Get-TsOutsideFacts $fresh) -join "`n") 'partial vs full'
    }
}

}
