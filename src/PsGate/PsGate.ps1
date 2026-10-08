<#
    PsGate.ps1 - the PowerShell half of structuregate (`--ps-discipline`).

    WHY A SCRIPT AND NOT C#: every rule here keys on the PowerShell AST, and the only authoritative parser
    for that is the one inside the host that will RUN the code. These apps are launched by `powershell.exe`
    (Windows PowerShell 5.1), so the gate parses with 5.1 as well. A PS7 parser would accept `??`, `?:` and
    `&&` - syntax 5.1 cannot even read - and would pass a file that dies on the target machine. Parsing
    with the shipping host also turns a ParseError into a VIOLATION instead of a surprise at runtime.

    It is EMBEDDED in structuregate.exe and written to a temp folder on demand, so a consumer still deploys
    two files and cannot end up with a gate whose second half is missing.

    NO REGEX, ANYWHERE - and the build refuses one (see the BanRegex target in StructureGate.csproj).
    Every decision is read off the AST or off a type NAME with plain string operations. A pattern over
    extent text is a second, worse parser: `List` matched `$lstBox` and `ListBox`, and `@\{` matched the
    literals inside an assigned worker scriptblock. Both were measured as false positives on the
    first repo this ran against, and both disappeared when the same question was asked of the tree.

    Contract with the caller (structuregate.exe):
      in   -ListFile <path>   UTF-8, one file per line: <relative-path><TAB><absolute-path>
      out  PSGATE-LINES|<rel>|<n>                        source lines, by TOKEN, for every file parsed
           PSGATE|error|<rel>|<line>|<message>            one finding per line
           PSGATE-DONE|<files parsed>                     LAST line; its absence means this half failed
      exit 0 even when findings exist - the caller owns the verdict.

    A line carrying `# psgate-ok` (on it or on the line above) is waived. Every rule cost someone a
    debugging session; the waiver is where the case that genuinely needs the shape keeps its reason.

    ONE PASS, NOT ONE PASS PER RULE. The first version called Ast.FindAll() inside each rule and inside
    each scope, which is O(nodes x scopes) with a PowerShell scriptblock predicate on every visit: a few dozen
    files took minutes. Everything is now bucketed by type in a single walk, and scope membership is a parent
    walk (a few hops) rather than a subtree search. Same findings, seconds.
#>
using namespace System.Management.Automation.Language

[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$ListFile,
    [string]$Root = '.'
)

$ErrorActionPreference = 'Stop'
# EVERY NODE, ASKED IN .NET: a delegate bound to `[Ast].IsInstanceOfType` runs no script per node, where
# `FindAll({ $true })` invoked a scriptblock for each of them.
$script:EveryNode = [Delegate]::CreateDelegate([Func[Ast, bool]], [Ast], [Type].GetMethod('IsInstanceOfType', [Type[]]@([object])))
# The caller reads this stream as UTF-8. Without this line a snippet quoted out of a file with a non-ASCII
# identifier arrives mangled, because powershell.exe writes in the console's OEM code page.
[Console]::OutputEncoding = New-Object System.Text.UTF8Encoding $false

# ---------------------------------------------------------------------------------------------------
# Vocabulary the rules are keyed on.
# ---------------------------------------------------------------------------------------------------

# Automatic variables the engine owns. Assigning one does not fail - it silently replaces something the
# session needs, and the breakage appears somewhere else entirely ($host reused for a server name
# broke Write-Host far from the assignment). Preference variables are NOT here: those are meant to be set.
# `$null` is not here either: `$null = $p.StandardOutput.ReadToEnd()` is THE idiom for discarding output.
$script:Automatic = @(
    'host', 'input', 'error', 'matches', 'args', 'this', '_', 'psitem', 'pwd', 'pid', 'profile',
    'true', 'false', 'psscriptroot', 'pscommandpath', 'psboundparameters', 'myinvocation',
    'executioncontext', 'psversiontable', 'psculture', 'psuiculture', 'shellid', 'stacktrace',
    'lastexitcode', 'nestedpromptlevel', 'psdebugcontext', 'foreach', 'switch'
)

