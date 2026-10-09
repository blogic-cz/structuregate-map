<#
    PsGate.Rules.WinForms.ps1 - the three rules a VIRTUAL ListView breaks, and the reason they are here.

    `VirtualMode = $true` changes the contract of the control: the list no longer OWNS its rows. It asks for
    each visible one through RetrieveVirtualItem, keeps nothing, and quietly stops honouring the members
    that assume it does. None of that throws. The list draws, the code compiles, and the state it reports is
    wrong - which is exactly the failure shape this gate exists for.

    All three read the property assignments the single walk in PsGate.ps1 recorded, and all three FAIL the
    build. The flag is only decidable where it is a literal `$true` in the same file, so these rules are
    narrow by construction - a list configured somewhere else is invisible to them and stays that way.
    Nothing here reaches across files to guess. The one shape that is correct with the box on - a mouse
    handler toggling the state image - is read off the tree; a helper named in $script:VirtualCheckHelpers
    is still accepted by name.
#>
using namespace System.Management.Automation.Language

# RULE 13 - `CheckBoxes = $true` on a list that is also `VirtualMode = $true`. The box is drawn from the item
# `RetrieveVirtualItem` returns, but a click on it raises no ItemCheck and toggles nothing: the user clicks and
# the state stays. The toggle has to be done by hand - a mouse handler on the list that hit-tests the STATE
# IMAGE, and that fix NEEDS `CheckBoxes = $true` - so a list that has one, or is handed to a function in this
# file that attaches one, or calls a helper in $script:VirtualCheckHelpers, is left alone.
function Test-VirtualCheckBoxes([string]$Rel, [hashtable]$Ix) {
    $toggled = Get-StateImageOwners $Ix
    foreach ($owner in $Ix.TrueProps.psbase.Keys) {
        $props = $Ix.TrueProps[$owner]
        if (-not $props.ContainsKey('virtualmode')) { continue }
        if (-not $props.ContainsKey('checkboxes')) { continue }
        if ($Ix.Helped[$owner] -or $toggled.Contains($owner)) { continue }

        # Reported with the name AS WRITTEN, off the node: the index keys are lowercase so two spellings are
        # one list, and `$lvfiles` in a message about `$lvFiles` is a name the reader has to translate.
        $node = $props['checkboxes']
        Add-Finding -Rel $Rel -Node $node `
            -What "sets CheckBoxes on `$$($node.Expression.VariablePath.UserPath), which is also VirtualMode - a click on a virtual list's check box toggles nothing" `
            -Remedy 'toggle it in a MouseDown handler on the list that hit-tests ListViewHitTestLocations.StateImage, or drop VirtualMode'
    }
}

# Every list variable (lowercase) a mouse handler hit-testing the STATE IMAGE is attached to: `$lv.Add_MouseDown({
# ... [ListViewHitTestLocations]::StateImage ... })` directly, or inside a function of this file the list is handed
# to - the handler is then on the function's parameter, and every variable a call passes it counts.
function Get-StateImageOwners([hashtable]$Ix) {
    $owners = [System.Collections.Generic.HashSet[string]]::new()
    $functions = [System.Collections.Generic.HashSet[string]]::new()
    foreach ($i in $Ix.Invoke) {
        if ($i.Member -isnot [StringConstantExpressionAst]) { continue }
        if (@('add_mousedown', 'add_mouseclick', 'add_mouseup') -notcontains $i.Member.Value.ToLowerInvariant()) { continue }
        if ($i.Expression -isnot [VariableExpressionAst] -or -not $i.Arguments) { continue }
        $hits = foreach ($a in $i.Arguments) { $a.FindAll({ param($x) Test-StateImage $x }, $true) }
        if (-not $hits) { continue }
        $name = $i.Expression.VariablePath.UserPath.ToLowerInvariant()
        [void]$owners.Add($name)
        for ($at = $i.Parent; $at; $at = $at.Parent) {
            if ($at -isnot [FunctionDefinitionAst]) { continue }
            $params = @($at.Parameters) + @(if ($at.Body.ParamBlock) { $at.Body.ParamBlock.Parameters })
            if ($params | Where-Object { $_ -and $_.Name.VariablePath.UserPath.ToLowerInvariant() -eq $name }) {
                [void]$functions.Add($at.Name.ToLowerInvariant())
            }
            break
        }
    }
    foreach ($c in $Ix.Command) {
        $called = Get-CommandName $c
        if (-not $called -or -not $functions.Contains($called.ToLowerInvariant())) { continue }
        foreach ($element in $c.CommandElements) {
            if ($element -is [VariableExpressionAst]) { [void]$owners.Add($element.VariablePath.UserPath.ToLowerInvariant()) }
        }
    }
    return , $owners
}

# `[System.Windows.Forms.ListViewHitTestLocations]::StateImage`, by the type's name and the member read off it.
function Test-StateImage([Ast]$Node) {
    if ($Node -isnot [MemberExpressionAst] -or -not $Node.Static) { return $false }
    if ($Node.Member -isnot [StringConstantExpressionAst] -or $Node.Member.Value -ne 'StateImage') { return $false }
    if ($Node.Expression -isnot [TypeExpressionAst]) { return $false }
    $type = $Node.Expression.TypeName.Name
    return $type -eq 'ListViewHitTestLocations' -or $type.EndsWith('.ListViewHitTestLocations')
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
