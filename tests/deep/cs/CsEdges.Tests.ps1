<#
    THE C# FILE EDGES, and what the compiler adds to them.

    - A NAME SEVERAL FILES DECLARE is joined to the one the deep map's compiler bound it to (`file_refs`) - and
      only among the files that declare it NOW: a target from rows an unchanged file kept is never an edge alone.
    - A GENERIC OR QUALIFIED TYPE NAME is a reference: a `refs` row naming the type's DEFINITION, and an edge.
    - A CLASS A FRAMEWORK ACTIVATES BY CONVENTION (a controller, a page model, a hub) is `registered`, not NO READER.

    Its helpers are its own - `-Only CsEdges` runs this suite alone.
#>

$script:CsEdgesProject = '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework></PropertyGroup></Project>'

# The file map over $Tree - with the deep map beside it when -Deep - and what it wrote.
function Invoke-CsEdgesMap([string]$Tree, [switch]$Deep) {
    $sqlite = if ($Deep) { @('--map-sqlite', (Join-Path $Tree 'map.sqlite')) } else { @() }
    $result = Invoke-Gate --root $Tree --ext .cs --skip 'bin,obj' --map --map-check --map-out (Join-Path $Tree 'm.json') @sqlite
    $map = [System.IO.File]::ReadAllText((Join-Path $Tree 'm.json')) | ConvertFrom-Json
    return [pscustomobject]@{ Result = $result; Map = $map }
}

# What a file imports, sorted and joined - '' for none.
function Get-CsEdgesImports($Map, [string]$Rel) {
    $listed = $Map.imports.$Rel
    if ($null -eq $listed) { return '' }
    return (@($listed) | Sort-Object) -join ','
}

function Get-CsEdgesNumber([string]$Tree, [string]$Sql) {
    $r = Invoke-Gate --map-query (Join-Path $Tree 'map.sqlite') --width 0 --sql $Sql
    Assert-Exit $r 0
    foreach ($line in $r.Lines) {
        $trimmed = $line.Trim()
        if ($trimmed.Length -gt 0 -and ($trimmed.ToCharArray() | Where-Object { -not [char]::IsDigit($_) }).Count -eq 0) { return $trimmed }
    }
    return ''
}

Test-Case 'edges: a name declared in two files joins the one the compiler bound, and the twin is not ambiguous' {
    $tree = Use-Tree @{
        'Demo.csproj'      = $script:CsEdgesProject
        'Results/Result.cs' = 'namespace Demo.Results; public class Result { public static Result Ok() => new Result(); }'
        'Other/Result.cs'   = 'namespace Demo.Other; public class Result { }'
        'Use.cs'            = 'namespace Demo; using Demo.Results; public class Use { public Result Go() => Result.Ok(); }'
    }
    $run = Invoke-CsEdgesMap $tree -Deep
    Assert-Equal (Get-CsEdgesImports $run.Map 'Use.cs') 'Results/Result.cs' 'the bound edge'
    Assert-NoLine $run.Result 'AMBIGUOUS'
    Assert-Equal (Get-CsEdgesNumber $tree "SELECT count(*) FROM file_refs r JOIN files f ON f.id = r.file WHERE f.path = 'Use.cs' AND r.target = 'Results/Result.cs'") '1' 'the file_refs row'
    # WITHOUT THE COMPILER the name join is what it always was: no edge, and the name reported.
    $plain = Invoke-CsEdgesMap $tree
    Assert-Equal (Get-CsEdgesImports $plain.Map 'Use.cs') '' 'no edge from the name alone'
}

Test-Case 'edges: a bound target that no longer declares the name is no edge - the rows an unchanged file kept are not taken on their word' {
    $tree = Use-Tree @{
        'Demo.csproj'  = $script:CsEdgesProject
        'A/Result.cs'  = 'namespace Demo.A; public class Result { }'
        'B/Result.cs'  = 'namespace Demo.B; public class Result { }'
        'C/Result.cs'  = 'namespace Demo.C; public class Result { }'
        'Use.cs'       = 'namespace Demo; using Demo.A; public class Use { public Result Held; }'
    }
    $first = Invoke-CsEdgesMap $tree -Deep
    Assert-Equal (Get-CsEdgesImports $first.Map 'Use.cs') 'A/Result.cs' 'bound to A first'
    # A's Result is renamed: Use.cs did not change, so its deep rows - and their target A - are kept.
    [System.IO.File]::WriteAllText((Join-Path $tree 'A/Result.cs'), 'namespace Demo.A; public class Renamed { }')
    $after = Invoke-CsEdgesMap $tree -Deep
    Assert-Equal (Get-CsEdgesImports $after.Map 'Use.cs') '' 'no edge to a file that no longer declares Result'
}

