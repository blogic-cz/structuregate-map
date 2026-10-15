# NODE ROWS, read as SQL. A list or an object cell of the deep map is not JSON text: it is rows of `<table>__<column>`, one per node:
# `owner` (the row's id, `#<rowid>` for a table without one), `n` (its preorder number - the root is 0, a node IS (owner, n)),
# `parent` (the parent's `n`, NULL for the root), `role`, `ord`, `node`, `value`. EVERY member of an object, a scalar too, is its
# own row below it, its key in `role` - there are no per-key columns, and no `id`. A suite that pinned a cell to its old JSON
# spelling asks these for the SQL that spells it again. Dot-sourced by the helpers of each half.

# A `_meta` VALUE AS TEXT, or $null when the key is not there. A scalar is its cell; a list or an object is NODE ROWS in
# `_meta__value` (owner = the key, the cell NULL - `rows::meta`), which `--tables --width 0` prints as the JSON they
# rebuild. The LAST line naming the key: `--tables` lists the tables first, and a key could spell a table's name.
function Get-MetaText([string]$Db, [string]$Key) {
    $r = Invoke-Gate --map-query $Db --tables --width 0
    Assert-Exit $r 0
    $found = $null
    foreach ($line in $r.Lines) {
        $t = $line.Trim()
        if ($t.StartsWith("$Key ")) { $found = $t.Substring($Key.Length).Trim() }
    }
    return $found
}

# The `owner` a row's cell is keyed by: `k.rowid` for a table with no `id` is `'#' || k.rowid`, anything else (`x.id`) as it is.
function Get-NodeOwnerSql([string]$Ref) {
    if ($Ref.EndsWith('.rowid')) { return "'#' || $Ref" }
    return $Ref
}

# A LIST CELL IS NODE ROWS (`<table>__<column>`, one row per node), never JSON text. These are the SQL for a list spelled as JSON
# text again - ["a","b"], [] for an empty list, NULL for a cell that holds none - for an assertion pinned to that spelling.
# `zl` is the list node `$Where` picks, `zx` its items. `Get-NodeListSql` is the list a node `$Item` (an alias) holds under
# `$Role`; `Get-NodeRootListSql` is the whole cell of the row `$Ref` names (`k.rowid` for a table with no `id`, else `x.id`).
function Get-NodeTextSql([string]$Table, [string]$Where, [string]$Sep = ',') {
    $item = "CASE WHEN typeof(zx.value) = 'text' THEN char(34) || zx.value || char(34) ELSE coalesce(zx.value, 'null') END"
    return "(SELECT CASE WHEN count(zx.n) = 0 THEN '[]' ELSE '[' || group_concat($item, '$Sep' ORDER BY zx.ord) || ']' END " +
        "FROM $Table zl LEFT JOIN $Table zx ON zx.owner = zl.owner AND zx.parent = zl.n WHERE $Where GROUP BY zl.owner, zl.n)"
}
function Get-NodeListSql([string]$Table, [string]$Item, [string]$Role, [string]$Sep = ',') {
    return Get-NodeTextSql $Table "zl.owner = ${Item}.owner AND zl.parent = ${Item}.n AND zl.role = '$Role'" $Sep
}
function Get-NodeRootListSql([string]$Table, [string]$Ref, [string]$Sep = ',') {
    return Get-NodeTextSql $Table "zl.owner = $(Get-NodeOwnerSql $Ref) AND zl.n = 0" $Sep
}
# The `$Role` list of item number `$Ord` of a row's list-of-objects cell, as JSON text (see Get-NodeListSql).
function Get-NodeItemListSql([string]$Table, [string]$Ref, [int]$Ord, [string]$Role, [string]$Sep = ',') {
    $owner = Get-NodeOwnerSql $Ref
    return Get-NodeTextSql $Table ("zl.owner = $owner AND zl.role = '$Role' AND " +
        "zl.parent = (SELECT ji.n FROM $Table ji WHERE ji.owner = $owner AND ji.parent = 0 AND ji.ord = $Ord)") $Sep
}

# ONE MEMBER OF AN OBJECT NODE, by its key - the value of the row below `$Item` (a node's alias) whose `role` is `$Role`,
# NULL when the object holds no such member. There is no column per key: `v.op` is this, `Get-NodeFieldSql $t 'v' 'op'`.
function Get-NodeFieldSql([string]$Table, [string]$Item, [string]$Role) {
    return "(SELECT zf.value FROM $Table zf WHERE zf.owner = ${Item}.owner AND zf.parent = ${Item}.n AND zf.role = '$Role')"
}
# The `$Role` member of a row's OBJECT cell - its root node (`$Ref` as Get-NodeRootListSql takes it).
function Get-NodeRootFieldSql([string]$Table, [string]$Ref, [string]$Role) {
    return "(SELECT zf.value FROM $Table zf WHERE zf.owner = $(Get-NodeOwnerSql $Ref) AND zf.parent = 0 AND zf.role = '$Role')"
}
# The `$Role` member of item number `$Ord` of a row's list-of-objects cell (`$Ref` as Get-NodeRootListSql takes it).
function Get-NodeItemFieldSql([string]$Table, [string]$Ref, [int]$Ord, [string]$Role) {
    $owner = Get-NodeOwnerSql $Ref
    return "(SELECT zf.value FROM $Table zi JOIN $Table zf ON zf.owner = zi.owner AND zf.parent = zi.n AND zf.role = '$Role' " +
        "WHERE zi.owner = $owner AND zi.parent = 0 AND zi.ord = $Ord)"
}

