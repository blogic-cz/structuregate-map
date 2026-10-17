<#
    An Angular HTTP call is linked to the C# action it reaches: `http_endpoints` (the attribute routes), `http_calls`
    (the `@angular/common/http` calls, a wrapper's callers standing for the wrapper's own call) and `http_links`.

    Its helpers are its own - `-Only HttpLinks` runs this suite alone. The compiler cache is the one TsRows fills.
#>

# The node rows' SQL (a list cell is `<table>__<column>`, never JSON text).
. (Join-Path $PSScriptRoot 'nodes/NodeSql.Helpers.ps1')

function Get-HttpLinksModules {
    $modules = Join-Path ([System.IO.Path]::GetTempPath()) 'sgtest-tsrows\node_modules'
    if (-not (Get-Command node -ErrorAction SilentlyContinue)) { return $null }
    if ((Test-Path (Join-Path $modules 'typescript\package.json')) -and
        (Test-Path (Join-Path $modules '@angular\compiler\package.json'))) { return $modules }
    if (-not (Get-Command npm -ErrorAction SilentlyContinue)) { return $null }
    $cache = Split-Path $modules
    [void](New-Item -ItemType Directory -Path $cache -Force)
    & npm install --no-save --silent --prefix $cache 'typescript@5' '@angular/compiler@17' '@angular/core@17' '@angular/router@17' '@ngrx/store@17' 2>&1 | Out-Null
    if (Test-Path (Join-Path $modules '@angular\compiler\package.json')) { return $modules }
    return $null
}

$script:HttpLinksModules = Get-HttpLinksModules
if (-not $script:HttpLinksModules) { Write-Host '    (no node/npm-installed typescript + @angular/compiler - the http link cases are not run)' }
if (-not (Get-Command dotnet -ErrorAction SilentlyContinue)) { Write-Host '    (no dotnet on this machine - the http link cases are not run)'; $script:HttpLinksModules = $null }

function New-HttpLinksTree {
    $nl = [string][char]10
    return Use-Tree @{
        'web/angular.json' = '{"projects":{"shop":{"projectType":"application","root":"apps/shop",' +
            '"sourceRoot":"apps/shop/src","architect":{"build":{"options":{"tsConfig":"apps/shop/tsconfig.app.json"}}}}}}'
        'web/apps/shop/tsconfig.app.json' = '{"compilerOptions":{"strict":true},"include":["src/**/*.ts"]}'
        'web/node_modules/@angular/common/package.json' = '{"name":"@angular/common","version":"17.0.0"}'
        'web/node_modules/@angular/common/http/package.json' = '{"types":"./index.d.ts"}'
        'web/node_modules/@angular/common/http/index.d.ts' = "export declare class HttpClient {$nl" +
            "  get<T>(url: string, options?: any): any; post<T>(url: string, body: any, options?: any): any;$nl" +
            "  put<T>(url: string, body: any, options?: any): any; delete<T>(url: string, options?: any): any;$nl}$nl" +
            "export declare class HttpHeaders { get(name: string): string | null; }$nl"
        # A SINGLE-QUOTED here-string: a double-quoted `${this.x}` is a PowerShell variable and expands to nothing.
        'web/apps/shop/src/api.service.ts' = @'
import { HttpClient, HttpHeaders } from '@angular/common/http';
export class ApiService {
  readonly baseUrl = (globalThis as any).config.url;
  readonly baseApiUrl = `${this.baseUrl}api/`;
  readonly calcUrl = `${this.baseApiUrl}calculations`;
  constructor(private httpClient: HttpClient) {}
  post<T>(url: string, body: any) { return this.httpClient.post<T>(url, body); }
  save(d: any) { return this.post(`${this.calcUrl}/save-calculation`, d); }
  load(id: number) { return this.httpClient.get(`${this.calcUrl}/${id}`); }
  ping() { return this.httpClient.get(`${this.baseUrl}absolute/ping`); }
  orders() { return this.httpClient.get(`${this.baseApiUrl}orders/list`); }
  gone() { return this.httpClient.delete(`${this.calcUrl}/nothing-here`); }
  // a header read: `get` on another class of the same package is no request
  header(h: HttpHeaders) { return h.get('content-disposition'); }
}
'@
        'api/Api.csproj' = '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework></PropertyGroup></Project>'
        'api/Controllers.cs' = "namespace Demo.Api;$nl" +
            "public class RouteAttribute : System.Attribute { public RouteAttribute(string t) {} }$nl" +
            "public class HttpGetAttribute : System.Attribute { public HttpGetAttribute() {} public HttpGetAttribute(string t) {} }$nl" +
            "public class HttpPostAttribute : System.Attribute { public HttpPostAttribute() {} }$nl" +
            "public class HttpDeleteAttribute : System.Attribute { public HttpDeleteAttribute() {} }$nl" +
            "public abstract class ApiControllerBase { protected const string ApiRouteBase = `"api/`"; }$nl" +
            "[Route(ApiRouteBase + `"calculations`")]$nl" +
            "public class CalculationController : ApiControllerBase {$nl" +
            "    [HttpPost] [Route(`"save-calculation`")] public object Save(object dto) => dto;$nl" +
            "    [HttpGet(`"{id}`")] public object Get(int id) => id;$nl" +
            "    [HttpGet] [Route(`"/absolute/ping`")] public string Ping() => `"`";$nl}$nl" +
            "[Route(`"api/[controller]`")]$nl" +
            "public class OrdersController : ApiControllerBase { [HttpGet(`"[action]`")] public object List() => 1; [HttpGet(`"{code}`")] public object One(string code) => code; }$nl"
    }
}

