<#
    A SLICE OF THE DEEP MAP (#111, `rust/fbtcore/src/slice/`): `--map-slice <base> --slice-files <file|@list> [--slice-hops N]
    --map-sqlite <out>` keeps the listed files and the files whose rows a RESOLVED link points into them, N hops deep, with
    every per-file row of those and every whole-program table whole; `_meta` `partial` and `files.sliced` mark it, a query
    says so, and no map run writes into it. A SLIM MAP (`slice/slim.rs`, `--map-slim <base> --slim-config <json> --map-sqlite <out>`)
    keeps every file and only the tables, rows and derived tables its config picks. Its helpers are `TsRows.Helpers.ps1`.
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
    # THE TRANSLATION-KEY CLOSURE is left out by default - table and node tables - and said so; `--slice-closure` keeps it whole.
    Assert-Equal (& $count $made.Db "SELECT count(*) FROM sqlite_master WHERE name = 'key_reach'") '1' 'the base map holds the closure'
    Assert-Equal (& $count $one "SELECT count(*) FROM sqlite_master WHERE name LIKE 'key\_reach%' ESCAPE '\'") '0' 'the closure is left out'
    Assert-Line (Invoke-Gate --map-query $one --sql "SELECT 1") 'left out (--slice-closure keeps them): '
    $closed = Join-Path $at 'closed.sqlite'
    Assert-Exit (Invoke-Gate --map-slice $made.Db --slice-files 'apps/shop/src/total.ts' --slice-closure --map-sqlite $closed) 0
    Assert-Equal (& $count $closed "SELECT count(*) FROM key_reach") (& $count $made.Db "SELECT count(*) FROM key_reach") '--slice-closure keeps it whole'
    # (#114) A LEAN SLICE leaves out the expression and value trees and the node tables' secondary indexes; the references stay, joinable.
    $lean = Join-Path $at 'lean.sqlite'
    Assert-Exit (Invoke-Gate --map-slice $made.Db --slice-files 'apps/shop/src/total.ts' --slice-lean --map-sqlite $lean) 0
    Assert-Equal (& $count $made.Db "SELECT count(*) FROM sqlite_master WHERE name = 'expressions'") '1' 'the base map holds the trees'
    Assert-Equal (& $count $lean "SELECT count(*) FROM sqlite_master WHERE name = 'expressions' OR name LIKE 'expressions\_\_%' ESCAPE '\' OR (name LIKE '%\_\_value' ESCAPE '\' AND name <> '_meta__value')") '0' 'the trees are left out'
    Assert-Equal (& $count $lean "SELECT count(*) FROM calls") (& $count $one "SELECT count(*) FROM calls") 'the calls stay'
    Assert-Equal (& $count $lean "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = 'calls__target'") '1' 'a reference node table stays'
    Assert-Equal (& $count $lean "SELECT count(*) FROM sqlite_master WHERE type = 'index' AND tbl_name LIKE '%\_\_%' ESCAPE '\' AND (name LIKE '%\_role' ESCAPE '\' OR name LIKE '%\_value' ESCAPE '\')") '0' 'no secondary node index'
    Assert-Equal (& $count $one "SELECT count(*) FROM sqlite_master WHERE type = 'index' AND tbl_name = 'calls__target' AND name LIKE '%\_value' ESCAPE '\'") '1' 'a full slice keeps them'
    Assert-Line (Invoke-TsRowsQ $lean "SELECT 'lean=' || value AS r FROM _meta WHERE key = 'slice:lean'") 'lean=1'
    # `_meta`'s own list values are no tree: the base's structured keys survive a lean slice.
    Assert-Equal (& $count $lean "SELECT count(*) FROM _meta__value WHERE owner = 'spec:typescript'") (& $count $made.Db "SELECT count(*) FROM _meta__value WHERE owner = 'spec:typescript'") 'the base meta lists stay'
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

# A SLIM MAP KEEPS WHAT ITS CONFIG PICKS: the tables named (node tables, indexes and full text with them), the rows a `where` holds, the
# derived `ts_reads` - one row per read site naming its declaration - and nothing else; a name the base does not hold is an error.
Test-Case 'tsrows: a slim map keeps the tables, rows and derived reads its config picks' {
    $made = New-TsRowsDb (New-TsRowsWorkspace $script:TsSliceFiles) -Shared
    Assert-Exit $made.Result 0
    $at = New-Tree @{ 'slim.json' = ('{ "tables": ["files", "file_text", "calls", "expressions"], "where": { "calls": "callee = ''total''" },' +
        ' "nodes": { "calls": ["target"], "expressions": [] }, "derived": ["ts_reads"] }')
        'bad.json' = '{ "tables": ["files", "no_such_table"] }' }
    $slim = Join-Path $at 'slim.sqlite'
    Assert-Exit (Invoke-Gate --map-slim $made.Db --slim-config (Join-Path $at 'slim.json') --map-sqlite $slim) 0
    $count = { param($db, $sql) ((Invoke-TsRowsQ $db $sql).Lines | Where-Object { $_ -match '^\s*\d+\s*$' } | Select-Object -First 1).Trim() }
    Assert-Equal (& $count $slim "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = 'functions'") '0' 'a table not named is left out'
    Assert-Equal (& $count $slim "SELECT count(*) FROM files") (& $count $made.Db "SELECT count(*) FROM files") 'a table named is whole'
    Assert-Equal (& $count $slim "SELECT count(*) FROM file_text WHERE file_text MATCH 'loner'") (& $count $made.Db "SELECT count(*) FROM file_text WHERE file_text MATCH 'loner'") 'the full text answers'
    # THE ROWS A `where` HOLDS, and their node rows alone.
    $calls = Invoke-TsRowsQ $slim "SELECT 'call=' || callee AS r FROM calls"
    Assert-Line $calls 'call=total'
    Assert-NoLine $calls 'call=twice'
    Assert-Equal (& $count $slim "SELECT count(*) FROM calls__target WHERE owner NOT IN (SELECT id FROM calls)") '0' 'no node row of a row left out'
    # THE NODE TABLES `nodes` LISTS, and only those: `calls__target` stays, `calls__args` and every `expressions__*` go - `ts_reads` is read off the base's trees.
    Assert-Equal (& $count $made.Db "SELECT count(*) FROM sqlite_master WHERE name = 'calls__args'") '1' 'the base map holds the node table'
    Assert-Equal (& $count $slim "SELECT count(*) FROM sqlite_master WHERE name = 'calls__args' OR name LIKE 'expressions\_\_%' ESCAPE '\'") '0' 'a node table not listed is left out'
    # A READ AND ITS DECLARATION: `total` in caller.ts (declared in total.ts line 1), `this.http` in loner.ts (the constructor parameter, line 3).
    $reads = Invoke-TsRowsQ $slim ("SELECT 'read=' || r.name || '|' || f.path || ':' || r.line || ':' || r.col || '|' || d.path || ':' || r.decl_line AS r " +
        "FROM ts_reads r JOIN files f ON f.id = r.file JOIN files d ON d.id = r.decl_file")
    Assert-Line $reads 'read=total|apps/shop/src/caller.ts:2:42|apps/shop/src/total.ts:1'
    Assert-Line $reads 'read=http|apps/shop/src/loner.ts:4:22|apps/shop/src/loner.ts:3'
    Assert-Equal (& $count $slim "SELECT count(*) FROM (SELECT file, line, col, name FROM ts_reads GROUP BY 1, 2, 3, 4 HAVING count(*) > 1)") '0' 'one row per read site'
    Assert-Line (Invoke-TsRowsQ $slim "SELECT 'meta=' || key || '|' || value AS r FROM _meta WHERE key = 'partial'") 'meta=partial|1'
    Assert-Line (Invoke-Gate --map-query $slim --sql "SELECT 1") 'slim map of '
    $bad = Invoke-Gate --map-slim $made.Db --slim-config (Join-Path $at 'bad.json') --map-sqlite (Join-Path $at 'bad.sqlite')
    Assert-Exit $bad 2
    Assert-Line $bad 'the base map holds no table no_such_table'
    # A BASE OF ANOTHER SCHEMA is refused before anything is copied.
    $old = Join-Path $at 'old.sqlite'
    Copy-Item -LiteralPath $slim -Destination $old
    & $script:Python -c "import sqlite3, sys; c = sqlite3.connect(sys.argv[1]); c.execute(`"UPDATE _meta SET value = '1' WHERE key = 'schema'`"); c.commit()" $old
    $stale = Invoke-Gate --map-slim $old --slim-config (Join-Path $at 'slim.json') --map-sqlite (Join-Path $at 'stale.sqlite')
    Assert-Exit $stale 2
    Assert-Line $stale 'the base map is of schema 1'
}

}
