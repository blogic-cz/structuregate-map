<#
    The modes a per-project gate needed, and the argument parsing in front of them.

    --tracked is the one worth the git setup: "what a COMMIT will contain" is a different file set from
    "what is on disk", and the difference is where build output, editor backups and a deleted-but-unstaged
    file live.
#>

Test-Case '--worst never fails, and ranks by closeness to the limit' {
    $tree = Use-Tree @{ 'big.cs' = (New-Code 20); 'small.cs' = (New-Code 2) }
    $result = Invoke-Gate --root $tree --max-lines 5 --worst
    Assert-Exit $result 0
    Assert-Line $result 'big.cs'
    Assert-NoLine $result 'error:'
}

Test-Case '--dump is parseable JSON with all three sections' {
    $tree = Use-Tree @{ 'a.cs' = "class A { }`n"; 'CLAUDE.md' = "# d`n" }
    $dump = Get-Dump --root $tree
    Assert-Equal (Get-Count $dump 'files' 'a.cs') 1 'files section'
    Assert-Equal (Get-Count $dump 'dirs' '.') 1 'dirs section'
    Assert-Equal (Get-Count $dump 'docs' 'CLAUDE.md') 1 'docs section'
}

Test-Case '--dump exits 0 even over a tree that violates every rule' {
    $tree = Use-Tree @{ 'big.cs' = (New-Code 20) }
    $result = Invoke-Gate --root $tree --max-lines 2 --dump
    Assert-Exit $result 0
}

function Initialize-Repo([string]$Path) {
    # git writes ordinary notices to stderr (the CRLF warning), and under $ErrorActionPreference = 'Stop'
    # `2>&1` makes each one a terminating error. The fixture is not what is being tested.
    $previous = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try {
        & git -C $Path init --quiet 2>&1 | Out-Null
        & git -C $Path config user.email 'test@example.com' 2>&1 | Out-Null
        & git -C $Path config user.name 'test' 2>&1 | Out-Null
        & git -C $Path config core.autocrlf false 2>&1 | Out-Null
        & git -C $Path add -A 2>&1 | Out-Null
        & git -C $Path commit -m 'fixture' --quiet 2>&1 | Out-Null
    } finally { $ErrorActionPreference = $previous }
}

Test-Case '--tracked measures the commit, not the disk' {
    $tree = Use-Tree @{ 'tracked.cs' = (New-Code 8) }
    Initialize-Repo $tree
    [System.IO.File]::WriteAllText((Join-Path $tree 'untracked.cs'), (New-Code 9))

    $tracked = Invoke-Gate --root $tree --max-lines 5 --tracked
    Assert-Exit $tracked 1
    Assert-Line $tracked 'tracked.cs'
    Assert-NoLine $tracked 'untracked.cs'

    $both = Invoke-Gate --root $tree --max-lines 5 --include-untracked
    Assert-Exit $both 1
    Assert-Line $both 'untracked.cs'
}

Test-Case 'a tracked file deleted in the tree is NAMED, never silently dropped' {
    $tree = Use-Tree @{ 'gone.cs' = "class G { }`n"; 'stays.cs' = "class S { }`n" }
    Initialize-Repo $tree
    Remove-Item (Join-Path $tree 'gone.cs')

    $result = Invoke-Gate --root $tree --tracked
    Assert-Exit $result 0
    Assert-Line $result 'deleted in the working tree'
    Assert-Line $result 'gone.cs'
}

Test-Case 'a deleted file the gate never measured is not reported as unchecked' {
    $tree = Use-Tree @{ 'notes.txt' = "text`n"; 'a.cs' = "class A { }`n" }
    Initialize-Repo $tree
    Remove-Item (Join-Path $tree 'notes.txt')

    $result = Invoke-Gate --root $tree --tracked
    Assert-Exit $result 0
    Assert-NoLine $result 'notes.txt'
}

