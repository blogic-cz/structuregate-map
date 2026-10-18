<#
    What a SQL statement IS, beside what it touches: `sql_statements` (one row per statement - batch, place, kind,
    nesting, the object it belongs to), `sql_refs.statement`, the normalized tree `sql_nodes`, and the fingerprint
    rust derives from it (`rows/sqlshape/`). And that a run which re-read one file holds exactly what a fresh map
    of the same tree holds.

    Its helpers are its own, prefixed `SqlStmt` - `-Only` runs this suite alone.
#>

$script:SqlStmtNl = [string][char]10

function New-SqlStmtDb([hashtable]$Files) {
    $tree = Use-Tree $Files
    $db = Join-Path $tree 'sql.sqlite'
    $run = Invoke-Gate --root $tree --ext '.sql' --map-sqlite $db
    Assert-Exit $run 0
    return [pscustomobject]@{ Tree = $tree; Db = $db; Run = $run }
}

# Every row of one query, as `v=` lines.
function Get-SqlStmtRows([string]$Db, [string]$Sql) {
    $r = Invoke-Gate --map-query $Db --sql $Sql --width 0 --limit 0
    Assert-Exit $r 0
    return @($r.Lines | Where-Object { $_.TrimStart().StartsWith('v=') } | ForEach-Object { $_.Trim().Substring(2) })
}

