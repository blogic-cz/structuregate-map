<#
    The deep C# map, continued: CONSTRUCTS and DIAGNOSTICS, and what the PROJECT FILE says - symbols,
    generated sources, the compile list, the reference graph. Split from `CsRows` at its size limit.
#>

. (Join-Path $PSScriptRoot 'CsRows.Helpers.ps1')

# ---------------------------------------------------------------------------------------------------
# CONSTRUCTS AND DIAGNOSTICS: a value built out of parts, and what the compiler says is wrong
# ---------------------------------------------------------------------------------------------------

function Get-ConstructSample {
    return @'
namespace Demo;

public class Options
{
    public string Region { get; set; } = "";
    public int Retry { get; set; }
}

public static class Build
{
    public const string Us = "US";

    public static Options Make(int retry)
    {
        var routes = new[] { "api/" + Us, "api/eu" };
        var pair = (code: Us, count: 2);
        return new Options { Region = Us, Retry = retry };
    }
}
'@
}

if ($script:CsRowsPython) {

Test-Case 'objects: an array, a tuple and an initializer are CONSTRUCTS with their type and their slots' {
    # "What is this thing configured with" is a question about the construct, and the construct had no row:
    # an initializer was one assignment per line with no sense of the object they belong to.
    $db = New-ProjectTree @{ 'Build.cs' = Get-ConstructSample }
    Assert-Equal (Get-CsScalar $db "SELECT count(*) FROM objects WHERE kind = 'array' AND type = 'string[]' AND slots = 2") '1' 'the array'
    Assert-Line (Invoke-CsQ $db --sql "SELECT type FROM objects WHERE kind = 'tuple'") '(string code, int count)'
    Assert-Equal (Get-CsScalar $db "SELECT count(*) FROM objects WHERE kind = 'initializer' AND type = 'Demo.Options'") '1' 'the initializer, resolved'
}

Test-Case 'objects: a slot carries the member name and what the value folds to' {
    $db = New-ProjectTree @{ 'Build.cs' = Get-ConstructSample }
    $r = Invoke-CsQ $db --sql "SELECT name, const, const_kind FROM arguments a JOIN objects o ON o.id = a.object WHERE o.kind = 'initializer' ORDER BY a.position"
    Assert-Line $r 'Region'
    Assert-Line $r 'US'
    # The one that cannot be known says what it binds to instead, and claims no value.
    Assert-Equal (Get-CsScalar $db "SELECT count(*) FROM arguments WHERE name = 'Retry' AND const = '' AND symbol = 'retry'") '1' 'the parameter it was given'
    # A slot of a tuple is named too, where the language names it.
    Assert-Equal (Get-CsScalar $db "SELECT count(*) FROM arguments a JOIN objects o ON o.id = a.object WHERE o.kind = 'tuple' AND a.name = 'code'") '1' 'the tuple element'
}

Test-Case 'objects: an initializer slot is NOT also an assignment row' {
    # The same fact under two names is two places to keep right. The slot row is the one that says which
    # object it belongs to, so the assignment row stands down.
    $db = New-ProjectTree @{ 'Build.cs' = Get-ConstructSample }
    # `kind = 'assign'` is what an initializer's `Region = Us` would have produced. The row that remains is
    # the PROPERTY's own default (`kind = 'property'`), which is a different fact about a different line.
    Assert-Equal (Get-CsScalar $db "SELECT count(*) FROM assignments WHERE target = 'Region' AND kind = 'assign'") '0' 'no duplicate assignment row'
    Assert-Equal (Get-CsScalar $db "SELECT count(*) FROM assignments WHERE target = 'Region' AND kind = 'property'") '1' 'the property default is still recorded'
    # And the creation is still a CALL, because a constructor is called - which is how the python half says it.
    Assert-Equal (Get-CsScalar $db "SELECT count(*) FROM calls WHERE callee = 'Options'") '1' 'the constructor call'
}

Test-Case 'diagnostics: what the compiler could not resolve is a row, and a count on the file' {
    # This map builds its own compilation, so an error is usually a reference this pass could not find
    # rather than a bug in the code - which is exactly why it has to be visible. Without it, an empty
    # `symbol` column cannot be told apart from a file that never bound.
    $db = New-ProjectTree @{
        'A.cs' = 'namespace Demo; public class A { public int Go() { var x = new NotDeclaredAnywhere(); return 1; } }'
    }
    Assert-Line (Invoke-CsQ $db --sql "SELECT name, line FROM diagnostics") 'CS0246'
    Assert-Equal (Get-CsScalar $db "SELECT errors FROM files WHERE path = 'A.cs'") '1' 'counted on the file'
}

Test-Case 'diagnostics: every error of a file has a row, and the count agrees' {
    # NOTHING IS CUT. Ten rows of twenty-five read as the whole list, and the fifteen missing types were
    # the ones a reader never learned about.
    $lines = New-Object System.Text.StringBuilder
    [void]$lines.AppendLine('namespace Demo; public class Broken { public void Go() {')
    for ($i = 1; $i -le 25; $i++) { [void]$lines.AppendLine("var v$i = new Missing$i();") }
    [void]$lines.AppendLine('} }')
    $db = New-CsDb @{ 'Broken.cs' = $lines.ToString(); 'Demo.csproj' = '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework></PropertyGroup></Project>' }
    Assert-Equal (Get-CsScalar $db 'SELECT count(*) FROM diagnostics') '25' 'a row per error'
    Assert-Equal (Get-CsScalar $db "SELECT errors FROM files WHERE path = 'Broken.cs'") '25' 'and the whole count'
}

Test-Case 'diagnostics: a declaration error is a row of the file it is reported in, asked once per project' {
    # THE DECLARATION DIAGNOSTICS ARE THE COMPILATION'S, grouped by tree: asked per file, Roslyn completed every
    # declaration again for each one. Grouped wrongly, the error lands in no file or in the wrong one.
    $db = New-ProjectTree @{
        'A.cs' = 'namespace Demo; public class Dup {}'
        'B.cs' = 'namespace Demo; public class Dup {}'
    }
    Assert-Equal (Get-CsScalar $db "SELECT count(*) FROM diagnostics WHERE name = 'CS0101'") '1' 'one duplicate, one row'
    Assert-Equal (Get-CsScalar $db "SELECT sum(errors) FROM files WHERE path IN ('A.cs', 'B.cs')") '1' 'counted on the file'
}

}

