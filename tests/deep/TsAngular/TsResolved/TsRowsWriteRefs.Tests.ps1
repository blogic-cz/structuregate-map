<#
    A TEMPLATE WRITE NAMES THE MEMBER IT WRITES (#99, `TsTpl/TsTplReads.mjs::chainOf`): `(valueChange)="f.On = $event"` gives its `Write`
    node the `target` the `Read` of `f.On` in an input binding gets. Its helpers are `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '../../TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

$script:TsWriteFiles = @{
    'apps/shop/src/write.component.ts' = "import { Component, EventEmitter, Input, Output } from '@angular/core';`n" +
        "export interface Flags { On: boolean; }`n" +
        "@Component({ selector: 'app-check', template: '' })`n" +
        "export class CheckComponent { @Input() value = false; @Output() valueChange = new EventEmitter<boolean>(); }`n" +
        "@Component({ selector: 'app-write', imports: [CheckComponent], template: '<app-check [value]=`"f.On`" (valueChange)=`"f.On = `$event`"></app-check><app-check (valueChange)=`"done = `$event`"></app-check>' })`n" +
        "export class WriteComponent {`n  f: Flags = { On: false };`n  done = false;`n}`n"
}

# The member each Write node of a handler targets, `name@line`.
function Get-TsWriteTargets([string]$Db) {
    return Invoke-TsRowsQ $Db ("SELECT 'write=' || n.value || '@' || l.value AS r FROM expressions x " +
        "JOIN expressions__ast w ON w.owner = x.id AND w.role = 'k' AND w.value = 'Write' " +
        "JOIN expressions__ast t ON t.owner = x.id AND t.parent = w.parent AND t.role = 'target' " +
        "JOIN expressions__ast n ON n.owner = x.id AND n.parent = t.n AND n.role = 'name' " +
        "JOIN expressions__ast l ON l.owner = x.id AND l.parent = t.n AND l.role = 'line' ORDER BY r")
}

Test-Case 'tsrows: a template write carries the target of the member it writes (#99)' {
    $made = New-TsRowsDb (New-TsRowsWorkspace $script:TsWriteFiles)
    Assert-Exit $made.Result 0
    $w = Get-TsWriteTargets $made.Db
    # `f.On = $event` writes Flags.On (line 2); `done = $event` the component's own member (line 8).
    Assert-Line $w 'write=On@2'
    Assert-Line $w 'write=done@8'
}

}
