<#
    A TEMPLATE READ OF AN ENUM A FIELD HOLDS RESOLVES TO THE MEMBER (`TsTpl/TsTplReads.mjs`, enum owners): `Kinds = Kind` is typed
    `typeof Kind`, and `{{ Kinds.Alpha }}` targets `Alpha` at its line, the enum's row. An enum member carries its `line`
    (`TsDecls/TsTypeRows.mjs::enumRow`). Its helpers are `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '../../TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

$script:TsEnumFiles = @{
    'apps/shop/src/kinds.component.ts' = "import { Component } from '@angular/core';`n" +
        "export enum Kind {`n  Alpha = 'a',`n  Beta = 'b',`n}`n" +
        "@Component({ selector: 'app-kinds', template: '<p>{{ Kinds.Alpha }}</p><p>{{ Kinds.Gamma }}</p>' })`n" +
        "export class KindsComponent {`n  Kinds = Kind;`n}`n"
}

function Get-TsEnumTargets([string]$Db, [string]$Source) {
    return Invoke-TsRowsQ $Db ("SELECT 'targets=' || group_concat(n.value || '@' || l.value, ',') AS r FROM (SELECT j.owner, j.n FROM expressions x " +
        "JOIN expressions__ast j ON j.owner = x.id AND j.role = 'target' WHERE x.source = '$Source' ORDER BY j.n) t " +
        "JOIN expressions__ast n ON n.owner = t.owner AND n.parent = t.n AND n.role = 'name' " +
        "JOIN expressions__ast l ON l.owner = t.owner AND l.parent = t.n AND l.role = 'line'")
}

Test-Case 'tsrows: a template read of an enum a field holds resolves to the member, at its line' {
    $made = New-TsRowsDb (New-TsRowsWorkspace $script:TsEnumFiles)
    Assert-Exit $made.Result 0
    # `Kinds` is the field (line 8), `Alpha` the member (line 3).
    Assert-Line (Get-TsEnumTargets $made.Db '{{ Kinds.Alpha }}') 'targets=Kinds@8,Alpha@3'
    # A name the enum does not declare resolves no further than the field.
    $gamma = Get-TsEnumTargets $made.Db '{{ Kinds.Gamma }}'
    Assert-Line $gamma 'targets=Kinds@8'
    Assert-NoLine $gamma 'Gamma'
}

}
