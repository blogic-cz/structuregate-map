<#
    PsMap.ps1 - the PowerShell half of `structuregate --map` (see rust/fbtcore/src/mapper/protocol.rs).

    WHY A SCRIPT AND NOT C#: the same reason the rules half gives. The only authoritative parser for this
    grammar is the one inside the host that will RUN the code, and `--ps-host` defaults to the 5.1 that
    launches these apps. A grammar approximated in C# would error-recover into a partial tree and the map
    would quietly stop covering the newest files.

    WHAT AN EDGE IS HERE, and why it is two things. PowerShell has no module graph to read: a script tree is
    wired by DOT-SOURCING, and once a file is dot-sourced its functions are global - the caller names a
    FUNCTION, not a file. So a file declares both:
      * its own relative PATH, which is what a dot-source or an Import-Module names, and
      * every FUNCTION it defines, which is what every other file names.
    Reading only the dot-sources would draw a star out of the one entry script and call every library dead;
    reading only the function names would miss the wiring that makes them reachable at all.

    A COMMAND NAME THAT MATCHES NOTHING IS A CMDLET, and is not reported. Every file calls `Join-Path` and
    `Write-Host`; listing those as external dependencies would be hundreds of rows per file that say nothing
    about the codebase - the same reason the C# half does not report its unresolved identifiers.

    NO REGEX, ANYWHERE - the build refuses one over this folder. Every decision is read off the AST, off a
    token KIND, or off a path question put to the filesystem.

    Contract with the caller (structuregate.exe):
      in   -ListFile <path>   UTF-8, one file per line: <relative-path><TAB><absolute-path>
           -Root <dir>        the tree being mapped; a dot-sourced path is resolved against it
           -KnownFile <path>  every file of the map, in -ListFile's format, when -ListFile is only the files to
                              parse: the rest are answered from the caller's cache, and still resolve a dot-source
      out  the MAP-* protocol on stdout (see rust/fbtcore/src/mapper/protocol.rs), MAP-DONE last
      exit 0 even when findings exist - the caller owns the verdict.
#>
using namespace System.Management.Automation.Language

[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$ListFile,
    [string]$Root = '.',
    [string]$KnownFile = ''
)

$ErrorActionPreference = 'Stop'
# EVERY NODE, ASKED IN .NET: a delegate bound to `[Ast].IsInstanceOfType` is true for each node and runs no script.
# `FindAll({ $true })` invoked a scriptblock per node - 77 000 of them, 650 ms of this repo's cold map.
# NO CMDLET ON THE WAY IN: the first `New-Object` or `Test-Path` autoloads its module, ~70 ms of a one-file run each,
# so this script builds its .NET objects with `::new()` and asks the file system directly.
$script:EveryNode = [Delegate]::CreateDelegate([Func[Ast, bool]], [Ast], [Type].GetMethod('IsInstanceOfType', [Type[]]@([object])))
# THE RECORDS OF ONE FILE, written out once it is read. A PowerShell function call costs ~60 us - more than the record
# it writes - so the hot records are appended in the walk itself, and every record goes through this one buffer to
# keep their order.
$script:Out = [System.Text.StringBuilder]::new()
# The caller reads this stream as UTF-8. Without this line a summary quoted out of a file with a non-ASCII
# identifier arrives mangled, because powershell.exe writes in the console's OEM code page.
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)

# A body this short is not a duplication worth reporting - see the same constant in the other three halves.
$script:MinBodyStatements = 3

# An expression smaller than this is shared vocabulary, not a copy. Counted in TOKENS, with a cheap
# CHARACTER pre-filter in front of it: PowerShell nests expressions deeply, so taking a token slice for
# every one of them is what this walk actually spends its time on, and nothing under 120 characters is a
# copy anyone has to keep in step.
$script:MinExpressionTokens = 20
$script:MinExpressionChars = 120

$script:MaxSummary = 160

