<#
    A MEMBER A TYPE ALIAS BORROWS FROM A GENERIC IS TYPED AS THE ALIAS INSTANTIATES IT (#96, `TsDecls/TsTypeRows.mjs::typeMemberRowsFromType`):
    `type PayloadBox = Box<Payload>` gives its computed member `Payload` the type `Payload`, never `Box`'s type parameter, so a
    template read past it resolves. Its helpers are `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '../../TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

$script:TsComputedFiles = @{
    'apps/shop/src/box.component.ts' = "import { Component } from '@angular/core';`n" +
        "export interface Flags { On: boolean; }`n" +
        "export interface Payload { Flags: Flags; }`n" +
        "export class Box<TPayload> { Payload!: TPayload; Note!: string; }`n" +
        "export type PayloadBox = Box<Payload>;`n" +
        "class Holder { get<T>(): T { return {} as T; } }`n" +
        "@Component({ selector: 'app-box', template: '<p>{{ vm?.Payload.Flags.On }}</p>' })`n" +
        "export class BoxComponent {`n" +
        "  holder = new Holder();`n" +
        "  get vm() { return this.holder.get<PayloadBox>(); }`n" +
        "}`n"
}

Test-Case 'tsrows: a member an alias borrows from a generic is typed as the alias instantiates it, and reads resolve past it (#96)' {
    $made = New-TsRowsDb (New-TsRowsWorkspace $script:TsComputedFiles)
    Assert-Exit $made.Result 0
    $members = Invoke-TsRowsQ $made.Db ("SELECT 'tm=' || m.name || ':' || coalesce(m.type, '-') AS r FROM type_members m " +
        "JOIN type_aliases a ON a.id = m.owner WHERE a.name = 'PayloadBox' AND m.via = 'computed' ORDER BY m.name")
    Assert-Line $members 'tm=Payload:Payload'
    # A member nothing instantiates keeps its annotation's text.
    Assert-Line $members 'tm=Note:string'
    $r = Invoke-TsRowsQ $made.Db ("SELECT 'targets=' || group_concat(n.value, ',') AS r FROM (SELECT j.owner, j.n FROM expressions x " +
        "JOIN expressions__ast j ON j.owner = x.id AND j.role = 'target' WHERE x.source = '{{ vm?.Payload.Flags.On }}' ORDER BY j.n) t " +
        "JOIN expressions__ast n ON n.owner = t.owner AND n.parent = t.n AND n.role = 'name'")
    Assert-Line $r 'targets=vm,Payload,Flags,On'
}

}