# ---------------------------------------------------------------------------------------------------
# WHAT THE PROJECT FILE SAYS: symbols, generated sources, the compile list, the reference graph
# ---------------------------------------------------------------------------------------------------

# A project whose csproj carries whatever the case needs. `Demo.csproj` is the name New-ProjectTree uses.
function New-ProjectTreeWith([string]$Properties, [hashtable]$Files) {
    $project = "<Project Sdk=`"Microsoft.NET.Sdk`"><PropertyGroup><TargetFramework>net10.0</TargetFramework>$Properties</PropertyGroup></Project>"
    return New-CsDb ($Files + @{ 'Demo.csproj' = $project })
}

if ($script:CsRowsPython) {

Test-Case 'project: code behind an #if the project DEFINES is mapped, and the other side is not' {
    # A region whose symbol is not defined is not code to a parser - it is disabled text. Many hand-written
    # files of one solution open with one, and every row in them was missing.
    $source = 'namespace Demo; public class A { public int Go() {' + [char]10 +
              '#if FEATURE_X' + [char]10 + 'return Helper.Enabled();' + [char]10 +
              '#else' + [char]10 + 'return Helper.Disabled();' + [char]10 +
              '#endif' + [char]10 + '} }'
    $db = New-ProjectTreeWith '<DefineConstants>FEATURE_X</DefineConstants>' @{ 'A.cs' = $source }
    Assert-Equal (Get-CsScalar $db "SELECT count(*) FROM calls WHERE callee = 'Helper.Enabled'") '1' 'the defined side is code'
    Assert-Equal (Get-CsScalar $db "SELECT count(*) FROM calls WHERE callee = 'Helper.Disabled'") '0' 'the other side is not'
}

Test-Case 'project: a file the csproj REMOVES is not compiled and not mapped' {
    # Mapping what the build does not compile reports edges into code that is not in the assembly.
    $db = New-ProjectTreeWith '' @{
        'Kept.cs'         = 'namespace Demo; public class Kept { public int Go() { return 1; } }'
        'Legacy/Old.cs'   = 'namespace Demo; public class Old { public int Go() { return 2; } }'
        'Demo.csproj.tmp' = ''
    }
    Assert-Equal (Get-CsScalar $db "SELECT count(*) FROM files WHERE path LIKE 'Legacy/%'") '1' 'it is there by default'

    $removed = New-CsDb @{
        'Kept.cs'       = 'namespace Demo; public class Kept { public int Go() { return 1; } }'
        'Legacy/Old.cs' = 'namespace Demo; public class Old { public int Go() { return 2; } }'
        'Demo.csproj'   = '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework></PropertyGroup>' +
                          '<ItemGroup><Compile Remove="Legacy\**" /></ItemGroup></Project>'
    }
    Assert-Equal (Get-CsScalar $removed "SELECT count(*) FROM functions f JOIN files fl ON fl.id = f.file WHERE fl.path LIKE 'Legacy/%'") '0' 'and gone when the project removes it'
    # Listed, not dropped: a file missing from the map reads as a file the tree does not have.
    Assert-Equal (Get-CsScalar $removed "SELECT compiled FROM files WHERE path LIKE 'Legacy/%'") '0' 'and it says why'
    Assert-Equal (Get-CsScalar $removed "SELECT count(*) FROM functions WHERE name = 'Go'") '1' 'the kept one still maps'
}

Test-Case 'project: a generated global using is compiled, never mapped' {
    # `ImplicitUsings` puts every `using` a file relies on in obj\...GlobalUsings.g.cs. Without it, thousands of
    # names went unresolved in one project; with it as a MAPPED file it would be rows nobody wrote.
    $tree = Use-Tree @{
        'Demo.csproj' = '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework></PropertyGroup></Project>'
        'A.cs'        = 'namespace Demo; public class A { public StringBuilder Make() { return new StringBuilder(); } }'
        'obj/Debug/net10.0/Demo.GlobalUsings.g.cs' = '// <auto-generated/>' + [char]10 + 'global using System.Text;'
    }
    $db = Join-Path $tree 'map.sqlite'
    # QUOTED: the harness flattens a bare `bin,obj` into two arguments - see New-MixedTree above.
    Assert-Exit (Invoke-Gate --root $tree --skip 'bin,obj' --map-sqlite $db) 0
    Assert-Equal (Get-CsScalar $db "SELECT count(*) FROM calls WHERE symbol = 'System.Text.StringBuilder.StringBuilder'") '1' 'the type resolved through the global using'
    Assert-Equal (Get-CsScalar $db "SELECT count(*) FROM files WHERE path LIKE '%GlobalUsings%'") '0' 'and the generated file is not a mapped file'
}

Test-Case 'project: a reference is followed to SOURCE when nothing was ever built' {
    # A reference resolves through the referenced project's assembly only if somebody built it. On a fresh
    # clone there is none - and thousands of names of one project resolved to nothing for exactly that reason.
    $tree = Use-Tree @{
        'app/App.csproj' = '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework></PropertyGroup>' +
                           '<ItemGroup><ProjectReference Include="..\lib\Lib.csproj" /></ItemGroup></Project>'
        'app/App.cs'     = 'namespace App; using Lib; public class Program { public int Go() { return new Engine().Run(); } }'
        'lib/Lib.csproj' = '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework></PropertyGroup></Project>'
        'lib/Lib.cs'     = 'namespace Lib; public class Engine { public int Run() { return 1; } }'
    }
    $db = Join-Path $tree 'map.sqlite'
    Assert-Exit (Invoke-Gate --root $tree --map-sqlite $db) 0
    Assert-Equal (Get-CsScalar $db "SELECT count(*) FROM calls WHERE symbol = 'Lib.Engine.Run'") '1' 'bound across the project boundary'
    Assert-Equal (Get-CsScalar $db "SELECT sum(errors) FROM files WHERE path LIKE 'app/%'") '0' 'and nothing went unresolved'
}

Test-Case 'project: an <InternalsVisibleTo> item lets the friend project bind the internals when nothing was built' {
    # THE SDK TURNS THE ITEM INTO AN ASSEMBLY ATTRIBUTE in a generated AssemblyInfo.cs - one that only exists after a
    # build. Without it every call a test project makes into the internals it was given was CS0122, and stayed unbound.
    $tree = Use-Tree @{
        'app/App.csproj' = '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework></PropertyGroup>' +
                           '<ItemGroup><ProjectReference Include="..\lib\Lib.csproj" /></ItemGroup></Project>'
        'app/App.cs'     = 'namespace App; using Lib; public class Program { public int Go() { return Engine.Secret(); } }'
        'lib/Lib.csproj' = '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework></PropertyGroup>' +
                           '<ItemGroup><InternalsVisibleTo Include="App" /></ItemGroup></Project>'
        'lib/Lib.cs'     = 'namespace Lib; public static class Engine { internal static int Secret() { return 1; } }'
    }
    $db = Join-Path $tree 'map.sqlite'
    Assert-Exit (Invoke-Gate --root $tree --map-sqlite $db) 0
    Assert-Equal (Get-CsScalar $db "SELECT sum(errors) FROM files WHERE path LIKE 'app/%'") '0' 'no inaccessible-member error'
    Assert-Equal (Get-CsScalar $db "SELECT count(*) FROM calls WHERE symbol = 'Lib.Engine.Secret'") '1' 'the call into the internals bound'
    Assert-Equal (Get-CsScalar $db "SELECT count(*) FROM files WHERE path LIKE '%AssemblyInfo%'") '0' 'and the generated file is not a mapped file'
}

Test-Case 'project: an unchanged file is not opened, and a file outside the tree map is digested only when it moved' {
    # Thousands of OPENS of files nothing had touched were most of a large consumer's cold run; project.assets.json was read whole each time.
    $tree = Use-Tree @{
        'lib/Lib.csproj' = '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework></PropertyGroup></Project>'
        'lib/Lib.cs'     = 'namespace Lib; public class Engine { public int Run() { return 1; } }'
        'lib/Other.cs'   = 'namespace Lib; public class Other { }'
        'lib/obj/project.assets.json' = '{"version":3,"targets":{},"libraries":{},"packageFolders":{}}'
    }
    $db = Join-Path $tree 'map.sqlite'
    # `obj` SKIPPED, as by default: the assets file is outside the tree map, and has to be digested here.
    $deep = @('--root', $tree, '--skip', 'bin,obj', '--map-sqlite', $db)
    Assert-Exit (Invoke-Gate @deep) 0
    Assert-Equal (Get-CsScalar $db "SELECT count(*) FROM _digests WHERE path LIKE '%project.assets.json'") '1' 'the assets digest kept with its size and time'
    if (-not $script:OnWindows) {
        # SHUT, NOT CHANGED: a file nobody touched is answered from the tree map, never opened.
        & chmod 000 (Join-Path $tree 'lib/Other.cs')
        try {
            $again = Invoke-Gate @deep
            Assert-Exit $again 0
            Assert-Line $again 'the deep C# half had nothing to do: none of its 2 file(s) moved'
        } finally { & chmod 644 (Join-Path $tree 'lib/Other.cs') }
    }
}

Test-Case 'project: a moved file that will not open is dropped as the caller reads it, not opened first' {
    # EVERY MOVED FILE WAS OPENED ONCE BEFORE IT WAS READ: on Windows each first open is an antivirus scan, about a third of a large
    # consumer's run. The caller now says which it could not read, and the run still accounts for every file.
    if ($script:OnWindows) { return }
    $tree = Use-Tree @{
        'lib/Lib.csproj' = '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework></PropertyGroup></Project>'
        'lib/Lib.cs'     = 'namespace Lib; public class Engine { public int Run() { return 1; } }'
        'lib/Other.cs'   = 'namespace Lib; public class Other { }'
    }
    $db = Join-Path $tree 'map.sqlite'
    $deep = @('--root', $tree, '--map-sqlite', $db)
    Assert-Exit (Invoke-Gate @deep) 0
    $other = Join-Path $tree 'lib/Other.cs'
    [System.IO.File]::WriteAllText($other, 'namespace Lib; public class Other { public int Moved; }')
    & chmod 000 $other
    try {
        $shut = Invoke-Gate @deep
        Assert-Exit $shut 0
        Assert-NoLine $shut 'said nothing about the rest'
        Assert-Equal (Get-CsScalar $db "SELECT count(*) FROM files WHERE path = 'lib/Other.cs'") '0' 'the shut file has no row'
        Assert-Equal (Get-CsScalar $db "SELECT count(*) FROM files WHERE path = 'lib/Lib.cs'") '1' 'the rest is kept'
    } finally { & chmod 644 $other }
    $open = Invoke-Gate @deep
    Assert-Exit $open 0
    Assert-Equal (Get-CsScalar $db "SELECT count(*) FROM members WHERE name = 'Moved'") '1' 'opened again, it is read'
}

Test-Case 'project: a project that was never restored is a WARNING in the map summary and a list in _meta' {
    # ROSLYN STILL BINDS IT, without the packages: on one tree hundreds of files carried compile errors under a summary that
    # said every file was mapped.
    $tree = Use-Tree @{
        'lib/Lib.csproj' = '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework></PropertyGroup></Project>'
        'lib/Lib.cs'     = 'namespace Lib; public class Engine { public Missing.Thing Run() { return null; } }'
    }
    $db = Join-Path $tree 'map.sqlite'
    $map = @('--root', $tree, '--ext', '.cs', '--map', '--map-out', (Join-Path $tree 'm.json'), '--map-sqlite', $db)
    $first = Invoke-Gate @map
    Assert-Exit $first 0
    Assert-Line $first 'WARNING      1 C# project(s) never restored (no obj/project.assets.json: lib/Lib.csproj)'
    Assert-Line $first '1 C# file(s) have compile errors; run `dotnet restore`'
    Assert-Equal (Get-CsScalar $db "SELECT count(*) FROM _meta WHERE key = 'unrestored:csharp' AND value LIKE '%lib/Lib.csproj%'") '1' 'the project listed in _meta'
    # RESTORED: the record of the restore is on disk, and the warning goes.
    [void](New-Item -ItemType Directory -Path (Join-Path $tree 'lib/obj') -Force)
    [System.IO.File]::WriteAllText((Join-Path $tree 'lib/obj/project.assets.json'), '{"version":3,"targets":{},"libraries":{},"packageFolders":{}}')
    $restored = Invoke-Gate @map
    Assert-Exit $restored 0
    Assert-NoLine $restored 'never restored'
    Assert-Equal (Get-CsScalar $db "SELECT count(*) FROM _meta WHERE key = 'unrestored:csharp' AND value = '[]'") '1' 'and the list empty'
}

}

if ($script:CsRowsPython) {

# A REAL assembly that collides with source, built once and cached. A dll whose types do not collide proves
# nothing about the self-reference rule: the first two versions of the case below used an unrelated assembly
# and then a plain class, and C# lets SOURCE win over metadata silently in both. An extension method is
# where the collision actually shows - the compiler reports CS0121 and refuses to pick.
function Get-ProbeAssembly {
    $cache = Join-Path ([System.IO.Path]::GetTempPath()) 'sgtest-probe'
    $dll = Join-Path $cache 'bin/Debug/net10.0/Probe.dll'
    if (Test-Path $dll) { return $dll }
    [void](New-Item -ItemType Directory -Path $cache -Force)
    [System.IO.File]::WriteAllText((Join-Path $cache 'Probe.csproj'),
        '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework></PropertyGroup></Project>')
    [System.IO.File]::WriteAllText((Join-Path $cache 'Ext.cs'),
        'namespace Probe; public static class Ext { public static int Twice(this int n) { return n * 2; } }')
    & dotnet build (Join-Path $cache 'Probe.csproj') -v quiet --nologo 2>&1 | Out-Null
    if (Test-Path $dll) { return $dll }
    return $null
}

Test-Case 'project: the project does not reference ITS OWN build output' {
    # A bin folder holds the project's own assembly. Referencing it declares everything twice - once from
    # source, once from metadata - and the compiler answers CS0121, "the call is ambiguous": over a thousand of
    # them in one real project, every one read as a row that resolved to nothing.
    $probe = Get-ProbeAssembly
    if (-not $probe) { Write-Host '        (no dotnet SDK to build the probe assembly - case skipped)'; return }
    $tree = Use-Tree @{
        'Probe.csproj' = '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework></PropertyGroup></Project>'
        'Ext.cs'       = 'namespace Probe; public static class Ext { public static int Twice(this int n) { return n * 2; } }'
        'Use.cs'       = 'namespace Probe; public class Use { public int Go() { return 21.Twice(); } }'
    }
    $bin = Join-Path $tree 'bin/Debug/net10.0'
    [void](New-Item -ItemType Directory -Path $bin -Force)
    Copy-Item $probe (Join-Path $bin 'Probe.dll') -Force
    $db = Join-Path $tree 'map.sqlite'
    Assert-Exit (Invoke-Gate --root $tree --skip 'bin,obj' --map-sqlite $db) 0
    Assert-Equal (Get-CsScalar $db 'SELECT sum(errors) FROM files') '0' 'nothing is declared twice'
    Assert-Equal (Get-CsScalar $db "SELECT count(*) FROM calls WHERE symbol = 'Probe.Ext.Twice'") '1' 'and the call bound to the source'
}

Test-Case 'calls: a chain is anchored at the NAME called, not at the start of the expression' {
    # `GetAll()` newline `.Where(...)` newline `.ToList()` starts on one line and calls three methods on
    # three others. A row that reported the first line for all of them puts every call of the chain on a
    # line that calls nothing - which is how about a tenth of the call sites failed to line up with another map.
    $source = 'namespace Demo; public class A { public System.Collections.Generic.List<int> Go() {' + [char]10 +
              '    return Source.All()' + [char]10 +
              '        .Where(x => x > 1)' + [char]10 +
              '        .ToList();' + [char]10 + '} }'
    $db = New-CsDb @{ 'A.cs' = $source }
    # `Where` and `ToList` are named by the METHOD, because their receiver is itself a call - see the case
    # below. What is pinned here is the LINE each one sits on.
    Assert-Equal (Get-CsScalar $db "SELECT line FROM calls WHERE callee = 'Where'") '3' 'Where sits on its own line'
    Assert-Equal (Get-CsScalar $db "SELECT line FROM calls WHERE callee = 'ToList'") '4' 'and ToList on its'
    # The whole expression is still recorded, so a reader can see how far the chain runs.
    Assert-Equal (Get-CsScalar $db "SELECT end_line FROM calls WHERE callee = 'ToList'") '4' 'with its own span'
}

}

if ($script:CsRowsPython) {

Test-Case 'calls: a callee whose receiver is itself a call is named by the METHOD, not by the chain' {
    # `Get().Where(...)` calls `Where`. Naming the row after the whole chain's text puts it under a name
    # nothing can join to - five call sites of one file, invisible to any query for `Where`.
    $db = New-CsDb @{ 'A.cs' = 'namespace Demo; public class A { public int Go() { return Source.All().Count(); } }' }
    Assert-Equal (Get-CsScalar $db "SELECT count(*) FROM calls WHERE callee = 'Count'") '1' 'named by the method'
    Assert-Equal (Get-CsScalar $db "SELECT count(*) FROM calls WHERE callee LIKE '%All()%'") '0' 'not by the chain'
    # The full text is still on the row, where a reader can see it.
    Assert-Line (Invoke-CsQ $db --sql "SELECT source FROM calls WHERE callee = 'Count'") 'Source.All().Count()'
}

Test-Case 'project: a library that uses the hosting framework gets it, without declaring it' {
    # A class library can use `IHttpContextAccessor` while declaring neither the web SDK nor a framework
    # reference - the type reaches it through what its consumers bring. The restored closure is the
    # evidence, and without acting on it several call sites of one real project resolved to nothing.
    $tree = Use-Tree @{
        'Lib.csproj' = '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework></PropertyGroup></Project>'
        'A.cs'       = 'namespace Demo; using Microsoft.AspNetCore.Http; public class A { public string Go(IHttpContextAccessor a) { return a.HttpContext.Request.Path.ToString(); } }'
        # A restore that names an ASP.NET package is what says this library is hosted.
        'obj/project.assets.json' = '{"version":3,"targets":{"net10.0":{}},"libraries":{"Microsoft.AspNetCore.Http.Abstractions/2.2.0":{"type":"package","path":"microsoft.aspnetcore.http.abstractions/2.2.0"}},"packageFolders":{}}'
    }
    $db = Join-Path $tree 'map.sqlite'
    Assert-Exit (Invoke-Gate --root $tree --skip 'bin,obj' --map-sqlite $db) 0
    Assert-Equal (Get-CsScalar $db "SELECT sum(errors) FROM files") '0' 'the hosting types resolved'
    # And the call through those types BOUND, which is the fact the resolved columns exist for.
    # The chain here IS a written name, so the callee keeps its dotted form; what is pinned is the SYMBOL.
    Assert-Equal (Get-CsScalar $db "SELECT count(*) FROM calls WHERE symbol = 'Microsoft.AspNetCore.Http.PathString.ToString'") '1' 'the call through them resolved'
}

}

if ($script:CsRowsPython) {

Test-Case 'project: a project compiled as someone else REFERENCE is still bound when its own turn comes' {
    # `app` is mapped first and compiles `lib` to reference it. When `lib`'s own files arrive, the cache
    # answers with that compilation - built without keeping its trees by path, because nothing was going to
    # be mapped out of it. Most of the files of one solution silently lost their model that way and
    # produced syntax-only rows, while every count still looked right.
    $tree = Use-Tree @{
        'app/App.csproj' = '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework></PropertyGroup>' +
                           '<ItemGroup><ProjectReference Include="..\lib\Lib.csproj" /></ItemGroup></Project>'
        'app/App.cs'     = 'namespace App; using Lib; public class Program { public int Go() { return new Engine().Run(); } }'
        'lib/Lib.csproj' = '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework></PropertyGroup></Project>'
        'lib/Lib.cs'     = 'namespace Lib; public class Engine { public int Run() { return Helper.Value(); } }'
        'lib/Helper.cs'  = 'namespace Lib; public static class Helper { public static int Value() { return 1; } }'
    }
    $db = Join-Path $tree 'map.sqlite'
    Assert-Exit (Invoke-Gate --root $tree --map-sqlite $db) 0
    Assert-Equal (Get-CsScalar $db "SELECT count(*) FROM files WHERE path LIKE 'lib/%' AND semantic = 0") '0' 'every file of the referenced project bound'
    Assert-Equal (Get-CsScalar $db "SELECT count(*) FROM calls WHERE symbol = 'Lib.Helper.Value'") '1' 'and its own calls resolved'
}

}

if ($script:CsRowsPython) {

Test-Case 'objects: a construct inside a construct names the one that holds it' {
    # `new (string, string)[] { (path, name), ... }` is an array whose ELEMENTS ARE TUPLES. Without the
    # link each tuple is a row floating free of its array: a reader can see both and cannot say which array
    # a pair came from - which is exactly the join a reader of this map does.
    $source = 'namespace Demo; public class Files { public static (string, string)[] Pairs = new (string, string)[]' + [char]10 +
              '{ ("wwwroot/a.pdf", "A"), ("wwwroot/b.pdf", "B") }; }'
    # A PROJECT TREE, because the folded values below need a compilation: without one the rows are there
    # and every resolved column is empty, and the case would be asserting on the syntax pass.
    $db = New-ProjectTree @{ 'Files.cs' = $source }
    $array = Get-CsScalar $db "SELECT count(*) FROM objects WHERE kind = 'array' AND parent = ''"
    Assert-Equal $array '1' 'the array is the outer one'
    Assert-Equal (Get-CsScalar $db "SELECT count(*) FROM objects o JOIN objects p ON p.id = o.parent WHERE o.kind = 'tuple' AND p.kind = 'array'") '2' 'both tuples name it'
    # And the values inside the nested construct are folded, so the join answers with strings.
    Assert-Equal (Get-CsScalar $db "SELECT count(*) FROM arguments a JOIN objects o ON o.id = a.object WHERE o.kind = 'tuple' AND a.const LIKE 'wwwroot/%'") '2' 'with the paths themselves'
}

}

if ($script:CsRowsPython) {

Test-Case 'objects: a construct says what it is STORED IN, and a nested one claims nothing' {
    # Two facts a reader of this map joins on. `_files = new (string,string)[] {…}` is the array its entries
    # live in, found by the construct and not by LINE, so an initializer wrapped onto the next line still is.
    # And the TUPLE inside that array is not stored in `_files` - it must not claim the name too.
    $source = 'namespace Demo; public class P {' + [char]10 +
              '  private static readonly (string, string)[] _files = new (string, string)[]' + [char]10 +
              '  { ("a.pdf", "A") };' + [char]10 + '}'
    $db = New-ProjectTree @{ 'P.cs' = $source }
    Assert-Equal (Get-CsScalar $db "SELECT count(*) FROM objects WHERE kind = 'array' AND target = '_files'") '1' 'the array is the one stored'
    Assert-Equal (Get-CsScalar $db "SELECT count(*) FROM objects WHERE kind = 'tuple' AND target <> ''") '0' 'the tuple inside it claims nothing'
}

Test-Case 'scope: a row inside a FIELD initializer names the field as its member' {
    # Three fields of one class are otherwise indistinguishable - every row carries the same empty member
    # name - and a reader looking for "the array this creation iterates" finds the first one in the CLASS.
    $source = 'namespace Demo; public class P {' + [char]10 +
              '  private static readonly int[] _first = new int[] { Source.One() };' + [char]10 +
              '  private static readonly int[] _second = new int[] { Source.Two() };' + [char]10 + '}'
    $db = New-ProjectTree @{ 'P.cs' = $source }
    # `Get-CsScalar` reads a NUMBER out of a lens result; these answers are names, so they are counted.
    Assert-Equal (Get-CsScalar $db "SELECT count(*) FROM calls WHERE callee = 'Source.One' AND func = '_first'") '1' 'the first field owns its call'
    Assert-Equal (Get-CsScalar $db "SELECT count(*) FROM calls WHERE callee = 'Source.Two' AND func = '_second'") '1' 'and the second owns its own'
    Assert-Equal (Get-CsScalar $db "SELECT count(*) FROM objects WHERE func = '_first'") '1' 'the array belongs to the field too'
}

}