# The variables a dot-sourced path is written with, and what they mean for the file being read. Anything
# else in a path expression is computed, and a computed path is NOT guessed at - a wrong edge is worse than
# a missing one, because the reader cannot tell it from a proven one.
$script:PathVariables = @('PSScriptRoot', 'PSCommandPath')

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
            if ($line.Length -eq 0) { continue }
            if ($line.Length -gt $script:MaxSummary) { return $line.Substring(0, $script:MaxSummary) + ' ...' }
            return $line
        }
        return ''
    }
    return ''
}

function Get-TokenIndex([int[]]$Starts, [int]$Offset) {
    # The first token at or after an offset, by binary search. A linear scan per expression is O(tokens x
    # expressions), which on a 500-line file is over a million PowerShell loop iterations - slow enough that
    # the map would not be worth running.
    $found = [Array]::BinarySearch($Starts, $Offset)
    if ($found -ge 0) { return $found }
    return -$found - 1
}

function Get-TokenKeys([Token[]]$Tokens) {
    # EVERY TOKEN'S PART OF A FINGERPRINT, ONCE PER FILE: its kind and text (a variable blanked to `_`), or ''
    # for a token no fingerprint reads, with where each token starts and ends and how many counted tokens come
    # before it. A fingerprint is then a slice of these, joined in .NET - the per-token PowerShell loop it was,
    # repeated for every nested function and expression, was 68 ms a file.
    #
    # AND THE FILE'S SOURCE LINES, in the same pass: BY TOKEN, exactly as the gate counts C# by Roslyn - a source line
    # is a line a token sits on, so a comment and a blank line produce none and a measured-reason header costs nothing.
    $n = $Tokens.Count
    $lines = [System.Collections.Generic.HashSet[int]]::new()
    $prepared = @{
        Parts = [string[]]::new($n)
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
            $prepared.Counted[$i + 1] = $prepared.Counted[$i]
            continue
        }
        for ($line = $token.Extent.StartLineNumber; $line -le $token.Extent.EndLineNumber; $line++) { [void]$lines.Add($line) }
        $text = if ($token -is [VariableToken]) { '_' } else { $token.Text }
        # [char]31 and not a `u escape: that syntax is PowerShell 6+, and this half is parsed and RUN by
        # the 5.1 that launches these apps.
        $prepared.Parts[$i] = [string][int]$token.Kind + ':' + $text + [char]31
        $prepared.Counted[$i + 1] = $prepared.Counted[$i] + 1
    }
    $prepared.Lines = $lines.Count
    return $prepared
}

function Get-Fingerprint($Prepared, $Extent) {
    # The shape of a piece of code, with VARIABLE NAMES BLANKED so two spellings of one idiom fingerprint
    # alike, and command and member names KEPT because those are what make it that code. Comments never
    # enter: they are their own token kind and are skipped, so two functions match when they DO the same
    # thing however differently they are described.
    #
    # THE TOKENS FROM THE FIRST AT THE EXTENT'S START UP TO THE FIRST THAT ENDS PAST IT: token ends only grow,
    # so both are binary searches.
    $first = Get-TokenIndex $Prepared.Starts $Extent.StartOffset
    $ends = $Prepared.Ends
    $last = [Array]::BinarySearch($ends, $Extent.EndOffset)
    if ($last -lt 0) { $last = -$last - 1 } else { while ($last -lt $ends.Count -and $ends[$last] -le $Extent.EndOffset) { $last++ } }
    if ($last -lt $first) { $last = $first }
    $joined = [string]::Join('', $Prepared.Parts, $first, $last - $first)
    $count = $Prepared.Counted[$last] - $Prepared.Counted[$first]
    $sha = [System.Security.Cryptography.SHA256]::Create()
    try {
        $bytes = $sha.ComputeHash([System.Text.Encoding]::UTF8.GetBytes($joined))
    } finally {
        $sha.Dispose()
    }
    $hex = [System.BitConverter]::ToString($bytes).Replace('-', '').ToLowerInvariant()
    # TOKENS DECIDE WHETHER TO REPORT IT, CHARACTERS SAY HOW BIG IT IS. A token count is the right filter -
    # a long variable name cannot inflate it - but it is not comparable with what the other three halves
    # count, and the groups are ranked against them in ONE list. A source span is the same unit everywhere.
    return [pscustomobject]@{
        Digest = $hex.Substring(0, 16)
        Tokens = $count
        Span   = $Extent.EndOffset - $Extent.StartOffset
    }
}

