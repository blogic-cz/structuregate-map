<#
    The TypeScript half goes TO THE END: an import is traced through a chain of barrels of any length (a cut at sixteen
    left the name declared nowhere), and a template expression's `ast` is whole however deep (a cut at sixty levels
    left a null where the rest of the tree was). Its helpers are `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '../../TsRows.Helpers.ps1')
. (Join-Path $PSScriptRoot '../../nodes/NodeSql.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

Test-Case 'tsrows: a name re-exported through twenty-one star barrels is traced to the file that declares it' {
    $extra = @{
        'apps/shop/src/chain/b21.ts' = "export const Deep = 1;`n"
        'apps/shop/src/uses-chain.ts' = "import { Deep } from './chain/b0';`nexport const d = Deep;`n"
    }
    for ($i = 0; $i -le 20; $i++) { $extra["apps/shop/src/chain/b$i.ts"] = "export * from './b$($i + 1)';`n" }
    $made = New-TsRowsDb (New-TsRowsWorkspace $extra)
    Assert-Exit $made.Result 0
    Assert-Line (Invoke-TsRowsQ $made.Db "SELECT 'd=' || coalesce(declared, '-') AS r FROM import_names WHERE local = 'Deep'") 'd=apps/shop/src/chain/b21.ts'
}

Test-Case 'tsrows: two barrels exporting each other still map, and the name lands where it is declared' {
    $made = New-TsRowsDb (New-TsRowsWorkspace @{
        'apps/shop/src/ring/left.ts' = "export * from './right';`n"
        'apps/shop/src/ring/right.ts' = "export * from './left';`nexport const Real = 1;`n"
        'apps/shop/src/uses-ring.ts' = "import { Real } from './ring/left';`nexport const r = Real;`n"
    })
    Assert-Exit $made.Result 0
    Assert-Line (Invoke-TsRowsQ $made.Db "SELECT 'r=' || coalesce(declared, '-') AS r FROM import_names WHERE local = 'Real'") 'r=apps/shop/src/ring/right.ts'
}

Test-Case 'tsrows: a template expression eighty terms deep keeps its whole ast' {
    $chain = (1..80 | ForEach-Object { 'n' }) -join ' + '
    $made = New-TsRowsDb (New-TsRowsWorkspace @{
        'apps/shop/src/deep.component.html' = "<b>{{ $chain }}</b>`n"
        'apps/shop/src/deep.component.ts' = "import { Component } from '@angular/core';`n" +
            "@Component({ selector: 'app-deep', templateUrl: './deep.component.html', standalone: true })`n" +
            "export class DeepComponent {`n  n = 1;`n}`n"
    })
    Assert-Exit $made.Result 0
    $deepest = Invoke-TsRowsQ $made.Db ("SELECT 'binary=' || max(n) AS r FROM (SELECT count(*) n FROM expressions__ast " +
        "WHERE k = 'Binary' GROUP BY owner)")
    Assert-Line $deepest 'binary=79'
}

Test-Case 'tsrows: no cell of the Angular map is JSON text - every list and object is node rows' {
    $made = New-TsRowsDb (New-TsRowsWorkspace)
    Assert-Exit $made.Result 0
    Assert-Line (Invoke-TsRowsQ $made.Db "SELECT 'ast=' || (count(*) > 0) AS r FROM expressions__ast") 'ast=1'
    $found = Get-NodeJsonCell $made.Db
    if ($found) { throw "$($found.Count) JSON cell(s):`n$($found -join "`n")" }
}

}
