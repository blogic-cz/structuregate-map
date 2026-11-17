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
    # EVERY SECTION OF THE SOURCE IS A ROW, facts or not: its path, its text, each table cell by row and column.
    $sections = Get-DocApiRows $db "SELECT 'v=' || n || '|' || level || '|' || path FROM doc_sections ORDER BY n"
    Assert-Equal ($sections -join ',') '1|1|Items API,2|2|Items API > Fields,3|2|Items API > Columns' 'the sections'
    Assert-DocApiRows $db ("SELECT 'v=' || s.path || '|' || t.tbl || '|' || t.row || '|' || t.col || '|' || t.value FROM doc_tables t " +
        "JOIN doc_sections s ON s.id = t.section") @('Items API > Fields|1|0|1|Field', 'Items API > Fields|1|3|1|weight', 'Items API > Columns|1|2|2|text') 'the cells'
    $said = Get-DocApiRows $db "SELECT 'v=' || count(*) FROM doc_sections WHERE n = 1 AND text LIKE '%Old clients still call%'"
    Assert-Equal ($said -join ',') '1' 'the text of the first section'
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
        # THE MAP IS OFFERED AS A TOOL, and the prompt says when to use it.
        if (@($asked.tools).Count -ne 1 -or $asked.tools[0].type -ne 'sqlite_query' -or $asked.tools[0].path -ne $db -or -not $asked.system_prompt.Contains('map_query')) {
            throw "the question did not offer the map: $($asked.tools | ConvertTo-Json -Depth 4 -Compress)"
        }
        Assert-Line $r 'map_query offered, calls not reported)'
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

Test-Case 'docfactsapi: a field table under a routed heading is matched to the type that endpoint takes or returns' {
    if (-not (Get-Command dotnet -ErrorAction SilentlyContinue)) { Write-Host '    (no dotnet - skipped)'; return }
    $nl = $script:DocApiNl
    $three = 'public string TicketCode { get; set; } = "";' + ' public int ExpiresIn { get; set; }' + ' public string UserName { get; set; } = "";'
    # TWO CLASSES WITH THE SAME PROPERTY NAMES: only the resolved return type tells which one an endpoint returns.
    $tree = Use-Tree @{
        'App/App.csproj' = '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework></PropertyGroup></Project>'
        'App/Auth.cs' = "namespace Demo.Api;$nl" +
            "public class Envelope<T> { public T Data { get; set; } = default!; public int Code { get; set; } }$nl" +
            "public class ProfileDto { $three }$nl" +
            "public class TicketDto { $three }$nl" +
            "public class LoginDto { public string UserName { get; set; } = `"`"; public string Password { get; set; } = `"`"; }$nl" +
            # AN ACTION OF A GENERIC BASE takes its type parameter, bound by the controller serving it.
            "public class WidgetDto { public string Colour { get; set; } = `"`"; public int Weight { get; set; } }$nl" +
            "public class PaintDto { public string Colour { get; set; } = `"`"; public int Weight { get; set; } }$nl" +
            "public abstract class JobBase<TA> { [HttpPost] [Route(`"run-all`")] public object RunAll(TA item) => item!; }$nl" +
            "[Route(`"api/widgets`")] public class WidgetsController : JobBase<WidgetDto> { }$nl" +
            # A ROUTE THE TREE CALLS: a wrapper of HttpClient, its url from a helper the base class has, two response types.
            "public class ApiClient { private readonly System.Net.Http.HttpClient http = new();$nl" +
            "    public async System.Threading.Tasks.Task<TResp?> PostAsync<TReq, TResp>(string url, TReq body) {$nl" +
            "        var r = await System.Net.Http.Json.HttpClientJsonExtensions.PostAsJsonAsync(http, url, body);$nl" +
            "        return await System.Net.Http.Json.HttpContentJsonExtensions.ReadFromJsonAsync<TResp>(r.Content); } }$nl" +
            "public class OrderA { public string OrderNo { get; set; } = `"`"; public string Status { get; set; } = `"`"; }$nl" +
            "public class OrderB { public string OrderNo { get; set; } = `"`"; public int Score { get; set; } }$nl" +
            "public abstract class ServiceBase { protected string Prefix() => System.Environment.GetEnvironmentVariable(`"BASE`") ?? `"`";$nl" +
            "    protected string DetailUrl() => `$`"{Prefix()}/demo/orders/lookup`"; }$nl" +
            "public class AService : ServiceBase { private readonly ApiClient c = new(); public object Run(LoginDto q) => c.PostAsync<LoginDto, OrderA>(DetailUrl(), q); }$nl" +
            "public class BService : ServiceBase { private readonly ApiClient c = new(); public object Run(LoginDto q) => c.PostAsync<LoginDto, OrderB>(DetailUrl(), q); }$nl" +
            "[Route(`"api/auth`")]$nl" +
            "public class AuthController {$nl" +
            "    [HttpPost(`"ticket`")] public System.Threading.Tasks.Task<Envelope<TicketDto>> Ticket(LoginDto login) => null!;$nl" +
            "    [HttpGet(`"me`")]$nl    [ProducesResponseType(typeof(ProfileDto), 200)]$nl    public object Me() => 1;$nl}$nl"
        'docs/api.md' = "# Auth$nl$nl## 2.1 /api/auth/ticket POST$nl$nl### Request$nl$nl| Name | Meaning |$nl|---|---|$nl| userName | who |$nl| password | secret |$nl$nl" +
            "### Response$nl$nl| Name | Meaning |$nl|---|---|$nl| ticketCode | the ticket |$nl| expiresIn | seconds |$nl| userName | who |$nl$nl" +
            "## 2.2 GET /api/auth/me$nl$nl| Name | Meaning |$nl|---|---|$nl| ticketCode | the ticket |$nl| expiresIn | seconds |$nl| userName | who |$nl$nl" +
            "## 2.3 POST /api/widgets/run-all$nl$nl| Field | Meaning |$nl|---|---|$nl| colour | paint |$nl| weight | grams |$nl$nl" +
            "## 3.1 POST /api/demo/orders/lookup$nl$nl| Item | Meaning |$nl|---|---|$nl| orderNo | which |$nl| status | state |$nl| score | rank |$nl"
        'structuregate.facts.json' = '{ "sources": [ { "name": "api", "kind": "file", "path": "docs/api.md" } ] }'
    }
    $db = Join-Path $tree 'api.sqlite'
    Assert-Exit (Invoke-Gate --root $tree --ext '.cs' --map-sqlite $db) 0
    $s = Invoke-Gate --facts-suggest $db --facts-config (Join-Path $tree 'structuregate.facts.json')
    Assert-Exit $s 0
    Assert-Line $s 'api: table 1 under "Request", column 1 "Name": 2/2 values = property Demo.Api.LoginDto (2 code values, the request type of POST /api/auth/ticket)'
    Assert-Line $s 'api: table 1 under "Response", column 1 "Name": 3/3 values = property Demo.Api.TicketDto (3 code values, the response type of POST /api/auth/ticket)'
    Assert-Line $s 'api: table 1 under "2.2 GET /api/auth/me", column 1 "Name": 3/3 values = property Demo.Api.ProfileDto (3 code values, the response type of GET /api/auth/me)'
    Assert-Line $s 'api: table 1 under "2.3 POST /api/widgets/run-all", column 1 "Field": 2/2 values = property Demo.Api.WidgetDto (2 code values, the request type of POST /api/widgets/run-all)'
    Assert-Line $s 'values = property Demo.Api.OrderA (2 code values, the outbound response type of POST /demo/orders/lookup)'
    Assert-Line $s 'values = property Demo.Api.OrderB (2 code values, the outbound response type of POST /demo/orders/lookup)'
}