# A parameter or a foreach variable may not be an automatic at all - there is no discard idiom there.
$script:AutomaticDeclared = @('null') + $script:Automatic

# Names too generic to survive a modal message pump. While ShowDialog() runs, a timer tick or an event
# handler resolves an unqualified variable against the BLOCKED dialog's locals, so the dialog's $name
# wins over the caller's. A prefix ($dlgName) makes the collision impossible.
$script:GenericNames = @('name', 'key', 'path', 'value', 'text', 'note', 'item', 'index', 'count')

# Variables a scriptblock may read without capturing anything: the engine binds them, or the author
# already said which scope they come from.
$script:NotCaptured = @(
    '_', 'psitem', 'this', 'args', 'null', 'true', 'false', 'error', 'input', 'matches', 'host',
    'sender', 'eventargs', 'e', 's', 'psboundparameters', 'myinvocation', 'pwd', 'lastexitcode',
    'psscriptroot', 'pscommandpath', 'ofs', 'erroractionpreference', 'progresspreference',
    'warningpreference', 'verbosepreference', 'debugpreference', 'informationpreference',
    'confirmpreference', 'pscmdlet'
)

# `Try...([ref]$x)` shapes. Every one leaves the [ref] target UNTOUCHED when it returns $false, so
# ignoring the boolean means reading whatever the previous iteration put there.
$script:TryMethods = @('TryDequeue', 'TryPop', 'TryTake', 'TryGetValue', 'TryPeek', 'TryAdd', 'TryParse', 'TryRemove')

# Collections a scriptblock UNROLLS on the way out, by SIMPLE TYPE NAME - read off the constructor, never
# off the text of the right-hand side.
$script:UnrolledTypes = @(
    'HashSet', 'List', 'Queue', 'Stack', 'Dictionary', 'SortedSet', 'SortedDictionary', 'SortedList',
    'ArrayList', 'ConcurrentQueue', 'ConcurrentStack', 'ConcurrentBag', 'ConcurrentDictionary'
)

# Types whose keys compare case-INSENSITIVELY, so two spellings are one slot.
$script:HashtableTypes = @('hashtable', 'ordereddictionary', 'ordered')

# The helper that makes a check box work on a VIRTUAL ListView: it draws the box and tracks the state,
# because the control does neither. Named here rather than inside the rule, so a repo that calls the same
# job something else is one line to add. Compared lowercase.
$script:VirtualCheckHelpers = @('enable-virtualcheck')

# Members a VIRTUAL ListView does not maintain. `.Items` stays EMPTY however many rows the list shows, and
# `.CheckedItems` / `.CheckedIndices` answer out of that empty collection - so the count and the content
# both lie, and nothing throws to say so.
$script:VirtualMembers = @('items', 'checkeditems', 'checkedindices')

# ---------------------------------------------------------------------------------------------------
# Reporting
# ---------------------------------------------------------------------------------------------------

$script:Findings = New-Object System.Collections.ArrayList
$script:Waiver = 'psgate-ok'
$script:Lines = @()

function Test-Waived([int]$line) {
    # The comment may sit on the line itself or on the line above it, which is how people write them.
    foreach ($candidate in @(($line - 1), $line)) {
        if ($candidate -ge 1 -and $candidate -le $script:Lines.Count) {
            if ($script:Lines[$candidate - 1].Contains($script:Waiver)) { return $true }
        }
    }
    return $false
}

# EVERY FINDING IS AN ERROR. There was a `warn` severity, for the rules that read a type off a constructor
# in one scope - and a finding that does not fail the build is a finding nobody fixes: it scrolls past in a
# build log that ends with OK. The exception is not a lower severity, it is `# psgate-ok` ON THE LINE, which
# fails nothing and carries the reason next to the code. The severity FIELD stays in the protocol so the
# caller's parse does not change.
function Add-Finding {
    param(
        [string]$Rel,
        [Ast]$Node,
        [string]$What,
        [string]$Remedy
    )
    $line = $Node.Extent.StartLineNumber
    if (Test-Waived $line) { return }
    $snippet = $Node.Extent.Text.Split([char]10)[0].Trim()
    if ($snippet.Length -gt 60) { $snippet = $snippet.Substring(0, 60) + ' ...' }
    [void]$script:Findings.Add("PSGATE|error|$Rel|$line|$What - $Remedy (``$snippet``)")
}

