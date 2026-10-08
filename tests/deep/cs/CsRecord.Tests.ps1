<#
    WHAT THE BUILD RECORDED IT COMPILED - `structuregate.inputs.tsv`, which `StructureGate.targets` writes after
    every CoreCompile - against what the deep C# map would otherwise reconstruct from the project file.

    Its own suite because CsRows.Tests.ps1 is held in the size baseline, and its helpers are named for it:
    every suite is dot-sourced into one scope, and a helper named like another suite's replaces it.
#>

function Get-RecordScalar([string]$Db, [string]$Sql) {
    $r = Invoke-Gate --map-query $Db --sql $Sql
    Assert-Exit $r 0
    return ($r.Lines | Where-Object { $_ -match '^\s*\d+\s*$' } | Select-Object -First 1).Trim()
}

# A project with a RECORD of what its last build compiled (StructureGate.targets writes one after every
# CoreCompile): here it defines RECORDED, which the csproj itself does not.
function New-RecordedTree {
    $source = 'namespace Demo; public class A { public int Go() {' + [char]10 +
              '#if RECORDED' + [char]10 + 'return Helper.Recorded();' + [char]10 +
              '#else' + [char]10 + 'return Helper.Guessed();' + [char]10 +
              '#endif' + [char]10 + '} }'
    $tree = Use-Tree @{
        'A.cs' = $source
        'Demo.csproj' = '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework></PropertyGroup></Project>'
    }
    $record = Join-Path $tree 'obj\Debug\net10.0\structuregate.inputs.tsv'
    New-Item -ItemType Directory -Force (Split-Path $record) | Out-Null
    [System.IO.File]::WriteAllText($record, "tfm`tnet10.0`nlang`t`ndefine`tRECORDED;DEBUG`ncompile`t$(Join-Path $tree 'A.cs')`n")
    (Get-Item (Join-Path $tree 'Demo.csproj')).LastWriteTime = (Get-Date).AddMinutes(-5)
    return $tree
}

function Invoke-RecordedMap([string]$Tree) {
    $db = Join-Path $Tree 'map.sqlite'
    Assert-Exit (Invoke-Gate --root $Tree --ext .cs --map-sqlite $db) 0
    return $db
}

Test-Case 'project: what the BUILD recorded it compiled wins over what the csproj alone would say' {
    $db = Invoke-RecordedMap (New-RecordedTree)
    Assert-Equal (Get-RecordScalar $db "SELECT count(*) FROM calls WHERE callee = 'Helper.Recorded'") '1' 'the recorded symbol is defined'
    Assert-Equal (Get-RecordScalar $db "SELECT count(*) FROM calls WHERE callee = 'Helper.Guessed'") '0' 'the reconstruction is not used'
}

Test-Case 'project: a record OLDER than its csproj is not used - the project changed since that build' {
    $tree = New-RecordedTree
    (Get-Item (Join-Path $tree 'Demo.csproj')).LastWriteTime = (Get-Date).AddMinutes(5)
    $db = Invoke-RecordedMap $tree
    Assert-Equal (Get-RecordScalar $db "SELECT count(*) FROM calls WHERE callee = 'Helper.Guessed'") '1' 'the csproj is read again'
    Assert-Equal (Get-RecordScalar $db "SELECT count(*) FROM calls WHERE callee = 'Helper.Recorded'") '0' 'the stale record is not'
}

