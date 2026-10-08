<#
    What the disk walk refuses to die on. Every case here is a real tree that once took the gate down or
    made it count wrong, and the property under test is the same one: the gate SURVIVES the bad path and
    NAMES it, rather than throwing or passing quietly.

    A junction needs no privileges; a symbolic link does, so these use junctions and skip what the machine
    will not let them build. Off Windows there are no junctions and a symbolic link needs no privilege, so a
    linked folder is the same case there.
#>

function New-Junction([string]$Link, [string]$Target) {
    if ($script:OnWindows) { & cmd /c mklink /J "$Link" "$Target" 2>&1 | Out-Null }
    else { [void](New-Item -ItemType SymbolicLink -Path $Link -Target $Target -ErrorAction SilentlyContinue) }
    return (Test-Path $Link)
}

# A file this account cannot READ: a deny ACE on Windows, no permission bits elsewhere.
function Set-WalkUnreadable([string]$Path) {
    if ($script:OnWindows) { & icacls $Path /deny "$env:USERNAME`:(R)" 2>&1 | Out-Null }
    else { & chmod 000 $Path 2>&1 | Out-Null }
}

function Clear-WalkUnreadable([string]$Path) {
    if ($script:OnWindows) { & icacls $Path /remove:d "$env:USERNAME" 2>&1 | Out-Null }
    else { & chmod 644 $Path 2>&1 | Out-Null }
}

Test-Case 'walk: a directory junction is NOT followed, so its target is counted once' {
    $tree = Use-Tree @{ 'real/a.cs' = "class A { }`n"; 'real/b.cs' = "class B { }`n" }
    if (-not (New-Junction (Join-Path $tree 'linked') (Join-Path $tree 'real'))) { return }

    $dump = Get-Dump --root $tree
    $measured = Get-Measured $dump 'files'
    Assert-Equal $measured.Count 2 'files behind a junction are counted once'
    if ($measured -contains 'linked/a.cs') { throw 'the junction was followed' }
}

Test-Case 'walk: a junction pointing at its own ancestor does not recurse to death' {
    $tree = Use-Tree @{ 'a.cs' = "class A { }`n" }
    if (-not (New-Junction (Join-Path $tree 'loop') $tree)) { return }

    $result = Invoke-Gate --root $tree
    Assert-Exit $result 0
    Assert-Line $result 'structuregate OK:'
}

Test-Case 'walk: a file that cannot be READ is named and the rest is still measured' {
    $tree = Use-Tree @{ 'locked.cs' = "class L { }`n"; 'fine.cs' = "class F { }`n" }
    $locked = Join-Path $tree 'locked.cs'
    Set-WalkUnreadable $locked
    try {
        # If the lock did not take (elevated shell, root, odd account), there is nothing to assert.
        try { [void][System.IO.File]::ReadAllText($locked); return } catch { }

        $result = Invoke-Gate --root $tree
        Assert-Line $result 'could not be read and were NOT measured'
        Assert-Line $result 'locked.cs'
        Assert-Line $result 'structuregate OK:'
        Assert-Exit $result 0
    } finally { Clear-WalkUnreadable $locked }
}

Test-Case 'walk: the unreadable NOTE goes to stderr under --dump, so the JSON stays parseable' {
    $tree = Use-Tree @{ 'locked.cs' = "class L { }`n"; 'fine.cs' = "class F { }`n" }
    $locked = Join-Path $tree 'locked.cs'
    Set-WalkUnreadable $locked
    try {
        try { [void][System.IO.File]::ReadAllText($locked); return } catch { }
        # Get-Dump strips the NOTE line and parses what is left; a NOTE on stdout would break it.
        $dump = Get-Dump --root $tree
        Assert-Equal (Get-Count $dump 'files' 'fine.cs') 1 'the readable file is still measured'
    } finally { Clear-WalkUnreadable $locked }
}

Test-Case 'walk: a build tree is not authored code' {
    $tree = Use-Tree @{
        'src/a.cs' = "class A { }`n"
        'bin/Debug/gen.cs' = (New-Code 40)
        'obj/tmp.cs' = (New-Code 40)
        '.venv/lib/site.py' = (New-Code 40)
        'node_modules/pkg/index.js' = (New-Code 40)
    }
    $result = Invoke-Gate --root $tree --max-lines 5 --ext '.cs,.py,.js'
    Assert-Exit $result 0
}

Test-Case 'walk: --skip replaces the list, so a custom tree name can be excluded' {
    $tree = Use-Tree @{ 'vendor/big.cs' = (New-Code 20); 'a.cs' = "class A { }`n" }
    Assert-Exit (Invoke-Gate --root $tree --max-lines 5) 1
    Assert-Exit (Invoke-Gate --root $tree --max-lines 5 --skip vendor) 0
}