Test-Case 'an unknown flag exits 2 with the usage, not 1' {
    $tree = Use-Tree @{ 'a.cs' = "class A { }`n" }
    $result = Invoke-Gate --root $tree --nope
    Assert-Exit $result 2
    Assert-Line $result 'unknown argument: --nope'
}

Test-Case 'a bad --doc-scope value names the two it takes' {
    $tree = Use-Tree @{ 'a.cs' = "class A { }`n" }
    $result = Invoke-Gate --root $tree --doc-scope everything
    Assert-Exit $result 2
    Assert-Line $result '--doc-scope takes'
}

Test-Case '--update-baseline without --baseline is refused' {
    $tree = Use-Tree @{ 'a.cs' = "class A { }`n" }
    $result = Invoke-Gate --root $tree --update-baseline
    Assert-Exit $result 2
    Assert-Line $result 'needs --baseline'
}

Test-Case 'a --root that does not exist is refused rather than passing by measuring nothing' {
    $result = Invoke-Gate --root 'C:\no\such\tree\anywhere'
    Assert-Exit $result 2
    Assert-Line $result '--root does not exist'
}

Test-Case 'a flag with no value is refused' {
    $result = Invoke-Gate --root
    Assert-Exit $result 2
    Assert-Line $result 'needs a value'
}

Test-Case '-h prints the usage and exits 0' {
    $result = Invoke-Gate -h
    Assert-Exit $result 0
    Assert-Line $result '--ps-discipline'
    Assert-Line $result '--async-discipline'
}

Test-Case 'a numeric flag given a word is refused with one line, not a stack trace' {
    $tree = Use-Tree @{ 'a.cs' = "class A { }`n" }
    foreach ($flag in '--max-lines', '--max-files', '--max-doc-lines') {
        $result = Invoke-Gate --root $tree $flag 'abc'
        Assert-Exit $result 2
        Assert-Line $result "$flag takes a number, not ``abc``"
        Assert-NoLine $result 'Unhandled exception'
    }
}

Test-Case 'a limit of zero is refused - it is not a stricter gate, it fails every file' {
    $tree = Use-Tree @{ 'a.cs' = "class A { }`n" }
    $result = Invoke-Gate --root $tree --max-lines 0
    Assert-Exit $result 2
    Assert-Line $result 'must be at least 1'
}

Test-Case 'a NON-ASCII tracked path is measured, not silently dropped' {
    # git prints such a path as C-style octal escapes inside quotes unless core.quotepath=off, and that
    # string matches nothing on disk - so the file disappears from the gate's input in exactly the
    # repositories that have non-ASCII names in them.
    $tree = Use-Tree @{ 'Café.cs' = "class M { }`n"; 'Über/naïve.cs' = "class S { }`n" }
    Initialize-Repo $tree

    $dump = Get-Dump --root $tree --tracked
    $measured = Get-Measured $dump 'files'
    Assert-Equal $measured.Count 2 'both non-ASCII paths measured'
    if ($measured -notcontains 'Café.cs') { throw "Café.cs missing. Measured: $($measured -join ', ')" }
    if ($measured -notcontains 'Über/naïve.cs') { throw "the nested one is missing. Measured: $($measured -join ', ')" }
}

Test-Case 'the verdict goes to STDERR, so a Claude Code hook shows it' {
    # A Stop hook that fails non-blocking shows the caller stderr and DISCARDS stdout. With the verdict on
    # stdout the reader got "No stderr output" and never saw which rule broke.
    $tree = Use-Tree @{ 'big.cs' = (New-Code 20) }
    $result = Invoke-GateStream --root $tree --max-lines 5
    Assert-Exit $result 1
    if ($result.Err -notmatch 'structure violation\(s\)') { throw "the headline is not on stderr. stderr: $($result.Err)" }
    if ($result.Err -notmatch 'error: .*big\.cs') { throw "the violation is not on stderr. stderr: $($result.Err)" }
    if ($result.Out -match 'error:') { throw "a violation is still on stdout: $($result.Out)" }
}