Test-Case 'refs: an open generic, a constructed generic and a qualified type name are rows naming the definition, and edges' {
    $tree = Use-Tree @{
        'Demo.csproj'    = $script:CsEdgesProject
        'Pipeline.cs'    = 'namespace Demo; public class Pipeline<TReq, TRes> { }'
        'Other/Thing.cs' = 'namespace Demo.Other; public class Thing { }'
        'Reg.cs'         = "namespace Demo;`npublic static class Reg`n{`n    public static System.Type[] Go() => new[] { typeof(Pipeline<,>) };`n" +
                           "    public static Pipeline<int, string> Held;`n    public static Demo.Other.Thing Named;`n}`n"
    }
    $run = Invoke-CsEdgesMap $tree -Deep
    $refs = "SELECT count(*) FROM refs r JOIN files f ON f.id = r.file WHERE f.path = 'Reg.cs' AND r.kind = 'namedtype' AND r.symbol = "
    Assert-Equal (Get-CsEdgesNumber $tree ($refs + "'Demo.Pipeline<TReq, TRes>'")) '2' 'typeof(Pipeline<,>) and Pipeline<int, string>, by the definition'
    Assert-Equal (Get-CsEdgesNumber $tree ($refs + "'Demo.Other.Thing'")) '1' 'the qualified name'
    Assert-Equal (Get-CsEdgesImports $run.Map 'Reg.cs') 'Pipeline.cs' 'the generic name is an edge of the file map'
    Assert-NoLine $run.Result 'NO READER Pipeline.cs'
}

Test-Case 'map: a class a framework activates is registered, and an abstract one is not' {
    $tree = Use-Tree @{
        'Api/BaseController.cs'  = 'namespace N.Api; public abstract class BaseController : Microsoft.AspNetCore.Mvc.ControllerBase { }'
        'Api/ThingsController.cs' = 'namespace N.Api; public class ThingsController : BaseController { public int Get() => 1; }'
        'Pages/Index.cshtml.cs'   = 'namespace N.Pages; public class IndexModel : PageModel { }'
        'Hubs/Chat.cs'            = 'namespace N.Hubs; [ApiController] public class Chat { }'
        'Api/Zombie.cs'           = 'namespace N.Api; public abstract class Zombie : Microsoft.AspNetCore.Mvc.ControllerBase { }'
    }
    $run = Invoke-CsEdgesMap $tree
    Assert-Equal ((@($run.Map.files.'Api/ThingsController.cs'.registered)) -join ',') 'Controller' 'the controller, by its suffix'
    Assert-Equal ((@($run.Map.files.'Pages/Index.cshtml.cs'.registered)) -join ',') 'PageModel' 'the page model, by its base'
    Assert-Equal ((@($run.Map.files.'Hubs/Chat.cs'.registered)) -join ',') 'ApiController' 'the attribute'
    Assert-NoLine $run.Result 'NO READER Api/ThingsController.cs'
    Assert-NoLine $run.Result 'NO READER Pages/Index.cshtml.cs'
    Assert-NoLine $run.Result 'NO READER Api/BaseController.cs'
    Assert-Line $run.Result 'NO READER Api/Zombie.cs'
}

Test-Case 'map: a class a test framework runs is registered - MSTest, NUnit and xUnit, by class or by method' {
    $tree = Use-Tree @{
        'test/MsTests.cs'    = 'namespace T; [TestClass] public class MsTests { [TestMethod] public void Works() { } }'
        'test/NunitTests.cs' = 'namespace T; public class NunitTests { [Test] public void Works() { } }'
        'test/XunitTests.cs' = 'namespace T; public class XunitTests { [Xunit.Fact] public void Works() { } }'
        'test/Helpers.cs'    = 'namespace T; public class Helpers { public void Help() { } }'
    }
    $run = Invoke-CsEdgesMap $tree
    Assert-Equal ((@($run.Map.files.'test/MsTests.cs'.registered)) -join ',') 'TestClass,TestMethod' 'MSTest'
    Assert-Equal ((@($run.Map.files.'test/NunitTests.cs'.registered)) -join ',') 'Test' 'NUnit, by its method alone'
    Assert-Equal ((@($run.Map.files.'test/XunitTests.cs'.registered)) -join ',') 'Fact' 'xUnit, qualified'
    Assert-NoLine $run.Result 'NO READER test/XunitTests.cs'
    Assert-Line $run.Result 'NO READER test/Helpers.cs'
}

Test-Case 'imports: a C# using is used when the compiler needs it, and --unused-imports lists only the one it does not' {
    $tree = Use-Tree @{
        'Demo.csproj' = $script:CsEdgesProject
        'A.cs'        = "using System;`nusing System.Text;`nusing System.Linq;`nnamespace Demo;`npublic class A { public int Go(int[] x) => x.Count() + DateTime.Now.Year; }`n"
        'Broken.cs'   = "using System.Text;`nnamespace Demo;`npublic class Broken { public Missing.Thing Held; }`n"
    }
    Invoke-CsEdgesMap $tree -Deep | Out-Null
    Assert-Equal (Get-CsEdgesNumber $tree "SELECT count(*) FROM imports i JOIN files f ON f.id = i.file WHERE f.path = 'A.cs' AND i.used = 1") '2' 'System and System.Linq'
    $lens = Invoke-Gate --map-query (Join-Path $tree 'map.sqlite') --unused-imports --width 0
    Assert-Exit $lens 0
    Assert-Line $lens 'System.Text'
    Assert-Equal (@($lens.Lines | Where-Object { $_.Contains('import System') -or $_.Contains('System.Linq') -or $_.Contains('Broken.cs') })).Count 1 'only A.cs''s System.Text - a file with errors says nothing'
}
