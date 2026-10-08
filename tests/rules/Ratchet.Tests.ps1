<#
    The baseline RATCHET. Three behaviours, and the third is the one that makes it a ratchet rather than an
    exemption list with extra steps: a baseline file that is now under the limit must be REMOVED from the
    list, and the gate says so.
#>

function New-BaselineTree {
    return Use-Tree @{
        'debt.cs' = (New-Code 8)
        'ok.cs'   = (New-Code 2)
    }
}

Test-Case 'a baseline holds existing debt, and the gate says how much it is holding' {
    $tree = New-BaselineTree
    $baseline = Join-Path $tree 'baseline.json'

    $written = Invoke-Gate --root $tree --max-lines 5 --baseline $baseline --update-baseline
    Assert-Exit $written 0
    Assert-Line $written 'baseline written: 1 file(s)'

    $result = Invoke-Gate --root $tree --max-lines 5 --baseline $baseline
    Assert-Exit $result 0
    Assert-Line $result 'held in the baseline'
}

Test-Case 'a NEW file over the limit fails even with a baseline' {
    $tree = New-BaselineTree
    $baseline = Join-Path $tree 'baseline.json'
    Assert-Exit (Invoke-Gate --root $tree --max-lines 5 --baseline $baseline --update-baseline) 0

    [System.IO.File]::WriteAllText((Join-Path $tree 'fresh.cs'), (New-Code 9))
    $result = Invoke-Gate --root $tree --max-lines 5 --baseline $baseline
    Assert-Exit $result 1
    Assert-Line $result 'fresh.cs'
    Assert-NoLine $result 'debt.cs'
}

Test-Case 'baseline debt may only SHRINK' {
    $tree = New-BaselineTree
    $baseline = Join-Path $tree 'baseline.json'
    Assert-Exit (Invoke-Gate --root $tree --max-lines 5 --baseline $baseline --update-baseline) 0

    [System.IO.File]::WriteAllText((Join-Path $tree 'debt.cs'), (New-Code 12))
    $result = Invoke-Gate --root $tree --max-lines 5 --baseline $baseline
    Assert-Exit $result 1
    Assert-Line $result 'GREW'
}

Test-Case 'a baseline file back under the limit must be REMOVED from the list' {
    $tree = New-BaselineTree
    $baseline = Join-Path $tree 'baseline.json'
    Assert-Exit (Invoke-Gate --root $tree --max-lines 5 --baseline $baseline --update-baseline) 0

    [System.IO.File]::WriteAllText((Join-Path $tree 'debt.cs'), (New-Code 3))
    $result = Invoke-Gate --root $tree --max-lines 5 --baseline $baseline
    Assert-Exit $result 1
    Assert-Line $result 'remove it from the baseline'
}

Test-Case '--strict ignores the baseline entirely' {
    $tree = New-BaselineTree
    $baseline = Join-Path $tree 'baseline.json'
    Assert-Exit (Invoke-Gate --root $tree --max-lines 5 --baseline $baseline --update-baseline) 0

    $result = Invoke-Gate --root $tree --max-lines 5 --baseline $baseline --strict
    Assert-Exit $result 1
    Assert-Line $result 'debt.cs'
}

Test-Case 'a missing baseline file is not an error - it is an empty ratchet' {
    $tree = Use-Tree @{ 'ok.cs' = (New-Code 2) }
    $result = Invoke-Gate --root $tree --max-lines 5 --baseline (Join-Path $tree 'absent.json')
    Assert-Exit $result 0
}

Test-Case 'the docs have NO ratchet: a doc is always splittable' {
    $tree = Use-Tree @{ 'CLAUDE.md' = "# t`nl`nl`nl`n" }
    $baseline = Join-Path $tree 'baseline.json'
    Assert-Exit (Invoke-Gate --root $tree --max-doc-lines 2 --baseline $baseline --update-baseline) 0
    $result = Invoke-Gate --root $tree --max-doc-lines 2 --baseline $baseline
    Assert-Exit $result 1
    Assert-Line $result 'CLAUDE.md'
}

# ---------------------------------------------------------------- the baseline FILE, as a format

Test-Case 'baseline: the file records the limits it was written against, and says not to add to it' {
    $tree = New-BaselineTree
    $baseline = Join-Path $tree 'baseline.json'
    Assert-Exit (Invoke-Gate --root $tree --max-lines 5 --max-files 3 --baseline $baseline --update-baseline) 0

    $json = [System.IO.File]::ReadAllText($baseline) | ConvertFrom-Json
    Assert-Equal $json.max_source_lines 5 'recorded line limit'
    Assert-Equal $json.max_files_per_dir 3 'recorded folder limit'
    Assert-Equal ($json._comment.Contains('may only SHRINK')) 'True' 'the comment warns against adding'
    Assert-Equal (Get-Count $json 'files' 'debt.cs') 8 'the recorded count'
}

Test-Case 'baseline: a FOLDER over the limit is ratcheted the same way a file is' {
    $files = @{}
    1..4 | ForEach-Object { $files["src/f$_.cs"] = "class F$_ { }`n" }
    $tree = Use-Tree $files
    $baseline = Join-Path $tree 'baseline.json'

    Assert-Exit (Invoke-Gate --root $tree --max-files 2 --baseline $baseline --update-baseline) 0
    Assert-Exit (Invoke-Gate --root $tree --max-files 2 --baseline $baseline) 0

    [System.IO.File]::WriteAllText((Join-Path $tree 'src\f5.cs'), "class F5 { }`n")
    $grown = Invoke-Gate --root $tree --max-files 2 --baseline $baseline
    Assert-Exit $grown 1
    Assert-Line $grown 'GREW'
}