. $PSScriptRoot\PsGate.Ast.ps1   # the AST vocabulary, split out when this file hit its own limit
# ---------------------------------------------------------------------------------------------------
# Source lines, BY TOKEN. Exactly the C# rule ("a line a token sits on"), from the same parse: comments
# and blank lines are excluded because they produce no token, not because a scanner guessed at them.
# These repos carry long measured-reason headers in `<# ... #>`, and counting those as source would push
# authors to delete the comments the repo exists to keep.
# ---------------------------------------------------------------------------------------------------

function Measure-TokenLines($Tokens) {
    $lines = New-Object 'System.Collections.Generic.HashSet[int]'
    foreach ($t in $Tokens) {
        if ($t.Kind -eq [TokenKind]::Comment) { continue }
        if ($t.Kind -eq [TokenKind]::NewLine) { continue }
        if ($t.Kind -eq [TokenKind]::LineContinuation) { continue }
        if ($t.Kind -eq [TokenKind]::EndOfInput) { continue }
        for ($line = $t.Extent.StartLineNumber; $line -le $t.Extent.EndLineNumber; $line++) { [void]$lines.Add($line) }
    }
    return $lines.Count
}

# ---------------------------------------------------------------------------------------------------
# The index: one walk, every rule's input.
# ---------------------------------------------------------------------------------------------------

function New-AstIndex([Ast]$Ast) {
    $ix = @{
        Root      = $Ast
        Assign    = New-Object System.Collections.ArrayList   # AssignmentStatementAst
        Param     = New-Object System.Collections.ArrayList   # ParameterAst
        Loop      = New-Object System.Collections.ArrayList   # ForEachStatementAst
        Array     = New-Object System.Collections.ArrayList   # ArrayExpressionAst
        Member    = New-Object System.Collections.ArrayList   # MemberExpressionAst (property access)
        Invoke    = New-Object System.Collections.ArrayList   # InvokeMemberExpressionAst
        Binary    = New-Object System.Collections.ArrayList   # BinaryExpressionAst
        Return    = New-Object System.Collections.ArrayList   # ReturnStatementAst
        Func      = New-Object System.Collections.ArrayList   # FunctionDefinitionAst
        Command   = New-Object System.Collections.ArrayList   # CommandAst
        Assigned  = @{}    # scope -> names it assigns anywhere inside (lowercase)
        Reads     = @{}    # scope -> names read anywhere inside (as written)
        Unroll    = @{}    # scope -> names holding a collection that unrolls on return
        ListObj   = @{}    # scope -> names holding a List[object]
        Modal     = @{}    # scope -> $true when a ShowDialog() call is inside it
        HashKeys  = @{}    # hashtable variable (lowercase) -> the literal keys it was created with
        TrueProps = @{}    # variable (lowercase) -> property (lowercase) -> the node that set it $true
        Helped    = @{}    # variable (lowercase) -> $true when a virtual-check helper was called with it
    }

    foreach ($n in $Ast.FindAll($script:EveryNode, $true)) {
        if ($n -is [VariableExpressionAst]) {
            $name = $n.VariablePath.UserPath
            if (-not (Test-Scoped $name)) {
                foreach ($scope in (Get-ScopeChain $n $Ast)) { Add-ToSet $ix.Reads $scope $name }
            }
            continue
        }
        if ($n -is [AssignmentStatementAst]) {
            [void]$ix.Assign.Add($n)
            Add-AssignmentToIndex $ix $n $Ast
            continue
        }
        if ($n -is [InvokeMemberExpressionAst]) {
            [void]$ix.Invoke.Add($n)
            # ONLY the nearest scope is modal, not every ancestor. Marking the whole chain meant one
            # ShowDialog() anywhere in a long panel builder made every generic local in that function
            # a finding - including locals inside handler scriptblocks that pump nothing. Measured on a real
            # repo: many of the rule-7 findings were that, and the scope that BLOCKS is the only one whose locals
            # a handler firing during the pump can resolve against.
            if ($n.Member -is [StringConstantExpressionAst] -and $n.Member.Value -eq 'ShowDialog') {
                $modal = Get-NearestScope $n
                if ($null -ne $modal) { $ix.Modal[$modal] = $true }
            }
            continue
        }
        if ($n -is [ParameterAst]) {
            [void]$ix.Param.Add($n)
            Add-DeclaredName $ix $n (Get-VarName $n.Name) $Ast
            continue
        }
        if ($n -is [ForEachStatementAst]) {
            [void]$ix.Loop.Add($n)
            Add-DeclaredName $ix $n (Get-VarName $n.Variable) $Ast
            continue
        }
        if ($n -is [CommandAst]) {
            [void]$ix.Command.Add($n)
            Add-HelperCall $ix $n
            continue
        }
        if ($n -is [MemberExpressionAst]) { [void]$ix.Member.Add($n); continue }
        if ($n -is [ArrayExpressionAst]) { [void]$ix.Array.Add($n); continue }
        if ($n -is [BinaryExpressionAst]) { [void]$ix.Binary.Add($n); continue }
        if ($n -is [ReturnStatementAst]) { [void]$ix.Return.Add($n); continue }
        if ($n -is [FunctionDefinitionAst]) { [void]$ix.Func.Add($n); continue }
    }
    return $ix
}

