<#
    What `sql_links` makes of SQL that is only partly written out: an interpolated string whose holes
    are values, `string.Format`, constants and `nameof` in the holes, a loop over literal strings, a name built
    at run time beside names that are not, a trigger dynamic SQL creates, a table in several databases, and an
    EF mapping applied to several contexts.

    Its helpers are its own - `-Only` runs this suite alone, and it sorts BEFORE SqlMap, whose helpers it must
    not use.
#>

$script:SqlLinksNl = [string][char]10

function Get-SqlLinksTree {
    $nl = $script:SqlLinksNl
    $main = '<Project DefaultTargets="Build"><ItemGroup><Build Include="Tables.sql" /><PostDeploy Include="Post.sql" />' +
        '<None Include="Triggers.sql" /></ItemGroup></Project>'
    return @{
        'Main/Main.sqlproj' = $main
        'Main/Tables.sql' = (@(
            'CREATE TABLE Sales.Drafts (DraftID INT NOT NULL PRIMARY KEY, DateCreated DATETIME NOT NULL)',
            'GO', 'CREATE TABLE Sales.Notes (NoteID INT NOT NULL PRIMARY KEY, DateCreated DATETIME NOT NULL)',
            'GO', 'CREATE TABLE Inventory.StockData (SKU NVARCHAR(20) NOT NULL PRIMARY KEY)',
            'GO', 'CREATE TABLE Sales.Products (ProductID INT NOT NULL PRIMARY KEY, ProductTypeID INT NOT NULL)',
            'GO', 'CREATE TABLE Sales.PriceRules (ID INT NOT NULL PRIMARY KEY)',
            'GO', 'CREATE TABLE Sales.PriceArchive (TierID INT NOT NULL PRIMARY KEY)',
            'GO', 'CREATE TABLE App.Countries (CountryID INT NOT NULL PRIMARY KEY, Code NVARCHAR(2) NOT NULL)',
            'GO', 'CREATE TABLE App.OrderLogs (ID INT NOT NULL PRIMARY KEY)',
            'GO', 'CREATE TABLE App.Accounts (AccountID INT NOT NULL PRIMARY KEY)',
            'GO', 'CREATE TABLE App.Options (Name NVARCHAR(50) NOT NULL PRIMARY KEY)',
            'GO', 'CREATE TABLE Mail.Messages (ID INT NOT NULL PRIMARY KEY)') -join $nl) + $nl
        'Main/Post.sql' = ":r .\Triggers.sql$nl"
        # A SCRIPT THAT BUILDS A TRIGGER, in small: a VALUES list, and the name built with `+`.
        'Main/Triggers.sql' = (@(
            'DECLARE @Tables TABLE (TableName SYSNAME)',
            "INSERT INTO @Tables VALUES ('Accounts'), ('Orders')",
            'DECLARE @TableName SYSNAME, @sql NVARCHAR(MAX)',
            'SELECT TOP 1 @TableName = TableName FROM @Tables',
            "SET @sql = 'CREATE TRIGGER Track_' + @TableName + ' ON App.' + @TableName + ' AFTER UPDATE AS SET NOCOUNT ON'",
            'EXEC sp_executesql @sql') -join $nl) + $nl
        'Other/Other.sqlproj' = '<Project><ItemGroup><Build Include="Settings.sql" /></ItemGroup></Project>'
        'Other/Settings.sql' = "CREATE TABLE App.Options (Name NVARCHAR(50) NOT NULL PRIMARY KEY)$nl"
        'Logs/Logs.sqlproj' = '<Project><ItemGroup><Build Include="Log.sql" /></ItemGroup></Project>'
        'Logs/Log.sql' = "CREATE TABLE App.Log (ID INT NOT NULL PRIMARY KEY)$nl"
        # A SCRIPT OUTSIDE EVERY .sqlproj: its own table, and a procedure that reads it.
        'Shared/Setup.sql' = "CREATE TABLE Mail.Messages (ID INT NOT NULL PRIMARY KEY)${nl}GO$nl" +
            "CREATE PROCEDURE Mail.Queue AS SELECT ID FROM Mail.Messages$nl"
        'App/App.csproj' = '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework></PropertyGroup>' +
            '<ItemGroup><ProjectReference Include="..\Lib\Lib.csproj" /></ItemGroup></Project>'
        # A REFERENCED PROJECT, compiled here without its source: its readonly built from consts folds only where it is
        # declared, and that is the value the hole is.
        'Lib/Lib.csproj' = '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework></PropertyGroup></Project>'
        'Lib/LibTables.cs' = "namespace Demo.Lib { public static class LibTables { public const string Schema = ""Sales""; " +
            "public const string Archive = ""PriceArchive""; public static readonly string ArchiveWithSchema = `$""{Schema}.{Archive}""; } }$nl"
        'App/StandIns.cs' = @'
namespace Microsoft.EntityFrameworkCore.Metadata.Builders { public class EntityTypeBuilder<T> { } }
namespace Microsoft.EntityFrameworkCore
{
    using Microsoft.EntityFrameworkCore.Metadata.Builders;
    public static class RelationalEntityTypeBuilderExtensions
    {
        public static EntityTypeBuilder<T> ToTable<T>(this EntityTypeBuilder<T> builder, string name, string schema) => builder;
        public static EntityTypeBuilder<T> HasTrigger<T>(this EntityTypeBuilder<T> builder, string name) => builder;
    }
}
namespace Dapper { public static class SqlMapper { public static int Execute(this System.IDisposable connection, string sql, object param = null) => 0; } }
'@
        'App/Repository.cs' = @'
namespace Demo
{
    using System;
    using Dapper;
    public static class Names { public const string Qualified = "Sales.Products"; public const string Rules = "PriceRules"; public const string Log = "OrderLogs"; }
    public static class Schemas { public const string Sales = "Sales"; }
    public class Product { public int ProductID { get; set; } public int ProductTypeID { get; set; } }
    public class Repository
    {
        private IDisposable Open(string name) => null;
        public void Purge(int n, DateTime before)
        {
            var c = Open("main");
            var sql = $@"DELETE TOP ({n}) FROM [Sales].[Notes] WHERE DateCreated < '{before.ToString("yyyy-MM-dd")}'
                DELETE FROM Sales.Drafts WHERE DraftID IN ({string.Join(",", new[] { 1, 2 })})";
            c.Execute(sql);
        }
        public void StockData(string condition) { var c = Open("main"); c.Execute($"SELECT SKU FROM Inventory.StockData {condition}"); }
        public void Anything(string table) { var c = Open("main"); c.Execute($"SELECT * FROM [App].[{table}]"); }
        public void Folded() { var c = Open("main"); c.Execute($"SELECT {nameof(Product.ProductID)}, {nameof(Product.ProductTypeID)} FROM {Names.Qualified}"); }
        public void SchemaAndTable() { var c = Open("main"); c.Execute($"SELECT ID FROM {Schemas.Sales}.{Names.Rules}"); }
        public void Country(int code) { var c = Open("main"); c.Execute($"SELECT {nameof(Product.ProductID)} FROM App.Countries WHERE {nameof(Product.ProductTypeID)} = {code}"); }
        public void Logs(string name) { var c = Open("main"); c.Execute($"SELECT ID FROM [App].{Names.Log} UNION ALL SELECT ID FROM [dbo].[{name}_Log]"); }
        public void ClearArchive() { var c = Open("main"); c.Execute(string.Format(@"DELETE FROM [Sales].[PriceArchive]")); }
        public void ByTier(int id) { var c = Open("main"); c.Execute(string.Format("SELECT TierID FROM Sales.PriceArchive WHERE TierID = {0}", id)); }
        public void Both() { var c = Open("main"); c.Execute(string.Format("SELECT * FROM Sales.Products WHERE ProductID = @id;" + "SELECT * FROM Sales.PriceArchive WHERE TierID = @id;")); }
        public void ClearAll()
        {
            var c = Open("main");
            var queries = new[] { "DELETE FROM Sales.PriceArchive", "DELETE FROM Sales.PriceRules" };
            foreach (var query in queries) c.Execute(query);
        }
        public void ClearList()
        {
            var c = Open("main");
            var queries = new System.Collections.Generic.List<string>() { "DELETE FROM Sales.PriceArchive", "DELETE FROM Sales.Products" };
            foreach (var query in queries) c.Execute(query);
        }
        public void CrossReadonly() { var c = Open("main"); c.Execute($"SELECT TierID FROM {Demo.Lib.LibTables.ArchiveWithSchema}"); }
        public void Settings() { var c = Open("x"); c.Execute("SELECT Name FROM App.Options"); }
        public void Notify() { var c = Open("x"); c.Execute("SELECT ID FROM Mail.Messages"); }
    }
}
'@
        'App/Mappings.cs' = @'
namespace Demo
{
    using Microsoft.EntityFrameworkCore;
    using Microsoft.EntityFrameworkCore.Metadata.Builders;
    public class MainContext { }
    public class OtherContext { }
    public class LogsContext { }
    public class Account { public int AccountID { get; set; } }
    public class Option { public string Name { get; set; } }
    public class OptionMapping { public void Configure(EntityTypeBuilder<Option> builder) { builder.ToTable("Options", "App"); } }
    public class AccountMapping
    {
        public void Configure(EntityTypeBuilder<Account> builder) { builder.ToTable("Accounts", "App"); builder.HasTrigger("Track_Accounts"); builder.HasTrigger("Track_Customers"); }
    }
}
'@
        'structuregate.sql.json' = '{ "databases": [' +
            ' { "name": "Main", "project": "Main/Main.sqlproj", "contexts": ["Demo.MainContext"], "connections": ["main"] },' +
            ' { "name": "Other", "project": "Other/Other.sqlproj", "contexts": ["Demo.OtherContext"] },' +
            ' { "name": "Logs", "project": "Logs/Logs.sqlproj", "contexts": ["Demo.LogsContext"] } ],' +
            ' "unattributedContexts": ["Demo.MainContext", "Demo.OtherContext", "Demo.LogsContext"], "default": "Main" }'
    }
}