function Assert-SqlStmtRow([string]$Db, [string]$Sql, [string]$Expected, [string]$What) {
    $rows = Get-SqlStmtRows $Db $Sql
    if ($rows -notcontains $Expected) { throw "$What`: no row '$Expected' in:`n$($rows -join "`n")" }
}

function Get-SqlStmtProject { '<Project Sdk="Microsoft.Build.Sql"></Project>' }

Test-Case 'sqlstatements: a row per statement - batch, place, kind, a SET that touches nothing - and the statement each ref is in' {
    $nl = $script:SqlStmtNl
    $made = New-SqlStmtDb @{
        'Demo/Demo.sqlproj' = Get-SqlStmtProject
        'Demo/q.sql' = "SELECT o.Amount${nl}FROM dbo.Orders o${nl}JOIN dbo.Customers c ON c.Id = o.CustomerId;${nl}" +
            "SELECT 1; SELECT 2;${nl}GO${nl}SET NOCOUNT ON;${nl}SELECT COUNT(*) FROM dbo.Orders;$nl"
        'Demo/Proc.sql' = "CREATE PROCEDURE dbo.ListOrders AS${nl}BEGIN${nl}    SELECT Id FROM dbo.Orders;${nl}END$nl"
    }
    $placed = "SELECT 'v=' || group_concat(x, ',') FROM (SELECT s.batch || ':' || s.ordinal || ':' || s.line || '-' || s.end_line || ':' || s.kind x" +
        " FROM sql_statements s JOIN files f ON f.id = s.file WHERE f.path = 'Demo/q.sql' ORDER BY s.batch, s.ordinal)"
    Assert-SqlStmtRow $made.Db $placed '0:0:1-3:select,0:1:4-4:select,0:2:4-4:select,1:0:6-6:set,1:1:7-7:select' 'every statement, in its batch'
    Assert-SqlStmtRow $made.Db "SELECT 'v=' || count(*) FROM sql_refs r JOIN sql_statements s ON s.id = r.statement WHERE s.line = 1" '2' 'the first statement touches two tables'
    Assert-SqlStmtRow $made.Db "SELECT 'v=' || s.line FROM sql_refs r JOIN sql_statements s ON s.id = r.statement WHERE r.line = 7" '7' 'a ref in the second batch'
    # A LEAF KEEPS ITS TEXT, a compound statement its range; a body's statement hangs off its block, and is the procedure's.
    $nested = "SELECT 'v=' || s.kind || '|' || p.kind || '|' || o.name || '|' || s.source FROM sql_statements s" +
        " JOIN sql_statements p ON p.id = s.parent JOIN sql_objects o ON o.id = s.object WHERE s.kind = 'select' AND o.kind = 'procedure'"
    Assert-SqlStmtRow $made.Db $nested 'select|block|ListOrders|SELECT Id FROM dbo.Orders;' 'a statement of a procedure body'
    Assert-SqlStmtRow $made.Db "SELECT 'v=' || s.kind || '|' || length(s.source) FROM sql_statements s WHERE s.parent = ''  AND s.kind LIKE 'create%'" 'create procedure|0' 'a compound statement keeps no text'
}

Test-Case 'sqlstatements: a statement is a tree of clauses, and its fingerprint is the same whatever its literals' {
    $nl = $script:SqlStmtNl
    $made = New-SqlStmtDb @{
        'Demo/Demo.sqlproj' = Get-SqlStmtProject
        'Demo/q.sql' = "SELECT o.Amount FROM dbo.Orders o WHERE o.Status = 1 AND o.CustomerId IN (10, 11);$nl" +
            "SELECT o.Amount FROM dbo.Orders o WHERE o.Status = 7 AND o.CustomerId IN (42);$nl" +
            "SELECT o.Amount + 1 FROM dbo.Orders o WHERE o.Status = 7 AND o.CustomerId IN (42);$nl"
    }
    $tree = "SELECT 'v=' || group_concat(x, ' ') FROM (SELECT coalesce(n.parent, '') || '>' || n.kind || ':' || n.value x FROM sql_nodes n" +
        " JOIN sql_statements s ON s.id = n.statement WHERE s.line = 1 ORDER BY n.n)"
    Assert-SqlStmtRow $made.Db $tree ('>select: 0>query: 1>item: 2>column:o.Amount 1>from: 4>table:Orders 5>schema:dbo 5>alias:o' +
        ' 1>where: 8>op:and 9>op:= 10>column:o.Status 10>literal:1 9>in: 13>column:o.CustomerId 13>literal:10 13>literal:11') 'the tree of a query'
    $alike = "SELECT 'v=' || count(DISTINCT fingerprint) || '|' || min(length(fingerprint)) FROM sql_statements WHERE line IN (1, 2)"
    Assert-SqlStmtRow $made.Db $alike '1|16' 'two statements that differ only in literals and the length of an IN list'
    Assert-SqlStmtRow $made.Db "SELECT 'v=' || count(DISTINCT fingerprint) FROM sql_statements" '2' 'one more operator is another statement'
}

Test-Case 'sqlstatements: a statement''s shape - its clauses, what it joins, its functions and CTEs' {
    $nl = $script:SqlStmtNl
    $made = New-SqlStmtDb @{
        'Demo/Demo.sqlproj' = Get-SqlStmtProject
        'Demo/q.sql' = "WITH Totals AS (SELECT o.CustomerId, SUM(o.Amount) AS s FROM dbo.Orders o GROUP BY o.CustomerId)$nl" +
            "SELECT TOP 10 c.Name, t.s, ROW_NUMBER() OVER (ORDER BY t.s DESC) AS rn$nl" +
            "FROM Totals t LEFT JOIN dbo.Customers c ON c.Id = t.CustomerId${nl}WHERE t.s > 100${nl}ORDER BY t.s DESC;$nl"
    }
    Assert-SqlStmtRow $made.Db "SELECT 'v=' || group_concat(kind, ',') FROM (SELECT DISTINCT kind FROM sql_clauses ORDER BY kind)" 'cte,groupby,leftjoin,orderby,top,where,window' 'every clause, an OVER as a window'
    Assert-SqlStmtRow $made.Db "SELECT 'v=' || type || '|' || schema || '.' || name || '|' || alias || '|' || on_line FROM sql_joins" 'left|dbo.Customers|c|3' 'the side a join brings in'
    Assert-SqlStmtRow $made.Db "SELECT 'v=' || group_concat(name || ':' || windowed, ',') FROM (SELECT name, windowed FROM sql_functions ORDER BY name)" 'row_number:1,sum:0' 'the functions, one over a window'
    Assert-SqlStmtRow $made.Db "SELECT 'v=' || name || '|' || line FROM sql_ctes" 'Totals|1' 'the CTE'
}

Test-Case 'sqlstatements: every column a statement names, its table through the alias, and the clause it is in' {
    $nl = $script:SqlStmtNl
    $made = New-SqlStmtDb @{
        'Demo/Demo.sqlproj' = Get-SqlStmtProject
        'Demo/q.sql' = "SELECT o.Amount, c.Name FROM dbo.Orders o LEFT JOIN dbo.Customers c ON c.Id = o.CustomerId WHERE o.Status = 1;$nl" +
            "UPDATE dbo.Orders SET Status = 2 WHERE Id = 5;${nl}INSERT INTO dbo.Orders (Id, Amount) VALUES (1, 10);$nl" +
            "SELECT a.Id FROM dbo.Orders a JOIN dbo.Customers b ON b.Id = a.CustomerId WHERE X = 1;$nl" +
            "MERGE dbo.Orders AS T USING (SELECT Id FROM dbo.Archive) AS S ON T.Id = S.Id WHEN MATCHED THEN UPDATE SET Status = 1;$nl" +
            "UPDATE o SET Status = 3 FROM dbo.Orders o JOIN dbo.Customers c ON c.Id = o.CustomerId WHERE c.Name = 'x';$nl"
    }
    $named = "SELECT 'v=' || group_concat(x, ',') FROM (SELECT line || ':' || schema || '.' || name || '.' || column || ':' || role || ':' || source_kind x" +
        " FROM sql_column_refs ORDER BY line, name, column, role)"
    # An unqualified column of a two-table join (`X`) belongs to no table and stays a row; a SET's column is the target's.
    Assert-SqlStmtRow $made.Db $named ('1:dbo.Customers.Id:join:table,1:dbo.Customers.Name:select:table,1:dbo.Orders.Amount:select:table,' +
        '1:dbo.Orders.CustomerId:join:table,1:dbo.Orders.Status:where:table,2:dbo.Orders.Id:where:table,2:dbo.Orders.Status:set:table,' +
        '3:dbo.Orders.Amount:insert:table,3:dbo.Orders.Id:insert:table,4:..X:where:,4:dbo.Customers.Id:join:table,4:dbo.Orders.CustomerId:join:table,' +
        '4:dbo.Orders.Id:select:table,5:..Id:merge_on:derived,5:dbo.Archive.Id:select:table,5:dbo.Orders.Id:merge_on:table,5:dbo.Orders.Status:set:table,' +
        '6:dbo.Customers.Id:join:table,6:dbo.Customers.Name:where:table,6:dbo.Orders.CustomerId:join:table,6:dbo.Orders.Status:set:table') 'every column and its table'
}

# THE STATEMENTS OF A RE-READ FILE ARE REPLACED, and those of the files that did not move kept as they were: a partial
# run holds the same rows as a fresh map of the same tree, compared by what each row SAYS - an id is a handle.
function Get-SqlStmtContent([string]$Db) {
    $statement = "(SELECT f2.path || '#' || s2.batch || '.' || s2.ordinal FROM sql_statements s2 JOIN files f2 ON f2.id = s2.file WHERE s2.id = {0})"
    $queries = @(
        ("SELECT 'v=S|' || f.path || '|' || s.batch || '|' || s.ordinal || '|' || coalesce($($statement -f 's.parent'), '') || '|' || s.line || '|' ||" +
            " s.end_line || '|' || s.kind || '|' || s.source || '|' || s.fingerprint FROM sql_statements s JOIN files f ON f.id = s.file"),
        # A NODE LEFT BEHIND names a statement that is gone: NULL would drop the whole line, so it reads `gone`.
        ("SELECT 'v=N|' || coalesce($($statement -f 'n.statement'), 'gone') || '|' || n.n || '|' || coalesce(n.parent, '') || '|' || n.ord || '|' || n.kind || '|' ||" +
            " n.value || '|' || n.line FROM sql_nodes n"),
        ("SELECT 'v=R|' || f.path || '|' || r.line || '|' || r.action || '|' || r.name || '|' || r.column || '|' ||" +
            " coalesce($($statement -f 'r.statement'), '') FROM sql_refs r JOIN files f ON f.id = r.file"),
        ("SELECT 'v=C|' || coalesce($($statement -f 'c.statement'), 'gone') || '|' || c.line || '|' || c.role || '|' || c.schema || '|' ||" +
            " c.name || '|' || c.source_kind || '|' || c.alias || '|' || c.column FROM sql_column_refs c"),
        ("SELECT 'v=V|' || $($statement -f 'v.statement') || '|' || v.kind || '|' || v.line FROM sql_clauses v"),
        ("SELECT 'v=J|' || $($statement -f 'v.statement') || '|' || v.type || '|' || v.schema || '|' || v.name || '|' || v.alias FROM sql_joins v"))
    return @($queries | ForEach-Object { Get-SqlStmtRows $Db $_ } | Sort-Object)
}

function Get-SqlStmtChain([string]$Queries) {
    $nl = $script:SqlStmtNl
    return @{
        'Demo/Demo.sqlproj' = Get-SqlStmtProject
        'Demo/Procs.sql' = "CREATE PROCEDURE dbo.ListOrders @CustomerId INT AS${nl}BEGIN${nl}    SET NOCOUNT ON;$nl" +
            "    WITH Totals AS (SELECT o.CustomerId, SUM(o.Amount) AS s FROM dbo.Orders o GROUP BY o.CustomerId)$nl" +
            "    SELECT TOP 10 c.Name, t.s FROM Totals t LEFT JOIN dbo.Customers c ON c.Id = t.CustomerId WHERE c.Id = @CustomerId;$nl" +
            "    IF EXISTS (SELECT 1 FROM dbo.Orders WHERE Status = 3) UPDATE dbo.Orders SET Status = 4 WHERE Status = 3;${nl}END$nl"
        'Demo/Queries.sql' = $Queries
        'Demo/Tables.sql' = "CREATE TABLE dbo.Orders (Id INT NOT NULL PRIMARY KEY, CustomerId INT, Amount INT, Status INT);${nl}GO$nl" +
            "MERGE dbo.Orders AS T USING (SELECT Id FROM dbo.Orders) AS S ON T.Id = S.Id WHEN MATCHED THEN UPDATE SET Status = 1;$nl"
    }
}

Test-Case 'sqlstatements: a run that re-read one file holds what a fresh map of the same tree holds' {
    $nl = $script:SqlStmtNl
    $before = "SELECT o.Amount, c.Name FROM dbo.Orders o LEFT JOIN dbo.Customers c ON c.Id = o.CustomerId WHERE o.Status = 1;$nl" +
        "UPDATE dbo.Orders SET Status = 2 WHERE Id = 5;${nl}DELETE FROM dbo.Orders WHERE Id IN (1, 2, 3);$nl"
    $after = "SELECT o.Amount, c.Name FROM dbo.Orders o LEFT JOIN dbo.Customers c ON c.Id = o.CustomerId WHERE o.Status = 9 AND o.Amount > 5;$nl" +
        "SET NOCOUNT ON;${nl}UPDATE dbo.Orders SET Status = 2 WHERE Id = 5;${nl}SELECT COUNT(*) FROM dbo.Customers;$nl"
    $partial = New-SqlStmtDb (Get-SqlStmtChain $before)
    [System.IO.File]::WriteAllText((Join-Path $partial.Tree 'Demo/Queries.sql'), $after)
    $again = Invoke-Gate --root $partial.Tree --ext '.sql' --map-sqlite $partial.Db
    Assert-Exit $again 0
    Assert-Line $again '1 file(s) re-read'
    $kept = Get-SqlStmtContent $partial.Db
    $fresh = Get-SqlStmtContent (New-SqlStmtDb (Get-SqlStmtChain $after)).Db
    $diff = @(Compare-Object $kept $fresh)
    if ($diff.Count -gt 0 -or $kept.Count -lt 40 -or @($kept | Where-Object { $_.StartsWith('C|') }).Count -lt 10) {
        throw "the partial run and the fresh map differ ($($kept.Count) vs $($fresh.Count) rows):`n$(($diff | ForEach-Object { "$($_.SideIndicator) $($_.InputObject)" }) -join "`n")"
    }
}

Test-Case 'sqlstatements: a file with one bad statement keeps the rows of the good ones and counts one error' {
    $nl = $script:SqlStmtNl
    $tree = Use-Tree @{
        'Demo/Demo.sqlproj' = Get-SqlStmtProject
        'Demo/q.sql' = "SELECT Id FROM dbo.Orders;${nl}not sql at all;${nl}SELECT Email FROM dbo.Customers;$nl"
    }
    $db = Join-Path $tree 'sql.sqlite'
    Invoke-Gate --root $tree --ext '.sql' --map-sqlite $db | Out-Null
    Assert-SqlStmtRow $db "SELECT 'v=' || errors FROM files WHERE path = 'Demo/q.sql'" '1' 'one error'
    Assert-SqlStmtRow $db "SELECT 'v=' || count(*) FROM sql_statements s JOIN files f ON f.id = s.file WHERE f.path = 'Demo/q.sql'" '2' 'two statements'
}
