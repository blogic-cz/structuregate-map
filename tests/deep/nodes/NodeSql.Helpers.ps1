# NODE ROWS, read as SQL. A list or an object cell of the deep map is not JSON text: it is rows of `<table>__<column>`, one per node
# (`id`, `owner`, `parent`, `role`, `ord`, `node`, `value`; an object's scalar members are columns of its own row). A suite that
# pinned a cell to its old JSON spelling asks these for the SQL that spells it again. Dot-sourced by the helpers of each half.

# A LIST CELL IS NODE ROWS (`<table>__<column>`, one row per node), never JSON text. These are the SQL for a list spelled as JSON
# text again - ["a","b"], [] for an empty list, NULL for a cell that holds none - for an assertion pinned to that spelling. `Get-NodeListSql` is the list a node
# `$Item` holds under `$Role`; `Get-NodeRootListSql` is the whole cell of the row `$Ref` names (`k.rowid` for a table with no `id`, else `x.id`).
function Get-NodeTextSql([string]$Table, [string]$Where, [string]$Sep = ',') {
    $item = "CASE WHEN typeof(zx.value) = 'text' THEN char(34) || zx.value || char(34) ELSE coalesce(zx.value, 'null') END"
    return "(SELECT CASE WHEN count(zx.id) = 0 THEN '[]' ELSE '[' || group_concat($item, '$Sep' ORDER BY zx.ord) || ']' END " +
        "FROM $Table zl LEFT JOIN $Table zx ON zx.parent = zl.id WHERE $Where GROUP BY zl.id)"
}
function Get-NodeListSql([string]$Table, [string]$Item, [string]$Role, [string]$Sep = ',') {
    return Get-NodeTextSql $Table "zl.parent = ${Item}.id AND zl.role = '$Role'" $Sep
}
function Get-NodeRootListSql([string]$Table, [string]$Ref, [string]$Sep = ',') {
    $owner = if ($Ref.EndsWith('.rowid')) { "'#' || $Ref" } else { $Ref }
    return Get-NodeTextSql $Table "zl.id = $owner || '/0'" $Sep
}
# The `$Role` list of item number `$Ord` of a row's list-of-objects cell, as JSON text (see Get-NodeListSql).
function Get-NodeItemListSql([string]$Table, [string]$Ref, [int]$Ord, [string]$Role, [string]$Sep = ',') {
    $owner = if ($Ref.EndsWith('.rowid')) { "'#' || $Ref" } else { $Ref }
    return Get-NodeTextSql $Table "zl.role = '$Role' AND zl.parent = (SELECT ji.id FROM $Table ji WHERE ji.owner = $owner AND ji.parent = $owner || '/0' AND ji.ord = $Ord)" $Sep
}

# A list of OBJECTS spelled as JSON text again: [{"a":"x","b":1}], the named members in that order (an object's scalar members are
# columns of its node row). `$Ref` is the row owning the cell, as Get-NodeRootListSql takes it; with `$Role` the list is that member of the cell's root object.
function Get-NodeObjectListSql([string]$Table, [string]$Ref, [string[]]$Members, [string]$Role = '') {
    $owner = if ($Ref.EndsWith('.rowid')) { "'#' || $Ref" } else { $Ref }
    $parts = $Members | ForEach-Object {
        "char(34) || '$_' || char(34) || ':' || CASE WHEN typeof(zi.[$_]) = 'text' THEN char(34) || zi.[$_] || char(34) ELSE coalesce(zi.[$_], 'null') END"
    }
    $item = "'{' || " + ($parts -join " || ',' || ") + " || '}'"
    $where = if ($Role) { "zr.parent = $owner || '/0' AND zr.role = '$Role'" } else { "zr.id = $owner || '/0'" }
    return "(SELECT CASE WHEN count(zi.id) = 0 THEN '[]' ELSE '[' || group_concat($item, ',' ORDER BY zi.ord) || ']' END " +
        "FROM $Table zr LEFT JOIN $Table zi ON zi.parent = zr.id WHERE $where GROUP BY zr.id)"
}

# EVERY CELL OF THE MAP AT $Db THAT IS STILL A STRUCTURE AS TEXT - JSON, or python's literal value (`arguments.value`)
# spelled as a container (`(1, 2)`) - as `<table>.<column> = <cell>` - a list or an object that did not
# become node rows. A text opening with `[`/`{` that PARSES is a list or an object; `source` is the code as written, which
# may well look like one (`[]`, a dict literal), and is the one column that is not a structure's. Bookkeeping (`_`), the
# source text and the search index are not rows. Throws when the listing itself is too small to mean anything.
function Get-NodeJsonCell([string]$Db) {
    # `--limit 0` on both: the lens prints 50 rows by default, and the listing then held one table's columns.
    $listing = Invoke-Gate --map-query $Db --width 0 --limit 0 --sql ("SELECT 'c=' || m.name || '|' || p.name AS c FROM sqlite_master m, " +
        "pragma_table_info(m.name) p WHERE m.type = 'table' AND m.name NOT LIKE '\_%' ESCAPE '\' " +
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
