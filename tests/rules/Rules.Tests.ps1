<#
    The three rules, and the split ADVICE the doc rule gives. The advice is tested as carefully as the
    limit: it is the whole reason the failure is actionable, and it is the part that has to agree with the
    folder rule instead of sending an author in the opposite direction.

    Limits are lowered with flags rather than met with 500-line fixtures - the rule is the same one.
#>

Test-Case 'a file over the line limit fails the build and says what to do' {
    $tree = Use-Tree @{ 'big.cs' = (New-Code 8) }
    $result = Invoke-Gate --root $tree --max-lines 5
    Assert-Exit $result 1
    Assert-Line $result 'over the 500-source-line limit'.Replace('500', '5')
    Assert-Line $result 'big.cs'
    Assert-Line $result 'error:'
}

Test-Case 'a file inside the LAST TENTH of the limit fails, while the split is still small' {
    # This used to pass with a `warning: NEAR LIMIT` line under a headline that said OK, which is how a file
    # arrived at its limit anyway. The last tenth is not spendable.
    $tree = Use-Tree @{ 'edge.cs' = (New-Code 10) }
    $result = Invoke-Gate --root $tree --max-lines 10
    Assert-Exit $result 1
    Assert-Line $result '10/10 is inside the last tenth'
    Assert-Line $result 'ceiling 9'
}

Test-Case 'a file BELOW the ceiling passes and says nothing' {
    $tree = Use-Tree @{ 'edge.cs' = (New-Code 8) }
    $result = Invoke-Gate --root $tree --max-lines 10
    Assert-Exit $result 0
    Assert-NoLine $result 'last tenth'
}

Test-Case 'a crowded folder fails, counting only measured files' {
    $tree = Use-Tree @{
        'src/a.cs' = "class A { }`n"; 'src/b.cs' = "class B { }`n"; 'src/c.cs' = "class C { }`n"
        'src/d.txt' = "not source`n"
    }
    $result = Invoke-Gate --root $tree --max-files 2
    Assert-Exit $result 1
    Assert-Line $result 'holding more than 2 source files'
    Assert-Line $result 'src'
    # 3 measured .cs, and the .txt is not one of them
    Assert-Line $result 'error: 3  src'
}

Test-Case 'a long context doc fails with SPLIT advice, its largest sections, and a sibling target' {
    $tree = Use-Tree @{ 'CLAUDE.md' = "# t`n## First`nline`nline`n## Second`nline`n" }
    $result = Invoke-Gate --root $tree --max-doc-lines 3
    Assert-Exit $result 1
    Assert-Line $result 'SPLIT, do not trim'
    Assert-Line $result 'BESIDE it'
    Assert-Line $result '## First'
}

Test-Case 'when the folder is FULL the advice sends the split into a subfolder instead' {
    $files = @{ 'CLAUDE.md' = "# t`n## S`nline`nline`n" }
    1..3 | ForEach-Object { $files["f$_.cs"] = "class F$_ { }`n" }
    $tree = Use-Tree $files
    $result = Invoke-Gate --root $tree --max-doc-lines 2 --max-files 3
    Assert-Exit $result 1
    Assert-Line $result 'SPLIT, do not trim'
    # The folder holds 3 sources + 1 doc against a limit of 3, so a sibling does not fit any more and the
    # advice sends the section into a subfolder named after the doc instead.
    Assert-Line $result 'CLAUDE/<topic>.md'
    Assert-Line $result 'ONLY because the folder is full'
}

Test-Case 'a dangling local .md link is a violation' {
    $tree = Use-Tree @{ 'CLAUDE.md' = "# t`nsee [topic](topic.md)`n" }
    $result = Invoke-Gate --root $tree
    Assert-Exit $result 1
    Assert-Line $result 'links to topic.md, which does not exist'
}

Test-Case 'a reference-style link is a link, and an example of one in code is not' {
    # PARSED, not scanned: `[x][ref]` with `[ref]: gone.md` was invisible to the old scan, and a link written
    # inside a code block - an example in a doc ABOUT links - failed the build.
    $nl = [string][char]10
    $tree = Use-Tree @{ 'CLAUDE.md' = "# t${nl}see [the guide][g]${nl}${nl}[g]: ref-gone.md${nl}${nl}``````md${nl}[x](block-gone.md)${nl}``````${nl}" }
    $result = Invoke-Gate --root $tree
    Assert-Exit $result 1
    Assert-Line $result 'links to ref-gone.md, which does not exist'
    Assert-NoLine $result 'block-gone.md'
}

Test-Case 'a link that resolves is not reported' {
    $tree = Use-Tree @{ 'CLAUDE.md' = "# t`nsee [topic](topic.md)`n"; 'topic.md' = "# topic`n" }
    $result = Invoke-Gate --root $tree
    Assert-Exit $result 0
}

Test-Case '--doc-scope context measures CLAUDE.md and leaves a README alone' {
    $tree = Use-Tree @{
        'CLAUDE.md' = "# c`nl`nl`nl`n"
        'README.md' = "# r`nl`nl`nl`n"
    }
    $all = Invoke-Gate --root $tree --max-doc-lines 2
    Assert-Exit $all 1
    Assert-Line $all 'README.md'

    $context = Invoke-Gate --root $tree --max-doc-lines 2 --doc-scope context
    Assert-Exit $context 1
    Assert-Line $context 'CLAUDE.md'
    Assert-NoLine $context 'README.md'
}