function Invoke-HttpLinksMap([string]$Tree) {
    $db = Join-Path $Tree 'map.sqlite'
    $run = Invoke-Gate --root $Tree --ext '.cs,.ts' --map-sqlite $db --ts-node-modules $script:HttpLinksModules
    Assert-Exit $run 0
    return $run
}

function Get-HttpLinksRows([string]$Tree, [string]$Sql) {
    $r = Invoke-Gate --map-query (Join-Path $Tree 'map.sqlite') --width 0 --sql $Sql
    Assert-Exit $r 0
    return $r
}

if ($script:HttpLinksModules) {

# A tree per case: the harness removes a case's trees when it ends.
function Get-HttpLinksTree {
    $tree = New-HttpLinksTree
    $script:HttpLinksRun = Invoke-HttpLinksMap $tree
    return $tree
}

Test-Case 'httplinks: a C# endpoint carries its class route, constants folded and tokens replaced' {
    $tree = Get-HttpLinksTree
    $r = Get-HttpLinksRows $tree "SELECT 'ep=' || verb || '|' || template || '|' || cls || '.' || method AS e FROM http_endpoints"
    Assert-Line $r 'ep=POST|api/calculations/save-calculation|CalculationController.Save'
    Assert-Line $r 'ep=GET|api/calculations/{id}|CalculationController.Get'
    Assert-Line $r 'ep=GET|absolute/ping|CalculationController.Ping'
    Assert-Line $r 'ep=GET|api/Orders/List|OrdersController.List'
    $v = Get-HttpLinksRows $tree "SELECT 'av=' || $(Get-NodeRootListSql 'decorators__arg_values' 'decorators.id') AS a FROM decorators WHERE name = 'Route' AND target_kind = 'class' AND target = 'CalculationController'"
    Assert-Line $v 'av=["api/calculations"]'
}

Test-Case 'httplinks: a call through a wrapper and a direct call each reach their endpoint, and the wrapper itself is no call' {
    $tree = Get-HttpLinksTree
    $r = Get-HttpLinksRows $tree ("SELECT 'link=' || h.verb || '|' || m.name || '->' || e.cls || '.' || e.method || '|' || l.how AS x " +
        "FROM http_links l JOIN http_calls h ON h.id = l.call JOIN http_endpoints e ON e.id = l.endpoint JOIN members m ON m.id = h.member")
    Assert-Line $r 'link=POST|save->CalculationController.Save|exact'
    Assert-Line $r 'link=GET|load->CalculationController.Get|exact'
    Assert-Line $r 'link=GET|ping->CalculationController.Ping|exact'
    # `api/Orders/{code}` takes `list` as a value too, but `List` names it: the endpoint the url names best wins.
    Assert-Line $r 'link=GET|orders->OrdersController.List|exact'
    $n = Get-HttpLinksRows $tree "SELECT 'calls=' || count(*) AS c FROM http_calls"
    # five requests; the `HttpHeaders.get` beside them is no request
    Assert-Line $n 'calls=5'
    $u = Get-HttpLinksRows $tree "SELECT 'unmatched=' || count(*) AS c FROM http_calls h WHERE NOT EXISTS (SELECT 1 FROM http_links l WHERE l.call = h.id)"
    Assert-Line $u 'unmatched=1'
}

# NOT RUN AGAIN OVER THE SAME ROWS (`rows/http/inputs.rs`): a comment moves no row the pass reads, so it is said again from
# the run before; a url the service builds and a C# route each move one, and it runs again - a replay there kept the old url.
Test-Case 'httplinks: the pass is said again over the rows it read and runs again when a url or a route moves' {
    $tree = Get-HttpLinksTree
    $trace = Join-Path $tree 'map.sqlite.last-run.jsonl'
    $replayed = { [bool](Select-String -LiteralPath $trace -SimpleMatch 'structuregate.deep.http_replayed' -Quiet) }
    $rows = "SELECT 'row=' || verb || '|' || url AS r FROM http_calls UNION ALL SELECT 'row=' || verb || '|' || template FROM http_endpoints"
    $before = Get-HttpLinksRows $tree $rows
    $service = Join-Path $tree 'web/apps/shop/src/api.service.ts'
    Add-Content -LiteralPath $service -Value '// demo: one comment'
    [void](Invoke-HttpLinksMap $tree)
    Assert-Equal (& $replayed) $true 'replayed after a comment'
    Assert-Equal (Get-HttpLinksRows $tree $rows).Text $before.Text 'the rows after a comment'
    Set-Content -LiteralPath $service -Value ((Get-Content -LiteralPath $service -Raw).Replace('orders/list', 'orders/all')) -NoNewline
    [void](Invoke-HttpLinksMap $tree)
    Assert-Equal (& $replayed) $false 'replayed after a url moved'
    $after = Get-HttpLinksRows $tree $rows
    Assert-Line $after 'orders/all'
    Assert-NoLine $after 'orders/list'
    $api = Join-Path $tree 'api/Controllers.cs'
    Set-Content -LiteralPath $api -Value ((Get-Content -LiteralPath $api -Raw).Replace('[HttpGet("[action]")] public object List()', '[HttpGet("all")] public object List()')) -NoNewline
    [void](Invoke-HttpLinksMap $tree)
    Assert-Equal (& $replayed) $false 'replayed after a route moved'
    Assert-Line (Get-HttpLinksRows $tree $rows) 'row=GET|api/Orders/all'
}

}

