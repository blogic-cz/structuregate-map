<#
    `.razor` and `.cshtml`, compiled as the C# the Razor compiler generates for them and walked as the markup
    file: rows on the markup LINE, the generator's plumbing left out, components discovered in a declaration
    pass so a render is a row, and a view's base type and namespaces read from its `Web.config`, with MVC 5's
    `@helper` rewritten to what the Core compiler accepts.

    Its helpers are its own - `-Only` runs this suite alone.
#>

$script:CsMarkupNl = [string][char]10

function New-CsMarkupDb([hashtable]$Files, [string]$Ext) {
    $tree = Use-Tree $Files
    $db = Join-Path $tree 'markup.sqlite'
    Assert-Exit (Invoke-Gate --root $tree --ext $Ext --map-sqlite $db) 0
    return $db
}

function Get-CsMarkupRows([string]$Db, [string]$Sql) {
    $r = Invoke-Gate --map-query $Db --sql $Sql --width 0 --limit 0
    Assert-Exit $r 0
    return @($r.Lines | Where-Object { $_.TrimStart().StartsWith('v=') } | ForEach-Object { $_.Trim().Substring(2) })
}

function Assert-CsMarkupRow([string]$Db, [string]$Sql, [string]$Expected, [string]$What) {
    $rows = Get-CsMarkupRows $Db $Sql
    if ($rows -notcontains $Expected) { throw "$What`: no row '$Expected' in:`n$($rows -join "`n")" }
}

Test-Case 'csmarkup: a component is walked as its .razor - rows on the markup line, renders as rows, and C# binds into it' {
    $nl = $script:CsMarkupNl
    $db = New-CsMarkupDb @{
        'Docs/Docs.csproj' = '<Project Sdk="Microsoft.NET.Sdk.Razor"><PropertyGroup><TargetFramework>net10.0</TargetFramework></PropertyGroup>' +
            '<ItemGroup><FrameworkReference Include="Microsoft.AspNetCore.App" /></ItemGroup></Project>'
        'Docs/Shared/Badge.razor' = "<span>@Text</span>$nl@code {$nl    [Parameter] public string Text { get; set; }$nl" +
            "    public static int Twice(int x) => x * 2;$nl}$nl"
        'Docs/Pages/Page.razor' = "<h1>@Title()</h1>$nl<Badge Text=`"@Title()`" />$nl@code {$nl    string Title() => string.Concat(`"a`", `"b`");$nl}$nl"
        'Docs/_Imports.razor' = "@using Docs.Shared$nl"
        'Docs/Layout.cs' = "namespace Docs;$nl" + "public static class Layout { public static int Go() => Docs.Shared.Badge.Twice(2); }$nl"
        # `App` sorts first and references Docs, so Docs is compiled as a REFERENCE before its own turn.
        'App/App.csproj' = '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework></PropertyGroup>' +
            '<ItemGroup><ProjectReference Include="..\Docs\Docs.csproj" /></ItemGroup></Project>'
        'App/Use.cs' = "namespace App;$nl" + "public static class Use { public static int Go() => Docs.Layout.Go(); }$nl"
    } '.cs,.razor'
    Assert-CsMarkupRow $db "SELECT 'v=' || path || '|' || razor || '|' || errors FROM files" 'Docs/Pages/Page.razor|component|0' 'a component file'
    Assert-CsMarkupRow $db "SELECT 'v=' || c.line || '|' || c.symbol FROM calls c JOIN files f ON f.id = c.file WHERE f.path = 'Docs/Pages/Page.razor' AND c.callee = 'Title'" '1|Docs.Pages.Page.Title' 'a markup call on its markup line'
    $plumbing = Get-CsMarkupRows $db "SELECT 'v=' || c.callee FROM calls c JOIN files f ON f.id = c.file WHERE f.razor = 'component' AND c.callee GLOB '__*'"
    if ($plumbing.Count -gt 0) { throw "the generator's calls are not the author's: $($plumbing -join ', ')" }
    Assert-CsMarkupRow $db "SELECT 'v=' || r.line || '|' || r.component || '|' || r.tag || '|' || r.attributes FROM razor_renders r JOIN files f ON f.id = r.file WHERE f.path = 'Docs/Pages/Page.razor'" '2|Docs.Shared.Badge|Badge|["Text"]' 'a component render'
    Assert-CsMarkupRow $db "SELECT 'v=' || c.symbol FROM calls c WHERE c.callee GLOB '*.Twice'" 'Docs.Shared.Badge.Twice' 'C# calling into @code'
    # The Razor trees of a project reached as a reference first must not stand in for its C# ones.
    Assert-CsMarkupRow $db "SELECT 'v=' || path || '|' || semantic FROM files" 'Docs/Layout.cs|1' 'the C# of a Razor project reached as a reference'
    Assert-CsMarkupRow $db "SELECT 'v=' || path || '|' || semantic FROM files" 'Docs/Pages/Page.razor|1' 'and its markup'
}

Test-Case 'csmarkup: a .cshtml view takes its base and namespaces from Web.config, and an @helper compiles' {
    $nl = $script:CsMarkupNl
    $db = New-CsMarkupDb @{
        'Tpl/Tpl.csproj' = '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework></PropertyGroup></Project>'
        'Tpl/Base.cs' = "namespace Tpl$nl{$nl    public abstract class TemplateBase<TModel>$nl    {$nl        public TModel Model { get; set; }$nl" +
            "        public string FormatDate(System.DateTime d) => d.ToString();$nl" +
            "        public virtual void Write(object value) { }$nl        public virtual void WriteLiteral(string value) { }$nl" +
            "        public virtual System.Threading.Tasks.Task ExecuteAsync() => System.Threading.Tasks.Task.CompletedTask;$nl    }$nl}$nl" +
            "namespace Tpl.Models { public class Person { public System.DateTime Born { get; set; } } }$nl"
        'Tpl/Views/Web.config' = '<configuration><system.web.webPages.razor><pages pageBaseType="Tpl.TemplateBase">' +
            '<namespaces><add namespace="Tpl.Models" /></namespaces></pages></system.web.webPages.razor></configuration>'
        'Tpl/Views/Card.cshtml' = "@model Person$nl<p>@FormatDate(Model.Born)</p>$nl@Line(`"x`")$nl@helper Line(string text)$nl{$nl" +
            "    if (text == null) { return; }$nl    <b>@text.Trim()</b>$nl}$nl"
        'Tpl/Views/_Row.cshtml' = "@model Person$nl<td>@Model.Born.Year</td>$nl"
    } '.cs,.cshtml'
    Assert-CsMarkupRow $db "SELECT 'v=' || path || '|' || razor || '|' || errors FROM files" 'Tpl/Views/Card.cshtml|view|0' 'a view, error-free'
    Assert-CsMarkupRow $db "SELECT 'v=' || path || '|' || razor FROM files" 'Tpl/Views/_Row.cshtml|view' 'a partial named with _ is a view'
    Assert-CsMarkupRow $db "SELECT 'v=' || c.line || '|' || c.symbol FROM calls c JOIN files f ON f.id = c.file WHERE f.path = 'Tpl/Views/Card.cshtml' AND c.callee = 'FormatDate'" '2|Tpl.TemplateBase<Tpl.Models.Person>.FormatDate' 'bound through the Web.config base'
    Assert-CsMarkupRow $db "SELECT 'v=' || c.line || '|' || c.symbol FROM calls c JOIN files f ON f.id = c.file WHERE f.path = 'Tpl/Views/Card.cshtml' AND c.callee = 'text.Trim'" '7|System.String.Trim' 'inside the helper, on its own line'
}

# A component project mapped again and again into fresh databases, so a case can edit it between runs.
function New-CsMarkupTree {
    $nl = $script:CsMarkupNl
    return Use-Tree @{
        'Docs/Docs.csproj' = '<Project Sdk="Microsoft.NET.Sdk.Razor"><PropertyGroup><TargetFramework>net10.0</TargetFramework></PropertyGroup>' +
            '<ItemGroup><FrameworkReference Include="Microsoft.AspNetCore.App" /></ItemGroup></Project>'
        'Docs/Shared/Badge.razor' = "<span>@Text</span>$nl@code {$nl    [Parameter] public string Text { get; set; }$nl}$nl"
        'Docs/Pages/Page.razor' = "<h1>@Title()</h1>$nl<Badge Text=`"x`" />$nl@code {$nl    string Title() => `"a`";$nl}$nl"
        'Docs/_Imports.razor' = "@using Docs.Shared$nl"
    }
}

function Invoke-CsMarkupMap([string]$Tree) {
    $db = Join-Path $Tree "markup.$([guid]::NewGuid().ToString('N').Substring(0, 6)).sqlite"
    Assert-Exit (Invoke-Gate --root $Tree --ext '.cs,.razor' --map-sqlite $db) 0
    return $db
}

$script:CsMarkupRenders = "SELECT 'v=' || r.component FROM razor_renders r JOIN files f ON f.id = r.file WHERE f.path = 'Docs/Pages/Page.razor'"

Test-Case 'csmarkup: a second run takes the generated code from the cache and maps the same rows' {
    $tree = New-CsMarkupTree
    $first = Invoke-CsMarkupMap $tree
    if (-not (Get-ChildItem (Join-Path $tree 'Docs/obj/structuregate.razor') -Filter '*.json' -ErrorAction SilentlyContinue)) { throw 'nothing was cached' }
    $second = Invoke-CsMarkupMap $tree
    $query = "SELECT 'v=' || c.line || '|' || c.symbol FROM calls c JOIN files f ON f.id = c.file WHERE f.razor = 'component' ORDER BY 1"
    $a = (Get-CsMarkupRows $first $query) -join ','
    $b = (Get-CsMarkupRows $second $query) -join ','
    if ($a -ne $b -or $a.Length -eq 0) { throw "the cached run mapped different rows:`n$a`n$b" }
    Assert-CsMarkupRow $second $script:CsMarkupRenders 'Docs.Shared.Badge' 'the render survives the cache'
}

Test-Case 'csmarkup: an EDITED .razor is generated again, never answered from the cache' {
    $nl = $script:CsMarkupNl
    $tree = New-CsMarkupTree
    Invoke-CsMarkupMap $tree | Out-Null
    [System.IO.File]::WriteAllText((Join-Path $tree 'Docs/Pages/Page.razor'), "<h1>@Other()</h1>$nl@code {$nl    string Other() => `"b`";$nl}$nl")
    $db = Invoke-CsMarkupMap $tree
    Assert-CsMarkupRow $db "SELECT 'v=' || c.callee FROM calls c JOIN files f ON f.id = c.file WHERE f.path = 'Docs/Pages/Page.razor'" 'Other' 'the edit is mapped'
}

Test-Case 'csmarkup: an edited _Imports.razor is generated again - it decides what a tag IS' {
    $tree = New-CsMarkupTree
    Invoke-CsMarkupMap $tree | Out-Null
    [System.IO.File]::WriteAllText((Join-Path $tree 'Docs/_Imports.razor'), '@* nothing imported *@' + $script:CsMarkupNl)
    $db = Invoke-CsMarkupMap $tree
    # NO RENDER ANYWHERE, so the table is not even written: <Badge /> is markup now, not a component.
    Assert-CsMarkupRow $db "SELECT 'v=' || count(*) FROM sqlite_master WHERE name = 'razor_renders'" '0' 'without the using no component is rendered - the old generation was not reused'
}

# ONE DISCOVERY PROVIDER THAT THROWS: Razor 6's EventHandlerTagHelperDescriptorProvider dereferenced a
# null over .NET 10's Components on Windows, and every file of the project went UNPARSED - plus a second error that
# only counted the same files again. The provider is made to throw here; its tag helpers are left out with a note,
# the components still render, nothing is UNPARSED, and the next run without the throw is not served the gap.
Test-Case 'csmarkup: a tag-helper provider that throws costs only its own tag helpers, with a note' {
    $tree = New-CsMarkupTree
    $saved = $env:STRUCTUREGATE_TEST_THROW_PROVIDER
    try {
        $env:STRUCTUREGATE_TEST_THROW_PROVIDER = 'EventHandlerTagHelperDescriptorProvider'
        $db = Join-Path $tree 'thrown.sqlite'
        $run = Invoke-Gate --root $tree --ext '.cs,.razor' --map-sqlite $db
    } finally {
        if ($null -eq $saved) { Remove-Item Env:STRUCTUREGATE_TEST_THROW_PROVIDER -ErrorAction SilentlyContinue }
        else { $env:STRUCTUREGATE_TEST_THROW_PROVIDER = $saved }
    }
    Assert-Exit $run 0
    Assert-NoLine $run 'UNPARSED'
    Assert-Line $run 'the EventHandlerTagHelperDescriptorProvider tag-helper discovery threw NullReferenceException'
    Assert-CsMarkupRow $db $script:CsMarkupRenders 'Docs.Shared.Badge' 'the component still renders'
    # NOT CACHED: a generation missing a provider's tag helpers would be served to every later run, the gap included,
    # and nothing in a row would show it - so it is the cache itself that is asked. The next clean run writes one.
    $cache = Join-Path $tree 'Docs/obj/structuregate.razor'
    Assert-Equal (Test-Path $cache) $false 'a Razor cache written by the run that skipped a provider'
    $again = Invoke-Gate --root $tree --ext '.cs,.razor' --map-sqlite (Join-Path $tree 'again.sqlite')
    Assert-Exit $again 0
    Assert-NoLine $again 'tag-helper discovery threw'
    Assert-Equal (Test-Path $cache) $true 'the Razor cache of the clean run'
}
