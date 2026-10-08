<#
    Counting. One suite per counter, because "a source line" means something different in each language and
    the whole point of the tool is that the difference is EXACT rather than approximate. Every assertion
    pins a NUMBER out of --dump, not a sentence: a message can stay right while a count drifts.
#>

Test-Case 'cs: comments and blanks are trivia, a trailing comment counts once' {
    $tree = Use-Tree @{
        'a.cs' = @"
// header
/* block
   continues */

using System;

namespace N;
class C { }     // trailing comment on a code line
"@
    }
    $dump = Get-Dump --root $tree
    Assert-Equal (Get-Count $dump 'files' 'a.cs') 3 'a.cs source lines'
}

Test-Case 'py: blank and # lines drop, a docstring counts as content' {
    $tree = Use-Tree @{
        'a.py' = @"
# comment
"""doc
spans lines"""
x = 1

def f():
    return x    # trailing
"@
    }
    $dump = Get-Dump --root $tree --ext .py
    Assert-Equal (Get-Count $dump 'files' 'a.py') 5 'a.py source lines'
}

Test-Case 'ts: a block comment is tracked ACROSS lines, not tested per line' {
    $tree = Use-Tree @{
        'a.ts' = @"
/* one
   two
   three */
const x = 1;
"@
    }
    $dump = Get-Dump --root $tree
    Assert-Equal (Get-Count $dump 'files' 'a.ts') 1 'a.ts source lines'
}

# Added after a MUTATION slipped through: deleting the `//` check from the C-style counter kept all 64
# cases green, because no case had a `//` line in it. A test suite that survives the bug it exists to catch
# is worth nothing, so the counter is now pinned on every shape it handles.
Test-Case 'ts: a // line drops, a trailing comment counts once, an inline block is stripped' {
    $tree = Use-Tree @{
        'a.ts' = @"
// header
const x = 1; // trailing
const y = /* inline */ 2;
    // indented
const z = 3;
"@
    }
    $dump = Get-Dump --root $tree
    Assert-Equal (Get-Count $dump 'files' 'a.ts') 3 'a.ts source lines'
}

Test-Case 'the OK line reports every default, so a changed default cannot pass unnoticed' {
    $tree = Use-Tree @{ 'a.cs' = "class A { }`n"; 'CLAUDE.md' = "# ok`n" }
    $result = Invoke-Gate --root $tree
    Assert-Exit $result 0
    Assert-Line $result '1 source files within 450 lines'
    Assert-Line $result '1 folders within 14 files'
    Assert-Line $result '1 docs within 180 lines'
}

Test-Case 'ps1: counted BY TOKEN - a <# #> header drops, a here-string body counts' {
    $tree = Use-Tree @{
        'a.ps1' = @"
<#
    a measured-reason header, the kind that must never be trimmed
#>
`$x = 1   # trailing comment
`$here = @'
body
'@
"@
    }
    $dump = Get-Dump --root $tree --ps-discipline
    # `$x = 1`, then the here-string's three lines: the token spans them, so each is a line a token sits on.
    Assert-Equal (Get-Count $dump 'files' 'a.ps1') 4 'a.ps1 source lines'
}

Test-Case 'ps1: without --ps-discipline the extension is not measured at all' {
    $tree = Use-Tree @{ 'a.ps1' = "`$x = 1`n" }
    $dump = Get-Dump --root $tree
    if ((Get-Measured $dump 'files') -contains 'a.ps1') {
        throw 'a.ps1 was measured without --ps-discipline'
    }
}

Test-Case 'docs are counted as NON-BLANK lines, not tokens' {
    $tree = Use-Tree @{ 'CLAUDE.md' = "# t`n`nline`n`n<!-- comment counts, it is content -->`n" }
    $dump = Get-Dump --root $tree
    Assert-Equal (Get-Count $dump 'docs' 'CLAUDE.md') 3 'CLAUDE.md doc lines'
}

Test-Case 'a folder count is the files a line rule also measured' {
    $tree = Use-Tree @{
        'src/a.cs' = "class A { }`n"
        'src/b.cs' = "class B { }`n"
        'src/notes.txt' = "not source`n"
        'src/README.md' = "# doc`n"
    }
    $dump = Get-Dump --root $tree
    Assert-Equal (Get-Count $dump 'dirs' 'src') 2 'src folder count'
}

Test-Case 'generated and vendored files are not authored code' {
    $tree = Use-Tree @{
        'a.generated.cs' = "class A { }`n"
        'schema_pb2.py'  = "x = 1`n"
        'bin/b.cs'       = "class B { }`n"
        'node_modules/c.js' = "var c = 1;`n"
        'real.cs'        = "class R { }`n"
    }
    $dump = Get-Dump --root $tree --ext '.cs,.py,.js'
    $measured = Get-Measured $dump 'files'
    Assert-Equal $measured.Count 1 'measured file count'
    Assert-Equal $measured[0] 'real.cs' 'the one measured file'
}

# ---------------------------------------------------------------- the STREAMED counters

Test-Case 'lines: a source file over 4 MB is counted by streaming, with the same rule' {
    # Above the streaming threshold the gate never holds the file as one string - a UTF-16 copy of twice
    # its size on the large object heap, which is what exhausted it on a repo carrying a few big files.
    # The COUNT must not change because of that: 60000 code lines, 60000 `#` lines, and blanks.
    $tree = Use-Tree @{ 'a.cs' = "class A { }`n" }
    $builder = New-Object System.Text.StringBuilder
    1..60000 | ForEach-Object {
        [void]$builder.AppendLine("# a rationale comment, line $_, padded so the file passes four megabytes")
        [void]$builder.AppendLine("value_$_ = $_")
        [void]$builder.AppendLine('')
    }
    [System.IO.File]::WriteAllText((Join-Path $tree 'big.py'), $builder.ToString())
    if ((Get-Item (Join-Path $tree 'big.py')).Length -lt 4MB) { throw 'the fixture did not exceed the streaming threshold' }

    $dump = Get-Dump --root $tree --ext '.cs,.py'
    Assert-Equal (Get-Count $dump 'files' 'big.py') 60000 'streamed python source lines'
    Assert-Equal (Get-Count $dump 'files' 'a.cs') 1 'the small file is unaffected'
}

Test-Case 'lines: a DOC over 4 MB is counted by streaming, as non-blank lines' {
    $tree = Use-Tree @{ 'a.cs' = "class A { }`n" }
    $builder = New-Object System.Text.StringBuilder
    1..80000 | ForEach-Object {
        [void]$builder.AppendLine("line $_ of a doc that nobody should have written this way, padded out")
        [void]$builder.AppendLine('')
    }
    [System.IO.File]::WriteAllText((Join-Path $tree 'CLAUDE.md'), $builder.ToString())
    if ((Get-Item (Join-Path $tree 'CLAUDE.md')).Length -lt 4MB) { throw 'the fixture did not exceed the streaming threshold' }

    $dump = Get-Dump --root $tree
    Assert-Equal (Get-Count $dump 'docs' 'CLAUDE.md') 80000 'streamed doc lines'
}
