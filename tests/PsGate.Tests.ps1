<#
    --ps-discipline end to end: the exe, the host launch, the counts and the rules folded into one verdict.
    The rules themselves are pinned line-for-line by fixtures/expected.txt, which Run-PsGateTests.ps1
    compares - that runner is invoked here so one command covers everything.
#>

Test-Case 'ps: every rule still fires exactly as recorded (fixtures vs expected.txt)' {
    $runner = Join-Path $PSScriptRoot 'Run-PsGateTests.ps1'
    $output = & $script:PowerShell -NoProfile -NonInteractive -ExecutionPolicy Bypass -File $runner 2>&1
    if ($LASTEXITCODE -ne 0) { throw ($output -join "`n") }
}

Test-Case 'ps: a rule finding is an error line and fails the build' {
    $tree = Use-Tree @{ 'bad.ps1' = "`$host = 'smtp.example.com'`n" }
    $clean = Invoke-Gate --root $tree
    Assert-Exit $clean 0                       # opt-in: silent until asked for

    $result = Invoke-Gate --root $tree --ps-discipline
    Assert-Exit $result 1
    Assert-Line $result 'bad.ps1:1'
    Assert-Line $result 'automatic variable $host'
}

Test-Case 'ps: there is ONE severity - every rule fails the build, waivers are the only exception' {
    # This shape was a `warn` and passed. A finding that does not fail scrolls past in a log ending in OK,
    # so the escape hatch is `# psgate-ok` next to the code, where the reason lives.
    $tree = Use-Tree @{
        'unrolled.ps1' = @"
function Get-Names {
    `$set = New-Object System.Collections.Generic.HashSet[string]
    [void]`$set.Add('one')
    return `$set
}
"@
    }
    $result = Invoke-Gate --root $tree --ps-discipline
    Assert-Exit $result 1
    Assert-Line $result 'error:'
    Assert-Line $result 'UNROLLED'
    Assert-NoLine $result 'warning:'
}

Test-Case 'ps: `# psgate-ok` waives the line' {
    $tree = Use-Tree @{ 'waived.ps1' = "`$host = 'smtp.example.com'  # psgate-ok: measured, and it is fine`n" }
    Assert-Exit (Invoke-Gate --root $tree --ps-discipline) 0
}

Test-WindowsCase 'ps: a file that does not PARSE is a violation, which is also the 5.1 version floor' {
    # `??` is PS7-only. Under powershell.exe it does not parse, and a partial tree would silently stop
    # covering the rest of the file.
    $tree = Use-Tree @{ 'seven.ps1' = "`$x = `$env:NOPE ?? 'fallback'`n" }
    $result = Invoke-Gate --root $tree --ps-discipline
    Assert-Exit $result 1
    Assert-Line $result 'does not parse under Windows PowerShell 5.1'
}

Test-Case 'ps: the same file parses under --ps-host pwsh, where that syntax is legal' {
    if (-not (Get-Command pwsh -ErrorAction SilentlyContinue)) { return }
    $tree = Use-Tree @{ 'seven.ps1' = "`$x = `$env:NOPE ?? 'fallback'`n" }
    Assert-Exit (Invoke-Gate --root $tree --ps-discipline --ps-host pwsh) 0
}

Test-Case 'ps: a host that cannot be launched is a VIOLATION, not a skip' {
    $tree = Use-Tree @{ 'a.ps1' = "`$x = 1`n" }
    $result = Invoke-Gate --root $tree --ps-discipline --ps-host no-such-powershell.exe
    Assert-Exit $result 1
    Assert-Line $result 'no-such-powershell.exe'
}

Test-Case 'ps: .psd1 is DATA - counted, never rule-checked' {
    $tree = Use-Tree @{ 'module.psd1' = "@{`n    ModuleVersion = '1.0'`n    Author = 'test'`n}`n" }
    $dump = Get-Dump --root $tree --ps-discipline
    Assert-Equal (Get-Count $dump 'files' 'module.psd1') 4 'psd1 source lines'
    Assert-Exit (Invoke-Gate --root $tree --ps-discipline) 0
}

Test-Case 'ps: .psm1 is measured and rule-checked like a script' {
    $tree = Use-Tree @{ 'mod.psm1' = "`$error = 'clobbered'`n" }
    $result = Invoke-Gate --root $tree --ps-discipline
    Assert-Exit $result 1
    Assert-Line $result 'mod.psm1:1'
}

Test-Case 'ps: --ps-discipline adds the extensions itself, so the rule cannot run over nothing' {
    $tree = Use-Tree @{ 'a.ps1' = (New-Code 8 '# header') }
    $result = Invoke-Gate --root $tree --ps-discipline --max-lines 5
    Assert-Exit $result 1
    Assert-Line $result 'a.ps1'
}

Test-Case 'ps: one host launch serves many files' {
    $files = @{}
    1..12 | ForEach-Object { $files["s$_.ps1"] = "`$x$_ = $_`n" }
    $files['bad.ps1'] = "`$matches = 'clobbered'`n"
    $tree = Use-Tree $files
    $result = Invoke-Gate --root $tree --ps-discipline --max-files 20
    Assert-Exit $result 1
    Assert-Line $result 'bad.ps1'
    $dump = Get-Dump --root $tree --ps-discipline
    Assert-Equal (Get-Measured $dump 'files').Count 13 'measured PowerShell files'
}

# ---------------------------------------------------------------- the AST reading behind the rules

Test-Case 'ps: a collection is recognised from BOTH construction syntaxes' {
    $tree = Use-Tree @{
        'newobject.ps1' = @"
function Get-A {
    `$set = New-Object System.Collections.Generic.HashSet[string]
    return `$set
}
"@
        'ctor.ps1' = @"
function Get-B {
    `$set = [System.Collections.Generic.HashSet[string]]::new()
    return `$set
}
"@
    }
    $result = Invoke-Gate --root $tree --ps-discipline
    Assert-Exit $result 1
    Assert-Line $result 'newobject.ps1'
    Assert-Line $result 'ctor.ps1'
}

Test-Case 'ps: a NAME that merely contains a type name is not that type' {
    # `$lstBox` and `ListBox` both contain "List". A pattern over the right-hand side reported several of these
    # as collections; reading the constructed TYPE off the tree does not.
    $tree = Use-Tree @{
        'a.ps1' = @"
function Get-Box {
    `$lstBox = New-Object System.Windows.Forms.ListBox
    return `$lstBox
}
"@
    }
    $result = Invoke-Gate --root $tree --ps-discipline
    Assert-Exit $result 0
    Assert-NoLine $result 'UNROLLED'
}

Test-Case 'ps: hashtable keys are only tracked for a variable this file makes a hashtable' {
    $tree = Use-Tree @{
        # A worker scriptblock full of literals is NOT a hashtable, so its keys belong to the objects built
        # inside it - this reported two invented collisions before the value was read off the tree.
        'body.ps1' = @"
`$body = {
    `$q.Enqueue([pscustomobject]@{ type = 'site'; name = "`$(`$w.name)" })
    `$q.Enqueue([pscustomobject]@{ type = 'bind'; Name = "`$(`$b.Name)" })
}
"@
        # A real hashtable, through the wrapper people actually write.
        'ctx.ps1' = @"
`$ctx = [hashtable]::Synchronized(@{ ticked = 0 })
`$ctx.Ticked = 1
"@
    }
    $result = Invoke-Gate --root $tree --ps-discipline
    Assert-Exit $result 1
    Assert-Line $result 'ctx.ps1'
    Assert-NoLine $result 'body.ps1'
}

Test-Case 'ps: an ordered hashtable compares keys case-insensitively too' {
    $tree = Use-Tree @{ 'a.ps1' = "`$map = [ordered]@{ alpha = 1 }`n`$map.Alpha = 2`n" }
    $result = Invoke-Gate --root $tree --ps-discipline
    Assert-Exit $result 1
    Assert-Line $result 'differ only by case'
}

Test-Case 'ps: .GetNewClosure() is the accepted shape, and a modal scope needs no closure at all' {
    $tree = Use-Tree @{
        'closure.ps1' = @"
function New-Panel {
    `$lbl = New-Object System.Windows.Forms.Label
    `$btn = New-Object System.Windows.Forms.Button
    `$btn.Add_Click({ `$lbl.Text = 'x' }.GetNewClosure())
    return `$btn
}
"@
        'modal.ps1' = @"
function Show-Dlg {
    `$dlgForm = New-Object System.Windows.Forms.Form
    `$dlgOk = New-Object System.Windows.Forms.Button
    `$dlgOk.Add_Click({ `$dlgForm.Close() })
    `$dlgForm.ShowDialog()
}
"@
    }
    Assert-Exit (Invoke-Gate --root $tree --ps-discipline) 0
}

Test-Case 'ps: a handler at SCRIPT scope keeps its variables, so it is not reported' {
    $tree = Use-Tree @{
        'a.ps1' = @"
`$lbl = New-Object System.Windows.Forms.Label
`$btn = New-Object System.Windows.Forms.Button
`$btn.Add_Click({ `$lbl.Text = 'x' })
"@
    }
    Assert-Exit (Invoke-Gate --root $tree --ps-discipline) 0
}

Test-Case 'ps: the waiver is honoured on the line ABOVE as well' {
    $tree = Use-Tree @{ 'a.ps1' = "# psgate-ok: measured, and this one is deliberate`n`$host = 'x'`n" }
    Assert-Exit (Invoke-Gate --root $tree --ps-discipline) 0
}

Test-Case 'ps: a [ref] result that IS tested is correct, and so is a scoped variable' {
    $tree = Use-Tree @{
        'a.ps1' = @"
`$q = New-Object System.Collections.Concurrent.ConcurrentQueue[object]
`$script:msg = `$null
while (`$q.TryDequeue([ref]`$script:msg)) { `$script:msg }
"@
    }
    Assert-Exit (Invoke-Gate --root $tree --ps-discipline) 0
}

Test-Case 'ps: initialise-and-try on ONE line is correct, on two lines it is the stale-value bug' {
    $tree = Use-Tree @{
        # Re-initialised every time the call runs, loop included: it cannot read a previous value.
        'sameline.ps1' = "`$ttl = 0; [void][int]::TryParse('60', [ref]`$ttl)`n"
        # Initialised once, outside the loop: iteration two reads what iteration one left behind.
        'loop.ps1' = @"
`$q = New-Object System.Collections.Concurrent.ConcurrentQueue[object]
`$msg = `$null
while (`$true) {
    `$q.TryDequeue([ref]`$msg)
    if (`$msg) { break }
}
"@
    }
    $result = Invoke-Gate --root $tree --ps-discipline
    Assert-Exit $result 1
    Assert-Line $result 'loop.ps1'
    Assert-NoLine $result 'sameline.ps1'
}

Test-Case 'ps: rule 7 fires in the scope that BLOCKS, not in every scope under a modal function' {
    $tree = Use-Tree @{
        'panel.ps1' = @"
function New-Panel {
    # This scope opens the dialog, so ITS generic locals are the ones a pump can shadow.
    `$name = 'shadowed'
    `$dlgForm = New-Object System.Windows.Forms.Form
    `$btn = New-Object System.Windows.Forms.Button
    `$btn.Add_Click({
        # A handler that pumps nothing. Its `$path is not reachable by anything firing during a pump.
        `$path = 'c:\temp'
        [System.Windows.Forms.MessageBox]::Show(`$path)
    }.GetNewClosure())
    `$dlgForm.ShowDialog()
}
"@
    }
    $result = Invoke-Gate --root $tree --ps-discipline
    Assert-Exit $result 1
    Assert-Line $result 'New-Panel opens a modal dialog and assigns the generic $name'
    Assert-NoLine $result '$path'
}

Test-Case 'ps: a dialog builder held in a SCRIPTBLOCK is named by its line, not left unattributed' {
    $tree = Use-Tree @{
        'ask.ps1' = @"
`$askDetails = {
    param([string]`$dlgName)
    `$name = `$dlgName
    `$d = New-Object System.Windows.Forms.Form
    `$d.ShowDialog()
}
"@
    }
    $result = Invoke-Gate --root $tree --ps-discipline
    Assert-Exit $result 1
    Assert-Line $result 'the scriptblock at line 1 opens a modal dialog'
}

# ---------------------------------------------------------------- rules 10 to 15

Test-Case 'ps: `param` used as a COMMAND is a violation' {
    $tree = Use-Tree @{
        'worker.ps1' = @"
`$worker = {
    'starting'
    param(`$jobName)
}
"@
    }
    $result = Invoke-Gate --root $tree --ps-discipline
    Assert-Exit $result 1
    Assert-Line $result 'invokes `param` as a COMMAND'
}

Test-Case 'ps: `param` as the FIRST statement is a declaration, not a call' {
    $tree = Use-Tree @{
        'worker.ps1' = @"
`$worker = {
    param(`$jobName)
    "starting `$jobName"
}
"@
    }
    Assert-Exit (Invoke-Gate --root $tree --ps-discipline) 0
}

Test-Case 'ps: a member access STRANDED in argument mode is a violation' {
    # The space makes `.Length` an argument: the member never runs and the parse is clean.
    $tree = Use-Tree @{ 'strand.ps1' = "Write-Host `$env:PATH .Length`n" }
    $result = Invoke-Gate --root $tree --ps-discipline
    Assert-Exit $result 1
    Assert-Line $result 'passes the member .Length as an ARGUMENT'
}

Test-Case 'ps: a dotfile and a relative path are NOT stranded members' {
    # `.gitignore` after a variable has the shape the rule looks for and is a PATH. PascalCase is what
    # separates the two, and both of these must stay silent.
    $tree = Use-Tree @{ 'paths.ps1' = "`$dir = 'c:\temp'`nGet-Item `$dir .gitignore`nGet-ChildItem .\src`n" }
    $result = Invoke-Gate --root $tree --ps-discipline
    Assert-Exit $result 0
    Assert-NoLine $result 'ARGUMENT'
}

Test-Case 'ps: -MemberDefinition at FILE scope fails, inside a function it does not' {
    $tree = Use-Tree @{
        'startup.ps1' = "Add-Type -MemberDefinition 'public static int X;' -Name 'W' -Namespace 'N'`n"
        'lazy.ps1' = @"
function Register-NativeType {
    Add-Type -MemberDefinition 'public static int X;' -Name 'W' -Namespace 'N'
}
"@
    }
    $result = Invoke-Gate --root $tree --ps-discipline
    Assert-Exit $result 1
    Assert-Line $result 'startup.ps1'
    Assert-Line $result 'STARTUP path'
    Assert-NoLine $result 'lazy.ps1'
}

Test-Case 'ps: an Add-Type inside an ASSIGNED scriptblock is deferred, not startup cost' {
    # A GUI app's worker body may compile its own P/Invoke inside a runspace. Assigning the
    # block does not run it, so this was a false positive of rule 12 and is now silent.
    $tree = Use-Tree @{
        'worker.ps1' = @"
`$script:workerBody = {
    if (-not ('N.W' -as [type])) {
        Add-Type -MemberDefinition 'public static int Y;' -Name 'W' -Namespace 'N'
    }
    'working'
}
"@
    }
    $result = Invoke-Gate --root $tree --ps-discipline
    Assert-Exit $result 0
    Assert-NoLine $result 'STARTUP'
}

Test-Case 'ps: CheckBoxes on a VirtualMode list fails, unless the file draws the check itself' {
    $tree = Use-Tree @{
        'dead.ps1' = @"
function New-List {
    `$lvFiles = New-Object System.Windows.Forms.ListView
    `$lvFiles.VirtualMode = `$true
    `$lvFiles.CheckBoxes = `$true
    return `$lvFiles
}
"@
        'drawn.ps1' = @"
function New-Checked {
    `$lvChecked = New-Object System.Windows.Forms.ListView
    `$lvChecked.VirtualMode = `$true
    `$lvChecked.CheckBoxes = `$true
    Enable-VirtualCheck `$lvChecked
    return `$lvChecked
}
"@
    }
    $result = Invoke-Gate --root $tree --ps-discipline
    Assert-Exit $result 1
    Assert-Line $result "a click on a virtual list's check box toggles nothing"
    Assert-Line $result 'dead.ps1'
    Assert-NoLine $result 'drawn.ps1'
}

Test-Case 'ps: ItemCheck and .Items on a VirtualMode list fail; an ordinary list is untouched' {
    $tree = Use-Tree @{
        'virtual.ps1' = @"
function Read-Virtual {
    `$lvFiles = New-Object System.Windows.Forms.ListView
    `$lvFiles.VirtualMode = `$true
    `$lvFiles.Add_ItemCheck({ 'checked' })
    return `$lvFiles.Items.Count
}
"@
        'plain.ps1' = @"
function Read-Plain {
    `$lvPlain = New-Object System.Windows.Forms.ListView
    `$lvPlain.CheckBoxes = `$true
    `$lvPlain.Add_ItemCheck({ 'checked' })
    return `$lvPlain.Items.Count
}
"@
    }
    $result = Invoke-Gate --root $tree --ps-discipline
    Assert-Exit $result 1
    Assert-Line $result 'the event is never raised'
    Assert-Line $result 'the list keeps no items'
    Assert-NoLine $result 'plain.ps1'
}
