<#
    PsGate.Rules.Scope.ps1 - the rules that need the SCOPE a node sits in, not just the node.

    All four read the per-scope sets the single walk in PsGate.ps1 filled in. Nothing here needs a symbol
    table across files, which is what keeps this gate in the same discipline as the C# async rule: one
    file, syntax only, silent where only a type could decide.

    RULE 6 AND RULE 7 SPLIT THE WORLD, and that is the point. A scope that opens a modal dialog is still on
    the call stack when its handlers fire, so its plain scriptblocks are CORRECT and rule 6 says nothing
    there - it was most of the findings on the first repo measured, which is how a gate gets switched off.
    What that scope gets instead is rule 7: the naming convention that survives a message pump. Every scope
    is therefore covered by exactly one of the two.
#>
using namespace System.Management.Automation.Language

# RULE 6 - an event handler built inside a function, reading that function's locals, without
# .GetNewClosure(). By the time Click or Tick fires the function has RETURNED and its locals are gone, so
# the handler reads $null - measured: a plain Add_Tick handler saw both its captured string and the timer
# object itself as empty, and threw "You cannot call a method on a null-valued expression" on every tick.
# Nothing warns you; the button simply does nothing.
#
# Only handlers that actually read an enclosing local are reported. A handler touching only its own
# variables, `$script:` state and its `param($s, $e)` is correct as written.
function Test-HandlerClosure([string]$Rel, [hashtable]$Ix) {
    foreach ($i in $Ix.Invoke) {
        if ($i.Member -isnot [StringConstantExpressionAst]) { continue }
        if (-not $i.Member.Value.StartsWith('Add_')) { continue }
        if (-not $i.Arguments) { continue }

        foreach ($arg in $i.Arguments) {
            # `{ ... }.GetNewClosure()` arrives as an InvokeMemberExpressionAst, not a scriptblock, so the
            # fixed shape never reaches here.
            if ($arg -isnot [ScriptBlockExpressionAst]) { continue }

            $scope = Get-NearestScope $i
            if ($null -eq $scope) { continue }              # script scope: its locals outlive the handler
            if ($Ix.Modal[$scope]) { continue }             # modal: the scope is still on the stack (rule 7)

            $outer = $Ix.Assigned[$scope]
            $inner = $Ix.Assigned[$arg]
            $read = $Ix.Reads[$arg]
            if ($null -eq $outer -or $null -eq $read) { continue }

            $captured = @()
            foreach ($name in $read.psbase.Keys) {
                $lower = $name.ToLowerInvariant()
                if ($script:NotCaptured -contains $lower) { continue }
                if ($null -ne $inner -and $inner.ContainsKey($lower)) { continue }
                if ($outer.ContainsKey($lower)) { $captured += "`$$name" }
            }
            if ($captured.Count -eq 0) { continue }

            Add-Finding -Rel $Rel -Node $i `
                -What "$($i.Member.Value) handler reads the enclosing scope's $(($captured | Sort-Object -Unique) -join ', ') without .GetNewClosure()" `
                -Remedy 'append .GetNewClosure() to the scriptblock, or keep the state in $script: - the locals are gone when it fires'
        }
    }
}

# RULE 7 - a generic variable name in a scope that opens a MODAL dialog. This is the only mechanically
# checkable defence against shadowing across a message pump: while ShowDialog() blocks, a timer tick or an
# event handler firing from elsewhere resolves an unqualified variable against the BLOCKED dialog's locals,
# so the dialog's $name wins over the caller's. Whether that happens depends on who is on the call stack at
# the time, which no static rule can know - so what gets enforced is the convention that makes it impossible.
function Test-ModalGenericName([string]$Rel, [hashtable]$Ix) {
    $reported = @{}
    foreach ($p in $Ix.Param) {
        Add-GenericName $Rel $Ix $p $p (Get-VarName $p.Name) 'declares' $reported
    }
    foreach ($a in $Ix.Assign) {
        Add-GenericName $Rel $Ix $a $a.Left (Get-VarName $a.Left) 'assigns' $reported
    }
}

