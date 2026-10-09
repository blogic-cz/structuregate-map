<#
    The deep SQL map: every `.sql` of a `.sqlproj` parsed by ScriptDom into sql_objects / sql_columns /
    sql_keys / sql_refs, and `sql_links` joining C# to it - EF `ToTable` to a table, entity properties to
    columns, Dapper SQL strings to what they touch, a deploy script to the seed files it runs - resolved
    under `structuregate.sql.json`, never guessed.

    The C# side declares small stand-ins for the EF and Dapper namespaces, so the symbols are the real
    ones without a package restore. Its helpers are its own - `-Only` runs this suite alone.
#>

$script:SqlMapNl = [string][char]10

function Get-SqlMapTree {
    $nl = $script:SqlMapNl
    $classic = '<Project DefaultTargets="Build"><ItemGroup>' +
        '<Build Include="Tables\Accounts.sql" /><Build Include="Tables\Orders.sql" />' +
        '<PostDeploy Include="Scripts\Post.sql" /><None Include="Scripts\Seed.sql" /></ItemGroup></Project>'
    return @{
        'Main/Main.sqlproj' = $classic
        'Main/Tables/Accounts.sql' = "CREATE TABLE [App].[Accounts]$nl($nl    [AccountID] INT NOT NULL PRIMARY KEY IDENTITY(1, 1),$nl" +
            "    [Name] NVARCHAR(200) NOT NULL,$nl    [DateCreated] DATETIME NOT NULL DEFAULT GETDATE()$nl)$nl"
        'Main/Tables/Orders.sql' = "CREATE TABLE App.Orders (OrderID INT NOT NULL PRIMARY KEY, AccountID INT NOT NULL,$nl" +
            "    CONSTRAINT FK_Orders_Accounts FOREIGN KEY (AccountID) REFERENCES App.Accounts (AccountID))$nl" +
            "GO$nl" + "CREATE TRIGGER App.TrigOrders ON App.Orders AFTER UPDATE AS$nl" +
            "BEGIN$nl    UPDATE o SET AccountID = i.AccountID FROM App.Orders o JOIN inserted i ON i.OrderID = o.OrderID$nl" + "END$nl"
        'Main/Scripts/Post.sql' = "PRINT '`$(Environment)'$nl" + ":r .\Seed.sql$nl"
        'Main/Scripts/Seed.sql' = "INSERT INTO App.Accounts (Name) VALUES (N'first');$nl"
        'Other/Other.sqlproj' = '<Project><ItemGroup><Build Include="Accounts.sql" /></ItemGroup></Project>'
        'Other/Accounts.sql' = "CREATE TABLE App.Accounts (AccountID INT NOT NULL PRIMARY KEY, Name NVARCHAR(50) NULL)$nl"
        'App/App.csproj' = '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework></PropertyGroup></Project>'
        'App/StandIns.cs' = "namespace Microsoft.EntityFrameworkCore.Metadata.Builders { public class EntityTypeBuilder<T> { } }$nl" +
            "namespace Microsoft.EntityFrameworkCore$nl{$nl    using Microsoft.EntityFrameworkCore.Metadata.Builders;$nl" +
            "    public static class RelationalEntityTypeBuilderExtensions$nl    {$nl" +
            "        public static EntityTypeBuilder<T> ToTable<T>(this EntityTypeBuilder<T> builder, string name, string schema) => builder;$nl" +
            "    }$nl}$nl" +
            "namespace Dapper { public static class SqlMapper { public static int Execute(this System.IDisposable connection, string sql, object param = null) => 0; } }$nl"
        'App/Model.cs' = "namespace Demo$nl{$nl    using Microsoft.EntityFrameworkCore;$nl    using Microsoft.EntityFrameworkCore.Metadata.Builders;$nl    using Dapper;$nl" +
            "    public class UsesContextAttribute : System.Attribute { public UsesContextAttribute(System.Type context) { } }$nl" +
            "    public class MainContext { }$nl    public class Entity { public int AccountID { get; set; } }$nl" +
            "    public class Account : Entity { public string Name { get; set; } }$nl" +
            "    public class Order { public int OrderID { get; set; } public int AccountID { get; set; } }$nl" +
            "    [UsesContext(typeof(MainContext))]$nl    public class LedgerMapping$nl    {$nl" +
            "        public void Configure(EntityTypeBuilder<Account> builder) { builder.ToTable(`"Accounts`", `"App`"); }$nl    }$nl" +
            "    public class OrderMapping$nl    {$nl" +
            "        public void Configure(EntityTypeBuilder<Order> builder) { builder.ToTable(`"Orders`", `"App`"); }$nl    }$nl" +
            "    public class Repository$nl    {$nl        private System.IDisposable Open(string name) => null;$nl" +
            "        public void Rename() { var c = Open(`"other`"); c.Execute(`"UPDATE App.Accounts SET Name = 'n'`"); }$nl" +
            "        public void Any() { var c = Open(`"x`"); c.Execute(`"SELECT AccountID FROM App.Accounts`"); }$nl" +
            "        public void Many() { var c = Open(`"main`"); c.Execute(`"DELETE FROM App.Orders WHERE OrderID IN @ids`"); }$nl" +
            "        public void Later(string sql) { var c = Open(`"main`"); c.Execute(sql); }$nl    }$nl}$nl"
        'structuregate.sql.json' = '{ "databases": [' +
            ' { "name": "Main", "project": "Main/Main.sqlproj", "contexts": ["Demo.MainContext"], "connections": ["main"] },' +
            ' { "name": "Other", "project": "Other/Other.sqlproj", "connections": ["other"] } ] }'
    }
}

function New-SqlMapDb([hashtable]$Files) {
    $tree = Use-Tree $Files
    $db = Join-Path $tree 'sql.sqlite'
    $run = Invoke-Gate --root $tree --ext '.cs,.sql' --sql-config (Join-Path $tree 'structuregate.sql.json') --map-sqlite $db
    Assert-Exit $run 0
    return [pscustomobject]@{ Tree = $tree; Db = $db; Run = $run }
}

# Every row of one query, as `v=` lines joined by `|`.
function Get-SqlMapRows([string]$Db, [string]$Sql) {
    $r = Invoke-Gate --map-query $Db --sql $Sql --width 0 --limit 0
    Assert-Exit $r 0
    return @($r.Lines | Where-Object { $_.TrimStart().StartsWith('v=') } | ForEach-Object { $_.Trim().Substring(2) })
}

function Assert-SqlMapRow([string]$Db, [string]$Sql, [string]$Expected, [string]$What) {
    $rows = Get-SqlMapRows $Db $Sql
    if ($rows -notcontains $Expected) { throw "$What`: no row '$Expected' in:`n$($rows -join "`n")" }
}

