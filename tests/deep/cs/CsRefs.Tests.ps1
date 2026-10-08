<#
    What the deep C# map COMPILES AGAINST, on a tree that was restored and never built: which file wins
    when a package and the framework ship the same assembly name, the global usings a build would have
    written, and a re-read when the references move while the source does not.

    Kept out of CsRows.Tests.ps1, which the baseline holds at its size. Its helpers are its own: `-Only CsRefs`
    runs this file alone.
#>

. (Join-Path $PSScriptRoot 'CsRows.Helpers.ps1')

# The deep map of a tree, the gate's own output kept for the re-read count.
function New-CsRefsDb([string]$Tree) {
    $db = Join-Path $Tree 'map.sqlite'
    $result = Invoke-Gate --root $Tree --ext .cs --map-sqlite $db
    Assert-Exit $result 0
    return [pscustomobject]@{ Db = $db; Result = $result }
}

function Get-CsRefsErrors([string]$Db) {
    $r = Invoke-Gate --map-query $Db --sql "SELECT 'errors=' || sum(errors) AS e FROM files"
    return ($r.Lines | Where-Object { $_.Trim().StartsWith('errors=') } | Select-Object -First 1).Trim()
}

# An older System.Runtime.dll than the framework's: the .NET 8 reference assembly, where one is installed.
function Get-CsRefsOldRuntime {
    $packs = Join-Path $script:DotnetRoot 'packs\Microsoft.NETCore.App.Ref'
    if (-not (Test-Path $packs)) { return $null }
    foreach ($version in Get-ChildItem $packs -Directory | Where-Object { $_.Name.StartsWith('8.') }) {
        $dll = Join-Path $version.FullName 'ref\net8.0\System.Runtime.dll'
        if (Test-Path $dll) { return $dll }
    }
    return $null
}

$nl = [string][char]10
$csproj = '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework></PropertyGroup></Project>'

Test-Case 'csrefs: a package assembly older than the framework one does not shadow it' {
    # System.Runtime/4.3.0 in a restore closure shipped its own System.Runtime.dll; added before the framework,
    # it took the name, and .NET's own types - here System.Threading.Lock, new in .NET 9 - were not defined.
    $old = Get-CsRefsOldRuntime
    if (-not $old) { Write-Host '    (no .NET 8 reference pack here - the facade case is not run)'; return }
    $tree = Use-Tree @{
        'Demo.csproj' = $csproj
        'Gate.cs'     = "namespace Demo;" + $nl + "public static class Gate" + $nl + "{" + $nl +
                        "    private static readonly System.Threading.Lock Guard = new();" + $nl +
                        "    public static void Go() { lock (Guard) { } }" + $nl + "}" + $nl
    }
    $packages = Join-Path $tree 'pkgs'
    $lib = Join-Path $packages 'old.facade\1.0.0\lib'
    [void](New-Item -ItemType Directory -Path $lib -Force)
    Copy-Item $old (Join-Path $lib 'System.Runtime.dll')
    $assets = '{"version":3,"targets":{"net10.0":{"Old.Facade/1.0.0":{"type":"package","compile":{"lib/System.Runtime.dll":{}}}}},' +
              '"libraries":{"Old.Facade/1.0.0":{"type":"package","path":"old.facade/1.0.0"}},' +
              '"packageFolders":{"' + ($packages -replace '\\', '\\') + '\\":{}}}'
    [void](New-Item -ItemType Directory -Path (Join-Path $tree 'obj') -Force)
    [System.IO.File]::WriteAllText((Join-Path $tree 'obj\project.assets.json'), $assets)
    $built = New-CsRefsDb $tree
    Assert-Equal (Get-CsRefsErrors $built.Db) 'errors=0' 'the framework System.Runtime is the one compiled against'
}

Test-Case 'csrefs: a restored, never-built project gets the global usings a build would have written' {
    # Only a build writes GlobalUsings.g.cs. ImplicitUsings comes from Directory.Build.props here, a <Using>
    # item adds System.Text, and nothing in the file says `using` at all.
    $tree = Use-Tree @{
        'Directory.Build.props' = '<Project><PropertyGroup><ImplicitUsings>enable</ImplicitUsings></PropertyGroup></Project>'
        'Demo.csproj'           = '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework>' +
                                  '</PropertyGroup><ItemGroup><Using Include="System.Text" /></ItemGroup></Project>'
        'Work.cs'               = "namespace Demo;" + $nl + "public static class Work" + $nl + "{" + $nl +
                                  "    public static Task Go(CancellationToken token) => Task.Delay(1, token);" + $nl +
                                  "    public static string Text() => new StringBuilder().Append(1).ToString();" + $nl +
                                  "    public static int Count(List<int> items) => items.Count();" + $nl + "}" + $nl
    }
    $built = New-CsRefsDb $tree
    Assert-Equal (Get-CsRefsErrors $built.Db) 'errors=0' 'every name the SDK would have imported resolves'
}