# A PROPERTY TABLE AS ONE IS WRITTEN for a type and the type nested in it: a name a named attribute argument gives
# (`PropertyName =`, `[DataMember(Name =)]`), a property a base class declares, a child row written `-> Name`, and the
# table linked to both types - a row either type holds is bound, and missing in code only when neither does.
Test-Case 'docfactsapi: a property table matches JSON names, inherited properties and every type its fact is linked to' {
    if (-not (Get-Command dotnet -ErrorAction SilentlyContinue)) { Write-Host '    (no dotnet - skipped)'; return }
    $nl = $script:DocApiNl
    # THE ARROWS A DOCUMENT WRITES, built from their code points: a 5.1 host reads a BOM-less script as ANSI.
    $em, $en, $to = [string][char]0x2014, [string][char]0x2013, [string][char]0x2192
    $tree = Use-Tree @{
        'App/App.csproj' = '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework></PropertyGroup></Project>'
        'App/Ticket.cs' = "namespace Demo.Api;$nl" + "using System.Runtime.Serialization;$nl" +
            "public class JsonPropertyAttribute : System.Attribute { public string PropertyName { get; set; } = `"`"; }$nl" +
            "public abstract class RequestBase<T> { public T Request { get; set; } = default!; public string Key { get; set; } = `"`"; }$nl" +
            "public class TicketDto : RequestBase<string> {$nl" +
            "    [JsonProperty(PropertyName = `"access_code`")] public string AccessCode { get; set; } = `"`";$nl" +
            "    [DataMember(Name = `"valid_for`")] public int ValidFor { get; set; }$nl" +
            "    public PartDto Part { get; set; } = new();$nl" +
            "    public System.Collections.Generic.List<HolderDto> Holders { get; set; } = new();$nl" +
            "    public BatchDto Batch { get; set; } = new();$nl}$nl" +
            "public class BatchDto { public OriginDto Origin { get; set; } = new(); public int Amount { get; set; } public string Region { get; set; } = `"`";" +
            " public int Total { get; set; } }$nl" +
            "public class OriginDto { public string Maker { get; set; } = `"`"; }$nl" +
            "public class PartDto { public int PartNo { get; set; } }$nl" +
            "public class HolderDto { public string ClientRef { get; set; } = `"`"; public AddressDto[] Addresses { get; set; } = [];" +
            " public string Note { get; set; } = `"`"; }$nl" +
            "public class AddressDto { public string Street { get; set; } = `"`"; }$nl" +
            "public class OtherDto { public string Note { get; set; } = `"`"; }$nl"
        'docs/ticket.md' = "# Ticket$nl$nl## Fields$nl$nl| Field | Meaning |$nl|---|---|$nl| access_code | a |$nl| valid_for | b |$nl" +
            "| Request | c |$nl| Key | d |$nl| Part | e |$nl| -> PartNo | f |$nl| colour | g |$nl" +
            "| Holders | h |$nl| $em>ClientRef | i |$nl| -> Addresses | j |$nl| -> -> Street | k |$nl| $to Note | l |$nl| $en> Street | m |$nl" +
            "| Batch | n |$nl| -> Origin | o |$nl| -> Maker | p |$nl| -> Amount | q |$nl| Region | r |$nl| -> Total | s |$nl| -> Key | t |$nl"
        'structuregate.facts.json' = '{ "sources": [ { "name": "api", "kind": "file", "path": "docs/ticket.md" } ],' +
            ' "facts": [ { "name": "fields", "source": "api", "section": "Fields", "key": "Field" } ],' +
            ' "links": [ { "facts": "fields", "to": { "kind": "property", "symbol": "Demo.Api.TicketDto" } },' +
            ' { "facts": "fields", "to": { "kind": "property", "symbol": "Demo.Api.PartDto" } } ] }'
    }
    $db = Join-Path $tree 'api.sqlite'
    Assert-Exit (Invoke-Gate --root $tree --ext '.cs' --map-sqlite $db --facts-config (Join-Path $tree 'structuregate.facts.json')) 0
    Assert-DocApiRows $db $script:DocApiLinks @(
        'fields|access_code|access_code|bound', 'fields|valid_for|valid_for|bound', 'fields|Request|Request|bound',
        'fields|Key|Key|bound', 'fields|Part|Part|bound', 'fields|-> PartNo|PartNo|bound', 'fields|colour||missing in code',
        'fields|Holders|Holders|bound', "fields|$em>ClientRef|ClientRef|bound", 'fields|-> -> Street|Street|bound', "fields|$to Note|Note|bound"
    ) 'each row against both linked types, a child row against its parent row''s type first'
    $missing = Get-DocApiRows $db "SELECT 'v=' || key FROM doc_links WHERE status = 'missing in code' ORDER BY rowid"
    Assert-Equal ($missing -join ',') "colour,$en> Street" 'the rows no linked type holds'
    # A ROW NESTED WRONG IS READ WHERE IT BELONGS AND SAID SO: `-> Maker` is the Origin row's (an arrow short), `Region`
    # the Batch row's (no arrow between its children), `-> Key` a top-level row (only the linked type holds it), and the
    # rows after each stand where it was read.
    $fixed = Get-DocApiRows $db "SELECT 'v=' || key || '|' || status || '|' || coalesce(nesting, '-') FROM doc_links WHERE key IN ('-> Maker', 'Region', '-> Amount', '-> Total', '-> Key') ORDER BY rowid"
    Assert-Equal ($fixed -join ',') '-> Maker|bound, nesting fixed|2,-> Amount|bound|-,Region|bound, nesting fixed|1,-> Total|bound|-,-> Key|bound, nesting fixed|0' 'the rows nested wrong'
    # THE PARENT'S TYPE, NOT A NAME ELSEWHERE: an arrowed `Note` under Holders is HolderDto's, never OtherDto's.
    $note = Get-DocApiRows $db "SELECT 'v=' || m.cls FROM doc_links l JOIN members m ON m.id = l.target WHERE l.key = '$to Note'"
    Assert-Equal ($note -join ',') 'HolderDto' 'the type a child row is read in'
    $undocumented = Get-DocApiRows $db "SELECT 'v=' || value FROM doc_links WHERE status = 'missing in doc'"
    Assert-Equal ($undocumented -join ',') '' 'the properties no row names'
}