function New-SqlLinksDb([hashtable]$Files) {
    $tree = Use-Tree $Files
    $db = Join-Path $tree 'links.sqlite'
    $run = Invoke-Gate --root $tree --ext '.cs,.sql' --sql-config (Join-Path $tree 'structuregate.sql.json') --map-sqlite $db
    Assert-Exit $run 0
    return $db
}

# Every row of one query, as `v=` lines.
function Get-SqlLinksRows([string]$Db, [string]$Sql) {
    $r = Invoke-Gate --map-query $Db --sql $Sql --width 0 --limit 0
    Assert-Exit $r 0
    return @($r.Lines | Where-Object { $_.TrimStart().StartsWith('v=') } | ForEach-Object { $_.Trim().Substring(2) })
}

# The sql_text rows of one method: `action|schema.name|status|database`.
function Get-SqlLinksText([string]$Db, [string]$Func) {
    return Get-SqlLinksRows $Db ("SELECT 'v=' || action || '|' || schema || '.' || name || '|' || status || '|' || database FROM sql_links " +
        "WHERE kind = 'sql_text' AND func = '$Func' ORDER BY action, schema, name")
}

function Assert-SqlLinksRows([string[]]$Actual, [string[]]$Expected, [string]$What) {
    if (($Actual -join "`n") -ne ($Expected -join "`n")) { throw "$What`:`nexpected:`n$($Expected -join "`n")`ngot:`n$($Actual -join "`n")" }
}

