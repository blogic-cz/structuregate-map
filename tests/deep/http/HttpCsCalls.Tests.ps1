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
            v = v.ToUpperInvariant();
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

# A URL WHOSE ALTERNATIVES DISAGREE (`cscalls.rs::folded`): a ternary of two paths is a row for each, written in the call or handed to a
# wrapper; alternatives one path once cut at the query are one row.
Test-Case 'httpcscalls: a url of several paths is a row for each of them' {
    $tree = Use-Tree @{
        'App/App.csproj' = '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework></PropertyGroup></Project>'
        'App/Api.cs' = @'
using System.Net.Http;
using System.Threading.Tasks;
namespace Demo;

public class ItemApi(HttpClient client)
{
    private const string Root = "/api/";
    public Task<HttpResponseMessage> Pick(bool preview) => client.GetAsync(preview ? "/items/preview" : "/items/final");
    public Task<HttpResponseMessage> Send(string url) => client.PostAsync(Root + url, null);
    public Task<HttpResponseMessage> Choose(bool preview)
    {
        var url = preview ? "demo/draft" : "demo/final";
        return Send(url);
    }
    public Task<HttpResponseMessage> Same(bool all) => client.GetAsync(all ? "/same" : "/same?all=1");
    public Task<HttpResponseMessage> Mixed(bool fixedOne, string url) => client.GetAsync(fixedOne ? "/fixed" : Root + url);
    public Task<HttpResponseMessage> MixedUse() => Mixed(true, "x");
}
'@
    }
    $db = Join-Path $tree 'map.sqlite'
    Assert-Exit (Invoke-Gate --root $tree --ext .cs --map-sqlite $db) 0
    $q = Invoke-Gate --map-query $db --width 0 --limit 0 --sql ("SELECT 'v=' || c.line || '|' || c.verb || '|' || c.url || '|' || " +
        "coalesce((SELECT f.name FROM functions f WHERE f.id = c.via), '') || '|' || (SELECT f.name FROM functions f WHERE f.id = c.member) AS r " +
        "FROM http_calls c ORDER BY c.line, c.url")
    Assert-Exit $q 0
    $rows = @($q.Lines | Where-Object { $_.TrimStart().StartsWith('v=') } | ForEach-Object { $_.Trim().Substring(2) })
    $want = @('8|GET|/items/final||Pick', '8|GET|/items/preview||Pick', '13|POST|/api/demo/draft|Send|Choose',
        '13|POST|/api/demo/final|Send|Choose', '15|GET|/same||Same', '16|GET|/api/{}||Mixed', '16|GET|/fixed||Mixed')
    if (($rows -join "`n") -ne ($want -join "`n")) { throw "http_calls:`nexpected:`n$($want -join "`n")`ngot:`n$($rows -join "`n")" }
}

# A WRAPPER CALLED THROUGH ITS INTERFACE (`IApi.PostAsync` bound, `Api.PostAsync` runs) is a wrapper still, through two of
# them and a `Lazy<>`; and a url a helper returns from a ternary whose other arm only the run time knows is the arm it knows.
Test-Case 'httpcscalls: a wrapper called through its interface, and a ternary arm only the run time knows, still give the path' {
    $tree = Use-Tree @{
        'App/App.csproj' = '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework></PropertyGroup></Project>'
        'App/Api.cs' = @'
using System;
using System.Net.Http;
using System.Net.Http.Json;
using System.Threading.Tasks;
namespace Demo;

public interface ITransport { Task<T?> PostAsync<TReq, T>(TReq input, string url, HttpClient client); }
public class Transport : ITransport
{
    public async Task<T?> PostAsync<TReq, T>(TReq input, string url, HttpClient client)
    {
        var r = await client.PostAsJsonAsync("/v1" + url, input);
        return await r.Content.ReadFromJsonAsync<T>();
    }
}
public interface IGateway { Task<T?> SendAsync<TReq, T>(string target, TReq input); }
public class Gateway : IGateway
{
    private readonly Lazy<ITransport> lazy = new(() => new Transport());
    public Task<T?> SendAsync<TReq, T>(string address, TReq input) => lazy.Value.PostAsync<TReq, T>(input, address, new HttpClient());
}
public abstract class ClientBase
{
    protected virtual string Custom() => Environment.GetEnvironmentVariable("CUSTOM") ?? "";
    protected string Prefix() => Environment.GetEnvironmentVariable("BASE") ?? "";
    protected string OrdersUrl() => string.IsNullOrEmpty(Custom()) ? $"{Prefix()}/demo/orders/lookup" : Custom();
}
public class FirstClient : ClientBase
{
    private readonly IGateway gateway = new Gateway();
    public object Run() => gateway.SendAsync<int, string>(OrdersUrl(), 1);
}
public class SecondClient : ClientBase
{
    private readonly IGateway gateway = new Gateway();
    public object Run() => gateway.SendAsync<int, long>(OrdersUrl(), 2);
}
'@
    }
    $db = Join-Path $tree 'map.sqlite'
    Assert-Exit (Invoke-Gate --root $tree --ext .cs --map-sqlite $db) 0
    $q = Invoke-Gate --map-query $db --width 0 --limit 0 --sql ("SELECT 'v=' || c.line || '|' || c.verb || '|' || c.url || '|' || " +
        "coalesce((SELECT f.name FROM functions f WHERE f.id = c.via), '') || '|' || (SELECT f.qualname FROM functions f WHERE f.id = c.member) AS r " +
        "FROM http_calls c ORDER BY c.line, c.url")
    Assert-Exit $q 0
    $rows = @($q.Lines | Where-Object { $_.TrimStart().StartsWith('v=') } | ForEach-Object { $_.Trim().Substring(2) })
    $want = @('31|POST|/v1{}/demo/orders/lookup|SendAsync|Demo.FirstClient.Run', '36|POST|/v1{}/demo/orders/lookup|SendAsync|Demo.SecondClient.Run')
    if (($rows -join "`n") -ne ($want -join "`n")) { throw "http_calls:`nexpected:`n$($want -join "`n")`ngot:`n$($rows -join "`n")" }
}