Test-Case 'sqlmap: a sqlproj is parsed into objects, columns, keys and what each file is to the build' {
    $db = (New-SqlMapDb (Get-SqlMapTree)).Db
    Assert-SqlMapRow $db "SELECT 'v=' || database || '|' || kind || '|' || schema || '.' || name FROM sql_objects" 'Main|table|App.Accounts' 'a table'
    Assert-SqlMapRow $db "SELECT 'v=' || database || '|' || kind || '|' || schema || '.' || name FROM sql_objects" 'Other|table|App.Accounts' 'the same table in the other database'
    Assert-SqlMapRow $db "SELECT 'v=' || database || '|' || kind || '|' || schema || '.' || name FROM sql_objects" 'Main|trigger|App.TrigOrders' 'a trigger'
    Assert-SqlMapRow $db "SELECT 'v=' || c.name || '|' || c.type || '|' || c.nullable || '|' || c.identity || '|' || c.key FROM sql_columns c JOIN sql_objects o ON o.id = c.object WHERE o.database = 'Main'" 'AccountID|INT|0|1|1' 'an identity key column'
    Assert-SqlMapRow $db "SELECT 'v=' || c.name || '|' || c.type || '|' || c.nullable || '|' || c.default_expr FROM sql_columns c JOIN sql_objects o ON o.id = c.object WHERE o.database = 'Main'" 'DateCreated|DATETIME|0|GETDATE()' 'a default'
    Assert-SqlMapRow $db "SELECT 'v=' || kind || '|' || name || '|' || ref_schema || '.' || ref_name FROM sql_keys WHERE kind = 'foreign'" 'foreign|FK_Orders_Accounts|App.Accounts' 'a foreign key'
    Assert-SqlMapRow $db "SELECT 'v=' || path || '|' || kind FROM files WHERE lang = 'sql'" 'Main/Scripts/Post.sql|postdeploy' 'a deploy script'
    Assert-SqlMapRow $db "SELECT 'v=' || path || '|' || kind FROM files WHERE lang = 'sql'" 'Main/Scripts/Seed.sql|script' 'a seed script'
    Assert-SqlMapRow $db "SELECT 'v=' || path || '|' || kind FROM files WHERE lang = 'sql'" 'Main/Tables/Accounts.sql|build' 'a schema file'
    # `$(Environment)` and `:r` are SQLCMD, not T-SQL: blanked, and the file still parses.
    Assert-SqlMapRow $db "SELECT 'v=' || sum(errors) FROM files WHERE lang = 'sql'" '0' 'every file parses'
}