Test-Case 'sqllinks: an interpolated string whose holes are values binds its tables' {
    $db = New-SqlLinksDb (Get-SqlLinksTree)
    Assert-SqlLinksRows (Get-SqlLinksText $db 'Purge') @(
        'delete|Sales.Drafts|bound|Main', 'delete|Sales.Notes|bound|Main') 'TOP, a quoted date and IN (...) are values'
    $detail = Get-SqlLinksRows $db "SELECT 'v=' || detail FROM sql_links WHERE func = 'Purge' AND name = 'Drafts'"
    if ("$detail" -notlike '*its holes are values: n, before.ToString("yyyy-MM-dd"), string.Join*') { throw "the detail names the holes: $detail" }
    # A HOLE THAT IS A CLAUSE after a literal FROM: the table binds, and the clause is `partial`.
    Assert-SqlLinksRows (Get-SqlLinksText $db 'StockData') @('|.|partial|', 'select|Inventory.StockData|bound|Main') 'a clause hole'
    # A HOLE THAT IS THE TABLE: nothing is known, and nothing is guessed.
    Assert-SqlLinksRows (Get-SqlLinksText $db 'Anything') @('|.|unknown|') 'a table name built at run time'
}

Test-Case 'sqllinks: string.Format, a literal concatenation and a loop over literals are SQL text' {
    $db = New-SqlLinksDb (Get-SqlLinksTree)
    Assert-SqlLinksRows (Get-SqlLinksText $db 'ClearArchive') @('delete|Sales.PriceArchive|bound|Main') 'string.Format with no arguments'
    Assert-SqlLinksRows (Get-SqlLinksText $db 'ByTier') @('select|Sales.PriceArchive|bound|Main') 'a {0} in a value position'
    Assert-SqlLinksRows (Get-SqlLinksText $db 'Both') @('select|Sales.PriceArchive|bound|Main', 'select|Sales.Products|bound|Main') 'two literals joined'
    Assert-SqlLinksRows (Get-SqlLinksText $db 'ClearAll') @('delete|Sales.PriceArchive|bound|Main', 'delete|Sales.PriceRules|bound|Main') 'each string the loop runs'
    Assert-SqlLinksRows (Get-SqlLinksText $db 'ClearList') @('delete|Sales.PriceArchive|bound|Main', 'delete|Sales.Products|bound|Main') 'a loop over a collection initializer'
    Assert-SqlLinksRows (Get-SqlLinksRows $db "SELECT 'v=' || a.const_kind FROM arguments a JOIN calls c ON c.id = a.call WHERE c.func = 'ClearArchive' AND a.source LIKE 'string.Format(%'") @('folded') 'the format folds whole'
}