function Add-DeclaredName([hashtable]$Ix, [Ast]$Node, $Name, [Ast]$Root) {
    if (-not $Name) { return }
    foreach ($scope in (Get-ScopeChain $Node $Root)) { Add-ToSet $Ix.Assigned $scope $Name.ToLowerInvariant() }
}

# A virtual-check helper call, by the VARIABLE it was given. Rule 12 fires on a check box that the control
# never draws, so the one shape that makes it work has to be visible to it.
function Add-HelperCall([hashtable]$Ix, [CommandAst]$Node) {
    $name = Get-CommandName $Node
    if (-not $name) { return }
    if ($script:VirtualCheckHelpers -notcontains $name.ToLowerInvariant()) { return }
    foreach ($element in $Node.CommandElements) {
        if ($element -is [VariableExpressionAst]) {
            $Ix.Helped[$element.VariablePath.UserPath.ToLowerInvariant()] = $true
        }
    }
}

# `$lv.VirtualMode = $true` - the property assignments rules 12 to 14 are keyed on. A LITERAL `$true` only:
# a flag read from a settings object is not decidable here, and a rule that assumed it would be the kind
# that gets switched off.
function Add-TrueProperty([hashtable]$Ix, [AssignmentStatementAst]$Node) {
    $target = $Node.Left
    if ($target.Expression -isnot [VariableExpressionAst]) { return }
    if ($target.Member -isnot [StringConstantExpressionAst]) { return }
    $value = Get-ValueExpression $Node.Right
    if ($value -isnot [VariableExpressionAst]) { return }
    if ($value.VariablePath.UserPath -ne 'true') { return }

    $owner = $target.Expression.VariablePath.UserPath.ToLowerInvariant()
    if (-not $Ix.TrueProps.ContainsKey($owner)) { $Ix.TrueProps[$owner] = @{} }
    $property = $target.Member.Value.ToLowerInvariant()
    # The FIRST assignment is kept: it is where the list was configured, and that is the line to look at.
    if (-not $Ix.TrueProps[$owner].ContainsKey($property)) { $Ix.TrueProps[$owner][$property] = $target }
}