# A LEAF WRAPPER AS ONE IS WRITTEN: the request built in either arm of an `if` into a local declared without its value, the url a
# joiner of a run-time base and the parameter that trims both edges - early returns of either one alone, the parameters trimmed in
# place, or `+` - and a ternary passing an absolute url through: the caller's path, one `/` between - beside an overload of the same name with another parameter count.
Test-Case 'httpcscalls: a request built in if/else arms and a url joiner that trims the parameter still make a wrapper' {
    $tree = Use-Tree @{
        'App/App.csproj' = '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework></PropertyGroup></Project>'
        'App/Api.cs' = @'
using System;
using System.Net.Http;
using System.Net.Http.Json;
using System.Threading.Tasks;
namespace Demo;

public interface IWire { Task<T?> PostAsync<TIn, T>(TIn input, string path, HttpClient client, string? overrideRoot = null); Task<T?> GetAsync<T>(string path, HttpClient client); Task<T?> PostAsync<T>(string path, HttpClient client); }
public class Transport : IWire
{
    private readonly string baseUrl = Environment.GetEnvironmentVariable("BASE") ?? "";
    public async Task<T?> PostAsync<TIn, T>(TIn input, string path, HttpClient client, string? overrideRoot = null)
    {
        HttpRequestMessage request;
        if (input is null) request = new HttpRequestMessage(HttpMethod.Post, MergeUrl(overrideRoot ?? baseUrl, path));
        else request = new HttpRequestMessage(HttpMethod.Post, MergeUrl(overrideRoot ?? baseUrl, path)) { Content = JsonContent.Create(input) };
        var response = await client.SendAsync(request);
        return await response.Content.ReadFromJsonAsync<T>();
    }
    public async Task<T?> GetAsync<T>(string path, HttpClient client)
    {
        var request = new HttpRequestMessage(HttpMethod.Get, Join(baseUrl, path));
        var response = await client.SendAsync(request);
        return await response.Content.ReadFromJsonAsync<T>();
    }
    public async Task<T?> PostAsync<T>(string path, HttpClient client)
    {
        var response = await client.SendAsync(new HttpRequestMessage(HttpMethod.Post, Resolve(path)));
        return await response.Content.ReadFromJsonAsync<T>();
    }
    private string MergeUrl(string root, string rest)
    {
        if (string.IsNullOrEmpty(root)) return rest;
        if (string.IsNullOrEmpty(rest)) return root;
        root = root.TrimEnd('/');
        rest = rest.TrimStart('/');
        return $"{root}/{rest}";
    }
    private string Resolve(string m) => m.StartsWith($"{Uri.UriSchemeHttps}://") ? m : MergeUrl(baseUrl, m);
    public async Task<T?> PutAsync<T>(string path, HttpClient client)
    {
        var response = await client.SendAsync(new HttpRequestMessage(HttpMethod.Put, Resolve(path)));
        return await response.Content.ReadFromJsonAsync<T>();
    }
    private static string Join(string b, string m) => b.TrimEnd('/') + "/" + m.TrimStart('/');
}
public class Caller
{
    private readonly IWire wire = new Transport();
    public object Find() => wire.PostAsync<int, string>(1, "/demo/orders/lookup", new HttpClient());
    public object List() => wire.GetAsync<string>("/demo/orders/list", new HttpClient());
    public object Save() => new Transport().PutAsync<string>("/demo/orders/save", new HttpClient());
    public object Short() => wire.PostAsync<string>("/demo/orders/short", new HttpClient());
}
'@
    }
    $db = Join-Path $tree 'map.sqlite'
    Assert-Exit (Invoke-Gate --root $tree --ext .cs --map-sqlite $db) 0
    $q = Invoke-Gate --map-query $db --width 0 --limit 0 --sql ("SELECT 'v=' || c.line || '|' || c.verb || '|' || c.url || '|' || " +
        "coalesce((SELECT f.name FROM functions f WHERE f.id = c.via), '') || '|' || (SELECT f.name FROM functions f WHERE f.id = c.member) AS r " +
        "FROM http_calls c ORDER BY c.line, c.url")
    Assert-Exit $q 0
    $rows = @($q.Lines | Where-Object { $_.TrimStart().StartsWith('v=') } | ForEach-Object { $_.Trim().Substring(2) })
    $want = @('49|POST|/demo/orders/lookup|PostAsync|Find', '50|GET|/demo/orders/list|GetAsync|List', '51|PUT|/demo/orders/save|PutAsync|Save', '52|POST|/demo/orders/short|PostAsync|Short')
    if (($rows -join "`n") -ne ($want -join "`n")) { throw "http_calls:`nexpected:`n$($want -join "`n")`ngot:`n$($rows -join "`n")" }
}
