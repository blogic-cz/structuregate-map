<#
    `--sql-parse <file|->`: one SQL text read as the deep SQL map reads a file - statements, refs, column refs, shape,
    nodes and the parser's errors as JSON on stdout, with no tree and no database file - and derived by the SAME code
    as the map, so a statement's fingerprint is the one a map of it holds.

    Its helpers are its own, prefixed `SqlParse` - `-Only` runs this suite alone.
#>

$script:SqlParseNl = [string][char]10

function Invoke-SqlParse([string]$Text) {
    $tree = Use-Tree @{ 'q.sql' = $Text }
    $run = Invoke-Gate --sql-parse (Join-Path $tree 'q.sql')
    return [pscustomobject]@{ Tree = $tree; Run = $run }
}

Test-Case 'sqlparse: one text, its statements, refs and columns as JSON - the fingerprint a map of it holds' {
    $text = "SELECT o.Amount FROM dbo.Orders o JOIN dbo.Customers c ON c.Id = o.CustomerId;$($script:SqlParseNl)"
    $parsed = Invoke-SqlParse $text
    Assert-Exit $parsed.Run 0
    $json = $parsed.Run.Text | ConvertFrom-Json
    $refs = @($json.refs | ForEach-Object { "$($_.action)|$($_.schema).$($_.name)" }) -join ','
    Assert-Equal $refs 'select|dbo.Orders,select|dbo.Customers' 'what the statement touches'
    $columns = @($json.column_refs | ForEach-Object { "$($_.name).$($_.column):$($_.role)" }) -join ','
    Assert-Equal $columns 'Orders.Amount:select,Customers.Id:join,Orders.CustomerId:join' 'the columns, through their aliases'
    Assert-Equal (@($json.joins | ForEach-Object { "$($_.type)|$($_.name)" }) -join ',') 'inner|Customers' 'the join'
    Assert-Equal @($json.statements).Count 1 'one statement'
    Assert-Equal @($json.diagnostics).Count 0 'no error'
    # THE SAME DERIVATION AS THE MAP: the fingerprint of the statement mapped in a tree.
    $tree = Use-Tree @{ 'Demo/Demo.sqlproj' = '<Project Sdk="Microsoft.Build.Sql"></Project>'; 'Demo/q.sql' = $text }
    $db = Join-Path $tree 'sql.sqlite'
    Assert-Exit (Invoke-Gate --root $tree --ext '.sql' --map-sqlite $db) 0
    $mapped = Invoke-Gate --map-query $db --sql "SELECT 'v=' || fingerprint FROM sql_statements" --width 0 --limit 0
    $print = @($mapped.Lines | Where-Object { $_.TrimStart().StartsWith('v=') } | ForEach-Object { $_.Trim().Substring(2) })
    if ($json.statements[0].fingerprint.Length -ne 16 -or $print -notcontains $json.statements[0].fingerprint) {
        throw "the parsed fingerprint '$($json.statements[0].fingerprint)' is not the mapped one: $($print -join ',')"
    }
}

Test-Case 'sqlparse: a text that does not parse exits 1 with the parser''s error, and --dialect needs --sql-parse' {
    $parsed = Invoke-SqlParse "SELECT FROM WHERE;$($script:SqlParseNl)"
    Assert-Exit $parsed.Run 1
    $json = $parsed.Run.Text | ConvertFrom-Json
    Assert-Equal (@($json.diagnostics | ForEach-Object { "$($_.line)|$($_.name)|$($_.kind)" }) -join ',') '1|SQL46010|error' 'the error'
    Assert-Exit (Invoke-Gate --root $parsed.Tree --dialect tsql) 2
    Assert-Exit (Invoke-Gate --sql-parse (Join-Path $parsed.Tree 'q.sql') --dialect mysql) 2
}
