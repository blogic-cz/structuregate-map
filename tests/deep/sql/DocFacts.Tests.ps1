<#
    Facts from documents outside the tree: `structuregate.facts.json` names a Markdown snapshot, the
    table under a heading of it, and the code each list is checked against. The deep map reads them into
    `doc_facts` and `doc_links`; beside them, every seeded table gets a typed view and a type name declared
    twice is listed in `duplicate_types`. `--facts-pull` is covered by the rust tests against a fake Drive
    API; here only what it does without one.

    Its helpers are its own - `-Only` runs this suite alone, and it sorts BEFORE SqlMap, whose helpers it
    must not use.
#>

$script:DocFactsNl = [string][char]10

function Get-DocFactsTree {
    $nl = $script:DocFactsNl
    return @{
        'Main/Main.sqlproj' = '<Project DefaultTargets="Build"><ItemGroup><Build Include="Tables\Products.sql" />' +
            '<PostDeploy Include="Scripts\Post.sql" /><None Include="Scripts\Products.sql" /></ItemGroup></Project>'
        'Main/Tables/Products.sql' = "CREATE TABLE [Sales].[Products] ([ProductID] INT NOT NULL PRIMARY KEY, [Name] NVARCHAR(200) NOT NULL)$nl"
        'Main/Scripts/Post.sql' = ":r .\Products.sql$nl"
        # Two INSERTs list the columns in different orders: the typed view reads each by name.
        'Main/Scripts/Products.sql' = "INSERT INTO Sales.Products (ProductID, Name) VALUES (101, N'Basic'), (102, N'Plus')$nl" +
            "INSERT INTO Sales.Products (Name, ProductID) VALUES (N'Old', 103)$nl"
        'App/App.csproj' = '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework></PropertyGroup></Project>'
        'App/Ids.cs' = "namespace Demo.Models { public enum SalesType { Retail = 1, Bulk = 2 } public enum ProductIDs { Basic = 101, Plus = 102 } }$nl"
        'App/Copy.cs' = "namespace Demo.Copy { public enum ProductIDs { Basic = 101 } }$nl"
        # A HAND-WRITTEN TABLE AS IT IS FOUND: an empty GFM header, the column named in the first body row, the emphasis
        # escaped, and one list split over two tables under bold paragraphs.
        'docs/guide.md' = "# Integration$nl$nl## **Item codes**$nl$nl**Hardware line**$nl$nl" +
            "|  |  |$nl| :-: | :-: |$nl| \*\*Product code\*\* |   |$nl| \*\*101\*\* | Basic |$nl$nl" +
            "**Software line**$nl$nl|  |  |$nl| :-: | :-: |$nl| \*\*Product code\*\* |   |$nl| \*\*250\*\* | Ghost |$nl$nl" +
            "## SalesType$nl$nl| Value | Description |$nl|---|---|$nl| 1 | Retail |$nl| 3 | Export |$nl"
        'structuregate.facts.json' = '{ "sources": [ { "name": "guide", "kind": "file", "path": "docs/guide.md" } ],' +
            ' "facts": [ { "name": "codes", "source": "guide", "section": "Item codes", "table": "all", "key": "Product code", "label": 2 },' +
            ' { "source": "guide", "section": "SalesType", "key": "Value", "label": "Description" } ],' +
            ' "links": [ { "facts": "codes", "to": { "kind": "seed", "object": "Sales.Products", "column": "ProductID" } },' +
            ' { "facts": "SalesType", "to": { "kind": "enum", "symbol": "Demo.Models.SalesType" } } ] }'
    }
}

function Invoke-DocFactsMap([string]$Tree) {
    $db = Join-Path $Tree 'facts.sqlite'
    return Invoke-Gate --root $Tree --ext '.cs,.sql' --map-sqlite $db --facts-config (Join-Path $Tree 'structuregate.facts.json')
}

function Get-DocFactsRows([string]$Tree, [string]$Sql) {
    $r = Invoke-Gate --map-query (Join-Path $Tree 'facts.sqlite') --sql $Sql --width 0 --limit 0
    Assert-Exit $r 0
    return @($r.Lines | Where-Object { $_.TrimStart().StartsWith('v=') } | ForEach-Object { $_.Trim().Substring(2) })
}

