<#
    Facts an API spec states without a table of codes: the routes its text names (in running text and in code spans)
    against the endpoints the C# half routes, a table of property names against a DTO's properties (named by their
    JSON attribute), a table of column names against a SQL table's columns. And `--facts-suggest`, which finds those
    parts of a document by exact values and writes them as candidates; and an uploaded Word file read as a snapshot.

    Its helpers are its own - `-Only DocFactsApi` runs this suite alone. It sorts after DocFacts and must not use its
    helpers either.
#>

$script:DocApiNl = [string][char]10

function Get-DocApiTree {
    $nl = $script:DocApiNl
    return @{
        'App/App.csproj' = '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework></PropertyGroup></Project>'
        'App/Items.cs' = "namespace Demo.Api;$nl" +
            "using System.Text.Json.Serialization;$nl" +
            "[Route(`"api/items`")]$nl" +
            "public class ItemsController {$nl" +
            "    [HttpGet] public object List() => 1;$nl" +
            "    [HttpGet(`"{id:int}`")] public object One(int id) => id;$nl" +
            "    [HttpPost] public object Add(ItemDto d) => d;$nl" +
            "    [HttpDelete(`"{id}`")] public object Drop(int id) => id;$nl}$nl" +
            # AN ACTION OF AN ABSTRACT GENERIC BASE is served by each controller deriving from it, under its own route.
            "public abstract class JobBase<TA, TB> { [HttpPost] [Route(`"run-all`")] public object RunAll(TA item) => item; }$nl" +
            "[Route(`"api/widgets`")] public class WidgetsController : JobBase<ItemDto, System.Collections.Generic.List<ItemDto>> { }$nl" +
            "public class ItemDto {$nl" +
            "    public int ItemId { get; set; }$nl" +
            "    [JsonPropertyName(`"displayName`")] public string Title { get; set; } = `"`";$nl" +
            "    public int Price { get; set; }$nl}$nl"
        'Db/Db.sqlproj' = '<Project DefaultTargets="Build"><ItemGroup><Build Include="Items.sql" /></ItemGroup></Project>'
        'Db/Items.sql' = "CREATE TABLE [dbo].[Items] ([ItemID] INT NOT NULL PRIMARY KEY, [Title] NVARCHAR(200) NOT NULL, [Price] INT NOT NULL)$nl"
        # THE SPEC AS ONE IS WRITTEN: routes in a sentence and in code spans, one the code does not serve, and two
        # tables keyed by names.
        'docs/api.md' = "# Items API$nl${nl}List them with ``GET /api/items``, read one by GET /api/items/{itemId} and add one by$nl" +
            "``POST /api/items``, run them with ``POST /api/widgets/run-all``. Old clients still call /api/legacy (see /docs/history).$nl$nl## Fields$nl$nl" +
            "| Field | Meaning |$nl|---|---|$nl| itemId | the key |$nl| displayName | what a person reads |$nl| weight | in grams |$nl$nl" +
            "## Columns$nl$nl| Column | Type |$nl|---|---|$nl| ItemID | int |$nl| Title | text |$nl"
        'structuregate.facts.json' = '{ "sources": [ { "name": "api", "kind": "file", "path": "docs/api.md" } ],' +
            ' "facts": [ { "name": "routes", "source": "api", "from": "routes", "prefix": "/api/" },' +
            ' { "name": "fields", "source": "api", "section": "Fields", "key": "Field" },' +
            ' { "name": "columns", "source": "api", "section": "Columns", "key": "Column" } ],' +
            ' "links": [ { "facts": "routes", "to": { "kind": "endpoint" } },' +
            ' { "facts": "fields", "to": { "kind": "property", "symbol": "Demo.Api.ItemDto" } },' +
            ' { "facts": "columns", "to": { "kind": "sql-column", "object": "dbo.Items" } } ] }'
    }
}

function Get-DocApiRows([string]$Db, [string]$Sql) {
    $r = Invoke-Gate --map-query $Db --sql $Sql --width 0 --limit 0
    Assert-Exit $r 0
    return @($r.Lines | Where-Object { $_.TrimStart().StartsWith('v=') } | ForEach-Object { $_.Trim().Substring(2) })
}

