<#
    FIXTURE: every PowerShell rule, once, on purpose. Run-PsGateTests.ps1 compares the findings against
    expected.txt, so a rule that stops firing fails the BUILD instead of going quiet.

    This file is deliberately WRONG. It is never dot-sourced and never runs. The repo's own structure check
    must exclude this folder (`--skip tests`), or the gate would report the planted bugs as its own.
#>

# rule 1 - assignment to an automatic variable, and the two declaration forms
$host = 'smtp.example.com'
function Get-BadParam { param($null, $name) }
foreach ($_ in 1..3) { }

# rule 2 - the array wrapper that 5.1 collapses to one element
$cfg = @(Get-Content c:\x.json -Raw | ConvertFrom-Json)

# rule 3 - one key, two spellings: the second write destroys the scriptblock
$ctx = [hashtable]::Synchronized(@{ ticked = 0 })
$ctx.Ticked = { 'scriptblock' }

# rule 4 - $null on the right
if ($cfg -eq $null) { 'nope' }

# rule 5 - the boolean is dropped, so $msg keeps the previous iteration's value
$q = New-Object System.Collections.Concurrent.ConcurrentQueue[object]
$msg = $null
$q.TryDequeue([ref]$msg)

# rule 6 - handler built in a NON-modal builder: $lbl is gone when Click fires
function New-Panel {
    $lbl = New-Object System.Windows.Forms.Label
    $btn = New-Object System.Windows.Forms.Button
    $btn.Add_Click({ $lbl.Text = 'clicked' })
    return $btn
}

# rule 7 - modal scope, generic names. The handler here is CORRECT (rule 6 must stay silent on it).
function Show-Thing {
    param($name)
    $f = New-Object System.Windows.Forms.Form
    $path = 'c:\temp'
    $ok = New-Object System.Windows.Forms.Button
    $ok.Add_Click({ $f.Close() })
    $f.ShowDialog()
}

# rule 8 - the caller receives the members, not the set
function Get-Names {
    $set = New-Object System.Collections.Generic.HashSet[string]
    [void]$set.Add('one')
    return $set
}

# rule 9 - throws ArgumentException on 5.1
function Use-List {
    $items = New-Object System.Collections.Generic.List[object]
    $items.Add(1)
    $copy = @($items)
    return $copy
}

# rule 10 - `param` after a statement is a CALL to a command named param, not a declaration
$worker = {
    'starting'
    param($jobName)
}

# rule 11 - the space makes `.Length` a bareword ARGUMENT, so the member never runs
Write-Host $env:PATH .Length

# rule 12 - csc runs at FILE scope, on the startup path of every launch
Add-Type -MemberDefinition 'public static int X;' -Name 'Win32' -Namespace 'Native'

# rules 13, 14 and 15 - a virtual list owns no rows, so the box, the event and .Items all lie
function New-VirtualList {
    $lvFiles = New-Object System.Windows.Forms.ListView
    $lvFiles.VirtualMode = $true
    $lvFiles.CheckBoxes = $true
    $lvFiles.Add_ItemCheck({ 'checked' })
    return $lvFiles.Items.Count
}
