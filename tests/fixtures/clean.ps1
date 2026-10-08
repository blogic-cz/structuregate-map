<#
    FIXTURE: the same shapes, written correctly. This file must produce NO findings.

    It is the half that matters most. A rule that fires on everything is worthless — the false positives
    this file pins down are the ones that were actually measured and removed: a `.GetNewClosure()` handler,
    a handler that only touches `$script:` and its own `param($s, $e)`, one spelling of a hashtable key, a
    tested Try...([ref]) call, `$null` as a discard target, and a waived line.
#>

# `$null =` is THE discard idiom, not an assignment to an automatic variable
$null = [System.IO.Path]::GetTempPath()
$smtpHost = 'smtp.example.com'

# assign first, then wrap - the array survives
$parsed = Get-Content c:\x.json -Raw | ConvertFrom-Json
$rows = @($parsed)

# one spelling
$ctx = [hashtable]::Synchronized(@{ ticked = 0 })
$ctx.ticked = 1

# $null on the left
if ($null -eq $parsed) { 'nope' }

# the boolean is tested, so $msg is only read when it was written
$q = New-Object System.Collections.Concurrent.ConcurrentQueue[object]
$msg = $null
if ($q.TryDequeue([ref]$msg)) { $msg }

function New-Panel {
    $lbl = New-Object System.Windows.Forms.Label
    $btn = New-Object System.Windows.Forms.Button
    $btn.Add_Click({ $lbl.Text = 'clicked' }.GetNewClosure())
    # captures nothing: $script: state and the handler's own parameters
    $btn.Add_MouseUp({ param($s, $e) $script:lastButton = $e.Button })
    return $btn
}

# a modal builder with prefixed names, and a plain handler - correct, because the scope is still on the
# stack while ShowDialog blocks
function Show-Thing {
    param($dlgName)
    $dlgForm = New-Object System.Windows.Forms.Form
    $dlgOk = New-Object System.Windows.Forms.Button
    $dlgOk.Add_Click({ $dlgForm.Close() })
    $dlgForm.ShowDialog()
}

function Get-Names {
    $set = New-Object System.Collections.Generic.HashSet[string]
    [void]$set.Add('one')
    return ,$set
}

# the waiver, with the reason beside it
$one = @(Get-Content c:\y.json -Raw | ConvertFrom-Json)  # psgate-ok: this file holds a single object

# param FIRST, which is the only position where it declares anything
$worker = {
    param($jobName)
    "starting $jobName"
}

# attached to its target, so the member runs
Write-Host $env:PATH.Length

# compiled on FIRST USE, not on the startup path
function Register-NativeType {
    Add-Type -MemberDefinition 'public static int X;' -Name 'Win32' -Namespace 'Native'
}

# a WORKER BODY: assigning a scriptblock does not run it, so this csc call is paid by whatever invokes the
# block inside its runspace - never by the startup path
$script:workerBody = {
    if (-not ('Native.Win32' -as [type])) {
        Add-Type -MemberDefinition 'public static int Y;' -Name 'Win32' -Namespace 'Native'
    }
    'working'
}

# a virtual list read the way a virtual list has to be read: the rows come from the backing store
function New-VirtualList {
    $lvFiles = New-Object System.Windows.Forms.ListView
    $lvFiles.VirtualMode = $true
    $lvFiles.Add_RetrieveVirtualItem({ param($s, $e) $e.Item = $script:rows[$e.ItemIndex] })
    return $lvFiles
}

# check boxes on a virtual list, drawn and tracked by the helper that has to do it
function New-CheckedList {
    $lvChecked = New-Object System.Windows.Forms.ListView
    $lvChecked.VirtualMode = $true
    $lvChecked.CheckBoxes = $true
    Enable-VirtualCheck $lvChecked
    return $lvChecked
}