Test-Case 'sqllinks: nameof and const holes are their values, and a dynamic name beside known ones is partial' {
    $db = New-SqlLinksDb (Get-SqlLinksTree)
    Assert-SqlLinksRows (Get-SqlLinksText $db 'Folded') @('select|Sales.Products|bound|Main') 'nameof in the select list, a const table'
    Assert-SqlLinksRows (Get-SqlLinksText $db 'SchemaAndTable') @('select|Sales.PriceRules|bound|Main') 'a const schema and a const table'
    Assert-SqlLinksRows (Get-SqlLinksText $db 'Country') @('select|App.Countries|bound|Main') 'nameof holes and one value hole'
    Assert-SqlLinksRows (Get-SqlLinksText $db 'Logs') @('select|App.OrderLogs|bound|Main', 'select|dbo.{name}_Log|partial|') 'a known table beside a dynamic one'
    Assert-SqlLinksRows (Get-SqlLinksText $db 'CrossReadonly') @('select|Sales.PriceArchive|bound|Main') 'a readonly a referenced project builds from consts'
}

Test-Case 'sqllinks: a trigger dynamic SQL builds for each row of a VALUES list is dynamic, a name the list lacks is missing' {
    $db = New-SqlLinksDb (Get-SqlLinksTree)
    $triggers = Get-SqlLinksRows $db "SELECT 'v=' || name || '|' || status || '|' || database || '|' || detail FROM sql_links WHERE kind = 'ef_trigger' ORDER BY name"
    Assert-SqlLinksRows $triggers @(
        "Track_Accounts|dynamic|Main|created by dynamic SQL at Main/Triggers.sql:5 as 'Track_' + 'Accounts', a row of the list at Main/Triggers.sql:2",
        "Track_Customers|missing||the dynamic SQL at Main/Triggers.sql:5 builds 'Track_' + a row of the script's VALUES list, and no row is 'Customers'") 'the triggers'
    Assert-SqlLinksRows (Get-SqlLinksRows $db "SELECT 'v=' || name || '|' || open FROM sql_dynamic") @('Track_|1') 'the prefix the script builds'
}

Test-Case 'sqllinks: several databases fall to the default, a script outside every project is no candidate, and binds its own objects' {
    $db = New-SqlLinksDb (Get-SqlLinksTree)
    Assert-SqlLinksRows (Get-SqlLinksRows $db "SELECT 'v=' || status || '|' || database || '|' || detail FROM sql_links WHERE func = 'Settings'") @(
        'bound|Main|default of Main, Other') 'the configured default decides'
    Assert-SqlLinksRows (Get-SqlLinksRows $db "SELECT 'v=' || status || '|' || database || '|' || detail FROM sql_links WHERE func = 'Notify'") @(
        'bound|Main|the only database that has it') 'a script outside every project is no database'
    Assert-SqlLinksRows (Get-SqlLinksRows $db ("SELECT 'v=' || l.status || '|' || f.path || '|' || l.detail FROM sql_links l JOIN sql_objects o ON o.id = l.object " +
        "JOIN files f ON f.id = o.file WHERE l.kind = 'sql_ref' AND l.name = 'Messages'")) @(
        'bound|Shared/Setup.sql|declared in the same file') 'the procedure reads its own script''s table'
    # WITHOUT A DEFAULT nothing decides, and the row says so.
    $tree = Get-SqlLinksTree
    $tree['structuregate.sql.json'] = $tree['structuregate.sql.json'].Replace(', "default": "Main"', '')
    $plain = New-SqlLinksDb $tree
    Assert-SqlLinksRows (Get-SqlLinksRows $plain "SELECT 'v=' || status || '|' || database || '|' || detail FROM sql_links WHERE func = 'Settings'") @(
        'ambiguous||ambiguous: Main, Other') 'no default'
}

Test-Case 'sqllinks: a mapping with no context attribute maps its table in each unattributedContexts database that has it' {
    $db = New-SqlLinksDb (Get-SqlLinksTree)
    Assert-SqlLinksRows (Get-SqlLinksRows $db "SELECT 'v=' || status || '|' || database FROM sql_links WHERE kind = 'ef_table' AND entity = 'Demo.Option' ORDER BY database") @(
        'bound|Main', 'bound|Other') 'Logs has no App.Options and gets no row'
    Assert-SqlLinksRows (Get-SqlLinksRows $db "SELECT 'v=' || database || '|' || member FROM sql_links WHERE kind = 'ef_column' AND entity = 'Demo.Option' ORDER BY database") @(
        'Main|Demo.Option.Name', 'Other|Demo.Option.Name') 'the columns of each'
    Assert-SqlLinksRows (Get-SqlLinksRows $db "SELECT 'v=' || status || '|' || database FROM sql_links WHERE kind = 'ef_table' AND entity = 'Demo.Account'") @(
        'bound|Main') 'a table only one of them has'
}
