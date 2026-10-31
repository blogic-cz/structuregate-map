<#
    THE C# HTTP CALLS (#143, `rust/fbtcore/src/rows/http/cscalls.rs`): a C#-only tree's `HttpClient` calls are `http_calls`
    rows, as an Angular tree's are - the verb from the method's name, or from the `HttpRequestMessage` a `SendAsync` is
    handed (built in its argument, or with the local it names, in THAT method; its arguments or its object initializer's
    `Method`/`RequestUri`); the url folded, a `new Uri(url, ..)` to its string, an interpolation's holes
    `{}`, the query cut; and a method whose url is its own parameter is a wrapper, its callers the rows, `via` its
    `functions` id - an extension method over `HttpClient` included.
    Its helpers are its own - `-Only HttpCsCalls` runs this suite alone.
#>

Test-Case 'httpcscalls: direct calls, a SendAsync over a request message and an extension wrapper are http_calls rows' {
    $tree = Use-Tree @{
        'App/App.csproj' = '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework></PropertyGroup></Project>'
        'App/Api.cs' = @'
using System;
using System.Net.Http;
using System.Net.Http.Json;
using System.Threading;
using System.Threading.Tasks;
namespace Demo;

public static class HttpClientExtensions
{
    public static async Task<T?> PostJsonAsync<T>(this HttpClient client, string url, object body, CancellationToken ct)
    {
        using var request = new HttpRequestMessage(HttpMethod.Post, url) { Content = JsonContent.Create(body) };
        using var response = await client.SendAsync(request, ct);
        return await response.Content.ReadFromJsonAsync<T>(cancellationToken: ct);
    }
}

public class OrderApi(HttpClient client)
{
    private const string Root = "/lines";
    public Task<string> Health(CancellationToken ct) => client.GetStringAsync("/health", ct);
    public Task<HttpResponseMessage> One(int id) => client.GetAsync($"{Root}/{id}?full=1");
    public Task<Order[]?> Search(CancellationToken ct) => client.PostJsonAsync<Order[]>("/orders/search", new { Page = 1 }, ct);
    public Task<HttpResponseMessage> Raw() => client.SendAsync(new HttpRequestMessage(HttpMethod.Put, "/lines/raw"));
    public Task<HttpResponseMessage> Drop()
    {
        var request = new HttpRequestMessage(HttpMethod.Delete, "/lines/old");
        return client.SendAsync(request);
    }
    public Task<HttpResponseMessage> Patch()
    {
        var request = new HttpRequestMessage { Method = HttpMethod.Patch, RequestUri = new Uri("/lines/patch", UriKind.Relative) };
        return client.SendAsync(request);
    }
}

public record Order(int OrderId);
'@
    }
    $db = Join-Path $tree 'map.sqlite'
    Assert-Exit (Invoke-Gate --root $tree --ext .cs --map-sqlite $db) 0
    $q = Invoke-Gate --map-query $db --width 0 --limit 0 --sql ("SELECT 'v=' || c.line || '|' || c.verb || '|' || c.url || '|' || " +
        "coalesce((SELECT f.name FROM functions f WHERE f.id = c.via), '') || '|' || (SELECT f.name FROM functions f WHERE f.id = c.member) AS r " +
        "FROM http_calls c ORDER BY c.line")
    Assert-Exit $q 0
    $rows = @($q.Lines | Where-Object { $_.TrimStart().StartsWith('v=') } | ForEach-Object { $_.Trim().Substring(2) })
    $want = @('21|GET|/health||Health', '22|GET|/lines/{}||One', '23|POST|/orders/search|PostJsonAsync|Search', '24|PUT|/lines/raw||Raw',
        '28|DELETE|/lines/old||Drop', '33|PATCH|/lines/patch||Patch')
    if (($rows -join "`n") -ne ($want -join "`n")) { throw "http_calls:`nexpected:`n$($want -join "`n")`ngot:`n$($rows -join "`n")" }
}