function Assert-DocFactsRow([string]$Tree, [string]$Sql, [string]$Expected, [string]$What) {
    $rows = Get-DocFactsRows $Tree $Sql
    if ($rows -notcontains $Expected) { throw "$What`: no row '$Expected' in:`n$($rows -join "`n")" }
}

$script:DocFactsLinks = "SELECT 'v=' || l.facts || '|' || l.key || '|' || l.value || '|' || l.name || '|' || l.status FROM doc_links l"

Test-Case 'docfacts: a document''s table is checked against a seeded column and an enum, both ways' {
    $tree = Use-Tree (Get-DocFactsTree)
    $run = Invoke-DocFactsMap $tree
    Assert-Exit $run 0
    Assert-Line $run 'the document facts: 4 row(s) in doc_facts; doc_links 2 bound, 2 missing in code, 3 missing in doc'
    Assert-DocFactsRow $tree "SELECT 'v=' || facts || '|' || subsection || '|' || key || '|' || label || '|' || file || '|' || line FROM doc_facts" 'codes|Software line|250|Ghost|docs/guide.md|17' 'a fact of the second table, its bold paragraph and its line'
    Assert-DocFactsRow $tree $script:DocFactsLinks 'codes|101|101||bound' 'a code the seed has'
    Assert-DocFactsRow $tree $script:DocFactsLinks 'codes|250|||missing in code' 'a code no seed row has'
    Assert-DocFactsRow $tree $script:DocFactsLinks 'codes||103||missing in doc' 'a seeded code the document leaves out'
    Assert-DocFactsRow $tree $script:DocFactsLinks 'SalesType|1|1|Retail|bound' 'an enum member by its value'
    Assert-DocFactsRow $tree $script:DocFactsLinks 'SalesType||2|Bulk|missing in doc' 'a member the document leaves out'
}

Test-Case 'docfacts: a seeded table is a typed view, and a type declared twice is listed with its members' {
    $tree = Use-Tree (Get-DocFactsTree)
    Assert-Exit (Invoke-DocFactsMap $tree) 0
    Assert-DocFactsRow $tree "SELECT 'v=' || ProductID || '|' || Name FROM seed_Sales_Products" '103|Old' 'a column read by name, not by position'
    Assert-DocFactsRow $tree "SELECT 'v=' || name || '|' || places || '|' || members_differ || '|' || detail FROM duplicate_types" 'ProductIDs|2|1|{Basic=101, Plus=102} at App/Ids.cs:1; {Basic=101} at App/Copy.cs:1' 'two copies that disagree'
}

Test-Case 'docfacts: a document that moved is checked again on a tree that did not' {
    $tree = Use-Tree (Get-DocFactsTree)
    Assert-Exit (Invoke-DocFactsMap $tree) 0
    $doc = Join-Path $tree 'docs/guide.md'
    [System.IO.File]::WriteAllText($doc, [System.IO.File]::ReadAllText($doc).Replace('250\*\* | Ghost', '102\*\* | Plus'))
    $again = Invoke-DocFactsMap $tree
    Assert-Exit $again 0
    Assert-Line $again 'the facts config or its snapshots moved'
    Assert-DocFactsRow $tree $script:DocFactsLinks 'codes|102|102||bound' 'the code the document now lists'
}

Test-Case 'docfacts: a config that is wrong fails the run, and --facts-pull has nothing to fetch for a file source' {
    $tree = Use-Tree (Get-DocFactsTree)
    $config = Join-Path $tree 'structuregate.facts.json'
    $pull = Invoke-Gate --facts-pull --facts-config $config
    Assert-Exit $pull 0
    Assert-Line $pull 'guide: a `file` source, kept by hand - nothing to pull'
    Assert-Exit (Invoke-Gate --facts-pull --facts-config (Join-Path $tree 'absent.json')) 2
    [System.IO.File]::WriteAllText($config, '{ "sources": [], "facts": [ { "source": "nowhere", "section": "x", "key": "k" } ] }')
    $bad = Invoke-DocFactsMap $tree
    Assert-Exit $bad 1
    Assert-Line $bad 'the facts config is wrong'
}
