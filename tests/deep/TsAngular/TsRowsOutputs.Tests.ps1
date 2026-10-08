<#
    The TypeScript half's OPTIONAL outputs - the template mirror, the row search index and the atlas -
    each written only when it is asked for. Split from `TsRowsMap` at its size limit; the helpers are
    `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '../TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

# THE READABLE VIEW, and it is OPTIONAL. The tables are the joinable view of a parsed template; this is the
# one a person opens, so "where is the parsed html" has a FILE as its answer rather than a query. Off by
# default because it is thousands of files on a real workspace and no build step reads them.
Test-Case 'tsrows: the template mirror is written only when it is asked for' {
    $tree = New-TsRowsWorkspace
    $db = Join-Path $tree 'map.sqlite'
    $config = Join-Path $tree 'structuregate.ts.json'
    $plain = Invoke-Gate --root $tree --map-sqlite $db --ts-node-modules $script:TsRowsModules --ts-config $config
    Assert-Exit $plain 0
    if (Test-Path (Join-Path $tree 'html_index.json')) { throw 'the mirror was written without being asked for' }

    $out = Join-Path $tree 'mirror'
    [void](New-Item -ItemType Directory -Path $out -Force)
    Remove-Item -LiteralPath $db -Force
    $asked = Invoke-Gate --root $tree --map-sqlite $db --ts-node-modules $script:TsRowsModules --ts-config $config --ts-html $out
    Assert-Exit $asked 0
    $index = Get-Content (Join-Path $out 'html_index.json') -Raw | ConvertFrom-Json
    $entry = $index | Where-Object { $_.source -like '*basket.component.html' }
    if (-not $entry) { throw "no mirror entry for the basket template. Got: $($index.source -join ', ')" }
    Assert-Equal $entry.component 'BasketComponent' 'the mirror names the component'

    # THE NODE IDS ARE THE ROWS' OWN, which is the whole reason two views of one model are worth having,
    # and a static attribute comes from its BINDING row rather than from the node - reading the node
    # published an empty list for every element that has one.
    $mirror = Get-Content (Join-Path $out $entry.tree_file.Replace('/', [IO.Path]::DirectorySeparatorChar)) -Raw
    if ($mirror -notlike '*"tag":"p"*') { throw "the mirror has no <p> node: $($mirror.Substring(0, 200))" }
    if ($mirror -notlike '*"name":"class","value":"intro"*') {
        throw "the static attribute is not in the mirror: $($mirror.Substring(0, 400))"
    }
    if ($mirror -notlike '*"id":"n:*') { throw 'the mirror carries no node ids' }
}


# THE SEARCH INDEX OVER EVERY ROW, and it is OPTIONAL too. `--map-query` answers with SQL and is the right
# tool for "which bindings does this component have"; this is for the one SQL is bad at - "which row
# ANYWHERE mentions this word". Off by default: it indexes every row of the map and no build step reads it.
Test-Case 'tsrows: the row search index is built only when it is asked for' {
    $tree = New-TsRowsWorkspace
    $db = Join-Path $tree 'map.sqlite'
    $config = Join-Path $tree 'structuregate.ts.json'
    $plain = Invoke-Gate --root $tree --map-sqlite $db --ts-node-modules $script:TsRowsModules --ts-config $config
    Assert-Exit $plain 0
    $none = Invoke-Gate --map-query $db --sql "SELECT count(*) AS n FROM sqlite_master WHERE name = 'row_map'"
    Assert-Line $none '0'

    Remove-Item -LiteralPath $db -Force
    $asked = Invoke-Gate --root $tree --map-sqlite $db --ts-node-modules $script:TsRowsModules --ts-config $config --map-row-fts
    Assert-Exit $asked 0
    Assert-Line $asked 'row search index holds'
    # A WORD IS FOUND, wherever it sits...
    $hit = Invoke-TsRowsQ $db ("SELECT m.tbl FROM row_fts f JOIN row_map m ON m.rid = f.rowid " +
        "WHERE row_fts MATCH 'IsPromoEnabled' GROUP BY m.tbl ORDER BY m.tbl")
    Assert-Line $hit 'gate_features'

    # ...and it spans the whole map rather than one half's tables, which is the reason it is built after
    # every half rather than inside one.
    $spread = Invoke-TsRowsQ $db "SELECT count(DISTINCT tbl) AS tables FROM row_map"
    Assert-NoLine $spread ' 1'

    # WHAT THIS CASE DELIBERATELY DOES NOT CLAIM: that the handles are excluded. `id` and the join columns
    # are left out of the indexed text - a handle is a number this run made up, and indexing it makes every
    # row match itself - but a contentless FTS5 index stores no text to read back, and no table here has
    # nothing BUT handles, so excluding them changes which rows are indexed not at all. Asserting it would
    # be a case that passes whatever the rule does.
}


# THE ATLAS, and it is OPTIONAL too. The database says how many rows each table holds; it does not say what
# the APPLICATION is. Two views of one model, exactly like the tables and their mirror: `atlas.json` keeps
# the ids so it joins back, `atlas.md` is the reading. It extracts nothing - every number is a join over
# rows already published.
Test-Case 'tsrows: the atlas is written only when it is asked for, in both views' {
    $tree = New-TsRowsWorkspace
    $db = Join-Path $tree 'map.sqlite'
    $config = Join-Path $tree 'structuregate.ts.json'
    $plain = Invoke-Gate --root $tree --map-sqlite $db --ts-node-modules $script:TsRowsModules --ts-config $config
    Assert-Exit $plain 0
    if (Test-Path (Join-Path $tree 'atlas.json')) { throw 'the atlas was written without being asked for' }

    Remove-Item -LiteralPath $db -Force
    $out = Join-Path $tree 'overview'
    $asked = Invoke-Gate --root $tree --map-sqlite $db --ts-node-modules $script:TsRowsModules --ts-config $config --map-atlas $out
    Assert-Exit $asked 0
    Assert-Line $asked 'the atlas covers'

    $atlas = Get-Content (Join-Path $out 'atlas.json') -Raw | ConvertFrom-Json
    Assert-Equal $atlas.totals.projects '1' 'the atlas counts the workspace projects'
    Assert-Equal $atlas.projects[0].name 'shop' 'the atlas names the project'
    # AN AREA IS AN NgModule, the frontend's own partitioning - never a directory depth.
    if ($atlas.projects[0].ng_modules -lt 1) { throw 'the atlas found no NgModule area' }

    # ...AND THE READABLE VIEW IS THE SAME MODEL, not a second derivation.
    $md = Get-Content (Join-Path $out 'atlas.md') -Raw
    if ($md -notlike '*# Atlas*') { throw 'the atlas markdown has no heading' }
    if ($md -notlike '*## shop*') { throw "the atlas markdown does not name the project: $($md.Substring(0, 300))" }
}

}