# ONE FILE, BY ITS PATH: `--skip svc` would drop the C# project `svc/Core` too, and re-admit `bin/`
# by replacing the default list. `--skip-file` drops the generated client alone, keeps the defaults, and says so.
function New-SkipFileTree {
    return Use-Tree @{
        'web/src/svc/schema.d.ts' = (New-Code 20)
        'svc/Core/big.cs'         = (New-Code 20)
        'svc/Core/bin/huge.cs'    = (New-Code 20)
        'svc/Core/A.cs'           = "class A { }`n"
    }
}

Test-Case 'walk: --skip-file leaves one file out by its path, keeps the default skips, and names what it left' {
    $tree = New-SkipFileTree
    foreach ($pattern in '**/schema.d.ts', 'web/src/svc/schema.d.ts') {
        $result = Invoke-Gate --root $tree --max-lines 5 --ext '.cs,.ts' --skip-file $pattern --no-gate-cache
        Assert-Exit $result 1
        Assert-Line $result 'svc/Core/big.cs'
        Assert-NoLine $result 'bin/huge.cs'
        Assert-NoLine $result 'schema.d.ts —'
        Assert-NoLine $result 'schema.d.ts:'
        Assert-Line $result "--skip-file ``$pattern`` left 1 file(s) unmeasured: web/src/svc/schema.d.ts"
    }
    [System.IO.File]::WriteAllText((Join-Path $tree 'svc/Core/big.cs'), "class B { }`n")
    Assert-Exit (Invoke-Gate --root $tree --max-lines 5 --ext '.cs,.ts' --skip-file '**/schema.d.ts' --no-gate-cache) 0
}

Test-Case 'walk: --skip-file works over what git tracks too, and a pattern that matches nothing is named' {
    $tree = New-SkipFileTree
    [System.IO.File]::WriteAllText((Join-Path $tree 'svc/Core/big.cs'), "class B { }`n")
    # git writes notices to stderr, which `Stop` would turn into a terminating error - the fixture is not under test.
    $previous = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try {
        foreach ($git in @('init', '--quiet'), @('config', 'user.email', 't@example.com'), @('config', 'user.name', 't'),
            @('add', '-A'), @('commit', '-m', 'fixture', '--quiet')) { & git -C $tree @git 2>&1 | Out-Null }
    } finally { $ErrorActionPreference = $previous }
    $tracked = Invoke-Gate --root $tree --max-lines 5 --ext '.cs,.ts' --tracked --skip-file '**/schema.d.ts' --no-gate-cache
    Assert-Exit $tracked 0
    $stale = Invoke-Gate --root $tree --max-lines 5 --ext '.cs,.ts' --skip-file '**/schema.d.ts' --skip-file '**/gone.ts' --no-gate-cache
    Assert-Exit $stale 0
    Assert-Line $stale '--skip-file `**/gone.ts` matched no file'
}

Test-Case 'walk: --skip-file on a map run is refused - the map keeps the file' {
    $tree = New-SkipFileTree
    $result = Invoke-Gate --root $tree --map --map-out (Join-Path $tree 'm.json') --skip-file '**/schema.d.ts'
    Assert-Exit $result 2
    Assert-Line $result '--skip-file leaves a file out of the GATE only'
}

Test-Case 'walk: a big NON-source file is never read into memory, and never measured' {
    $tree = Use-Tree @{ 'a.cs' = "class A { }`n" }
    # 5 MB of data with a non-source extension: the walk lists it, and deciding what is measured FIRST is
    # what keeps the read proportional to the code.
    $filler = New-Object System.Text.StringBuilder
    1..80000 | ForEach-Object { [void]$filler.AppendLine('{"row": 1234567890123456789012345678901234567890}') }
    [System.IO.File]::WriteAllText((Join-Path $tree 'data.json'), $filler.ToString())

    $dump = Get-Dump --root $tree
    Assert-Equal (Get-Measured $dump 'files').Count 1 'only the source file is measured'
}

Test-Case 'walk: a run leaves nothing of its own in the temp folder - no file list, no work folder' {
    # Every host launch wrote a file list and none was ever removed: tens of thousands of them sat in one machine's temp.
    $tree = Use-Tree @{ 'a.py' = "import b`n"; 'b.py' = "X = 1`n"; 'c.ps1' = "Write-Output 1`n" }
    $temp = Join-Path $tree '.temp'
    New-Item -ItemType Directory $temp | Out-Null
    $was = @($env:TEMP, $env:TMP)
    try {
        $env:TEMP = $temp; $env:TMP = $temp
        Assert-Exit (Invoke-Gate --root $tree --ext '.py,.ps1' --skip '.temp' --map --map-out (Join-Path $tree 'm.json') --map-sqlite (Join-Path $tree 'm.sqlite')) 0
    }
    finally { $env:TEMP = $was[0]; $env:TMP = $was[1] }
    $left = @(Get-ChildItem $temp | Where-Object { $_.Name -like 'structuregate-*list-*.txt' -or $_.Name -match '^structuregate-[a-z]+-\d+$' })
    if ($left.Count -gt 0) { throw "left behind: $(($left | ForEach-Object Name) -join ', ')" }
}