# What one assignment teaches the index: the name is a local of every enclosing scope, and its VALUE says
# whether the name is a hashtable, a collection that unrolls, or a List[object].
function Add-AssignmentToIndex([hashtable]$Ix, [AssignmentStatementAst]$Node, [Ast]$Root) {
    # `$lv.VirtualMode = $true` assigns a PROPERTY, not a local: no name enters any scope from it.
    if ($Node.Left -is [MemberExpressionAst]) {
        Add-TrueProperty $Ix $Node
        return
    }

    $name = Get-VarName $Node.Left
    if (-not $name -or (Test-Scoped $name)) { return }
    $lower = $name.ToLowerInvariant()

    if (-not $Ix.HashKeys.ContainsKey($lower) -and (Test-HashtableValue $Node.Right)) {
        $Ix.HashKeys[$lower] = Get-HashtableLiteral $Node.Right
    }

    $typeName = Get-ConstructedTypeName $Node.Right
    $simple = Get-SimpleTypeName $typeName
    $unrolls = $script:UnrolledTypes -contains $simple
    $listObject = $simple -eq 'List' -and (Get-GenericArgumentName $typeName).ToLowerInvariant() -eq 'object'

    foreach ($scope in (Get-ScopeChain $Node $Root)) {
        Add-ToSet $Ix.Assigned $scope $lower
        if ($unrolls) { Add-ToSet $Ix.Unroll $scope $lower }
        if ($listObject) { Add-ToSet $Ix.ListObj $scope $lower }
    }
}

# ---------------------------------------------------------------------------------------------------
# The rules
# ---------------------------------------------------------------------------------------------------

. $PSScriptRoot\PsGate.Rules.ps1
. $PSScriptRoot\PsGate.Rules.Scope.ps1
. $PSScriptRoot\PsGate.Rules.WinForms.ps1

# ---------------------------------------------------------------------------------------------------
# Drive
# ---------------------------------------------------------------------------------------------------

$parsed = 0
foreach ($row in [System.IO.File]::ReadAllLines($ListFile)) {
    if (-not $row.Trim()) { continue }
    $parts = $row.Split([char]9)
    if ($parts.Count -lt 2) { continue }
    $rel = $parts[0]
    $abs = $parts[1]

    $tokens = $null
    $errors = $null
    try {
        $ast = [Parser]::ParseFile($abs, [ref]$tokens, [ref]$errors)
        $script:Lines = [System.IO.File]::ReadAllLines($abs)
    } catch {
        Write-Output "PSGATE|error|$rel|1|cannot be parsed - $($_.Exception.Message)"
        continue
    }
    $parsed++
    Write-Output "PSGATE-LINES|$rel|$(Measure-TokenLines $tokens)"

    # A PARSE ERROR IS A VIOLATION, never a skip. The parser error-recovers and hands back a PARTIAL tree,
    # so every rule below would quietly stop covering the rest of the file. Under the 5.1 parser this is
    # also the version floor: `??`, `?:` and `&&` are PS7-only and fail here, which is the truth on a box
    # that runs the app with powershell.exe.
    if ($errors -and $errors.Count -gt 0) {
        foreach ($e in ($errors | Select-Object -First 3)) {
            $line = $e.Extent.StartLineNumber
            Write-Output ("PSGATE|error|$rel|$line|does not parse under Windows PowerShell 5.1: " +
                "$($e.Message) - fix the syntax, or drop the PS7-only construct (``??``, ``?:``, ``&&``)")
        }
        continue
    }

    # .psd1 is DATA. It is counted like any other file, but a rule about assignments and event handlers
    # has nothing to say about a manifest.
    if ([System.IO.Path]::GetExtension($rel) -eq '.psd1') { continue }

    $ix = New-AstIndex $ast

    Test-AutomaticAssignment $rel $ix
    Test-JsonArrayWrap $rel $ix
    Test-KeyCaseCollision $rel $ix
    Test-NullOnRight $rel $ix
    Test-IgnoredTryResult $rel $ix
    Test-HandlerClosure $rel $ix
    Test-ModalGenericName $rel $ix
    Test-UnrolledReturn $rel $ix
    Test-ListObjectWrap $rel $ix
    Test-ParamAsCommand $rel $ix
    Test-StrandedMember $rel $ix
    Test-EagerAddType $rel $ix
    Test-VirtualCheckBoxes $rel $ix
    Test-VirtualItemCheck $rel $ix
    Test-VirtualMember $rel $ix
}

foreach ($finding in $script:Findings) { Write-Output $finding }
Write-Output "PSGATE-DONE|$parsed"
exit 0