Test-Case 'an agent definition is a context doc, wherever the repo keeps it' {
    $tree = Use-Tree @{ '.claude/agents/big.md' = "---`nname: big`n---`nl`nl`nl`n" }
    $result = Invoke-Gate --root $tree --max-doc-lines 3 --doc-scope context
    Assert-Exit $result 1
    Assert-Line $result 'agents/big.md'
}

Test-Case 'a clean tree passes and reports what it measured' {
    $tree = Use-Tree @{ 'a.cs' = "class A { }`n"; 'CLAUDE.md' = "# ok`n" }
    $result = Invoke-Gate --root $tree
    Assert-Exit $result 0
    Assert-Line $result 'structuregate OK:'
    Assert-Line $result '1 source files within 450 lines'
}

Test-Case 'several --root trees are measured together, and a name collision stays readable' {
    $one = Use-Tree @{ 'CLAUDE.md' = "# one`nl`nl`n" }
    $two = New-Tree @{ 'CLAUDE.md' = "# two`nl`nl`n" }
    try {
        $result = Invoke-Gate --root $one --root $two --max-doc-lines 1
        Assert-Exit $result 1
        # Two roots, both holding CLAUDE.md: the folder name disambiguates them.
        Assert-Line $result "$(Split-Path $one -Leaf)/CLAUDE.md"
        Assert-Line $result "$(Split-Path $two -Leaf)/CLAUDE.md"
    } finally { Remove-Item $two -Recurse -Force -ErrorAction SilentlyContinue }
}

Test-Case 'a C# file that did not move is not counted again when the pass cache misses, and an edited one is' {
    $tree = Use-Tree @{ 'a.cs' = (New-Code 2); 'b.cs' = (New-Code 2) }
    Assert-Exit (Invoke-Gate --root $tree --max-lines 10) 0
    # FORGED: every kept count says 99 and the passes are forgotten - only a count the cache answers fails now.
    & $script:Python -c "import sqlite3,sys; c = sqlite3.connect(sys.argv[1]); c.execute('UPDATE gate_counts SET lines = 99'); c.execute('DELETE FROM gate_pass'); c.commit()" (Join-Path $tree '.fbt\gate.sqlite')
    $forged = Invoke-Gate --root $tree --max-lines 10
    Assert-Exit $forged 1
    Assert-Line $forged 'a.cs'
    # AN EDITED FILE is counted again; the one beside it is still answered from the cache.
    [System.IO.File]::WriteAllText((Join-Path $tree 'a.cs'), (New-Code 3))
    $edited = Invoke-Gate --root $tree --max-lines 10
    Assert-Exit $edited 1
    Assert-Line $edited 'b.cs'
    if ($edited.Text -match 'a\.cs') { throw "an edited file was answered from the cache:`n$($edited.Text)" }
}

Test-Case 'any other source that did not move is not counted again when the pass cache misses, and an edited one is' {
    # A LINE COUNT IS A FUNCTION OF THE CONTENT TOO: counting the files that had not moved was most of an edit turn.
    $tree = Use-Tree @{ 'a.py' = "x = 1`ny = 2`n"; 'b.py' = "x = 1`ny = 2`n" }
    Assert-Exit (Invoke-Gate --root $tree --ext .py --max-lines 10) 0
    & $script:Python -c "import sqlite3,sys; c = sqlite3.connect(sys.argv[1]); c.execute('UPDATE gate_counts SET lines = 99'); c.execute('DELETE FROM gate_pass'); c.commit()" (Join-Path $tree '.fbt\gate.sqlite')
    $forged = Invoke-Gate --root $tree --ext .py --max-lines 10
    Assert-Exit $forged 1
    Assert-Line $forged 'a.py'
    [System.IO.File]::WriteAllText((Join-Path $tree 'a.py'), "x = 1`ny = 2`nz = 3`n")
    $edited = Invoke-Gate --root $tree --ext .py --max-lines 10
    Assert-Exit $edited 1
    Assert-Line $edited 'b.py'
    if ($edited.Text.Contains('a.py')) { throw "an edited file was answered from the cache:`n$($edited.Text)" }
}

Test-Case 'a script whose answer is kept is not handed to its host again - and another host asks again' {
    $tree = Use-Tree @{ 'a.ps1' = "`$x = 1`n" }
    Assert-Exit (Invoke-Gate --root $tree --ps-discipline --max-lines 10) 0
    & $script:Python -c "import sqlite3,sys; c = sqlite3.connect(sys.argv[1]); c.execute('UPDATE gate_counts SET lines = 99'); c.execute('DELETE FROM gate_pass'); c.commit()" (Join-Path $tree '.fbt\gate.sqlite')
    $forged = Invoke-Gate --root $tree --ps-discipline --max-lines 10
    Assert-Exit $forged 1
    Assert-Line $forged 'a.ps1'
    # THE HOST IS IN THE KEY: one that cannot start is a violation, never an answer kept from another.
    $other = Invoke-Gate --root $tree --ps-discipline --max-lines 10 --ps-host no-such-host.exe
    Assert-Exit $other 1
    Assert-Line $other 'no-such-host.exe'
}