function Assert-DocApiRows([string]$Db, [string]$Sql, [string[]]$Expected, [string]$What) {
    $rows = Get-DocApiRows $Db $Sql
    foreach ($e in $Expected) {
        if ($rows -notcontains $e) { throw "$What`: no row '$e' in:`n$($rows -join "`n")" }
    }
}

$script:DocApiLinks = "SELECT 'v=' || l.facts || '|' || l.key || '|' || l.value || '|' || l.status FROM doc_links l"

Test-Case 'docfactsapi: routes in text, property and column names are checked against endpoints, a DTO and a table' {
    if (-not (Get-Command dotnet -ErrorAction SilentlyContinue)) { Write-Host '    (no dotnet - skipped)'; return }
    $tree = Use-Tree (Get-DocApiTree)
    $db = Join-Path $tree 'api.sqlite'
    $run = Invoke-Gate --root $tree --ext '.cs,.sql' --map-sqlite $db --facts-config (Join-Path $tree 'structuregate.facts.json')
    Assert-Exit $run 0
    Assert-DocApiRows $db $script:DocApiLinks @(
        'routes|GET /api/items|GET /api/items|bound',
        'routes|GET /api/items/{itemId}|GET /api/items/{id:int}|bound',
        'routes|POST /api/items|POST /api/items|bound',
        'routes|POST /api/widgets/run-all|POST /api/widgets/run-all|bound',
        'routes|/api/legacy||missing in code',
        'routes||DELETE /api/items/{id}|missing in doc',
        'fields|itemId|ItemId|bound',
        'fields|displayName|displayName|bound',
        'fields|weight||missing in code',
        'fields||Price|missing in doc',
        'columns|ItemID|ItemID|bound',
        'columns||Price|missing in doc'
    ) 'each fact against its code value'
    # THE ABSTRACT BASE ITSELF SERVES NOTHING: its action is no bare `run-all` endpoint of its own.
    $inherited = Get-DocApiRows $db "SELECT 'v=' || cls || '|' || template FROM http_endpoints WHERE method = 'RunAll'"
    Assert-Equal ($inherited -join ',') 'WidgetsController|api/widgets/run-all' 'the inherited action''s endpoints'
    # THE PREFIX KEEPS A ROUTE OF ANOTHER KIND OUT: /docs/history is no fact at all.
    $other = Get-DocApiRows $db "SELECT 'v=' || key FROM doc_facts WHERE key LIKE '%docs%'"
    if ($other.Count) { throw "a route outside the prefix became a fact: $($other -join ', ')" }

    # --facts-suggest FINDS THE SAME PARTS without being told: a config naming only the source.
    $only = Join-Path $tree 'suggest/structuregate.facts.json'
    New-Item -ItemType Directory -Force (Split-Path $only) | Out-Null
    Set-Content -LiteralPath $only -Value '{ "sources": [ { "name": "api", "kind": "file", "path": "../docs/api.md" } ] }' -Encoding UTF8
    $s = Invoke-Gate --facts-suggest $db --facts-config $only
    Assert-Exit $s 0
    Assert-Line $s 'api: routes the whole document: 4/6 values = endpoint every endpoint (5 code values)'
    Assert-Line $s 'api: table 1 under "Fields", column 1 "Field": 2/3 values = property Demo.Api.ItemDto (3 code values)'
    Assert-Line $s 'api: table 1 under "Columns", column 1 "Column": 2/2 values = sql-column dbo.Items (3 code values)'
    $written = Get-Content -Raw -LiteralPath (Join-Path $tree 'suggest/structuregate.facts.suggest.json') | ConvertFrom-Json
    $routes = @($written.candidates | Where-Object { $_.link.to.kind -eq 'endpoint' })[0]
    if ($routes.facts.from -ne 'routes' -or @($routes.unmatched) -join ',' -ne '/api/legacy,/docs/history') {
        throw "the route candidate is not the config entry it should be: $($routes | ConvertTo-Json -Depth 6 -Compress)"
    }

    # --facts-review ASKS THE CONFIGURED MODEL - here a fake LLM CLI that keeps the question and accepts the first.
    $saved = $env:STRUCTUREGATE_LLM
    try {
        $env:STRUCTUREGATE_LLM = ''
        Assert-Exit (Invoke-Gate --facts-suggest $db --facts-config $only --facts-review) 2
        $folder = Join-Path $tree 'fake-llm'
        New-Item -ItemType Directory -Force $folder | Out-Null
        $answer = '{"ok": true, "data": {"verdicts": [{"n": 1, "accept": true, "reason": "the spec lists the routes"}]}}'
        $request = Join-Path $folder 'request.json'
        if ($IsWindows -or $env:OS -eq 'Windows_NT') {
            $fake = Join-Path $folder 'ask-cli.cmd'
            Set-Content -LiteralPath $fake -Encoding ASCII -Value "@echo off`r`nmore > `"$request`"`r`necho $answer"
        } else {
            $fake = Join-Path $folder 'ask-cli'
            Set-Content -LiteralPath $fake -Encoding ASCII -Value "#!/bin/sh`ncat > '$request'`necho '$answer'"
            chmod +x $fake
        }
        $env:STRUCTUREGATE_LLM = $fake
        $r = Invoke-Gate --facts-suggest $db --facts-config $only --facts-review
        Assert-Exit $r 0
        Assert-Line $r 'api: routes the whole document: ACCEPTED - the spec lists the routes'
        $asked = Get-Content -Raw -LiteralPath $request | ConvertFrom-Json
        if (-not $asked.user_message.Contains('/api/legacy')) { throw "the question did not carry the candidates: $($asked.user_message)" }
        $fragment = Get-Content -Raw -LiteralPath (Join-Path $tree 'suggest/structuregate.facts.reviewed.json') | ConvertFrom-Json
        if (@($fragment.links).Count -ne 1 -or $fragment.links[0].to.kind -ne 'endpoint' -or $fragment.facts[0].from -ne 'routes') {
            throw "the fragment is not the one accepted candidate: $($fragment | ConvertTo-Json -Depth 6 -Compress)"
        }
    } finally {
        $env:STRUCTUREGATE_LLM = $saved
    }
}

