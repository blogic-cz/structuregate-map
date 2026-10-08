<#
    WHAT A PROJECT COMPILES AGAINST, found the way a build finds it - the four an audit of a large solution named: a
    `net472` project against the .NET Framework (not the .NET pack), a `<Reference HintPath>`, a netstandard
    project referenced from one, and a `.razor` component C# names. Plus a `*.Generated.cs` that keeps its
    `files` row, and a `nameof` over an overloaded method.

    Its helpers are its own - `-Only` runs this suite alone.
#>

$script:CsInputsNl = [string][char]10

function New-CsInputsDb([hashtable]$Files) {
    $tree = Use-Tree $Files
    $db = Join-Path $tree 'inputs.sqlite'
    Assert-Exit (Invoke-Gate --root $tree --ext .cs --map-sqlite $db) 0
    return [pscustomobject]@{ Tree = $tree; Db = $db }
}

# One cell out of the database, as the text after `v=`.
function Get-CsInputsValue([string]$Db, [string]$Sql) {
    $r = Invoke-Gate --map-query $Db --sql $Sql --width 0
    Assert-Exit $r 0
    $line = $r.Lines | Where-Object { $_.TrimStart().StartsWith('v=') } | Select-Object -First 1
    if ($null -eq $line) { throw "no v= row for: $Sql`n$($r.Text)" }
    return $line.Trim().Substring(2)
}

$x86 = [Environment]::GetFolderPath('ProgramFilesX86')
# Empty off Windows, where there is no .NET Framework to find - and Join-Path refuses an empty parent.
$script:CsInputsNet472 = $x86 -and (Test-Path (Join-Path $x86 'Reference Assemblies\Microsoft\Framework\.NETFramework\v4.7.2\mscorlib.dll'))
if (-not $script:CsInputsNet472) { Write-Host '    (no .NET Framework 4.7.2 targeting pack - the net472 cases are not run)' }