# The name must live in the scope that BLOCKS - the one whose ShowDialog() call is on the stack while the
# pump runs. Walking every ancestor instead reported locals inside handler scriptblocks that pump nothing,
# just because the function around them opened a dialog somewhere else entirely.
function Add-GenericName([string]$Rel, [hashtable]$Ix, [Ast]$Owner, [Ast]$Node, $Name, [string]$Verb, [hashtable]$Reported) {
    if (-not $Name) { return }
    if ($script:GenericNames -notcontains $Name.ToLowerInvariant()) { return }

    $scope = Get-NearestScope $Owner
    if ($null -eq $scope -or -not $Ix.Modal[$scope]) { return }

    $label = Get-ScopeLabel $scope
    $slot = $label + '|' + $Name.ToLowerInvariant()
    if ($Reported.ContainsKey($slot)) { return }
    $Reported[$slot] = $true
    Add-Finding -Rel $Rel -Node $Node -What "$label opens a modal dialog and $Verb the generic `$$Name" `
        -Remedy 'prefix it ($dlgName) - a handler firing while ShowDialog blocks resolves against these locals'
}

# A scope has a name only when it is a function. A scriptblock is named by where it starts, which is what
# the reader needs to find it - and a dialog builder held in a variable is a scriptblock more often than not.
function Get-ScopeLabel([Ast]$Scope) {
    if ($Scope -is [FunctionDefinitionAst]) { return $Scope.Name }
    return "the scriptblock at line $($Scope.Extent.StartLineNumber)"
}

# RULE 8 - `return $set` where $set is a HashSet or a List. A scriptblock ENUMERATES what it returns,
# so a one-element HashSet comes back as a [string] and `.Contains('x')` silently becomes substring
# matching - which is true far too often. `return ,$set` suppresses the unroll. The type is read off the
# constructor in the SAME scope, so a factory function hides it either way - which makes this rule silent
# more often than wrong, and what it does say it says on the shape in front of it.
function Test-UnrolledReturn([string]$Rel, [hashtable]$Ix) {
    foreach ($r in $Ix.Return) {
        if ($null -eq $r.Pipeline) { continue }
        # `return ,$set` is an ArrayLiteralAst, so the correct shape never survives this unwrap.
        $name = Get-SoleVariableName $r.Pipeline
        if (-not $name) { continue }

        $scope = Get-NearestScope $r
        if ($null -eq $scope) { continue }
        $vars = $Ix.Unroll[$scope]
        if ($null -eq $vars -or -not $vars.ContainsKey($name.ToLowerInvariant())) { continue }

        Add-Finding -Rel $Rel -Node $r `
            -What "returns the collection `$$name, which the caller receives UNROLLED" `
            -Remedy "return ,`$$name - a one-element set arrives as a bare string and .Contains becomes substring matching"
    }
}

# RULE 9 - `@($list)` where $list is a List[object]. On Windows PowerShell 5.1 that overload throws
# ArgumentException instead of copying. Same-scope construction only: real detection needs type flow, and a
# rule that reached across files for it would be guessing.
function Test-ListObjectWrap([string]$Rel, [hashtable]$Ix) {
    foreach ($arr in $Ix.Array) {
        if ($arr.SubExpression.Statements.Count -ne 1) { continue }
        $name = Get-SoleVariableName $arr.SubExpression.Statements[0]
        if (-not $name) { continue }

        foreach ($scope in (Get-ScopeChain $arr $Ix.Root)) {
            $vars = $Ix.ListObj[$scope]
            if ($null -eq $vars -or -not $vars.ContainsKey($name.ToLowerInvariant())) { continue }
            Add-Finding -Rel $Rel -Node $arr `
                -What "wraps the List[object] `$$name in @(), which throws ArgumentException on 5.1" `
                -Remedy "use `$$name.ToArray(), or iterate it directly"
            break
        }
    }
}

# The scope a node belongs to: the nearest enclosing function or scriptblock literal, or $null at script
# level - where locals outlive every handler and rule 6 has nothing to say.
function Get-NearestScope([Ast]$Node) {
    $current = $Node.Parent
    while ($null -ne $current) {
        if ($current -is [FunctionDefinitionAst] -or $current -is [ScriptBlockExpressionAst]) { return $current }
        $current = $current.Parent
    }
    return $null
}
