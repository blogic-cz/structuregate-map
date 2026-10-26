<#
    WHAT `_meta` COUNTS IS WHAT THE TABLES HOLD (#134): `files:<lang>` is the files a half's rows list - a write that carries
    no shas (the TypeScript closure) once set it to 0 - and `_meta.rows` and the printed `database N row(s)` are counted after
    the last pass, so the tables the passes make are in them on a fresh build too. Its helpers are `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '../TsRows.Helpers.ps1')

# Every row the map holds, table by table as `_meta.rows` counts them: no `_` table, no FTS shadow table.
function Get-MapCountsTotal([string]$Db) {
    $names = Invoke-Gate --map-query $Db --width 0 --limit 0 --sql "SELECT 't=' || name AS t FROM sqlite_master WHERE type = 'table' AND name NOT LIKE '\_%' ESCAPE '\' AND name NOT LIKE 'file\_text\_%' ESCAPE '\'"
    $total = 0
    foreach ($name in @($names.Lines | Where-Object { $_.Trim().StartsWith('t=') } | ForEach-Object { $_.Trim().Substring(2) })) {
        $c = Invoke-Gate --map-query $Db --width 0 --sql "SELECT 'c=' || count(*) AS c FROM `"$name`""
        $total += [int](@($c.Lines | Where-Object { $_.Trim().StartsWith('c=') })[0].Trim().Substring(2))
    }
    return $total
}

if ($script:TsRowsModules -and $script:TsRowsPython) {

Test-Case 'mapcounts: _meta.rows, the printed total and files:typescript are what the tables hold, on a fresh build and after (#134)' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/hue.ts' = "export enum Hue { Red = 1, Green = 2 }`nexport function label(h: Hue): string { switch (h) { case Hue.Red: return 'r'; default: return '?'; } }`n"
        'apps/shop/src/main.ts' = "import { label, Hue } from './hue';`nconsole.log(label(Hue.Green));`n"
    }
    $db = Join-Path $tree 'map.sqlite'
    $gateArgs = @('--root', $tree, '--map-sqlite', $db, '--ts-node-modules', $script:TsRowsModules)
    foreach ($run in 'fresh', 'again') {
        $r = Invoke-Gate @gateArgs
        Assert-Exit $r 0
        $total = Get-MapCountsTotal $db
        $m = Invoke-Gate --map-query $db --width 0 --sql ("SELECT 'm=' || (SELECT value FROM _meta WHERE key = 'rows') || '|' || (SELECT value FROM _meta WHERE key = 'files:typescript') || '|' || " +
            "(SELECT count(*) FROM files WHERE lang = 'typescript') AS m")
        $line = @($m.Lines | Where-Object { $_.Trim().StartsWith('m=') })[0].Trim().Substring(2).Split('|')
        if ([int]$line[2] -lt 1) { throw "the $run run wrote no typescript files row - the case proves nothing" }
        Assert-Equal $line[1] $line[2] "files:typescript after the $run run"
        Assert-Equal $line[0] "$total" "_meta.rows after the $run run"
        $printed = @($r.Lines | Where-Object { $_.Contains('database ') -and $_.Contains(' row(s) in ') })
        if ($printed.Count -ne 1 -or -not $printed[0].Contains(" $total row(s) in ")) { throw "the $run run printed $printed, the tables hold $total" }
    }
}

}