if (Get-Command dotnet -ErrorAction SilentlyContinue) {

Test-Case 'httplinks: a C#-only tree has its endpoints, one per class route' {
    $nl = [string][char]10
    $tree = Use-Tree @{
        'api/Api.csproj' = '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework></PropertyGroup></Project>'
        'api/Orders.cs' = "namespace Demo.Api;$nl" +
            "public class RouteAttribute : System.Attribute { public RouteAttribute(string t) {} }$nl" +
            "public class HttpGetAttribute : System.Attribute { public HttpGetAttribute(string t) {} }$nl" +
            "public class HttpPostAttribute : System.Attribute { public HttpPostAttribute() {} }$nl" +
            "[Route(`"api/v1/orders`")]$nl[Route(`"api/v2/orders`")]$nl" +
            "public class OrdersController {$nl" +
            "    [HttpGet(`"archived`")] public object Archived(int page) => page;$nl" +
            "    [HttpPost] public object Create() => 1;$nl" +
            "    [HttpGet(`"/health`")] public object Health() => 1;$nl}$nl"
    }
    $db = Join-Path $tree 'map.sqlite'
    Assert-Exit (Invoke-Gate --root $tree --ext '.cs' --map-sqlite $db) 0
    $r = Get-HttpLinksRows $tree "SELECT 'ep=' || verb || '|' || template || '|' || method AS e FROM http_endpoints"
    Assert-Line $r 'ep=GET|api/v1/orders/archived|Archived'
    Assert-Line $r 'ep=GET|api/v2/orders/archived|Archived'
    Assert-Line $r 'ep=POST|api/v1/orders|Create'
    Assert-Line $r 'ep=POST|api/v2/orders|Create'
    # an absolute template ignores the class routes: one endpoint, not one per prefix
    $n = Get-HttpLinksRows $tree "SELECT 'n=' || count(*) AS c FROM http_endpoints"
    Assert-Line $n 'n=5'
    $c = Get-HttpLinksRows $tree "SELECT 'calls=' || (SELECT count(*) FROM http_calls) || '|' || (SELECT count(*) FROM http_links) AS c"
    Assert-Line $c 'calls=0|0'
}

}