if ($script:CsInputsNet472) {

Test-WindowsCase 'csinputs: a net472 project compiles against the .NET Framework, with its own references' {
    $nl = $script:CsInputsNl
    $db = (New-CsInputsDb @{
        'Legacy/Legacy.csproj' = '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net472</TargetFramework></PropertyGroup>' +
            '<ItemGroup><Reference Include="System.Web" /><Reference Include="System.Configuration" /></ItemGroup></Project>'
        # System.Transactions is in no <Reference> and no facade: a restored package asks for it, and NuGet adds it.
        'Legacy/obj/project.assets.json' = '{"version":3,"targets":{".NETFramework,Version=v4.7.2":{"Fake.Http/1.0.0":' +
            '{"type":"package","frameworkAssemblies":["System.Transactions"]}}},"libraries":{"Fake.Http/1.0.0":{"type":"package","path":"fake.http/1.0.0"}},"packageFolders":{}}'
        'Legacy/Page.cs' = "namespace Legacy$nl{$nl    public static class Page$nl    {$nl" +
            "        public static string Path() => System.Web.HttpContext.Current.Request.Path;$nl" +
            "        public static string Setting() => System.Configuration.ConfigurationManager.AppSettings[`"key`"];$nl" +
            "        public static string Safe(string text) => System.Web.HttpUtility.HtmlEncode(text);$nl" +
            "        public static object Current() => System.Transactions.Transaction.Current;$nl" +
            "    }$nl}$nl"
    }).Db
    Assert-Equal (Get-CsInputsValue $db "SELECT 'v=' || sum(errors) FROM files") '0' 'System.Web and ConfigurationManager resolve'
    Assert-Equal (Get-CsInputsValue $db "SELECT 'v=' || symbol FROM calls WHERE callee GLOB '*.HtmlEncode'") 'System.Web.HttpUtility.HtmlEncode' 'a System.Web call bound'
}

Test-Case 'csinputs: a netstandard project a net472 one references is compiled against netstandard, not the runtime' {
    $nl = $script:CsInputsNl
    $db = (New-CsInputsDb @{
        'Lib/Lib.csproj' = '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>netstandard2.0</TargetFramework></PropertyGroup></Project>'
        'Lib/Api.cs' = "namespace Lib$nl{$nl    public enum Kind { A, B }$nl    public static class Api { public static Kind Get() => Kind.A; }$nl}$nl"
        'App/App.csproj' = '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net472</TargetFramework></PropertyGroup>' +
            '<ItemGroup><ProjectReference Include="..\Lib\Lib.csproj" /></ItemGroup></Project>'
        'App/Use.cs' = "namespace App$nl{$nl    public static class Use$nl    {$nl" +
            "        public static string Name() => Lib.Api.Get().ToString();$nl    }$nl}$nl"
    }).Db
    # Against the runtime, `Enum` read as "defined in an assembly that is not referenced: System.Private.CoreLib".
    Assert-Equal (Get-CsInputsValue $db "SELECT 'v=' || sum(f.errors) FROM files f WHERE f.path GLOB 'App/*'") '0' 'no CS0012'
    Assert-Equal (Get-CsInputsValue $db "SELECT 'v=' || symbol FROM calls WHERE symbol GLOB '*.ToString'") 'System.Enum.ToString' 'bound through the framework'
}

Test-Case 'csinputs: a multi-targeted project is compiled as the framework its consumer picks' {
    $nl = $script:CsInputsNl
    $db = (New-CsInputsDb @{
        'Loc/Loc.csproj' = '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFrameworks>net10.0;netstandard2.0</TargetFrameworks></PropertyGroup></Project>'
        'Loc/ICulture.cs' = "namespace Loc$nl{$nl    public interface ICulture { System.Globalization.CultureInfo Get(); }$nl}$nl"
        'App/App.csproj' = '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net472</TargetFramework></PropertyGroup>' +
            '<ItemGroup><ProjectReference Include="..\Loc\Loc.csproj" /></ItemGroup></Project>'
        'App/Culture.cs' = "namespace App$nl{$nl    public class Culture : Loc.ICulture$nl    {$nl" +
            "        public System.Globalization.CultureInfo Get() => System.Globalization.CultureInfo.InvariantCulture;$nl    }$nl}$nl"
    }).Db
    # Compiled as its first framework, net10.0, the interface named CultureInfo from System.Runtime 10.0 and
    # the net472 class failed with CS7069; a build hands net472 the netstandard2.0 assembly.
    Assert-Equal (Get-CsInputsValue $db "SELECT 'v=' || sum(errors) FROM files WHERE path GLOB 'App/*'") '0' 'no CS7069'
}

}

Test-Case 'csinputs: a HintPath reference is compiled against' {
    $cache = Join-Path $script:NuGetPackages 'mstest.testframework'
    $dll = Get-ChildItem $cache -Recurse -Filter 'Microsoft.VisualStudio.TestPlatform.TestFramework.dll' -ErrorAction SilentlyContinue |
        Where-Object { $_.FullName.Replace('\', '/') -like '*/lib/net8.0/*' } | Select-Object -First 1
    if (-not $dll) { Write-Host '    (no MSTest.TestFramework in the NuGet cache - the HintPath case is not run)'; return }
    $nl = $script:CsInputsNl
    $built = New-CsInputsDb @{
        'Demo.csproj' = '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework></PropertyGroup>' +
            '<ItemGroup><Reference Include="Microsoft.VisualStudio.TestPlatform.TestFramework"><HintPath>lib\Framework.dll</HintPath></Reference></ItemGroup></Project>'
        'Check.cs' = "namespace Demo;$nl" + "public static class Check { public static void Go() => Microsoft.VisualStudio.TestTools.UnitTesting.Assert.IsTrue(true); }$nl"
    }
    [void](New-Item -ItemType Directory -Path (Join-Path $built.Tree 'lib') -Force)
    Copy-Item $dll.FullName (Join-Path $built.Tree 'lib\Framework.dll')
    Remove-Item $built.Db
    Assert-Exit (Invoke-Gate --root $built.Tree --ext .cs --map-sqlite $built.Db) 0
    Assert-Equal (Get-CsInputsValue $built.Db "SELECT 'v=' || symbol FROM calls WHERE callee GLOB '*.IsTrue'") 'Microsoft.VisualStudio.TestTools.UnitTesting.Assert.IsTrue' 'bound to the hinted assembly'
}

Test-Case 'csinputs: C# that names a razor component binds to what its @code declares' {
    $nl = $script:CsInputsNl
    $db = (New-CsInputsDb @{
        'Docs/Docs.csproj' = '<Project Sdk="Microsoft.NET.Sdk.Razor"><PropertyGroup><TargetFramework>net10.0</TargetFramework></PropertyGroup></Project>'
        'Docs/Shared/Page/Layout.razor' = "<style>body{}</style>$nl@code {$nl    public const float Width = 100f;$nl" +
            "    public static double Scale(bool wide) { var s = `"}`"; return wide ? 2 : 1; }$nl" +
            "    public static int After() => 1;$nl}$nl"
        'Docs/Measure.cs' = "namespace Docs;$nl" +
            "public static class Measure { public static double Go() => Docs.Shared.Page.Layout.Scale(true) * Docs.Shared.Page.Layout.Width + Docs.Shared.Page.Layout.After(); }$nl"
    }).Db
    # The namespace is the root namespace plus the folders, as the generator makes it; a `}` in a string is
    # not the end of the block.
    Assert-Equal (Get-CsInputsValue $db "SELECT 'v=' || sum(errors) FROM files") '0' 'the component resolves'
    Assert-Equal (Get-CsInputsValue $db "SELECT 'v=' || symbol FROM calls WHERE callee GLOB '*.Scale'") 'Docs.Shared.Page.Layout.Scale' 'bound into @code'
    Assert-Equal (Get-CsInputsValue $db "SELECT 'v=' || symbol FROM calls WHERE callee GLOB '*.After'") 'Docs.Shared.Page.Layout.After' 'a member after the string is still in the block'
}

Test-Case 'csinputs: a *.Generated.cs keeps its files row, and nameof binds an overloaded method' {
    $nl = $script:CsInputsNl
    $db = (New-CsInputsDb @{
        'Demo.csproj' = '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework></PropertyGroup></Project>'
        'Model.Generated.cs' = "namespace Demo;$nl" + "public partial class Model { public int Count { get; set; } }$nl"
        'Use.cs' = "namespace Demo;$nl" + "public partial class Model$nl{$nl    public int Read() => Count;$nl" +
            "    public void Load(int a) { }$nl    public void Load(string s) { }$nl" +
            "    public string Which() => nameof(Load);$nl}$nl"
    }).Db
    Assert-Equal (Get-CsInputsValue $db "SELECT 'v=' || generated FROM files WHERE path = 'Model.Generated.cs'") '1' 'listed, marked generated'
    Assert-Equal (Get-CsInputsValue $db "SELECT 'v=' || sum(errors) FROM files") '0' 'and still compiled'
    Assert-Equal (Get-CsInputsValue $db "SELECT 'v=' || symbol FROM calls WHERE callee = 'nameof'") 'Demo.Model.Load' 'the member group names one method'
}