function Get-LiteralPath($Expression, [string]$FileDirectory, [string]$FilePath) {
    # A dot-sourced path, as the text it will be. Two shapes resolve: a bare or quoted constant, and an
    # expandable string whose only variables are the ones that name THIS file's own location. Anything else
    # is computed at run time and is skipped rather than guessed at.
    if ($null -eq $Expression) { return '' }
    $text = ''
    if ($Expression -is [StringConstantExpressionAst]) { $text = $Expression.Value }
    elseif ($Expression -is [ExpandableStringExpressionAst]) { $text = $Expression.Value }
    else { return '' }
    if ($text.Length -eq 0) { return '' }
    foreach ($name in $script:PathVariables) {
        $value = if ($name -eq 'PSCommandPath') { $FilePath } else { $FileDirectory }
        $text = $text.Replace('$' + $name, $value).Replace('${' + $name + '}', $value)
    }
    # A `$` left in the text is a variable this pass cannot evaluate.
    if ($text.Contains('$')) { return '' }
    return $text
}

function Resolve-Import([string]$Rel, $Expression, [string]$Root, [hashtable]$Known,
                        [string]$FileDirectory, [string]$FilePath, [bool]$Certain = $false) {
    $literal = Get-LiteralPath $Expression $FileDirectory $FilePath
    if ($literal.Length -eq 0) {
        if (-not $Certain -or $null -eq $Expression) { return }
        # A COMPUTED PATH USUALLY STILL SPELLS THE FILE OUT. `. (Join-Path $AppRoot 'lib\Thing.ps1')` is the
        # normal way a PowerShell app dot-sources, and treating the whole expression as unknowable made dozens
        # of one app's edges invisible - which is most of what its dead-file notes were.
        #
        # NOT A GUESS: the literal has to name exactly ONE mapped file, by the tail of its path. Two matches
        # is the ambiguity rule again and draws no edge, because which one `$AppRoot` points at is a
        # question about run time that this pass does not ask.
        foreach ($found in $Expression.FindAll({ $args[0] -is [StringConstantExpressionAst] }, $true)) {
            $tail = $found.Value.Replace('\', '/').TrimStart('.', '/')
            $suffix = [System.IO.Path]::GetExtension($tail).ToLowerInvariant()
            if ($suffix -ne '.ps1' -and $suffix -ne '.psm1' -and $suffix -ne '.psd1') { continue }
            $matched = @()
            foreach ($candidate in $Known.Keys) {
                if ($candidate -eq $tail -or $candidate.EndsWith('/' + $tail)) { $matched += $candidate }
            }
            if ($matched.Count -eq 1) {
                Write-Record 'MAP-PATH' $Rel $matched[0]
                return
            }
        }
        # Nothing in it names a file this map holds. REPORTED, not dropped: it is exactly the reason a file
        # with no dot-sourcers may still have one, and a blind spot nobody is told about gets acted on as if
        # it were not there. Only where the construct is certainly an import - `& $exe` is a launch, not one.
        $snippet = $Expression.Extent.Text.Trim()
        if ($snippet.Length -gt 60) { $snippet = $snippet.Substring(0, 60) + ' ...' }
        Write-Record 'MAP-COMPUTED' $Rel $Expression.Extent.StartLineNumber `
            ("a path built at run time: " + $snippet)
        return
    }
    # ONLY A POWERSHELL FILE IS AN IMPORT. `& arp.exe` and `& powershell.exe` are process launches written
    # with the same operator as a script call, and counting one as an import reported three EXECUTABLES as
    # broken imports on the first repo this ran against. A bare module name (`Import-Module Pester`) is not
    # a path either, and has no extension to match here.
    $extension = [System.IO.Path]::GetExtension($literal).ToLowerInvariant()
    if ($extension -ne '.ps1' -and $extension -ne '.psm1' -and $extension -ne '.psd1') { return }
    $full = if ([System.IO.Path]::IsPathRooted($literal)) { $literal }
            else { [System.IO.Path]::Combine($FileDirectory, $literal) }
    try { $full = [System.IO.Path]::GetFullPath($full) } catch { return }
    if (-not $full.StartsWith($Root, [StringComparison]::OrdinalIgnoreCase)) { return }
    $target = $full.Substring($Root.Length).TrimStart('\', '/').Replace('\', '/')
    if ($target.Length -eq 0) { return }
    # A path that IS in the map is an edge; one that is not on disk either is a dot-source of a file that
    # does not exist, which the caller reports as BROKEN. A path that exists but was left out of --ext is
    # neither, and saying nothing is the honest answer for it.
    if ($Known.ContainsKey($target) -or -not ([System.IO.File]::Exists($full) -or [System.IO.Directory]::Exists($full))) {
        Write-Record 'MAP-PATH' $Rel $target
    }
}

function Read-File([string]$Rel, [string]$AbsolutePath, [string]$Root, [hashtable]$Known) {
    $tokens = $null
    $errors = $null
    $ast = [Parser]::ParseFile($AbsolutePath, [ref]$tokens, [ref]$errors)
    $prepared = Get-TokenKeys $tokens
    Write-Record 'MAP-LINES' $Rel $prepared.Lines

    if ($errors -and $errors.Count -gt 0) {
        # NOT a skip. The parser error-recovers and hands back a PARTIAL tree, so every edge read out of
        # this file would be from that tree - and a file with no edges looks exactly like one with no
        # imports. This is also the version floor: PS7-only syntax does not parse under 5.1.
        foreach ($parseError in @($errors)[0..([Math]::Min($errors.Count, 3) - 1)]) {
            Write-Record 'MAP-ERROR' $Rel $parseError.Extent.StartLineNumber `
                ("does not parse under this host: " + $parseError.Message)
        }
        return
    }

    $summary = Get-Summary $tokens
    if ($summary.Length -gt 0) { Write-Record 'MAP-SUMMARY' $Rel $summary }
    # A file is reached by its PATH when it is dot-sourced, so that is one of the two things it declares.
    Write-Record 'MAP-DECL' $Rel $Rel

    $directory = [System.IO.Path]::GetDirectoryName($AbsolutePath)
    $field = $Rel.Replace('|', '/').Replace("`r", ' ').Replace("`n", ' ')
    $starts = $prepared.Starts
    $ends = $prepared.Ends

    # ONE WALK, bucketed by type. The rules half learned this the expensive way: FindAll with a predicate
    # per question is O(nodes x questions) with a scriptblock call on every visit, and a few dozen files took
    # minutes.
    $all = $ast.FindAll($script:EveryNode, $true)
    $functions = 0
    foreach ($node in $all) {
        if ($node -is [FunctionDefinitionAst]) {
            $functions++
            Write-Record 'MAP-DECL' $Rel $node.Name
            $body = $node.Body
            if ($body -and $body.EndBlock -and $body.EndBlock.Statements.Count -ge $script:MinBodyStatements) {
                $shape = Get-Fingerprint $prepared $body.Extent
                Write-Record 'MAP-BODY' $Rel $node.Extent.StartLineNumber $node.Name $shape.Digest $shape.Span
            }
            continue
        }

        if ($node -is [UsingStatementAst]) {
            if ($node.UsingStatementKind -eq [UsingStatementKind]::Module) {
                Resolve-Import $Rel $node.Name $Root $Known $directory $AbsolutePath
            }
            continue
        }

        if ($node -is [CommandAst]) {
            if ($node.InvocationOperator -ne [TokenKind]::Unknown) {
                # `. .\lib\Thing.ps1` and `& .\Thing.ps1`: the first element is the path, not a command.
                # A DOT-source is certainly a script; an `&` may just as well launch an executable.
                $certain = $node.InvocationOperator -eq [TokenKind]::Dot
                Resolve-Import $Rel $node.CommandElements[0] $Root $Known $directory $AbsolutePath $certain
                continue
            }
            $name = $node.GetCommandName()
            if (-not $name) { continue }
            if ($name.EndsWith('.ps1') -or $name.Contains('\') -or $name.Contains('/')) {
                Resolve-Import $Rel $node.CommandElements[0] $Root $Known $directory $AbsolutePath
                continue
            }
            if ($name -eq 'Import-Module' -and $node.CommandElements.Count -gt 1) {
                Resolve-Import $Rel $node.CommandElements[1] $Root $Known $directory $AbsolutePath $true
            }
            # The function this names, if any file in the tree defines it. The caller does the join and
            # drops the ones nothing declares - those are cmdlets.
            [void]$script:Out.Append('MAP-USE|').Append($field).Append('|').AppendLine($name.Replace('|', '/').Replace("`r", ' ').Replace("`n", ' '))
            continue
        }

        if ($node -is [ExpressionAst]) {
            $extent = $node.Extent
            if (($extent.EndOffset - $extent.StartOffset) -lt $script:MinExpressionChars) { continue }
            # TOO FEW TOKENS IS ASKED HERE, before the call: Get-Fingerprint's own slice, inline, so the thousands of
            # expressions too small to report never pay for a function call.
            $first = [Array]::BinarySearch($starts, $extent.StartOffset)
            if ($first -lt 0) { $first = -$first - 1 }
            $last = [Array]::BinarySearch($ends, $extent.EndOffset)
            if ($last -lt 0) { $last = -$last - 1 } else { while ($last -lt $ends.Count -and $ends[$last] -le $extent.EndOffset) { $last++ } }
            if ($last -lt $first) { $last = $first }
            if (($prepared.Counted[$last] - $prepared.Counted[$first]) -lt $script:MinExpressionTokens) { continue }
            $shape = Get-Fingerprint $prepared $extent
            if ($shape.Tokens -ge $script:MinExpressionTokens) {
                Write-Record 'MAP-EXPR' $Rel $node.Extent.StartLineNumber $shape.Digest $shape.Span
            }
        }
    }

    # A FILE THAT DEFINES NO FUNCTION IS A SCRIPT, not a library: nothing can call into it, so "nothing
    # imports this" is true of every such file and says nothing. It is run rather than imported, which is
    # what an entry point is.
    if ($functions -eq 0) { Write-Record 'MAP-ENTRY' $Rel }
}

function Read-ListFile([string]$Path) {
    $read = @()
    foreach ($raw in [System.IO.File]::ReadAllLines($Path)) {
        if ($raw.Trim().Length -eq 0) { continue }
        $parts = $raw.Split("`t")
        if ($parts.Count -lt 2) { continue }
        $read += , @($parts[0].Trim(), $parts[1].Trim())
    }
    return , $read
}

$rows = Read-ListFile $ListFile
$rootFull = [System.IO.Path]::GetFullPath($Root)
# EVERY FILE OF THE MAP resolves a dot-source, parsed this run or answered from the caller's cache.
$known = @{}
foreach ($row in $(if ($KnownFile) { Read-ListFile $KnownFile } else { $rows })) { $known[$row[0]] = $true }

foreach ($row in $rows) {
    try {
        Read-File $row[0] $row[1] $rootFull $known
    } catch {
        Write-Record 'MAP-ERROR' $row[0] 1 ("could not be read - " + $_.Exception.Message)
    }
    Write-Records
}
Write-Record 'MAP-DONE' $rows.Count
Write-Records