Test-Case 'baseline: an empty baseline is written when there is no debt, and holds nothing' {
    $tree = Use-Tree @{ 'ok.cs' = (New-Code 2) }
    $baseline = Join-Path $tree 'baseline.json'
    $written = Invoke-Gate --root $tree --max-lines 5 --baseline $baseline --update-baseline
    Assert-Exit $written 0
    Assert-Line $written 'baseline written: 0 file(s), 0 folder(s)'
    Assert-Exit (Invoke-Gate --root $tree --max-lines 5 --baseline $baseline) 0
}

Test-Case 'baseline: a MALFORMED baseline is reported, not a crash and not a silent empty ratchet' {
    $tree = Use-Tree @{ 'debt.cs' = (New-Code 8) }
    $baseline = Join-Path $tree 'baseline.json'
    [System.IO.File]::WriteAllText($baseline, '{ this is not json')

    $result = Invoke-Gate --root $tree --max-lines 5 --baseline $baseline
    # Either verdict is defensible, but it must not be exit 0: an unreadable baseline means the ratchet is
    # not being applied, and passing in that state reports on a check that never ran.
    if ($result.Exit -eq 0) { throw "a malformed baseline passed silently. Output:`n$($result.Text)" }
    Assert-Line $result 'baseline'
}

# ---------------------------------------------------------------- the last tenth
# THE GATE STOPS AT THE CEILING (90 % - 9 of a 10-line limit), so the baseline freezes from the ceiling too:
# frozen from the LIMIT, a file at 9 that predates the gate left a tree red on the day it was wired in.

Test-Case 'baseline: a file in the last tenth is frozen, and held' {
    $tree = Use-Tree @{ 'near.cs' = (New-Code 9); 'ok.cs' = (New-Code 2) }
    $baseline = Join-Path $tree 'baseline.json'
    $written = Invoke-Gate --root $tree --max-lines 10 --baseline $baseline --update-baseline
    Assert-Exit $written 0
    Assert-Line $written 'baseline written: 1 file(s)'
    $held = Invoke-Gate --root $tree --max-lines 10 --baseline $baseline --no-gate-cache
    Assert-Exit $held 0
    Assert-Line $held 'held in the baseline'

    # STILL A RATCHET: grown inside the last tenth it fails as debt that grew, and a NEW file there still fails.
    [System.IO.File]::WriteAllText((Join-Path $tree 'near.cs'), (New-Code 10))
    $grown = Invoke-Gate --root $tree --max-lines 10 --baseline $baseline --no-gate-cache
    Assert-Exit $grown 1
    Assert-Line $grown 'GREW'
    [System.IO.File]::WriteAllText((Join-Path $tree 'near.cs'), (New-Code 9))
    [System.IO.File]::WriteAllText((Join-Path $tree 'fresh.cs'), (New-Code 9))
    $fresh = Invoke-Gate --root $tree --max-lines 10 --baseline $baseline --no-gate-cache
    Assert-Exit $fresh 1
    Assert-Line $fresh 'fresh.cs: 9/10 is inside the last tenth'
    Assert-NoLine $fresh 'near.cs'
}

Test-Case 'baseline: a folder in the last tenth is frozen, and held' {
    $files = @{}
    1..9 | ForEach-Object { $files["src/f$_.cs"] = "class F$_ { }`n" }
    $tree = Use-Tree $files
    $baseline = Join-Path $tree 'baseline.json'
    $written = Invoke-Gate --root $tree --max-files 10 --baseline $baseline --update-baseline
    Assert-Exit $written 0
    Assert-Line $written '1 folder(s)'
    Assert-Exit (Invoke-Gate --root $tree --max-files 10 --baseline $baseline --no-gate-cache) 0
}

Test-Case 'baseline: debt paid down into the last tenth is held - only under the ceiling must it leave' {
    $tree = Use-Tree @{ 'debt.cs' = (New-Code 12) }
    $baseline = Join-Path $tree 'baseline.json'
    Assert-Exit (Invoke-Gate --root $tree --max-lines 10 --baseline $baseline --update-baseline) 0
    # 12 -> 10: smaller, still at or over the ceiling of 9. Shrinking debt must never turn the build red.
    [System.IO.File]::WriteAllText((Join-Path $tree 'debt.cs'), (New-Code 10))
    $shrunk = Invoke-Gate --root $tree --max-lines 10 --baseline $baseline --no-gate-cache
    Assert-Exit $shrunk 0
    Assert-NoLine $shrunk 'remove it from the baseline'
    # 10 -> 8: under the ceiling, so it is no longer debt and the list must shrink.
    [System.IO.File]::WriteAllText((Join-Path $tree 'debt.cs'), (New-Code 8))
    $paid = Invoke-Gate --root $tree --max-lines 10 --baseline $baseline --no-gate-cache
    Assert-Exit $paid 1
    Assert-Line $paid 'remove it from the baseline'
}