# A list of OBJECTS spelled as JSON text again: [{"a":"x","b":1}], the named members in that order (each member is a node row
# below its item, `role` = its key; one the item does not hold reads null). `$Ref` is the row owning the cell, as
# Get-NodeRootListSql takes it; with `$Role` the list is that member of the cell's root object.
function Get-NodeObjectListSql([string]$Table, [string]$Ref, [string[]]$Members, [string]$Role = '') {
    $owner = Get-NodeOwnerSql $Ref
    $parts = $Members | ForEach-Object {
        "char(34) || '$_' || char(34) || ':' || coalesce((SELECT CASE WHEN zm.node = 'string' THEN char(34) || zm.value || char(34) " +
            "ELSE coalesce(zm.value, 'null') END FROM $Table zm WHERE zm.owner = zi.owner AND zm.parent = zi.n AND zm.role = '$_'), 'null')"
    }
    $item = "'{' || " + ($parts -join " || ',' || ") + " || '}'"
    $where = if ($Role) { "zr.owner = $owner AND zr.parent = 0 AND zr.role = '$Role'" } else { "zr.owner = $owner AND zr.n = 0" }
    return "(SELECT CASE WHEN count(zi.n) = 0 THEN '[]' ELSE '[' || group_concat($item, ',' ORDER BY zi.ord) || ']' END " +
        "FROM $Table zr LEFT JOIN $Table zi ON zi.owner = zr.owner AND zi.parent = zr.n WHERE $where GROUP BY zr.owner, zr.n)"
}

# EVERY CELL OF THE MAP AT $Db THAT IS STILL A STRUCTURE AS TEXT - JSON, or python's literal value (`arguments.value`)
# spelled as a container (`(1, 2)`) - as `<table>.<column> = <cell>` - a list or an object that did not
# become node rows. A text opening with `[`/`{` that PARSES is a list or an object; `source` is the code as written, which
# may well look like one (`[]`, a dict literal), and is the one column that is not a structure's. Bookkeeping (`_`), the
# source text and the search index are not rows. Throws when the listing itself is too small to mean anything.
# `_meta` IS SCANNED TOO (its structures are `_meta__value` nodes, `rows::meta`); `_cells` and the node tables are not.
function Get-NodeJsonCell([string]$Db) {
    # `--limit 0` on both: the lens prints 50 rows by default, and the listing then held one table's columns.
    $listing = Invoke-Gate --map-query $Db --width 0 --limit 0 --sql ("SELECT 'c=' || m.name || '|' || p.name AS c FROM sqlite_master m, " +
        "pragma_table_info(m.name) p WHERE m.type = 'table' AND (m.name NOT LIKE '\_%' ESCAPE '\' OR m.name = '_meta') " +
        "AND m.name NOT LIKE 'file_text%' AND m.name NOT LIKE 'row\_%' ESCAPE '\' AND p.name <> 'source'")
    Assert-Exit $listing 0
    $pairs = @($listing.Lines | Where-Object { $_.Trim().StartsWith('c=') } |
        ForEach-Object { , ($_.Trim().Substring(2).Split('|', 2)) })
    if ($pairs.Count -lt 20) { throw "only $($pairs.Count) column(s) read - the listing is wrong" }
    $found = @()
    for ($at = 0; $at -lt $pairs.Count; $at += 200) {
        $part = $pairs[$at..([Math]::Min($at + 199, $pairs.Count - 1))] | ForEach-Object {
            $t = $_[0].Replace('"', '""'); $c = $_[1].Replace('"', '""')
            "SELECT 'j=' || '$($_[0]).$($_[1])' || '|' || replace(replace(""$c"", char(10), ' '), char(13), ' ') AS j " +
                "FROM ""$t"" WHERE typeof(""$c"") = 'text' AND substr(ltrim(""$c""), 1, 1) IN ('[', '{', '(')"
        }
        $r = Invoke-Gate --map-query $Db --width 0 --limit 0 --sql ($part -join ' UNION ALL ')
        Assert-Exit $r 0
        foreach ($line in @($r.Lines | Where-Object { $_.Trim().StartsWith('j=') })) {
            $where, $cell = $line.Trim().Substring(2).Split('|', 2)
            # A LITERAL'S VALUE SPELLED AS A CONTAINER - python's `(1, 2)`, `{'k': [1]}` - is a structure as text too.
            $column = $where.Substring($where.LastIndexOf('.') + 1)
            $open = $cell.TrimStart()[0]; $close = $cell.TrimEnd()[-1]
            # Only python's `arguments.value` holds a literal's SPELLING: a C# `const`, a string literal's `value` and a
            # template hole are strings, and `(SELECT ...)` is one.
            $spelled = ($where -eq 'arguments.value') -and
                (($open -eq '(' -and $close -eq ')') -or ($open -eq '[' -and $close -eq ']') -or ($open -eq '{' -and $close -eq '}'))
            if (-not $spelled) {
                # `[]` included, which pwsh unrolls to nothing: a parse that succeeds is the finding.
                try { [void]($cell | ConvertFrom-Json -ErrorAction Stop) } catch { continue }
            }
            $found += "$where = $cell"
        }
    }
    return $found
}