Test-Case 'docfactsapi: an uploaded Word file is a snapshot like any other' {
    if (-not (Get-Command dotnet -ErrorAction SilentlyContinue)) { Write-Host '    (no dotnet - skipped)'; return }
    $tree = Use-Tree (Get-DocApiTree)
    # A .docx AS .NET WRITES A ZIP: the body deflated, a heading by a style whose id is not English.
    Add-Type -AssemblyName System.IO.Compression
    $w = 'xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"'
    $styles = "<w:styles $w><w:style w:styleId=`"Nadpis1`"><w:name w:val=`"heading 1`"/></w:style></w:styles>"
    $body = "<w:document $w><w:body><w:p><w:pPr><w:pStyle w:val=`"Nadpis1`"/></w:pPr><w:r><w:t>Columns</w:t></w:r></w:p>" +
        "<w:tbl><w:tr><w:tc><w:p><w:r><w:t>Column</w:t></w:r></w:p></w:tc></w:tr>" +
        "<w:tr><w:tc><w:p><w:r><w:t>Price</w:t></w:r></w:p></w:tc></w:tr><w:tr><w:tc><w:p><w:r><w:t>Colour</w:t></w:r></w:p></w:tc></w:tr></w:tbl></w:body></w:document>"
    $file = Join-Path $tree 'docs/spec.docx'
    $stream = [System.IO.File]::Create($file)
    $zip = New-Object System.IO.Compression.ZipArchive($stream, [System.IO.Compression.ZipArchiveMode]::Create)
    foreach ($entry in @(@('word/document.xml', $body), @('word/styles.xml', $styles))) {
        $writer = New-Object System.IO.StreamWriter($zip.CreateEntry($entry[0]).Open())
        $writer.Write($entry[1])
        $writer.Dispose()
    }
    $zip.Dispose()
    $stream.Dispose()
    $config = Join-Path $tree 'structuregate.facts.json'
    Set-Content -LiteralPath $config -Encoding UTF8 -Value ('{ "sources": [ { "name": "spec", "kind": "file", "path": "docs/spec.docx" } ],' +
        ' "facts": [ { "name": "columns", "source": "spec", "section": "Columns", "key": "Column" } ],' +
        ' "links": [ { "facts": "columns", "to": { "kind": "sql-column", "object": "dbo.Items" } } ] }')
    $db = Join-Path $tree 'api.sqlite'
    $run = Invoke-Gate --root $tree --ext '.cs,.sql' --map-sqlite $db --facts-config $config
    Assert-Exit $run 0
    Assert-DocApiRows $db $script:DocApiLinks @('columns|Price|Price|bound', 'columns|Colour||missing in code', 'columns||Title|missing in doc') 'the Word table against the columns'
}