Test-Case 'sqlmap: what SQL touches - a trigger through its alias and never `inserted`, a seed script by column, a deploy script by the files it runs' {
    $db = (New-SqlMapDb (Get-SqlMapTree)).Db
    $refs = Get-SqlMapRows $db "SELECT 'v=' || r.action || '|' || r.schema || '.' || r.name || '|' || r.column FROM sql_refs r"
    if ($refs -notcontains 'update|App.Orders|AccountID') { throw "the aliased UPDATE target is App.Orders:`n$($refs -join "`n")" }
    if ($refs -like '*inserted*') { throw "`inserted` is no table:`n$($refs -join "`n")" }
    if ($refs -notcontains 'insert|App.Accounts|Name') { throw "the seed insert and its column:`n$($refs -join "`n")" }
    Assert-SqlMapRow $db "SELECT 'v=' || l.status || '|' || f.path FROM sql_links l JOIN files f ON f.id = l.object WHERE l.kind = 'sql_include'" 'bound|Main/Scripts/Seed.sql' 'the :r target'
    # A seed script's table resolves in ITS OWN database, though another one has the same name.
    Assert-SqlMapRow $db "SELECT 'v=' || l.status || '|' || l.database FROM sql_links l WHERE l.kind = 'sql_ref' AND l.action = 'insert'" 'bound|Main' 'the seed insert'
}

Test-Case 'sqlmap: C# reaches the database - EF by its context, Dapper by its connection, and nothing is guessed' {
    $db = (New-SqlMapDb (Get-SqlMapTree)).Db
    $ef = "SELECT 'v=' || l.kind || '|' || l.entity || '|' || l.status || '|' || l.database FROM sql_links l WHERE l.kind = 'ef_table'"
    # Both databases hold App.Accounts; the mapping class names MainContext in typeof(...), and Main is its database.
    Assert-SqlMapRow $db $ef 'ef_table|Demo.Account|bound|Main' 'the context decides'
    Assert-SqlMapRow $db $ef 'ef_table|Demo.Order|bound|Main' 'the only database that has it'
    $columns = "SELECT 'v=' || l.column || '|' || l.status || '|' || l.member FROM sql_links l WHERE l.kind = 'ef_column' AND l.entity = 'Demo.Account'"
    Assert-SqlMapRow $db $columns 'AccountID|bound|Demo.Entity.AccountID' 'a property the entity inherits'
    Assert-SqlMapRow $db $columns 'DateCreated|no property|' 'a column EF does not map'
    $text = "SELECT 'v=' || l.func || '|' || l.action || '|' || l.status || '|' || l.database FROM sql_links l WHERE l.kind = 'sql_text'"
    Assert-SqlMapRow $db $text 'Rename|update|bound|Other' 'the connection named in the method'
    Assert-SqlMapRow $db $text 'Any|select|ambiguous|' 'two databases and no default'
    Assert-SqlMapRow $db $text 'Many|delete|bound|Main' 'a Dapper list parameter parses'
    Assert-SqlMapRow $db $text 'Later||unknown|' 'SQL built at run time'
}

# A deploy chain for sql_seeds: Post.sql declares a variable, runs two seed files with `:r`, then changes the
# variable - so a value read in a seed file is the one set BEFORE its `:r`, never the last one in the chain. A
# third file runs only under the deploy's `$(Mode)`, and Accounts gains a column with a constant DEFAULT.
function Get-SqlSeedTree {
    $nl = $script:SqlMapNl
    $tree = Get-SqlMapTree
    $tree['Main/Tables/Accounts.sql'] = "CREATE TABLE [App].[Accounts] ([AccountID] INT NOT NULL PRIMARY KEY IDENTITY(1, 1),$nl" +
        "    [Name] NVARCHAR(200) NOT NULL, [DateCreated] DATETIME NOT NULL DEFAULT GETDATE(), [Active] BIT NOT NULL DEFAULT ((1)))$nl"
    $tree['Main/Scripts/Post.sql'] = "DECLARE @acme INT = 7;${nl}:r .\Seed.sql${nl}:r .\Temp.sql${nl}SET @acme = 8;$nl" +
        "IF '`$(Mode)' = 'Full'${nl}BEGIN${nl}PRINT 'full'${nl}:r .\Full.sql${nl}END$nl"
    $tree['Main/Scripts/Seed.sql'] = "INSERT INTO App.Accounts (Name) VALUES (N'first');$nl" +
        "INSERT INTO App.Accounts (Name) VALUES ('" + [char]0xD1 + "ab');$nl"
    $tree['Main/Scripts/Full.sql'] = "INSERT INTO App.Orders (OrderID, AccountID) VALUES (30, 1)$nl"
    $tree['Main/Scripts/Temp.sql'] = "SELECT TOP 0 AccountID, Name INTO #Accounts FROM App.Accounts$nl" +
        "INSERT INTO #Accounts (AccountID, Name) VALUES (@acme, N'" + [char]0xD1 + "ame'), (2, GETDATE())$nl" +
        "MERGE App.Accounts AS T USING #Accounts AS S ON T.AccountID = S.AccountID$nl" +
        "WHEN NOT MATCHED THEN INSERT (AccountID, Name) VALUES (S.AccountID, S.Name);$nl" +
        "CREATE TABLE #Orders (OrderID INT, AccountID INT)$nl" + "INSERT INTO #Orders VALUES (10, @acme + 1)$nl" +
        "MERGE App.Orders AS T USING #Orders AS S ON T.OrderID = S.OrderID WHEN NOT MATCHED THEN INSERT VALUES (S.OrderID, S.AccountID);$nl" +
        "INSERT INTO App.Orders (OrderID, AccountID) SELECT v.Id, v.Acc FROM (VALUES (20, 3)) AS v(Id, Acc)$nl" +
        "UPDATE App.Accounts SET Active = 0 WHERE AccountID IN (7, 99)$nl" +
        "INSERT INTO #Lost (A) VALUES (1)$nl" + "GO$nl" +
        "CREATE PROCEDURE App.AddAccount AS INSERT INTO App.Accounts (Name) VALUES (N'from a procedure')$nl"
    return $tree
}

Test-Case 'sqlseeds: a VALUES row lands in its table - through a #temp MERGE, @variables set by the chain that runs it' {
    $db = (New-SqlMapDb (Get-SqlSeedTree)).Db
    $rows = "SELECT 'v=' || s.schema || '.' || s.name || '|' || s.via || '|' || s.columns || '|' || s.row_values || '|' || s.status || '|' || s.deployed || '|' || s.root FROM sql_seeds s"
    # Active is left out and takes its DEFAULT; DateCreated's GETDATE() is the server's, and is no value.
    Assert-SqlMapRow $db $rows 'App.Accounts||["Name","Active"]|["first",1]|bound|1|Main/Scripts/Post.sql' 'a direct INSERT, its DEFAULT filled'
    # @acme is 7 where Temp.sql runs; the SET to 8 comes after its `:r`.
    Assert-SqlMapRow $db "SELECT 'v=' || s.via || '|' || json_extract(s.row_values, '$[0]') || '|' || s.status FROM sql_seeds s WHERE s.name = 'Accounts' AND s.via <> ''" '#Accounts|7|bound' 'a temp row merged into its table'
    Assert-SqlMapRow $db "SELECT 'v=' || (s.row_values LIKE '%' || char(209) || 'ame%') FROM sql_seeds s WHERE s.via = '#Accounts' AND s.status = 'bound'" '1' 'a string as the script spells it, not escaped'
    Assert-SqlMapRow $db "SELECT 'v=' || s.status || '|' || s.detail FROM sql_seeds s WHERE s.via = '#Accounts' AND s.status <> 'bound'" 'partial|not known before run time: Name = GETDATE()' 'a value the deploy computes'
    Assert-SqlMapRow $db "SELECT 'v=' || s.status || '|' || s.unbound FROM sql_seeds s WHERE s.via = '#Accounts'" 'partial|["Name"]' 'the column a deploy computes, as a list'
    Assert-SqlMapRow $db "SELECT 'v=' || s.status || '|' || s.unbound FROM sql_seeds s WHERE s.via = '#Accounts'" 'bound|[]' 'a row with every value known'
    # MERGE ... INSERT VALUES with no column list fills the table's columns in order; @acme + 1 is added up.
    Assert-SqlMapRow $db $rows 'App.Orders|#Orders|["OrderID","AccountID"]|[10,8]|bound|1|Main/Scripts/Post.sql' 'a positional MERGE insert'
    Assert-SqlMapRow $db "SELECT 'v=' || s.name || '|' || s.status FROM sql_seeds s WHERE s.name LIKE '#%'" '#Lost|temp' 'rows no statement moves'
    Assert-SqlMapRow $db $rows 'App.Orders||["OrderID","AccountID"]|[20,3]|bound|1|Main/Scripts/Post.sql' 'a SELECT over a VALUES table'
    # A string without N is varchar: the value is the script's, and the detail says the code page decides it.
    Assert-SqlMapRow $db "SELECT 'v=' || s.status || '|' || s.detail FROM sql_seeds s JOIN files f ON f.id = s.file WHERE f.path = 'Main/Scripts/Seed.sql' AND s.line = 2" "bound|a string without N, so the database's code page decides which characters it keeps: Name" 'a varchar literal'
    Assert-SqlMapRow $db "SELECT 'v=' || s.detail FROM sql_seeds s JOIN files f ON f.id = s.file WHERE f.path = 'Main/Scripts/Seed.sql' AND s.line = 1" '' 'an N string'
    # The UPDATE after the MERGE changes row 7 and names itself; row 2 keeps the DEFAULT and no mark.
    $updated = "SELECT 'v=' || json_extract(s.row_values, '$[0]') || '|' || json_extract(s.row_values, '$[2]') || '|' || s.defaults || '|' || s.updates FROM sql_seeds s WHERE s.via = '#Accounts'"
    Assert-SqlMapRow $db $updated '7|0|["Active"]|["Main/Scripts/Temp.sql:9"]' 'a seeded row an UPDATE changes'
    Assert-SqlMapRow $db $updated '2|1|["Active"]|[]' 'a row the UPDATE does not name'
    $condition = "SELECT 'v=' || json_extract(s.row_values, '$[0]') || '|' || s.condition FROM sql_seeds s WHERE s.name = 'Orders'"
    Assert-SqlMapRow $db $condition "30|'`$(Mode)' = 'Full'" 'a row only one deploy writes'
    Assert-SqlMapRow $db $condition '20|' 'a row every deploy writes'
    $all = Get-SqlMapRows $db "SELECT 'v=' || s.row_values FROM sql_seeds s"
    if ($all -like '*procedure*') { throw "a procedure body runs when called, not when deployed:`n$($all -join "`n")" }
}

Test-Case 'sqlmap: the passes over the finished rows are not run again over the SAME rows, and say the same' {
    $made = New-SqlMapDb (Get-SqlMapTree)
    $call = @('--root', $made.Tree, '--ext', '.cs,.sql', '--sql-config', (Join-Path $made.Tree 'structuregate.sql.json'), '--map-sqlite', $made.Db)
    Assert-SqlMapRow $made.Db "SELECT 'v=' || count(*) FROM sql_seeds" '1' 'the seed walk ran'
    # EMPTIED BY HAND: a pass that runs again puts the row back, a skipped one leaves the table as it is.
    & $script:Python -c "import sqlite3,sys; c = sqlite3.connect(sys.argv[1]); c.execute('DELETE FROM sql_seeds'); c.commit()" $made.Db
    $second = Invoke-Gate @call
    Assert-Exit $second 0
    Assert-SqlMapRow $made.Db "SELECT 'v=' || count(*) FROM sql_seeds" '0' 'nothing moved, so the seed walk did not run again'
    # NOR THE SQL HALF: no batch for the store to write back unchanged.
    Assert-Line $second 'the deep SQL half had nothing to do'
    $said = { param($run) @($run.Lines | Where-Object { $_ -match 'the sql (links|seeds):' }) -join "`n" }
    if ((& $said $second) -ne (& $said $made.Run) -or (& $said $second).Length -eq 0) {
        throw "a skipped pass must say what the run that did the work said:`n$(& $said $made.Run)`n---`n$(& $said $second)"
    }
    # THE TALLY TOO: the tables the passes write are counted on the run that writes them, not first on the next.
    $tally = { param($run) @($run.Lines | Where-Object { $_ -match '^\s*database\s' } | ForEach-Object { ($_ -split ',')[0] }) -join '' }
    if ((& $tally $second) -ne (& $tally $made.Run)) { throw "the database line moved with nothing changed:`n$(& $tally $made.Run)`n---`n$(& $tally $second)" }
    # A SOURCE THAT MOVED puts the passes back to work.
    [System.IO.File]::WriteAllText((Join-Path $made.Tree 'Main/Scripts/Seed.sql'),
        "INSERT INTO App.Accounts (Name) VALUES (N'first');$($script:SqlMapNl)INSERT INTO App.Accounts (Name) VALUES (N'second');$($script:SqlMapNl)")
    $third = Invoke-Gate @call
    Assert-Exit $third 0
    Assert-SqlMapRow $made.Db "SELECT 'v=' || count(*) FROM sql_seeds" '2' 'an edited seed script is walked again'
    # AND THE RUN SAYS WHY THE PASSES RAN: what moved in what they read.
    Assert-Line $third 'the sql files moved since they last ran'
}

Test-Case 'sqlmap: a files row with no sha does not stop the passes replaying' {
    # THE TYPESCRIPT HALF LISTS FILES IT DOES NOT HASH: a NULL sha failed the whole key, and on a large consumer the passes never
    # replayed - nor said why.
    $made = New-SqlMapDb (Get-SqlMapTree)
    $call = @('--root', $made.Tree, '--ext', '.cs,.sql', '--sql-config', (Join-Path $made.Tree 'structuregate.sql.json'), '--map-sqlite', $made.Db)
    & $script:Python -c "import sqlite3,sys; c = sqlite3.connect(sys.argv[1]); c.execute('INSERT INTO files (id, path, lang, sha) VALUES (?, ?, ?, NULL)', ('f:9999', 'web/x.ts', 'typescript')); c.commit()" $made.Db
    if ($LASTEXITCODE -ne 0) { throw 'the row with no sha was not inserted' }
    $keyed = Invoke-Gate @call
    Assert-Exit $keyed 0
    Assert-NoLine $keyed 'could not be keyed'
    & $script:Python -c "import sqlite3,sys; c = sqlite3.connect(sys.argv[1]); c.execute('DELETE FROM sql_seeds'); c.commit()" $made.Db
    Assert-Exit (Invoke-Gate @call) 0
    Assert-SqlMapRow $made.Db "SELECT 'v=' || count(*) FROM sql_seeds" '0' 'the passes replayed over the row with no sha'
}

Test-Case 'sqlmap: a DELETED source runs the passes again - dropping rows hands out no new id' {
    $made = New-SqlMapDb (Get-SqlMapTree)
    $call = @('--root', $made.Tree, '--ext', '.cs,.sql', '--sql-config', (Join-Path $made.Tree 'structuregate.sql.json'), '--map-sqlite', $made.Db)
    $links = { param($run) @($run.Lines | Where-Object { $_ -match 'the sql links:' }) -join '' }
    if ((& $links $made.Run) -notmatch 'ambiguous') { throw "App.Accounts is in two projects, so a link to it is ambiguous: $(& $links $made.Run)" }
    # WITH THE SECOND COPY GONE the table is unique, and only a pass that runs again can say so.
    Remove-Item -LiteralPath (Join-Path $made.Tree 'Other/Accounts.sql')
    $again = Invoke-Gate @call
    Assert-Exit $again 0
    if ((& $links $again) -match 'ambiguous') { throw "the links were not derived again after a delete: $(& $links $again)" }
}

# A KEY WITH ITS OWN COLUMN LIST, written after a column with no comma: SQL Server reads it as a table
# constraint over the columns it names, and so must the map, and not file it under the column it trails.
Test-Case 'sqlmap: a primary key trailing a column with its own column list keys the columns it names' {
    $nl = $script:SqlMapNl
    $made = New-SqlMapDb @{
        'Db/Db.sqlproj' = '<Project><ItemGroup><Build Include="Items.sql" /></ItemGroup></Project>'
        'Db/Items.sql' = "CREATE TABLE [App].[Items]$nl($nl    [ItemID] [int] NOT NULL,$nl" +
            "    [Note] [nvarchar](500) NULL$nl    CONSTRAINT [PK_Items] PRIMARY KEY ([ItemID])$nl)$nl"
        'structuregate.sql.json' = '{ "databases": [ { "name": "Db", "project": "Db/Db.sqlproj" } ] }'
    }
    $db = $made.Db
    Assert-SqlMapRow $db "SELECT 'v=' || kind || '|' || columns FROM sql_keys WHERE name = 'PK_Items'" 'primary|["ItemID"]' 'the key over the column it names'
    Assert-SqlMapRow $db "SELECT 'v=' || name || '|' || nullable || '|' || key FROM sql_columns WHERE name = 'ItemID'" 'ItemID|0|1' 'the named column is the key'
    Assert-SqlMapRow $db "SELECT 'v=' || name || '|' || nullable || '|' || key FROM sql_columns WHERE name = 'Note'" 'Note|1|0' 'the column it trails is untouched'
}