Test-Case 'csrefs: a change to what a project compiles against re-reads its files' {
    # A restore changes obj/project.assets.json and no .cs file; the rows it invalidates are the bound ones.
    $tree = Use-Tree @{ 'Demo.csproj' = $csproj; 'A.cs' = "namespace Demo;" + $nl + "public class A { }" + $nl }
    $first = New-CsRefsDb $tree
    Assert-Line $first.Result '1 file(s) re-read'
    $again = New-CsRefsDb $tree
    Assert-Line $again.Result '0 file(s) re-read'
    [void](New-Item -ItemType Directory -Path (Join-Path $tree 'obj') -Force)
    [System.IO.File]::WriteAllText((Join-Path $tree 'obj\project.assets.json'), '{"version":3,"targets":{},"libraries":{}}')
    $restored = New-CsRefsDb $tree
    Assert-Line $restored.Result '1 file(s) re-read'
    # AND IT SAYS SO: the file is the project's, and the input that moved is named.
    Assert-Line $restored.Result '1 whose project''s inputs moved'
    Assert-Line $restored.Result 'by project: Demo.csproj 1 (obj/project.assets.json)'
}

# `_meta.last_refresh`, as an object.
function Get-CsRefsRefresh([string]$Db) {
    $r = Invoke-Gate --map-query $Db --width 0 --sql "SELECT 'v=' || value FROM _meta WHERE key = 'last_refresh'"
    Assert-Exit $r 0
    $line = $r.Lines | Where-Object { $_.TrimStart().StartsWith('v=') } | Select-Object -First 1
    if ($null -eq $line) { throw "no last_refresh in _meta:`n$($r.Text)" }
    return $line.Trim().Substring(2) | ConvertFrom-Json
}

Test-Case 'csrefs: a re-read run says which files moved and which project forced the rest, and _meta keeps it' {
    $tree = Use-Tree @{
        'A/A.csproj' = $csproj; 'A/X.cs' = "namespace A;" + $nl + "public class X { }" + $nl; 'A/Y.cs' = "namespace A;" + $nl + "public class Y { }" + $nl
        'B/B.csproj' = $csproj; 'B/Z.cs' = "namespace B;" + $nl + "public class Z { }" + $nl
    }
    New-CsRefsDb $tree | Out-Null
    $quiet = New-CsRefsDb $tree
    Assert-Line $quiet.Result 'passes over the finished rows (replayed)'
    # A PROJECT FILE EDITED and a source file of ANOTHER project: three re-reads, two causes, one project named.
    [System.IO.File]::WriteAllText((Join-Path $tree 'A/A.csproj'), $csproj.Replace('</TargetFramework>', '</TargetFramework><Nullable>enable</Nullable>'))
    [System.IO.File]::WriteAllText((Join-Path $tree 'B/Z.cs'), "namespace B;" + $nl + "public class Z { public int Q; }" + $nl)
    $pulled = New-CsRefsDb $tree
    Assert-Line $pulled.Result '3 file(s) re-read'
    Assert-Line $pulled.Result 're-reads 3 file(s): 1 whose content moved, 2 whose project''s inputs moved'
    Assert-Line $pulled.Result 'by project: A/A.csproj 2 (A.csproj)'
    Assert-Line $pulled.Result 'the deep map took '
    $kept = Get-CsRefsRefresh $pulled.Db
    Assert-Equal $kept.csharp.causes.content 1 'files whose content moved'
    Assert-Equal $kept.csharp.causes.project 2 'files their project forced'
    Assert-Equal $kept.csharp.projects[0].project 'A/A.csproj' 'the project'
    Assert-Equal $kept.csharp.projects[0].moved[0] 'A.csproj' 'its input that moved'
    Assert-Equal $kept.passes 'ran' 'the passes ran'
    Assert-Equal ($null -ne $kept.steps_ms.csharp) $true 'the C# half is timed'
    # NOTHING MOVED: the passes still replay - last_refresh is in no key they read - and the slow run's account is
    # KEPT, with the trace beside it: a run that did nothing rewrites neither.
    $trace = "$($pulled.Db).last-run.jsonl"
    $traced = [System.IO.File]::ReadAllText($trace)
    $again = New-CsRefsDb $tree
    Assert-Line $again.Result '0 file(s) re-read'
    Assert-Line $again.Result 'passes over the finished rows (replayed)'
    $still = Get-CsRefsRefresh $again.Db
    Assert-Equal $still.when $kept.when 'the working run''s account is kept'
    Assert-Equal $still.csharp.causes.project 2 'with its causes'
    Assert-Equal ([System.IO.File]::ReadAllText($trace)) $traced 'and its trace'
}

