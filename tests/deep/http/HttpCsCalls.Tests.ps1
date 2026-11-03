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

# THE URL A C# CALL IS BUILT INTO (`CsTemplate.cs`, rows 35): `string.Empty` is the base address alone; a ternary is either arm and a
# local only ever appended to is its initializer, so both cut at the query to one path; a base the run time knows joined to the method's
# own parameter - by `+` or `string.Concat` - makes the method a wrapper, and each caller is a row with its own url.
Test-Case 'httpcscalls: an empty url, a ternary, an appended local and a base joined to a parameter each give their path' {
    $tree = Use-Tree @{
        'App/App.csproj' = '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework></PropertyGroup></Project>'
        'App/Api.cs' = @'
using System.Net.Http;
using System.Threading.Tasks;
namespace Demo;

public class LineApi(HttpClient client, string root)
{
    private readonly string _base = root;
    public Task<HttpResponseMessage> Ping() => client.GetAsync(string.Empty);
    public Task<HttpResponseMessage> Pick(bool all, int id)
    {
        var url = all ? "/lines/pick" : $"/lines/pick?id={id}";
        return client.GetAsync(url);
    }
    public Task<HttpResponseMessage> Page(int page, bool wide)
    {
        var url = $"/lines/page?page={page}";
        if (wide) url += "&wide=1";
        return client.GetAsync(url);
    }
    public Task<string> Fetch(string path) => client.GetStringAsync(_base + path);
    public Task<HttpResponseMessage> Send(string endpoint) => client.GetAsync(string.Concat(_base, endpoint));
    public Task<string> Based() => Fetch("/lines/based");
    public Task<HttpResponseMessage> Joined() => Send("/lines/joined");
}
'@
    }
    $db = Join-Path $tree 'map.sqlite'
    Assert-Exit (Invoke-Gate --root $tree --ext .cs --map-sqlite $db) 0
    $q = Invoke-Gate --map-query $db --width 0 --limit 0 --sql ("SELECT 'v=' || c.line || '|' || c.verb || '|' || c.url || '|' || " +
        "coalesce((SELECT f.name FROM functions f WHERE f.id = c.via), '') || '|' || (SELECT f.name FROM functions f WHERE f.id = c.member) AS r " +
        "FROM http_calls c ORDER BY c.line")
    Assert-Exit $q 0
    $rows = @($q.Lines | Where-Object { $_.TrimStart().StartsWith('v=') } | ForEach-Object { $_.Trim().Substring(2) })
    $want = @('8|GET|||Ping', '12|GET|/lines/pick||Pick', '18|GET|/lines/page||Page', '22|GET|/lines/based|Fetch|Based', '23|GET|/lines/joined|Send|Joined')
    if (($rows -join "`n") -ne ($want -join "`n")) { throw "http_calls:`nexpected:`n$($want -join "`n")`ngot:`n$($rows -join "`n")" }
}

# A FORMAT THE CALLER HANDS IN (`CsTemplate.cs`, rows 36): `string.Format(endpoint, args)` through a local makes the method a wrapper of
# `endpoint` that FORMATS it, so each caller's `{n}` is a hole `{}`; the wrapper asks with GET and DELETE, so its callers ask with ANY.
Test-Case 'httpcscalls: a url formatted from a parameter is a wrapper and each caller is a row with its holes' {
    $tree = Use-Tree @{
        'App/App.csproj' = '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework></PropertyGroup></Project>'
        'App/Api.cs' = @'
using System.Net.Http;
using System.Net.Http.Json;
using System.Threading.Tasks;
namespace Demo;

public class OrderApi(HttpClient client)
{
    private async Task<T?> Send<T>(string endpoint, bool drop, params object[] args)
    {
        var path = string.Format(endpoint, args);
        if (drop) return await client.DeleteFromJsonAsync<T>($"{path}");
        return await client.GetFromJsonAsync<T>($"{path}");
    }
    protected Task<T?> Read<T>(string endpoint, params object[] args) => Send<T>(endpoint, false, args);
    public Task<string?> One(int id) => Read<string>("/orders/{0}/lines/{1:D}?x={{y}}", id, 2);
    public Task<string?> Two(int id) => Send<string>("/orders/{0}", true, id);
}
'@
    }
    $db = Join-Path $tree 'map.sqlite'
    Assert-Exit (Invoke-Gate --root $tree --ext .cs --map-sqlite $db) 0
    $q = Invoke-Gate --map-query $db --width 0 --limit 0 --sql ("SELECT 'v=' || c.line || '|' || c.verb || '|' || c.url || '|' || " +
        "coalesce((SELECT f.name FROM functions f WHERE f.id = c.via), '') || '|' || (SELECT f.name FROM functions f WHERE f.id = c.member) AS r " +
        "FROM http_calls c ORDER BY c.line")
    Assert-Exit $q 0
    $rows = @($q.Lines | Where-Object { $_.TrimStart().StartsWith('v=') } | ForEach-Object { $_.Trim().Substring(2) })
    $want = @('15|ANY|/orders/{}/lines/{}|Read|One', '16|ANY|/orders/{}|Send|Two')
    if (($rows -join "`n") -ne ($want -join "`n")) { throw "http_calls:`nexpected:`n$($want -join "`n")`ngot:`n$($rows -join "`n")" }
}

