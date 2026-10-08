<#
    A project the consumer `--skip`s but a mapped project references is bound through what its BUILD wrote, not
    compiled from source (one templating project, seconds of a large consumer's refresh). Without a build it
    is compiled as before, so a skipped folder never costs a mapped file its binding.

    Its helpers are its own, beside `CsRows.Helpers.ps1` - `-Only CsSkipRef` runs this suite alone.
#>

. (Join-Path $PSScriptRoot '../CsRows.Helpers.ps1')

function New-CsSkipRefTree {
    $tree = Use-Tree @{
        'Skipme/Lib/Lib.csproj' = '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework></PropertyGroup></Project>'
        'Skipme/Lib/Engine.cs'  = 'namespace Lib; public class Engine { public int Run() { return 1; } }'
        'App/App.csproj'        = '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework></PropertyGroup>' +
            '<ItemGroup><ProjectReference Include="..\Skipme\Lib\Lib.csproj" /></ItemGroup></Project>'
        'App/Use.cs'            = 'namespace App; public class Use { public int Go() { return new Lib.Engine().Run(); } }'
    }
    return $tree
}

# How many source files the run parsed for the skipped project: its `csharp project:` span says so. Reading its
# project file alone still makes a span - one with no files.
function Get-CsSkipRefParsed([string]$Trace) {
    $request = [System.IO.File]::ReadAllLines($Trace)[-1] | ConvertFrom-Json
    $span = $request.resourceSpans[0].scopeSpans[0].spans | Where-Object { $_.name -like 'csharp project: *Lib.csproj' } | Select-Object -First 1
    if (-not $span) { return 0 }
    return [int](($span.attributes | Where-Object { $_.key -eq 'structuregate.files' }).value.intValue)
}

Test-Case 'csskipref: a skipped project with a build is bound through its dll, not compiled; without one it is compiled' {
    $tree = New-CsSkipRefTree
    $db = Join-Path $tree 'map.sqlite'
    $trace = Join-Path $tree 'trace.jsonl'
    $deep = @('--root', $tree, '--ext', '.cs', '--skip', 'bin,obj,Skipme', '--map-sqlite', $db, '--trace', $trace)

    # NO BUILD YET: compiled from source, and the call binds.
    Assert-Exit (Invoke-Gate @deep) 0
    Assert-Equal (Get-CsScalar $db "SELECT count(*) FROM calls WHERE symbol LIKE 'Lib.Engine.Run%'") '1' 'bound against the source'
    Assert-Equal (Get-CsSkipRefParsed $trace) 1 'compiled from its source while it has no build'

    # BUILT: the dll is referenced and the project is not compiled; the call still binds.
    & dotnet build (Join-Path $tree 'Skipme/Lib/Lib.csproj') -nologo -v q 2>&1 | Out-Null
    Remove-Item $db, $trace -Force
    Assert-Exit (Invoke-Gate @deep) 0
    Assert-Equal (Get-CsScalar $db "SELECT count(*) FROM calls WHERE symbol LIKE 'Lib.Engine.Run%'") '1' 'bound against the dll'
    Assert-Equal (Get-CsSkipRefParsed $trace) 0 'no source parsed once it is built'
}
