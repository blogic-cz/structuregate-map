<#
    PsShape.ps1 - what the two PowerShell map halves share: the file map's (`PsMap.ps1`) and the deep map's
    (`PsRows.ps1`). Dot-sourced by both, staged beside each (`rust/fbtcore/src/embedded/mod.rs`).

    ONE FINGERPRINT FOR BOTH MAPS. `functions.body_shape` and `expressions.shape` in the deep map are the digests
    the file map prints as MAP-BODY and MAP-EXPR, computed here over the same tokens, so a `GROUP BY shape` in SQL
    returns the groups buildmap.json lists as `duplicate_bodies` and `duplicate_expressions`.

    PARSED AND RUN BY 5.1 as well as 7: no `u escapes, no `??`, no ternary.

    NO REGEX, ANYWHERE - the build refuses one over this folder.
#>
using namespace System.Management.Automation.Language

# EVERY NODE, ASKED IN .NET: a delegate bound to `[Ast].IsInstanceOfType` is true for each node and runs no script.
# `FindAll({ $true })` invoked a scriptblock per node - 77 000 of them, 650 ms of this repo's cold map.
$script:EveryNode = [Delegate]::CreateDelegate([Func[Ast, bool]], [Ast], [Type].GetMethod('IsInstanceOfType', [Type[]]@([object])))

# A body this short is not a duplication worth reporting - see the same constant in the other three halves.
$script:MinBodyStatements = 3

# An expression smaller than this is shared vocabulary, not a copy. Counted in TOKENS, with a cheap
# CHARACTER pre-filter in front of it: PowerShell nests expressions deeply, so taking a token slice for
# every one of them is what a walk actually spends its time on, and nothing under 120 characters is a
# copy anyone has to keep in step.
$script:MinExpressionTokens = 20
$script:MinExpressionChars = 120

$script:MaxSummary = 160

# THE RECORDS OF ONE FILE, written out once it is read. A PowerShell function call costs ~60 us - more than the record
# it writes - so the hot records are appended in the walk itself, and every record goes through this one buffer to
# keep their order.
$script:Out = [System.Text.StringBuilder]::new()
# The caller reads this stream as UTF-8. Without this line a summary quoted out of a file with a non-ASCII
# identifier arrives mangled, because powershell.exe writes in the console's OEM code page.
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)

# The token kinds a SHINGLE blanks besides variables: two statements that differ in one literal are one statement shape.
$script:LiteralKinds = @([TokenKind]::StringLiteral, [TokenKind]::StringExpandable, [TokenKind]::HereStringLiteral,
    [TokenKind]::HereStringExpandable, [TokenKind]::Number)

# ONE FILE PARSED AS UTF-8, a byte-order mark still deciding. `[Parser]::ParseFile` decodes a file without one in the
# host's default: UTF-8 under pwsh 7, the ANSI code page under Windows PowerShell 5.1 - which made every non-ASCII name
# mojibake in the rows 5.1 wrote, against the UTF-8 text the store indexes beside them.
function Read-Ast([string]$Path) {
    $tokens = $null
    $errors = $null
    $ast = [Parser]::ParseInput([System.IO.File]::ReadAllText($Path), $Path, [ref]$tokens, [ref]$errors)
    return [pscustomobject]@{ Ast = $ast; Tokens = $tokens; Errors = $errors }
}

function Write-Record {
    # One protocol record. A field carrying `|` or a newline would split into the wrong slot on the other
    # side, so both are folded here rather than parsed around there.
    #
    # A PLAIN FUNCTION OVER `$args`: thousands of records a run, and an advanced one - a `[Parameter]` binding
    # its arguments, an array grown by `+=` - was a third of the half's time.
    $parts = foreach ($field in $args) { ([string]$field).Replace('|', '/').Replace("`r", ' ').Replace("`n", ' ') }
    [void]$script:Out.AppendLine(@($parts) -join '|')
}

function Write-Records {
    [Console]::Out.Write($script:Out.ToString())
    [void]$script:Out.Clear()
}

function Get-Summary([Token[]]$Tokens) {
    # The file's headline: the first line with words in it inside the FIRST comment. Read off the token
    # stream rather than the raw text, so a `#` inside a string is never mistaken for one.
    foreach ($token in $Tokens) {
        if ($token.Kind -ne [TokenKind]::Comment) { continue }
        foreach ($raw in $token.Text.Split("`n")) {
            $line = $raw.Trim()
            foreach ($lead in @('<#', '#>', '#')) {
                if ($line.StartsWith($lead)) { $line = $line.Substring($lead.Length).Trim(); break }
            }
            # A ONE-LINE BLOCK COMMENT closes on the line it opens.
            if ($line.EndsWith('#>')) { $line = $line.Substring(0, $line.Length - 2).Trim() }
            if ($line.Length -eq 0) { continue }
            if ($line.Length -gt $script:MaxSummary) { return $line.Substring(0, $script:MaxSummary) + ' ...' }
            return $line
        }
        return ''
    }
    return ''
}