Test-Case 'csrefs: --map-reread csharp reads every file of a tree where nothing moved, and replaces its rows' {
    $tree = Use-Tree @{
        'Demo.csproj' = '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework></PropertyGroup></Project>'
        'A.cs' = 'namespace Demo; public class A { public int Run() { return new B().Go(); } }'
        'B.cs' = 'namespace Demo; public class B { public int Go() { return 1; } }'
    }
    $first = New-CsRefsDb $tree
    $rows = Get-CsScalar $first.Db "SELECT count(*) FROM calls"
    $db = $first.Db
    $forced = Invoke-Gate --root $tree --ext .cs --map-sqlite $db --map-reread csharp
    Assert-Exit $forced 0
    Assert-Line $forced 're-reads every file: --map-reread csharp'
    Assert-Line $forced '2 file(s) re-read'
    Assert-Equal (Get-CsScalar $db "SELECT count(*) FROM calls") $rows 'rows replaced, not added'
    Assert-Line (New-CsRefsDb $tree).Result 'had nothing to do'
}

Test-Case 'csrefs: a file the project removes stays out when the project was first compiled as a reference' {
    # `a/A.csproj` references `b/B.csproj` and sorts first, so B is compiled as A's reference before its own
    # turn. B removes Old.cs; it read as compiled with no model - syntax rows, every call unbound.
    $tree = Use-Tree @{
        'a/A.csproj' = '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework></PropertyGroup>' +
                       '<ItemGroup><ProjectReference Include="..\b\B.csproj" /></ItemGroup></Project>'
        'a/A.cs'     = "namespace A;" + $nl + "public static class Use { public static int Go() => B.Lib.One(); }" + $nl
        'b/B.csproj' = '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework></PropertyGroup>' +
                       '<ItemGroup><Compile Remove="Old.cs" /></ItemGroup></Project>'
        'b/Lib.cs'   = "namespace B;" + $nl + "public static class Lib { public static int One() => 1; }" + $nl
        'b/Old.cs'   = "namespace B;" + $nl + "public static class Old { public static int Two() => Lib.One() + 1; }" + $nl
    }
    $built = New-CsRefsDb $tree
    $r = Invoke-Gate --map-query $built.Db --sql ("SELECT f.path || '|' || f.compiled || '|' || " +
        "(SELECT count(*) FROM calls c WHERE c.file = f.id) AS x FROM files f WHERE f.path = 'b/Old.cs'")
    Assert-Line $r 'b/Old.cs|0|0'
}

