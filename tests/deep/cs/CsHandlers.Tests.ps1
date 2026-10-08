<#
    The C# deep map's `handlers` rows: one per `catch` clause, in the python half's columns - what it catches,
    what it binds and what its own body does - plus `guard` (the `when` filter), `finally` and the bound
    `symbol`. See `src/Map/CsRows/CsBody/CsRowsHandlers.cs`.

    Its helpers are its own - `-Only CsHandlers` runs this suite alone.
#>

$script:CsHandlersProject = '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework></PropertyGroup></Project>'

# Lines as numbered in the cases: the `try` on 7, the catches on 8, 14 and 18.
$script:CsHandlersGuard = @'
using System;
namespace Demo;
public class Guard
{
    public void Run(Func<int> go, Action<string, Exception> log)
    {
        try { go(); }
        catch (InvalidOperationException ex) when (ex.HResult == 5) // known: retried upstream
        {
            log("x", ex);
            Action again = () => { throw ex; };
            throw;
        }
        catch (Demo.Fault)
        {
            // nothing to do
        }
        catch
        {
            throw new ArgumentException("bad");
        }
        finally { go(); }
    }
}
public class Fault : Exception { }
'@

# The deep map of a throwaway tree, as its database path.
function New-CsHandlersDb([hashtable]$Files) {
    $tree = Use-Tree $Files
    $db = Join-Path $tree 'map.sqlite'
    Assert-Exit (Invoke-Gate --root $tree --ext .cs --map-sqlite $db) 0
    return $db
}

# One query, every cell whole.
function Get-CsHandlersRows([string]$Db, [string]$Sql) {
    $r = Invoke-Gate --map-query $Db --width 0 --sql $Sql
    Assert-Exit $r 0
    return $r
}

# One exact line, so a containment check cannot pass on a neighbour.
function Assert-CsHandlersRow($Result, [string]$Expected) {
    $hits = @($Result.Lines | Where-Object { $_.Trim() -ceq $Expected })
    if ($hits.Count -eq 0) { throw "no line is exactly '$Expected'. Output:`n$($Result.Text)" }
}

Test-Case 'cshandlers: one row per catch clause says what it catches, binds and does' {
    $db = New-CsHandlersDb @{ 'Guard.cs' = $script:CsHandlersGuard; 'Demo.csproj' = $script:CsHandlersProject }
    # Digits: bare, name_read, passes, raises, reraises, finally.
    $r = Get-CsHandlersRows $db ("SELECT 'h=' || line || ':' || types || ':' || name || ':' || bare || name_read || passes" +
        " || raises || reraises || finally || ':' || guard || ':' || comment || ':' || calls || ':' || symbol FROM handlers ORDER BY line")
    # The lambda's `throw ex` is not the clause's own: one raise, and `throw;` is the reraise.
    Assert-CsHandlersRow $r 'h=8:["InvalidOperationException"]:ex:010111:ex.HResult == 5:// known: retried upstream:["log"]:System.InvalidOperationException'
    # A comment is no statement, and one on a later line is not the catch line's.
    Assert-CsHandlersRow $r 'h=14:["Demo.Fault"]::001001:::[]:Demo.Fault'
    Assert-CsHandlersRow $r 'h=18:[]::100101:::["ArgumentException"]:'
    Assert-CsHandlersRow (Get-CsHandlersRows $db "SELECT 'r=' || reads FROM handlers WHERE line = 14") 'r=["Demo", "Demo.Fault"]'
    Assert-CsHandlersRow (Get-CsHandlersRows $db "SELECT 'try=' || test FROM branches WHERE kind = 'try'") 'try=InvalidOperationException, Demo.Fault, *'
}

Test-Case 'cshandlers: without a project the bound type is empty and the written one stays' {
    $db = New-CsHandlersDb @{ 'Guard.cs' = $script:CsHandlersGuard }
    $r = Get-CsHandlersRows $db "SELECT 'h=' || line || ':' || types || ':' || symbol FROM handlers ORDER BY line"
    Assert-CsHandlersRow $r 'h=8:["InvalidOperationException"]:'
    Assert-CsHandlersRow $r 'h=14:["Demo.Fault"]:'
}