function Get-TokenKeys([Token[]]$Tokens) {
    # EVERY TOKEN'S PART OF A FINGERPRINT, ONCE PER FILE: its kind and text (a variable blanked to `_`), or ''
    # for a token no fingerprint reads, with where each token starts and ends and how many counted tokens come
    # before it. A fingerprint is then a slice of these, joined in .NET - the per-token PowerShell loop it was,
    # repeated for every nested function and expression, was 68 ms a file. `Blank` is the same with every
    # literal blanked too, for the shingles.
    #
    # AND THE FILE'S SOURCE LINES, in the same pass: BY TOKEN, exactly as the gate counts C# by Roslyn - a source line
    # is a line a token sits on, so a comment and a blank line produce none and a measured-reason header costs nothing.
    $n = $Tokens.Count
    $lines = [System.Collections.Generic.HashSet[int]]::new()
    $prepared = @{
        Parts = [string[]]::new($n)
        Blank = [string[]]::new($n)
        Starts = [int[]]::new($n)
        Ends = [int[]]::new($n)
        Counted = [int[]]::new($n + 1)
    }
    for ($i = 0; $i -lt $n; $i++) {
        $token = $Tokens[$i]
        $prepared.Starts[$i] = $token.Extent.StartOffset
        $prepared.Ends[$i] = $token.Extent.EndOffset
        $skipped = $token.Kind -eq [TokenKind]::Comment -or $token.Kind -eq [TokenKind]::NewLine -or $token.Kind -eq [TokenKind]::EndOfInput
        if ($skipped) {
            $prepared.Parts[$i] = ''
            $prepared.Blank[$i] = ''
            $prepared.Counted[$i + 1] = $prepared.Counted[$i]
            continue
        }
        for ($line = $token.Extent.StartLineNumber; $line -le $token.Extent.EndLineNumber; $line++) { [void]$lines.Add($line) }
        $text = if ($token -is [VariableToken]) { '_' } else { $token.Text }
        # [char]31 and not a `u escape: that syntax is PowerShell 6+, and this half is parsed and RUN by
        # the 5.1 that launches these apps.
        $kind = [string][int]$token.Kind + ':'
        $prepared.Parts[$i] = $kind + $text + [char]31
        $prepared.Blank[$i] = if ($script:LiteralKinds -contains $token.Kind) { $kind + [char]31 } else { $prepared.Parts[$i] }
        $prepared.Counted[$i + 1] = $prepared.Counted[$i] + 1
    }
    $prepared.Lines = $lines.Count
    return $prepared
}

function Get-TokenRange($Prepared, $Extent) {
    # THE TOKENS FROM THE FIRST AT THE EXTENT'S START UP TO THE FIRST THAT ENDS PAST IT, as (first, last): token
    # ends only grow, so both are binary searches.
    $first = [Array]::BinarySearch($Prepared.Starts, $Extent.StartOffset)
    if ($first -lt 0) { $first = -$first - 1 }
    $ends = $Prepared.Ends
    $last = [Array]::BinarySearch($ends, $Extent.EndOffset)
    if ($last -lt 0) { $last = -$last - 1 } else { while ($last -lt $ends.Count -and $ends[$last] -le $Extent.EndOffset) { $last++ } }
    if ($last -lt $first) { $last = $first }
    return $first, $last
}

function Get-Hex([string]$Text, [int]$Length) {
    $sha = [System.Security.Cryptography.SHA256]::Create()
    try {
        $bytes = $sha.ComputeHash([System.Text.Encoding]::UTF8.GetBytes($Text))
    } finally {
        $sha.Dispose()
    }
    return [System.BitConverter]::ToString($bytes).Replace('-', '').ToLowerInvariant().Substring(0, $Length)
}

function Get-Fingerprint($Prepared, $Extent) {
    # The shape of a piece of code, with VARIABLE NAMES BLANKED so two spellings of one idiom fingerprint
    # alike, and command and member names KEPT because those are what make it that code. Comments never
    # enter: they are their own token kind and are skipped, so two functions match when they DO the same
    # thing however differently they are described.
    $first, $last = Get-TokenRange $Prepared $Extent
    $joined = [string]::Join('', $Prepared.Parts, $first, $last - $first)
    # TOKENS DECIDE WHETHER TO REPORT IT, CHARACTERS SAY HOW BIG IT IS. A token count is the right filter -
    # a long variable name cannot inflate it - but it is not comparable with what the other three halves
    # count, and the groups are ranked against them in ONE list. A source span is the same unit everywhere.
    return [pscustomobject]@{
        Digest = Get-Hex $joined 16
        Tokens = $Prepared.Counted[$last] - $Prepared.Counted[$first]
        Span   = $Extent.EndOffset - $Extent.StartOffset
    }
}

function Get-Shingles($Prepared, $Statements) {
    # ONE DIGEST PER TOP-LEVEL STATEMENT of a body, names AND literals blanked, as a sorted set (python's `shingles`):
    # the body digest sees a copy with one line added as a different function; the graph compares these sets and
    # reports the pair that shares most of them (`similar_bodies`).
    $seen = [System.Collections.Generic.SortedSet[string]]::new([StringComparer]::Ordinal)
    foreach ($statement in $Statements) {
        $first, $last = Get-TokenRange $Prepared $statement.Extent
        [void]$seen.Add((Get-Hex ([string]::Join('', $Prepared.Blank, $first, $last - $first)) 8))
    }
    return [string]::Join(' ', [string[]]@($seen))
}

# THE SCRIPTBLOCK LITERAL A NODE SITS IN, or $null. A `{ ... }` assigned and run elsewhere (a runspace,
# `Start-Job`, `Invoke-Command`) is its own scope: a function declared in it is that block's, and a runspace
# body MUST redeclare its helpers because it cannot see the caller's.
function Get-LiteralScope([Ast]$Node) {
    for ($at = $Node.Parent; $at; $at = $at.Parent) {
        if ($at -is [ScriptBlockExpressionAst]) { return $at }
    }
    return $null
}
