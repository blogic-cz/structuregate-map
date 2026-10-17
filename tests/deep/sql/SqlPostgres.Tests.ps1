<#
    PostgreSQL in the SQL half: a file the SQL config's `dialects` names `postgres` is parsed by sqlparser-rs
    (`rust/fbtcore/src/sqlpg/`) into the same tables ScriptDom fills - `files.dialect` says which - with `public` as the
    default schema, one diagnostic per statement that does not parse, and a tree in the same kinds, so a query written
    in both dialects has one fingerprint.

    Its helpers are its own, prefixed `SqlPg` - `-Only` runs this suite alone.
#>

$script:SqlPgNl = [string][char]10

function New-SqlPgDb([hashtable]$Files) {
    $tree = Use-Tree $Files
    $db = Join-Path $tree 'sql.sqlite'
    $run = Invoke-Gate --root $tree --ext '.sql' --sql-config (Join-Path $tree 'structuregate.sql.json') --map-sqlite $db
    Assert-Exit $run 0
    return [pscustomobject]@{ Tree = $tree; Db = $db; Run = $run }
}

function Get-SqlPgRows([string]$Db, [string]$Sql) {
    $r = Invoke-Gate --map-query $Db --sql $Sql --width 0 --limit 0
    Assert-Exit $r 0
    return @($r.Lines | Where-Object { $_.TrimStart().StartsWith('v=') } | ForEach-Object { $_.Trim().Substring(2) })
}

function Assert-SqlPgRow([string]$Db, [string]$Sql, [string]$Expected, [string]$What) {
    $rows = Get-SqlPgRows $Db $Sql
    if ($rows -notcontains $Expected) { throw "$What`: no row '$Expected' in:`n$($rows -join "`n")" }
}

$script:SqlPgQuery = "SELECT o.amount FROM public.orders o WHERE o.created_at > now() - interval '1 day' LIMIT 10;"
$script:SqlPgConfig = '{ "dialects": [ { "glob": "pg/**", "dialect": "postgres" } ] }'

Test-Case 'sqlpostgres: a file the config names postgres is parsed as PostgreSQL - its refs, columns and clauses, public the default schema' {
    $nl = $script:SqlPgNl
    $made = New-SqlPgDb @{
        'structuregate.sql.json' = $script:SqlPgConfig
        'pg/q.sql' = "$($script:SqlPgQuery)${nl}INSERT INTO orders (id, amount) VALUES (1, 2) RETURNING id;$nl"
        'other/q.sql' = "$($script:SqlPgQuery)$nl"
    }
    $files = "SELECT 'v=' || path || '|' || dialect || '|' || errors FROM files WHERE lang = 'sql'"
    Assert-SqlPgRow $made.Db $files 'pg/q.sql|postgres|0' 'the PostgreSQL file parses'
    Assert-SqlPgRow $made.Db $files 'other/q.sql|tsql|1' 'the same text outside the glob is T-SQL, and is not'
    $refs = "SELECT 'v=' || group_concat(x, ',') FROM (SELECT r.line || ':' || r.action || ':' || r.schema || '.' || r.name || '.' || r.column x" +
        " FROM sql_refs r JOIN files f ON f.id = r.file WHERE f.path = 'pg/q.sql' ORDER BY r.line, r.column)"
    Assert-SqlPgRow $made.Db $refs '1:select:public.orders.,2:insert:public.orders.,2:insert:public.orders.amount,2:insert:public.orders.id' 'what it touches, in public'
    $columns = "SELECT 'v=' || group_concat(x, ',') FROM (SELECT c.line || ':' || c.schema || '.' || c.name || '.' || c.column || ':' || c.role x" +
        " FROM sql_column_refs c JOIN files f ON f.id = c.file WHERE f.path = 'pg/q.sql' ORDER BY c.line, c.column, c.role)"
    Assert-SqlPgRow $made.Db $columns ('1:public.orders.amount:select,1:public.orders.created_at:where,' +
        '2:public.orders.amount:insert,2:public.orders.id:insert,2:public.orders.id:output') 'the columns it names'
    Assert-SqlPgRow $made.Db "SELECT 'v=' || group_concat(kind, ',') FROM (SELECT c.kind FROM sql_clauses c JOIN files f ON f.id = c.file WHERE f.path = 'pg/q.sql' AND c.line = 1 ORDER BY c.kind)" 'limit,where' 'its LIMIT'
}

Test-Case 'sqlpostgres: a statement that does not parse costs one diagnostic, and a psql meta-command none' {
    $nl = $script:SqlPgNl
    $made = New-SqlPgDb @{
        'structuregate.sql.json' = $script:SqlPgConfig
        'pg/q.sql' = "SELECT id FROM orders;${nl}SELEC x;${nl}\i other.sql${nl}CREATE TABLE shop.items (id serial PRIMARY KEY, name varchar(50) NOT NULL DEFAULT 'x');$nl"
    }
    Assert-SqlPgRow $made.Db "SELECT 'v=' || group_concat(line || ':' || kind, ',') FROM (SELECT line, kind FROM sql_statements ORDER BY line)" '1:select,4:create table' 'the statements around the bad one'
    Assert-SqlPgRow $made.Db "SELECT 'v=' || count(*) || '|' || min(line) FROM diagnostics" '1|2' 'one error, on its line'
    $table = "SELECT 'v=' || o.schema || '.' || o.name || '|' || group_concat(c.name || ' ' || c.type || ' ' || c.nullable || c.identity || c.key || ' ' || c.default_expr, ',')" +
        " FROM sql_objects o JOIN sql_columns c ON c.object = o.id GROUP BY o.id"
    Assert-SqlPgRow $made.Db $table "shop.items|id serial 011 ,name varchar(50) 000 'x'" 'a table and its columns'
}

Test-Case 'sqlpostgres: one query in both dialects is one fingerprint' {
    $nl = $script:SqlPgNl
    $tree = Use-Tree @{ 'q.sql' = "SELECT o.Amount, c.Name FROM dbo.Orders o LEFT JOIN dbo.Customers c ON c.Id = o.CustomerId WHERE o.Status IN (1, 2) ORDER BY o.Amount DESC;$nl" }
    $prints = foreach ($dialect in 'tsql', 'postgres') {
        $run = Invoke-Gate --sql-parse (Join-Path $tree 'q.sql') --dialect $dialect
        Assert-Exit $run 0
        ($run.Text | ConvertFrom-Json).statements[0].fingerprint
    }
    if ($prints[0].Length -ne 16 -or $prints[0] -ne $prints[1]) { throw "the dialects disagree on the query's shape: $($prints -join ' vs ')" }
}