Test-Case 'csrefs: nameof carries the symbol its operand names' {
    # `nameof(x)` is no call and binds to no method; nearly all of one tree's unbound calls were it.
    $tree = Use-Tree @{
        'Demo.csproj' = $csproj
        'Names.cs'    = "namespace Demo;" + $nl + "public class Thing { public string Name { get; set; } = """"; }" + $nl +
                        "public static class Names { public static string Of() => nameof(Thing.Name); }" + $nl
    }
    $built = New-CsRefsDb $tree
    $r = Invoke-Gate --map-query $built.Db --sql "SELECT 'nameof=' || symbol AS x FROM calls WHERE callee = 'nameof'" --width 200
    Assert-Line $r 'nameof=Demo.Thing.Name'
}

Test-Case 'csrefs: a project compiles as its own C# version, not the parser newest' {
    # C# 14's first-class spans send `array.Reverse()` to MemoryExtensions.Reverse(Span), which returns void;
    # C# 13 picks Enumerable.Reverse. A LangVersion of 13 must bind the call the way the build does.
    $code = "namespace Demo;" + $nl + "public static class Order { public static int[] Back(int[] items) => items.Reverse().ToArray(); }" + $nl +
            "file static class Use { static System.Collections.Generic.IEnumerable<int> Keep() => System.Linq.Enumerable.Empty<int>(); }" + $nl
    $tree = Use-Tree @{
        'Demo.csproj' = '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework>' +
                        '<LangVersion>13</LangVersion><ImplicitUsings>enable</ImplicitUsings></PropertyGroup></Project>'
        'Order.cs'    = $code
    }
    $built = New-CsRefsDb $tree
    Assert-Equal (Get-CsRefsErrors $built.Db) 'errors=0' 'the explicit LangVersion is the one parsed with'
    # And with no LangVersion, a net9.0 project defaults to C# 13 - where a .NET 9 reference pack exists.
    $packs = Join-Path $script:DotnetRoot 'packs\Microsoft.NETCore.App.Ref'
    if (-not (Get-ChildItem $packs -Directory -ErrorAction SilentlyContinue | Where-Object { Test-Path (Join-Path $_.FullName 'ref\net9.0') })) {
        Write-Host '    (no .NET 9 reference pack here - the framework default is not checked)'; return
    }
    $nine = Use-Tree @{
        'Demo.csproj' = '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net9.0</TargetFramework>' +
                        '<ImplicitUsings>enable</ImplicitUsings></PropertyGroup></Project>'
        'Order.cs'    = $code
    }
    $again = New-CsRefsDb $nine
    Assert-Equal (Get-CsRefsErrors $again.Db) 'errors=0' 'net9.0 parses as C# 13'
}

# The 16-hex digest the map keys a file by: SHA-256, lowercase, cut to 16.
function Get-CsRefsDigest([byte[]]$Bytes) {
    # ComputeHash, not HashData: the harness runs on Windows PowerShell 5, whose .NET Framework has no HashData.
    $hash = [System.Security.Cryptography.SHA256]::Create().ComputeHash($Bytes)
    return ([System.BitConverter]::ToString($hash) -replace '-', '').Substring(0, 16).ToLowerInvariant()
}

Test-Case 'csrefs: a database the previous C# extractor wrote is re-read, not kept' {
    # Source and references unchanged, so only the extractor's own version can tell its rows are stale.
    # The previous release salted the sha with the project's inputs and no version: the stored sha is set to
    # exactly that, and the run must re-read the file rather than keep the old extractor's rows.
    if (-not $script:Python) { Write-Host '    (no python here - the aged-database case is not run)'; return }
    $tree = Use-Tree @{ 'Demo.csproj' = $csproj; 'A.cs' = "namespace Demo;" + $nl + "public class A { }" + $nl }
    $first = New-CsRefsDb $tree
    $file = Get-CsRefsDigest ([System.IO.File]::ReadAllBytes((Join-Path $tree 'A.cs')))
    $project = Get-CsRefsDigest ([System.IO.File]::ReadAllBytes((Join-Path $tree 'Demo.csproj')))
    $old = Get-CsRefsDigest ([System.Text.Encoding]::UTF8.GetBytes("$file|$project+-"))
    $age = Join-Path $tree 'age.pyhelper'
    [System.IO.File]::WriteAllText($age, (@('import sqlite3, sys', 'db = sqlite3.connect(sys.argv[1])',
        'db.execute("UPDATE files SET sha = ? WHERE path = ?", (sys.argv[2], "A.cs"))', 'db.commit()') -join $nl))
    & $script:Python $age $first.Db $old
    if ($LASTEXITCODE -ne 0) { throw 'the database could not be aged' }
    $again = New-CsRefsDb $tree
    Assert-Line $again.Result '1 file(s) re-read'
}

Test-Case 'csrefs: a root inside a project still compiles the whole project, and not a nested one' {
    # A consumer maps one folder of one big project: the types declared elsewhere in that project
    # were "not found" and dozens of calls stayed unbound. A NESTED project's files are not the outer project's -
    # its duplicate `Other.Helper` would make the outer compilation ambiguous if it were pulled in.
    $tree = Use-Tree @{
        'Proj/Demo.csproj'          = $csproj
        'Proj/Sub/Use.cs'           = "namespace Demo.Sub;" + $nl + "public static class Use { public static int Go() => Demo.Other.Helper.One(); }" + $nl
        'Proj/Other/Helper.cs'      = "namespace Demo.Other;" + $nl + "public static class Helper { public static int One() => 1; }" + $nl
        'Proj/Nested/Nested.csproj' = $csproj
        'Proj/Nested/Dup.cs'        = "namespace Demo.Other;" + $nl + "public static class Helper { public static int One() => 2; }" + $nl
    }
    $db = Join-Path $tree 'map.sqlite'
    Assert-Exit (Invoke-Gate --root (Join-Path $tree 'Proj\Sub') --ext .cs --map-sqlite $db) 0
    Assert-Equal (Get-CsRefsErrors $db) 'errors=0' 'the rest of the project is compiled, the nested project is not'
    $r = Invoke-Gate --map-query $db --sql "SELECT 'rows for=' || group_concat(path) AS x FROM files"
    Assert-Line $r 'rows for=Use.cs'
    $bound = Invoke-Gate --map-query $db --sql "SELECT 'symbol=' || symbol AS x FROM calls" --width 200
    Assert-Line $bound 'symbol=Demo.Other.Helper.One'
}

Test-Case 'csrefs: an assembly a package adds from its own build targets is compiled against' {
    # MSTest.TestFramework names only TestFramework.dll as a compile asset; its build/<tfm>/*.targets adds
    # TestFramework.Extensions.dll, where TestContext lives, through a HintPath under a UseWinUI condition.
    # On a restored, never-built tree `TestContext.CancellationTokenSource` read as a member that does not exist.
    $cache = Join-Path $script:NuGetPackages 'mstest.testframework'
    # The extensions assembly is under `build\` up to MSTest 3.8 and `buildTransitive\` after it; it is only
    # COPIED into the fake package below, so either source will do.
    $source = Get-ChildItem $cache -Directory -ErrorAction SilentlyContinue |
        Where-Object { Test-Path (Join-Path $_.FullName 'lib\net8.0\Microsoft.VisualStudio.TestPlatform.TestFramework.dll') } |
        Where-Object {
            foreach ($folder in 'build', 'buildTransitive') {
                $candidate = Join-Path $_.FullName "$folder\net8.0\Microsoft.VisualStudio.TestPlatform.TestFramework.Extensions.dll"
                if (Test-Path $candidate) { $script:CsRefsExtensions = $candidate; return $true }
            }
            return $false
        } | Select-Object -First 1
    if (-not $source) { Write-Host '    (no MSTest.TestFramework in the NuGet cache - the package-targets case is not run)'; return }
    $extensions = $script:CsRefsExtensions
    $tree = Use-Tree @{
        'Demo.csproj' = $csproj
        'Probe.cs'    = "namespace Demo;" + $nl + "public static class Probe" + $nl + "{" + $nl +
                        "    public static void Stop(Microsoft.VisualStudio.TestTools.UnitTesting.TestContext context) => context.CancellationTokenSource.Cancel();" + $nl + "}" + $nl
    }
    $package = Join-Path $tree 'pkgs\fake.testframework\1.0.0'
    # The assembly sits only where the != branch points: a condition misread leaves `_Root` empty or wrong.
    foreach ($folder in 'lib\net10.0', 'build\net10.0\lib') { [void](New-Item -ItemType Directory -Path (Join-Path $package $folder) -Force) }
    Copy-Item (Join-Path $source.FullName 'lib\net8.0\Microsoft.VisualStudio.TestPlatform.TestFramework.dll') (Join-Path $package 'lib\net10.0')
    Copy-Item $extensions (Join-Path $package 'build\net10.0\lib')
    [System.IO.File]::WriteAllText((Join-Path $package 'build\net10.0\Fake.targets'), @'
<Project>
  <PropertyGroup Condition=" '$(UseWinUI)' == 'true' "><_Root>$(MSBuildThisFileDirectory)winui/</_Root></PropertyGroup>
  <PropertyGroup Condition=" '$(UseWinUI)' != 'true' "><_Root>$(MSBuildThisFileDirectory)lib/</_Root></PropertyGroup>
  <ItemGroup>
    <Reference Include="Microsoft.VisualStudio.TestPlatform.TestFramework.Extensions">
      <HintPath>$(_Root)Microsoft.VisualStudio.TestPlatform.TestFramework.Extensions.dll</HintPath>
    </Reference>
  </ItemGroup>
</Project>
'@)
    $packages = (Join-Path $tree 'pkgs') -replace '\\', '\\'
    $assets = '{"version":3,"targets":{"net10.0":{"Fake.TestFramework/1.0.0":{"type":"package",' +
              '"compile":{"lib/net10.0/Microsoft.VisualStudio.TestPlatform.TestFramework.dll":{}},' +
              '"build":{"build/net10.0/Fake.targets":{}}}}},' +
              '"libraries":{"Fake.TestFramework/1.0.0":{"type":"package","path":"fake.testframework/1.0.0"}},' +
              '"packageFolders":{"' + $packages + '\\":{}}}'
    [void](New-Item -ItemType Directory -Path (Join-Path $tree 'obj') -Force)
    [System.IO.File]::WriteAllText((Join-Path $tree 'obj\project.assets.json'), $assets)
    $built = New-CsRefsDb $tree
    Assert-Equal (Get-CsRefsErrors $built.Db) 'errors=0' 'the Extensions assembly the targets add is referenced'
    $bound = Invoke-Gate --map-query $built.Db --sql "SELECT 'symbol=' || symbol AS x FROM calls WHERE callee LIKE '%Cancel'" --width 200
    Assert-Line $bound 'symbol=System.Threading.CancellationTokenSource.Cancel'
}
