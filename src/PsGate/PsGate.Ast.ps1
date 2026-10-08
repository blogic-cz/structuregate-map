<#
    AST HELPERS - the whole vocabulary of "what is this", with no pattern matching anywhere.

    Split out of `PsGate.ps1`, which reached 454 of its own 500-line limit. Dot-sourced from exactly
    where it used to sit, so every function is defined in the same ORDER as before: these are called by
    the rule files, and a helper defined after its caller is a helper that is not there yet.
#>

# ---------------------------------------------------------------------------------------------------
# AST helpers - the whole vocabulary of "what is this", with no pattern matching anywhere.
# ---------------------------------------------------------------------------------------------------

function Get-VarName([Ast]$Node) {
    # An assignment target arrives wrapped: `[int]$x = 1` is a ConvertExpressionAst, `[ref]$x` an
    # AttributedExpressionAst. Unwrap until a variable appears, or give up rather than guess.
    $current = $Node
    while ($null -ne $current) {
        if ($current -is [VariableExpressionAst]) { return $current.VariablePath.UserPath }
        if ($current -is [AttributedExpressionAst]) { $current = $current.Child; continue }
        return $null
    }
    return $null
}

function Test-Scoped([string]$Name) {
    # `$script:x` / `$global:x` / `$using:x` name their scope, so they are not accidental capture.
    return $Name.Contains(':')
}

function Get-CommandName([CommandAst]$Command) {
    try { return $Command.GetCommandName() } catch { return $null }
}

# The EXPRESSION a statement produces. An assignment's right-hand side is a statement, so `$x = @{}` is a
# PipelineAst wrapping a CommandExpressionAst wrapping the literal; parentheses add another layer.
function Get-ValueExpression([Ast]$Node) {
    $current = $Node
    while ($null -ne $current) {
        if ($current -is [PipelineAst]) {
            if ($current.PipelineElements.Count -ne 1) { return $null }
            $current = $current.PipelineElements[0]
            continue
        }
        if ($current -is [CommandExpressionAst]) { $current = $current.Expression; continue }
        if ($current -is [ParenExpressionAst]) { $current = $current.Pipeline; continue }
        return $current
    }
    return $null
}

# A single variable, if that is all this expression is: `$x`, `($x)`.
function Get-SoleVariableName([Ast]$Node) {
    $value = Get-ValueExpression $Node
    if ($value -is [VariableExpressionAst]) { return $value.VariablePath.UserPath }
    return $null
}

# The TYPE a value constructs, as its name: `New-Object System.Collections.Generic.List[object]` and
# `[System.Collections.Generic.List[object]]::new()` both answer the same thing. $null when the value is
# not a construction - which is the honest answer, and the rules stay silent on it.
function Get-ConstructedTypeName([Ast]$Node) {
    $value = Get-ValueExpression $Node
    if ($null -eq $value) { return $null }

    if ($value -is [ConvertExpressionAst]) { return $value.Type.TypeName.FullName }

    if ($value -is [InvokeMemberExpressionAst]) {
        if ($value.Expression -isnot [TypeExpressionAst]) { return $null }
        return $value.Expression.TypeName.FullName
    }

    if ($value -is [CommandAst]) {
        if ((Get-CommandName $value) -ne 'New-Object') { return $null }
        # `New-Object <TypeName> [-ArgumentList ...]`: the first bare argument is the type.
        foreach ($element in $value.CommandElements) {
            if ($element -is [StringConstantExpressionAst]) {
                if ($element.Value -eq 'New-Object') { continue }
                return $element.Value
            }
        }
    }
    return $null
}

# `System.Collections.Generic.List[object]` -> `List`.
function Get-SimpleTypeName([string]$TypeName) {
    if (-not $TypeName) { return '' }
    $open = $TypeName.IndexOf('[')
    $bare = if ($open -ge 0) { $TypeName.Substring(0, $open) } else { $TypeName }
    $dot = $bare.LastIndexOf('.')
    if ($dot -ge 0) { $bare = $bare.Substring($dot + 1) }
    return $bare.Trim()
}

# `System.Collections.Generic.List[object]` -> `object`; empty when the type is not generic.
function Get-GenericArgumentName([string]$TypeName) {
    if (-not $TypeName) { return '' }
    $open = $TypeName.IndexOf('[')
    if ($open -lt 0) { return '' }
    $close = $TypeName.LastIndexOf(']')
    if ($close -le $open) { return '' }
    return Get-SimpleTypeName $TypeName.Substring($open + 1, $close - $open - 1)
}

# The hashtable LITERAL a value hands over, through the wrappers people actually write:
# `@{...}`, `[ordered]@{...}`, `[hashtable]::Synchronized(@{...})`.
function Get-HashtableLiteral([Ast]$Node) {
    $value = Get-ValueExpression $Node
    if ($value -is [HashtableAst]) { return $value }
    if ($value -is [ConvertExpressionAst]) { return Get-HashtableLiteral $value.Child }
    if ($value -is [InvokeMemberExpressionAst] -and $value.Arguments) {
        foreach ($arg in $value.Arguments) {
            $inner = Get-HashtableLiteral $arg
            if ($null -ne $inner) { return $inner }
        }
    }
    return $null
}

# Does this value produce something whose keys are case-insensitive? A literal, or a construction of one
# of the hashtable-like types.
function Test-HashtableValue([Ast]$Node) {
    if ($null -ne (Get-HashtableLiteral $Node)) { return $true }
    $simple = (Get-SimpleTypeName (Get-ConstructedTypeName $Node)).ToLowerInvariant()
    return $script:HashtableTypes -contains $simple
}

# Is this argument passed `[ref]`?
function Test-ByRefArgument([Ast]$Node) {
    if ($Node -isnot [ConvertExpressionAst]) { return $false }
    return (Get-SimpleTypeName $Node.Type.TypeName.FullName).ToLowerInvariant() -eq 'ref'
}

# The scopes a node sits in, innermost first, with the file itself last. A scope is a function or a
# scriptblock literal - the two things whose locals disappear when they return.
function Get-ScopeChain([Ast]$Node, [Ast]$Root) {
    $chain = New-Object System.Collections.ArrayList
    $current = $Node.Parent
    while ($null -ne $current) {
        if ($current -is [FunctionDefinitionAst] -or $current -is [ScriptBlockExpressionAst]) { [void]$chain.Add($current) }
        $current = $current.Parent
    }
    [void]$chain.Add($Root)
    # `,` because a scriptblock unrolls what it returns, and a one-scope chain would come back as a bare
    # node. Rule 8 of this gate found this line in this gate.
    return ,$chain
}

function Add-ToSet([hashtable]$Map, $Key, $Value) {
    if (-not $Map.ContainsKey($Key)) { $Map[$Key] = @{} }
    $Map[$Key][$Value] = $true
}
