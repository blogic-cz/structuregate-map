<#
    PsGate.Rules.ps1 - the rules a single AST node decides. Dot-sourced by PsGate.ps1; every function
    takes the index that walk built, so no rule searches the tree again.

    Each rule keys on a NODE SHAPE. Where only a type would settle it, the rule either narrows itself to a
    case that IS decidable (a hashtable this file created) or it is not written at all. A rule that guesses
    gets switched off, and then nothing is checked. Every rule that IS here fails the build - see the note
    on Add-Finding in PsGate.ps1 for why there is no lower severity to hide in.

    No regex here, and none anywhere in this gate: the build fails on one. See the header of PsGate.ps1.
#>
using namespace System.Management.Automation.Language

# RULE 1 - assignment to a variable the ENGINE owns. It does not fail; it replaces something the session
# needs, and the breakage appears somewhere else entirely. A parameter and a foreach variable are the same
# mistake with different syntax, and there `$null` counts too - only the assignment form has a discard idiom.
function Test-AutomaticAssignment([string]$Rel, [hashtable]$Ix) {
    $remedy = 'rename it - the engine owns this name and the damage shows up somewhere else'

    foreach ($a in $Ix.Assign) {
        $name = Get-VarName $a.Left
        if ($name -and $script:Automatic -contains $name.ToLowerInvariant()) {
            Add-Finding -Rel $Rel -Node $a.Left -What "assigns the automatic variable `$$name" -Remedy $remedy
        }
    }
    foreach ($p in $Ix.Param) {
        $name = Get-VarName $p.Name
        if ($name -and $script:AutomaticDeclared -contains $name.ToLowerInvariant()) {
            Add-Finding -Rel $Rel -Node $p -What "declares a parameter named `$$name, an automatic variable" -Remedy $remedy
        }
    }
    foreach ($f in $Ix.Loop) {
        $name = Get-VarName $f.Variable
        if ($name -and $script:AutomaticDeclared -contains $name.ToLowerInvariant()) {
            Add-Finding -Rel $Rel -Node $f.Variable -What "loops over the automatic variable `$$name" -Remedy $remedy
        }
    }
}

