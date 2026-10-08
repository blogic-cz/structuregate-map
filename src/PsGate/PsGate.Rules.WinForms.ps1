<#
    PsGate.Rules.WinForms.ps1 - the three rules a VIRTUAL ListView breaks, and the reason they are here.

    `VirtualMode = $true` changes the contract of the control: the list no longer OWNS its rows. It asks for
    each visible one through RetrieveVirtualItem, keeps nothing, and quietly stops honouring the members
    that assume it does. None of that throws. The list draws, the code compiles, and the state it reports is
    wrong - which is exactly the failure shape this gate exists for.

    All three read the property assignments the single walk in PsGate.ps1 recorded, and all three FAIL the
    build. The flag is only decidable where it is a literal `$true` in the same file, so these rules are
    narrow by construction - a list configured somewhere else is invisible to them and stays that way.
    Nothing here reaches across files to guess, and the one shape that is correct with the box on (the
    helper that draws it) is looked for by name.
#>
using namespace System.Management.Automation.Language

# RULE 13 - `CheckBoxes = $true` on a list that is also `VirtualMode = $true`. A virtual list draws
# NO check box: the property is accepted, the column is empty, and the user has nothing to click. The state
# has to be drawn and tracked by hand, which is what the helper in $script:VirtualCheckHelpers does - so a
# list that calls it is left alone.
function Test-VirtualCheckBoxes([string]$Rel, [hashtable]$Ix) {
    foreach ($owner in $Ix.TrueProps.psbase.Keys) {
        $props = $Ix.TrueProps[$owner]
        if (-not $props.ContainsKey('virtualmode')) { continue }
        if (-not $props.ContainsKey('checkboxes')) { continue }
        if ($Ix.Helped[$owner]) { continue }

        # Reported with the name AS WRITTEN, off the node: the index keys are lowercase so two spellings are
        # one list, and `$lvfiles` in a message about `$lvFiles` is a name the reader has to translate.
        $node = $props['checkboxes']
        Add-Finding -Rel $Rel -Node $node `
            -What "sets CheckBoxes on `$$($node.Expression.VariablePath.UserPath), which is also VirtualMode - a virtual list draws no check box" `
            -Remedy 'drop VirtualMode, or draw and track the check yourself (Enable-VirtualCheck) - the column is empty as written'
    }
}

# RULE 14 - `Add_ItemCheck` on a virtual list. The event is raised by the control's own check-box
# handling, which a virtual list does not have, so the handler is dead code by definition: it never fires,
# and the code that depends on it reads the state it was supposed to maintain.
function Test-VirtualItemCheck([string]$Rel, [hashtable]$Ix) {
    foreach ($i in $Ix.Invoke) {
        if ($i.Member -isnot [StringConstantExpressionAst]) { continue }
        if ($i.Member.Value -ne 'Add_ItemCheck') { continue }
        $owner = Get-VirtualOwner $Ix $i.Expression
        if (-not $owner) { continue }

        Add-Finding -Rel $Rel -Node $i `
            -What "adds an ItemCheck handler to `$$owner, which is VirtualMode - the event is never raised" `
            -Remedy 'handle the click on the check area yourself, or drop VirtualMode - this handler is dead code'
    }
}

# RULE 15 - `.Items`, `.CheckedItems` or `.CheckedIndices` on a virtual list. The list holds no
# items, so `.Items.Count` is 0 while rows are on screen and `.CheckedItems` answers out of the same empty
# collection. Both LIE rather than fail, which is why a count taken from here disagrees with what the user
# sees.
function Test-VirtualMember([string]$Rel, [hashtable]$Ix) {
    foreach ($m in $Ix.Member) {
        if ($m.Member -isnot [StringConstantExpressionAst]) { continue }
        if ($script:VirtualMembers -notcontains $m.Member.Value.ToLowerInvariant()) { continue }
        $owner = Get-VirtualOwner $Ix $m.Expression
        if (-not $owner) { continue }

        Add-Finding -Rel $Rel -Node $m `
            -What "reads .$($m.Member.Value) on `$$owner, which is VirtualMode - the list keeps no items" `
            -Remedy 'answer from your own backing list - .Items is empty and .CheckedItems counts out of it'
    }
}

# The variable this member access is on, but ONLY when the file made it a virtual list. $null otherwise,
# which is what keeps these rules silent on an ordinary ListView - the common case by far.
function Get-VirtualOwner([hashtable]$Ix, [Ast]$Node) {
    if ($Node -isnot [VariableExpressionAst]) { return $null }
    $name = $Node.VariablePath.UserPath
    $props = $Ix.TrueProps[$name.ToLowerInvariant()]
    if ($null -eq $props) { return $null }
    if (-not $props.ContainsKey('virtualmode')) { return $null }
    # The name AS WRITTEN: the index is keyed lowercase, and a message naming `$lvfiles` for `$lvFiles`
    # sends the reader looking for a variable that is not in the file.
    return $name
}
