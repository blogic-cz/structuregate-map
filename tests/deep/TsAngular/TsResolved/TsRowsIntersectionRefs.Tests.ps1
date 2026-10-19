<#
    AN INTERSECTION NAMES WHAT ITS PARTS NAME (#97, `TsDecls/TsTypeRef.mjs::refsOfParts`), and a template read follows a member
    whose `type_refs` lists several types, resolving a name ONE of them declares (`TsTpl/TsTplReads.mjs::ownerOfRow`). A hop whose
    type is unknown ends the chain - it kept the previous owner and named that owner's member. Its helpers are `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '../../TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

$script:TsIntersectionFiles = @{
    'apps/shop/src/box.component.ts' = "import { Component } from '@angular/core';`n" +
        "export interface Flags { On: boolean; }`n" +
        "export interface Payload { Flags: Flags; }`n" +
        "export interface Extra { Note: string; }`n" +
        "export class Box<TPayload> { Payload!: TPayload & { Extra?: Extra; }; Both!: TPayload & Extra; }`n" +
        "export type PayloadBox = Box<Payload>;`n" +
        "class Holder { get<T>(): T { return {} as T; } }`n" +
        "@Component({ selector: 'app-box', template: '<p>{{ vm?.Payload.Flags.On }}</p><p>{{ vm?.Both.Note }}</p><p>{{ vm?.Both.Flags.On }}</p><p>{{ loose.Note }}</p>' })`n" +
        "export class BoxComponent {`n" +
        "  holder = new Holder();`n" +
        "  get vm() { return this.holder.get<PayloadBox>(); }`n" +
        "  loose: any;`n" +
        "  Note = 'own';`n" +
        "}`n"
}

# The names an expression's `Read` nodes resolved, in node order.
function Get-TsIntersectionTargets([string]$Db, [string]$Source) {
    return Invoke-TsRowsQ $Db ("SELECT 'targets=' || group_concat(n.value, ',') AS r FROM (SELECT j.owner, j.n FROM expressions x " +
        "JOIN expressions__ast j ON j.owner = x.id AND j.role = 'target' WHERE x.source = '$Source' ORDER BY j.n) t " +
        "JOIN expressions__ast n ON n.owner = t.owner AND n.parent = t.n AND n.role = 'name'")
}

Test-Case 'tsrows: an intersection names what its parts name, and reads resolve past it (#97)' {
    $made = New-TsRowsDb (New-TsRowsWorkspace $script:TsIntersectionFiles)
    Assert-Exit $made.Result 0
    # ONE named part: `Payload & { Extra?: Extra }` names Payload.
    Assert-Line (Get-TsIntersectionTargets $made.Db '{{ vm?.Payload.Flags.On }}') 'targets=vm,Payload,Flags,On'
    # TWO named parts: `Payload & Extra` lists both, and a name one of them declares resolves.
    Assert-Line (Get-TsIntersectionTargets $made.Db '{{ vm?.Both.Note }}') 'targets=vm,Both,Note'
    Assert-Line (Get-TsIntersectionTargets $made.Db '{{ vm?.Both.Flags.On }}') 'targets=vm,Both,Flags,On'
}

Test-Case 'tsrows: a hop of unknown type ends the read chain, never naming the previous owner''s member' {
    $made = New-TsRowsDb (New-TsRowsWorkspace $script:TsIntersectionFiles)
    Assert-Exit $made.Result 0
    # `loose: any` - its `Note` is not the component's own `Note`.
    $loose = Get-TsIntersectionTargets $made.Db '{{ loose.Note }}'
    Assert-Line $loose 'targets=loose'
    Assert-NoLine $loose 'loose,Note'
}

}