# A URL A METHOD OF THE TREE RETURNS (`CsTemplate.cs`, rows 37): the call is what the method returns, folded where it is written with
# each parameter the caller's argument - an extension's receiver and `QueryHelpers.AddQueryString`'s url included; a caller's own
# parameter handed on keeps the caller a wrapper, and one nothing calls is its own row, its url as written. A parameter the method
# writes is not what the caller handed, and a virtual method may run another body: neither is followed.
Test-Case 'httpcscalls: a url a method of the tree returns is folded with the arguments its caller hands it' {
    $tree = Use-Tree @{
        'App/App.csproj' = '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework></PropertyGroup></Project>'
        'App/Query.cs' = @'
namespace Microsoft.AspNetCore.WebUtilities
{
    public static class QueryHelpers
    {
        public static string AddQueryString(string uri, string name, string value) => throw new System.NotSupportedException();
    }
}
'@
        'App/Api.cs' = @'
using System;
using System.Net.Http;
using System.Threading.Tasks;
using Microsoft.AspNetCore.WebUtilities;

namespace Demo
{
    public static class UrlExtensions
    {
        public static string WithQuery<T>(this T item, string url) => QueryHelpers.AddQueryString(url, "id", item!.ToString()!);
    }

    public class OrderApi(HttpClient client)
    {
        private const string Prefix = "/api/v2";
        private string Build(string uri) => Prefix + uri;
        public virtual string Open(string uri) => uri;
        public Task<HttpResponseMessage> One() => client.GetAsync(Build("/orders"));
        public Task<string> Two(int id)
        {
            var url = id.WithQuery("api/lines/find");
            return client.GetStringAsync(new Uri(url, UriKind.Relative));
        }
        public Task<HttpResponseMessage> Three(string path) => client.GetAsync(Build(path));
        public Task<HttpResponseMessage> Four() => Three("/items");
        public Task<HttpResponseMessage> Five() => client.GetAsync(Open("/open"));
        public Task<HttpResponseMessage> Six(int id) => client.GetAsync($"{client.BaseAddress}lines/{id}");
        public Task<HttpResponseMessage> Seven(string v)
        {
            v = v.Trim();
            return client.GetAsync($"/api/v2/release/{v}");
        }
        public Task<HttpResponseMessage> Eight() => Seven("x");
    }
}
'@
    }
    $db = Join-Path $tree 'map.sqlite'
    Assert-Exit (Invoke-Gate --root $tree --ext .cs --map-sqlite $db) 0
    $q = Invoke-Gate --map-query $db --width 0 --limit 0 --sql ("SELECT 'v=' || c.line || '|' || c.verb || '|' || c.url || '|' || " +
        "coalesce((SELECT f.name FROM functions f WHERE f.id = c.via), '') || '|' || (SELECT f.name FROM functions f WHERE f.id = c.member) AS r " +
        "FROM http_calls c ORDER BY c.line")
    Assert-Exit $q 0
    $rows = @($q.Lines | Where-Object { $_.TrimStart().StartsWith('v=') } | ForEach-Object { $_.Trim().Substring(2) })
    $want = @('18|GET|/api/v2/orders||One', '22|GET|api/lines/find||Two', '25|GET|/api/v2/items|Three|Four', '26|GET|*||Five', '27|GET|{}lines/{}||Six', '31|GET|/api/v2/release/{}||Seven')
    if (($rows -join "`n") -ne ($want -join "`n")) { throw "http_calls:`nexpected:`n$($want -join "`n")`ngot:`n$($rows -join "`n")" }
}
