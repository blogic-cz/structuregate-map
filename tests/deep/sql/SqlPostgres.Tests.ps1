<#
    PostgreSQL in the SQL half: a file the SQL config's `dialects` names `postgres` is parsed by PostgreSQL's own parser
    (libpg_query, `rust/fbtcore/src/sqlpg/`) into the same tables ScriptDom fills - `files.dialect` says which - with
    `public` as the default schema, one diagnostic per statement that does not parse, where the parser stopped, and a
    tree in the same kinds, so a query written in both dialects has one fingerprint. A function's and a DO block's
    statements are statements of their own; a FOREIGN KEY's, a partition's parent's and an attached partition's tables
    are refs; a statement nested past bison's default stack parses.

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

# Each statement as `line:kind:` and the line of the statement it is inside (`-` for none); each ref as
# `line:action:schema.name.column`. Single quotes: `$$` and `$body$` are PostgreSQL's quotes, not PowerShell's.
$script:SqlPgStatements = "SELECT 'v=' || group_concat(x, ',') FROM (SELECT s.line || ':' || s.kind || ':' ||" +
    " CASE WHEN s.parent = '' THEN '-' ELSE (SELECT p.line FROM sql_statements p WHERE p.id = s.parent) END x FROM sql_statements s ORDER BY s.line, s.kind)"
$script:SqlPgRefs = "SELECT 'v=' || group_concat(x, ',') FROM (SELECT r.line || ':' || r.action || ':' || r.schema || '.' || r.name || '.' || r.column x" +
    " FROM sql_refs r ORDER BY r.line, r.action, r.name, r.column)"

Test-Case 'sqlpostgres: a DO block is a statement, and each statement in it is one of its own' {
    $nl = $script:SqlPgNl
    $made = New-SqlPgDb @{
        'structuregate.sql.json' = $script:SqlPgConfig
        'pg/do.sql' = ('DO $$' + $nl + 'BEGIN' + $nl + '  DELETE FROM alpha.logs WHERE id < 5;' + $nl +
            '  INSERT INTO alpha.audit (note) VALUES (''x'');' + $nl + 'END $$;' + $nl)
    }
    Assert-SqlPgRow $made.Db $script:SqlPgStatements '1:do:-,3:delete:1,4:insert:1' 'the DO block and the statements in it, on their own lines'
    Assert-SqlPgRow $made.Db $script:SqlPgRefs '3:delete:alpha.logs.,4:insert:alpha.audit.,4:insert:alpha.audit.note' 'what they touch'
}

Test-Case 'sqlpostgres: a PL/pgSQL function''s statements are its children, its parameters and variables params, never columns' {
    $nl = $script:SqlPgNl
    $body = @('CREATE FUNCTION shop.add_order(p_id int, p_amount numeric) RETURNS numeric', 'LANGUAGE plpgsql AS $body$', 'DECLARE',
        '  v_total numeric := 0;', 'BEGIN', '  INSERT INTO shop.orders (id, amount) VALUES (p_id, p_amount);',
        '  SELECT sum(amount) INTO v_total FROM shop.orders WHERE id = p_id;', '  IF v_total > (SELECT count(*) FROM shop.items) THEN',
        '    UPDATE shop.orders SET amount = amount - 1 WHERE id = p_id;', '  END IF;', '  RETURN v_total;', 'END', '$body$;') -join $nl
    $made = New-SqlPgDb @{ 'structuregate.sql.json' = $script:SqlPgConfig; 'pg/f.sql' = $body + $nl }
    Assert-SqlPgRow $made.Db $script:SqlPgStatements '1:create function:-,6:insert:1,7:select:1,8:if:1,9:update:1' 'the function and its statements, each on its line'
    Assert-SqlPgRow $made.Db $script:SqlPgRefs ('6:insert:shop.orders.,6:insert:shop.orders.amount,6:insert:shop.orders.id,7:select:shop.orders.,' +
        '8:select:shop.items.,9:update:shop.orders.,9:update:shop.orders.amount') 'what they touch'
    $columns = "SELECT 'v=' || group_concat(x, ',') FROM (SELECT c.line || ':' || c.name || '.' || c.column || ':' || c.role x FROM sql_column_refs c ORDER BY c.line, c.column, c.role)"
    Assert-SqlPgRow $made.Db $columns ('6:orders.amount:insert,6:orders.id:insert,7:orders.amount:select,7:orders.id:where,' +
        '9:orders.amount:set,9:orders.amount:set,9:orders.id:where') 'the columns they name - no parameter or variable among them'
    $params = "SELECT 'v=' || group_concat(value, ',') FROM (SELECT DISTINCT value FROM sql_nodes WHERE kind = 'param' ORDER BY value)"
    Assert-SqlPgRow $made.Db $params 'p_amount,p_id,v_total' 'the parameters and the variable, as params'
}

Test-Case 'sqlpostgres: a foreign key''s table, a partition''s parent and an attached partition are refs' {
    $nl = $script:SqlPgNl
    $made = New-SqlPgDb @{
        'structuregate.sql.json' = $script:SqlPgConfig
        'pg/p.sql' = ("CREATE TABLE shop.customers (id int PRIMARY KEY);$nl" +
            "CREATE TABLE shop.orders (id int, customer_id int REFERENCES shop.customers (id)) PARTITION BY RANGE (id);$nl" +
            "CREATE TABLE shop.orders_old PARTITION OF shop.orders FOR VALUES FROM (0) TO (100);$nl" +
            "ALTER TABLE shop.orders ATTACH PARTITION shop.orders_new FOR VALUES FROM (100) TO (200);$nl")
    }
    Assert-SqlPgRow $made.Db $script:SqlPgRefs '2:references:shop.customers.,3:inherits:shop.orders.,4:alter:shop.orders.,4:attach:shop.orders_new.' 'the tables they name'
}

Test-Case 'sqlpostgres: a statement that does not parse is one diagnostic where the parser stopped, not where the statement starts' {
    $nl = $script:SqlPgNl
    $made = New-SqlPgDb @{
        'structuregate.sql.json' = $script:SqlPgConfig
        'pg/b.sql' = "SELECT 1;${nl}SELECT a,$nl       b$nl  FORM shop.items;${nl}SELECT 2;$nl"
    }
    Assert-SqlPgRow $made.Db "SELECT 'v=' || group_concat(line || ':' || kind, ',') FROM (SELECT line, kind FROM sql_statements ORDER BY line)" '1:select,5:select' 'the statements around it'
    Assert-SqlPgRow $made.Db "SELECT 'v=' || count(*) || '|' || min(line) || '|' || (instr(min(source), '(line 4, column 8)') > 0) FROM diagnostics" '1|4|1' 'one error, at the parser''s position'
}

Test-Case 'sqlpostgres: 20 000 nested parentheses parse - the grammar''s stack has no depth cap' {
    $nl = $script:SqlPgNl
    $made = New-SqlPgDb @{
        'structuregate.sql.json' = $script:SqlPgConfig
        'pg/d.sql' = 'SELECT ' + ('(' * 20000) + '1' + (')' * 20000) + " AS deep FROM shop.t;$nl"
    }
    Assert-SqlPgRow $made.Db "SELECT 'v=' || errors FROM files WHERE path = 'pg/d.sql'" '0' 'no error'
    Assert-SqlPgRow $made.Db "SELECT 'v=' || count(*) || '|' || (SELECT group_concat(name) FROM sql_refs) FROM sql_statements" '1|t' 'the statement and its table'
}
