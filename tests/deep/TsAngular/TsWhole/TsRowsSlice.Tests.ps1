<#
    A SLICE OF THE DEEP MAP (#111, `rust/fbtcore/src/slice/`): `--map-slice <base> --slice-files <file|@list> [--slice-hops N]
    --map-sqlite <out>` keeps the listed files and the files whose rows a RESOLVED link points into them, N hops deep, with
    every per-file row of those and every whole-program table whole; `_meta` `partial` and `files.sliced` mark it, a query
    says so, and no map run writes into it. Its helpers are `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '../../TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

$script:TsSliceFiles = @{
    'apps/shop/src/total.ts' = "export function total(n: number): number { return n + 1; }`n"
    'apps/shop/src/caller.ts' = "import { total } from './total';`nexport function twice(): number { return total(1) * 2; }`n"
    'apps/shop/src/outer.ts' = "import { twice } from './caller';`nexport const v = twice();`n"
    'apps/shop/src/loner.ts' = "import { HttpClient } from '@angular/common/http';`nexport class LonerService {`n  constructor(private http: HttpClient) {}`n  go() { return this.http.get('/api/loner'); }`n}`n"
}

function Get-TsSliced([string]$Db) {
    return Invoke-TsRowsQ $Db "SELECT 'in=' || path || '|' || sliced AS r FROM files WHERE path LIKE '%.ts'"
}

Test-Case 'tsrows: a slice keeps the listed file and the files whose links point into it, hop by hop (#111)' {
    $made = New-TsRowsDb (New-TsRowsWorkspace $script:TsSliceFiles) -Shared
    Assert-Exit $made.Result 0
    # Beside the shared map, never in its tree: the harness freezes a shared tree.
    $at = New-Tree @{ 'keep.txt' = 'x' }
    $one = Join-Path $at 'one.sqlite'
    $two = Join-Path $at 'two.sqlite'
    Assert-Exit (Invoke-Gate --map-slice $made.Db --slice-files 'apps/shop/src/total.ts' --map-sqlite $one) 0
    $list = Join-Path $at 'changed.txt'
    Set-Content -LiteralPath $list -Value "# the pull request`napps/shop/src/total.ts`n"
    Assert-Exit (Invoke-Gate --map-slice $made.Db --slice-files "@$list" --slice-hops 2 --map-sqlite $two) 0
    $r = Get-TsSliced $one
    Assert-Line $r 'in=apps/shop/src/total.ts|1'
    Assert-Line $r 'in=apps/shop/src/caller.ts|1'
    Assert-Line $r 'in=apps/shop/src/outer.ts|0'
    Assert-Line $r 'in=apps/shop/src/loner.ts|0'
    $r = Get-TsSliced $two
    Assert-Line $r 'in=apps/shop/src/outer.ts|1'
    Assert-Line $r 'in=apps/shop/src/loner.ts|0'
    # PER-FILE ROWS of the sliced files only; a node table keeps its rows' nodes; a whole-program table whole.
    $calls = Invoke-TsRowsQ $one "SELECT 'call=' || c.callee || '|' || f.path AS r FROM calls c JOIN files f ON f.id = c.file"
    Assert-Line $calls 'call=total|apps/shop/src/caller.ts'
    Assert-NoLine $calls 'call=twice'
    $count = { param($db, $sql) ((Invoke-TsRowsQ $db $sql).Lines | Where-Object { $_ -match '^\s*\d+\s*$' } | Select-Object -First 1).Trim() }
    Assert-Equal (& $count $one "SELECT count(*) FROM functions__params p WHERE p.owner NOT IN (SELECT id FROM functions)") '0' 'no orphan node rows'
    # A TABLE KEEPING `file` AS A PATH, with no `owner_file`: the unsliced file's request stays out.
    Assert-Equal (& $count $made.Db "SELECT count(*) FROM http_calls WHERE url = '/api/loner'") '1' 'the base map holds the request'
    Assert-Equal (& $count $one "SELECT count(*) FROM http_calls WHERE url = '/api/loner'") '0' 'an unsliced file keeps its rows out'
    Assert-Equal (& $count $one "SELECT count(*) FROM ts_deps") (& $count $made.Db "SELECT count(*) FROM ts_deps") 'a whole-program table is whole'
    Assert-Line (Invoke-TsRowsQ $one "SELECT 'meta=' || key || '|' || value AS r FROM _meta WHERE key IN ('partial', 'slice:hops')") 'meta=partial|1'
}

# A SLICE SAYS SO, AND IS NO CACHE: every query notes it on stderr, a map run refuses to write into it, and a path the base map does not hold fails.
Test-Case 'tsrows: a slice is marked on every query, refused as a map database, and refuses an unknown file (#111)' {
    $made = New-TsRowsDb (New-TsRowsWorkspace $script:TsSliceFiles)
    Assert-Exit $made.Result 0
    $one = Join-Path $made.Tree 'one.sqlite'
    Assert-Exit (Invoke-Gate --map-slice $made.Db --slice-files 'apps/shop/src/total.ts' --map-sqlite $one) 0
    Assert-Line (Invoke-Gate --map-query $one --sql "SELECT 1") 'partial map: 2 of'
    $again = Invoke-Gate --root $made.Tree --map-sqlite $one --ts-node-modules $script:TsRowsModules
    Assert-Line $again 'is a partial map cut by --map-slice'
    if ($again.Exit -eq 0) { throw 'a map run wrote into a slice' }
    $bad = Invoke-Gate --map-slice $made.Db --slice-files 'apps/shop/src/nope.ts' --map-sqlite (Join-Path $made.Tree 'bad.sqlite')
    Assert-Exit $bad 2
    Assert-Line $bad 'not a file of the base map: apps/shop/src/nope.ts'
}

}