# RULE 2 - `@(... | ConvertFrom-Json)`. Windows PowerShell 5.1 emits a JSON array as ONE object, so the
# wrapper produces a one-element array holding an Object[]. Every field read after that is off by a level.
function Test-JsonArrayWrap([string]$Rel, [hashtable]$Ix) {
    foreach ($arr in $Ix.Array) {
        foreach ($stmt in $arr.SubExpression.Statements) {
            if ($stmt -isnot [PipelineAst]) { continue }
            $elements = $stmt.PipelineElements
            if ($elements.Count -eq 0) { continue }
            $last = $elements[$elements.Count - 1]
            if ($last -is [CommandAst] -and (Get-CommandName $last) -eq 'ConvertFrom-Json') {
                Add-Finding -Rel $Rel -Node $arr -What 'wraps ConvertFrom-Json in @(), which yields ONE element holding the whole array' `
                    -Remedy 'assign the result first, then iterate it - 5.1 does not enumerate a JSON array'
            }
        }
    }
}

# RULE 3 - two spellings of one hashtable key. Hashtable keys are case-INSENSITIVE, so `$ctx.ticked` and
# `$ctx.Ticked` are the same slot: a counter overwrote a scriptblock and the button died with
# "The term '114' is not recognized".
#
# Narrowed to variables THIS FILE creates a hashtable in, which is why the index reads the VALUE rather
# than its text. `$body = { ... @{ name = $w.name } ... }` assigns a scriptblock, and the keys inside it
# belong to the objects built in there - attributing them to `$body` reported two collisions in a worker
# that has no hashtable at all. A case-insensitive .NET property (`$btn.text` and `$btn.Text`) is left
# alone for the same reason: it is one member either way.
function Test-KeyCaseCollision([string]$Rel, [hashtable]$Ix) {
    # slot "<var>|<key lowercased>" -> spelling -> the node that used it
    $spellings = @{}

    foreach ($owner in $Ix.HashKeys.psbase.Keys) {
        $literal = $Ix.HashKeys[$owner]
        if ($null -eq $literal) { continue }
        foreach ($pair in $literal.KeyValuePairs) {
            if ($pair.Item1 -is [StringConstantExpressionAst]) {
                Add-Spelling $spellings $owner $pair.Item1.Value $pair.Item1
            }
        }
    }

    foreach ($m in $Ix.Member) {
        if ($m.Expression -isnot [VariableExpressionAst]) { continue }
        if ($m.Member -isnot [StringConstantExpressionAst]) { continue }
        $owner = $m.Expression.VariablePath.UserPath.ToLowerInvariant()
        if (-not $Ix.HashKeys.ContainsKey($owner)) { continue }
        Add-Spelling $spellings $owner $m.Member.Value $m.Member
    }

    foreach ($slot in $spellings.psbase.Keys) {
        $variants = $spellings[$slot]
        if ($variants.psbase.Count -lt 2) { continue }
        # ORDINAL, not Sort-Object: that compares by culture, and the culture rules differ by host - .NET
        # Framework puts `Ticked` first, ICU (pwsh off Windows) `ticked` - so one file read two ways.
        [string[]]$sorted = @($variants.psbase.Keys)
        [Array]::Sort($sorted, [StringComparer]::Ordinal)
        $owner = $slot.Split([char]124)[0]
        # Reported at the LAST use: the earlier spelling is the declaration, the later one is the write
        # that destroyed it, and that is the line the author has to look at.
        $latest = @($variants.psbase.Keys | Sort-Object { $variants[$_].Extent.StartLineNumber })[-1]
        Add-Finding -Rel $Rel -Node $variants[$latest] `
            -What "`$$owner is used with keys that differ only by case ($($sorted -join ', ')), which is ONE key" `
            -Remedy 'pick one spelling - a hashtable key is case-insensitive, so the second write destroys the first value'
    }

    # Two spellings INSIDE ONE literal need no rule: `@{ Key = 1; key = 2 }` is a 5.1 PARSE ERROR
    # ("Duplicate keys 'key' are not allowed in hash literals"), which the parse-error check already reports.
}

# The per-slot map of spellings must be CASE-SENSITIVE, so it is a Dictionary and not a hashtable: a
# PowerShell `@{}` is case-insensitive, which merged 'ticked' and 'Ticked' into one entry and made this
# rule blind to the exact bug it exists to find. Measured on the fixture before the fix: zero findings.
function Add-Spelling([hashtable]$Map, [string]$Owner, [string]$Spelling, [Ast]$Node) {
    $slot = "$Owner|" + $Spelling.ToLowerInvariant()
    if (-not $Map.ContainsKey($slot)) { $Map[$slot] = New-Object 'System.Collections.Generic.Dictionary[string,object]' }
    if (-not $Map[$slot].ContainsKey($Spelling)) { $Map[$slot][$Spelling] = $Node }
}

# RULE 4 - `$x -eq $null`. With an ARRAY on the left, `-eq` FILTERS instead of comparing, so the test is
# true only when the array holds a $null. `$null -eq $x` is the comparison that means what it reads like.
function Test-NullOnRight([string]$Rel, [hashtable]$Ix) {
    $operators = @([TokenKind]::Ieq, [TokenKind]::Ine, [TokenKind]::Ceq, [TokenKind]::Cne)
    foreach ($b in $Ix.Binary) {
        if ($operators -notcontains $b.Operator) { continue }
        if ($b.Right -isnot [VariableExpressionAst]) { continue }
        if ($b.Right.VariablePath.UserPath -ne 'null') { continue }
        if ($b.Left -is [ConstantExpressionAst]) { continue }
        Add-Finding -Rel $Rel -Node $b -What 'compares against $null on the RIGHT' `
            -Remedy 'put $null on the left - with an array on the left, -eq filters instead of comparing'
    }
}

# RULE 5 - a `Try...([ref]$x)` whose boolean result is thrown away. On $false the [ref] target is left
# UNTOUCHED, so the next read gets whatever the previous iteration put there; the value looks fresh. A bare
# call also writes its $true/$false into the output stream, which the enclosing function then returns.
function Test-IgnoredTryResult([string]$Rel, [hashtable]$Ix) {
    foreach ($i in $Ix.Invoke) {
        if ($i.Member -isnot [StringConstantExpressionAst]) { continue }
        if ($script:TryMethods -notcontains $i.Member.Value) { continue }
        if (-not $i.Arguments) { continue }

        $target = $null
        foreach ($arg in $i.Arguments) {
            if (Test-ByRefArgument $arg) { $target = Get-VarName $arg.Child }
        }
        if (-not $target) { continue }
        if (-not (Test-ResultDiscarded $i)) { continue }
        # INITIALISE-AND-TRY on one line is correct and common: `$ttl = 0; [int]::TryParse($s, [ref]$ttl)`
        # cannot read a stale value, because the initialisation runs again every time the call does - inside
        # a loop included. Only a target initialised somewhere ELSE can hold the previous round's value.
        if (Test-InitialisedOnSameLine $Ix $i $target) { continue }

        Add-Finding -Rel $Rel -Node $i `
            -What "ignores the result of $($i.Member.Value)(), so the [ref] variable keeps its PREVIOUS value on failure" `
            -Remedy "test it: if (`$x.$($i.Member.Value)([ref]`$out)) { ... } - and a bare call leaks its boolean into the output stream"
    }
}

# Was the [ref] target given a value earlier on this same line?
function Test-InitialisedOnSameLine([hashtable]$Ix, [Ast]$Call, [string]$Target) {
    $line = $Call.Extent.StartLineNumber
    $wanted = $Target.ToLowerInvariant()
    foreach ($a in $Ix.Assign) {
        if ($a.Extent.StartLineNumber -ne $line) { continue }
        if ($a.Extent.StartOffset -ge $Call.Extent.StartOffset) { continue }
        $name = Get-VarName $a.Left
        if ($name -and $name.ToLowerInvariant() -eq $wanted) { return $true }
    }
    return $false
}

# Is this expression's value dropped on the floor? A bare statement drops it; an assignment, a condition,
# an argument, a member access or a return consumes it.
function Test-ResultDiscarded([Ast]$Node) {
    $current = $Node
    while ($null -ne $current.Parent) {
        $parent = $current.Parent
        if ($parent -is [AssignmentStatementAst] -or $parent -is [IfStatementAst] -or $parent -is [WhileStatementAst] `
            -or $parent -is [DoWhileStatementAst] -or $parent -is [DoUntilStatementAst] -or $parent -is [UnaryExpressionAst] `
            -or $parent -is [BinaryExpressionAst] -or $parent -is [CommandAst] -or $parent -is [ReturnStatementAst] `
            -or $parent -is [InvokeMemberExpressionAst] -or $parent -is [MemberExpressionAst] -or $parent -is [IndexExpressionAst]) {
            return $false
        }
        if ($parent -is [PipelineAst]) {
            # A bare pipeline statement inside a block: nothing consumes what it produced.
            return ($parent.Parent -is [StatementBlockAst] -or $parent.Parent -is [NamedBlockAst])
        }
        $current = $parent
    }
    return $true
}

# RULE 10 - `param` used as a COMMAND. `param(...)` is a DECLARATION only as the first statement of a
# script, function or scriptblock. Anywhere else the parser reads it as a call to a command named `param`,
# and nothing says so: the block has no parameters, every name inside it is $null, and the failure surfaces
# wherever those values were supposed to be used. Nothing legitimately invokes a command called `param`.
function Test-ParamAsCommand([string]$Rel, [hashtable]$Ix) {
    foreach ($c in $Ix.Command) {
        $name = Get-CommandName $c
        if (-not $name) { continue }
        if ($name.ToLowerInvariant() -ne 'param') { continue }
        Add-Finding -Rel $Rel -Node $c -What 'invokes `param` as a COMMAND, so this is not a parameter declaration' `
            -Remedy 'move param(...) to the FIRST statement of the function or scriptblock - the block currently takes no parameters'
    }
}

# RULE 11 - a member access STRANDED in argument mode. In a command call the space is an argument separator,
# so `f $x {...} .GetNewClosure()` passes the bareword `.GetNewClosure` as an EXTRA argument and the member
# never runs: the scriptblock arrives without its closure, the callee gets one argument more than it
# expects, and the parse is clean. Same shape when the member is continued on the next line.
#
# Reported only where the bareword cannot be a path: the element before it is an EXPRESSION - the thing the
# member was meant for - and the name is PascalCase. That is what separates `.Length` from `.gitignore`
# after a variable, and every member on these objects is PascalCase while no dotfile is.
function Test-StrandedMember([string]$Rel, [hashtable]$Ix) {
    $remedy = 'attach it to its target with no space (`$x.Member`), or wrap the target in parentheses'

    foreach ($c in $Ix.Command) {
        $elements = $c.CommandElements
        for ($i = 0; $i -lt $elements.Count; $i++) {
            $element = $elements[$i]
            if ($element -isnot [StringConstantExpressionAst]) { continue }
            if ($element.StringConstantType -ne [StringConstantType]::BareWord) { continue }
            if (-not (Test-MemberBareword $element.Value)) { continue }
            # Element 0 is the command NAME: a member access that starts a statement, which is the
            # line-continuation form of the same mistake.
            if ($i -gt 0 -and -not (Test-MemberTarget $elements[$i - 1])) { continue }

            Add-Finding -Rel $Rel -Node $element `
                -What "passes the member $($element.Value) as an ARGUMENT - the space made it a bareword, so the member never runs" `
                -Remedy $remedy
        }
    }
}

# A bareword that is a .NET MEMBER name rather than a path. `.gitignore` and `.\src` are paths and are left
# alone; the second character decides it, because a member on these objects is PascalCase and a dotfile is
# not. A name of one character (`.`, the dot-source operator) is a path too.
function Test-MemberBareword([string]$Text) {
    if ($Text.Length -lt 2) { return $false }
    if (-not $Text.StartsWith('.')) { return $false }
    if (-not [char]::IsLetter($Text[1])) { return $false }
    $second = $Text.Substring(1, 1)
    return $second -ceq $second.ToUpperInvariant()
}

# Could this element be the TARGET of a member access? Only an expression can: a bareword before the dot is
# a command name or another argument, and a `-Parameter` is neither.
function Test-MemberTarget([Ast]$Node) {
    return ($Node -is [VariableExpressionAst] -or $Node -is [ScriptBlockExpressionAst] `
        -or $Node -is [ParenExpressionAst] -or $Node -is [MemberExpressionAst] `
        -or $Node -is [ArrayExpressionAst] -or $Node -is [SubExpressionAst] `
        -or $Node -is [HashtableAst] -or $Node -is [IndexExpressionAst])
}

# RULE 12 - `Add-Type -MemberDefinition` on the STARTUP path. That parameter COMPILES: csc runs on the
# snippet, measured at 240-280 ms, and a call at STATEMENT level pays it on every launch - before the first
# window appears, whether the P/Invoke is ever called or not.
#
# DEFERRED means a function OR A SCRIPTBLOCK LITERAL, and the scriptblock had to be measured: `$body = {...}`
# assigns, it does not run, so a worker body that compiles its own P/Invoke inside a runspace costs the
# startup path nothing. Counting it was this rule's one false positive on a real repo - in a WinForms app's
# worker script, an `Add-Type` guarded by `if (-not ('X' -as [type]))` inside
# a `$script:` scriptblock. What is left is the shape nothing can defer: a call at statement level.
function Test-EagerAddType([string]$Rel, [hashtable]$Ix) {
    foreach ($c in $Ix.Command) {
        $name = Get-CommandName $c
        if (-not $name) { continue }
        if ($name.ToLowerInvariant() -ne 'add-type') { continue }
        if (-not (Test-HasParameter $c 'memberdefinition')) { continue }
        if ($null -ne (Get-DeferringScope $c)) { continue }

        Add-Finding -Rel $Rel -Node $c `
            -What 'compiles a -MemberDefinition on the STARTUP path - a csc run (~250 ms) before the first window' `
            -Remedy 'move it into a function or a scriptblock the first caller runs (Register-NativeType), so it is paid on use'
    }
}

# Was this parameter passed? By PREFIX, because `-Member` binds -MemberDefinition just as well, and a rule
# that only knew the full spelling would miss the shape people actually write.
function Test-HasParameter([CommandAst]$Command, [string]$Name) {
    foreach ($element in $Command.CommandElements) {
        if ($element -isnot [CommandParameterAst]) { continue }
        $written = $element.ParameterName.ToLowerInvariant()
        if ($written.Length -lt 3) { continue }
        if ($Name.StartsWith($written)) { return $true }
    }
    return $false
}

# The scope that DEFERS this node's cost: a function, or a scriptblock literal. $null when the node sits at
# statement level, where the cost is paid the moment the file is loaded. A scriptblock counts because
# ASSIGNING one does not run it - the body pays when something invokes it, which is what a worker body is.
function Get-DeferringScope([Ast]$Node) {
    $current = $Node.Parent
    while ($null -ne $current) {
        if ($current -is [FunctionDefinitionAst] -or $current -is [ScriptBlockExpressionAst]) { return $current }
        $current = $current.Parent
    }
    return $null
}
