<#
    A GENERIC IS READ AS IT IS INSTANTIATED (#98, `TsTpl/TsTplReads.mjs::resolveRef`): `Concrete extends Base<Payload>` binds
    `Base`'s `T`, and `Wrapped: Wrapper<T>` binds `Wrapper`'s own parameter to it, so a template read past a member typed by a
    type parameter resolves. Rows: `classes.type_params`, a type_ref's `param` and `args` (`TsDecls/TsTypeRef.mjs`). Its helpers
    are `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '../../TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

$script:TsGenericFiles = @{
    'apps/shop/src/generic.component.ts' = "import { Component } from '@angular/core';`n" +
        "export interface Flags { On: boolean; }`n" +
        "export interface Payload { Flags: Flags; }`n" +
        "export interface PageModel { Title: string; }`n" +
        "export class Wrapper<T> { Inner!: T; }`n" +
        "export class Base<T> { Wrapped!: Wrapper<T>; }`n" +
        "export class Concrete extends Base<Payload> {}`n" +
        "export abstract class BasePage<TModel> { model!: TModel; }`n" +
        "@Component({ selector: 'app-generic', template: '<p>{{ c.Wrapped.Inner.Flags.On }}</p><p>{{ model.Title }}</p>' })`n" +
        "export class GenericComponent extends BasePage<PageModel> {`n" +
        "  c = new Concrete();`n" +
        "}`n"
}

function Get-TsGenericTargets([string]$Db, [string]$Source) {
    return Invoke-TsRowsQ $Db ("SELECT 'targets=' || group_concat(n.value, ',') AS r FROM (SELECT j.owner, j.n FROM expressions x " +
        "JOIN expressions__ast j ON j.owner = x.id AND j.role = 'target' WHERE x.source = '$Source' ORDER BY j.n) t " +
        "JOIN expressions__ast n ON n.owner = t.owner AND n.parent = t.n AND n.role = 'name'")
}

Test-Case 'tsrows: a read through a generic base and a generic member binds the type arguments (#98)' {
    $made = New-TsRowsDb (New-TsRowsWorkspace $script:TsGenericFiles)
    Assert-Exit $made.Result 0
    # `Wrapped` is Base's, `Inner` is Wrapper's `T` - bound through `extends Base<Payload>` and `Wrapper<T>`.
    Assert-Line (Get-TsGenericTargets $made.Db '{{ c.Wrapped.Inner.Flags.On }}') 'targets=c,Wrapped,Inner,Flags,On'
    # A component extending a generic base component: `model: TModel` is PageModel.
    Assert-Line (Get-TsGenericTargets $made.Db '{{ model.Title }}') 'targets=model,Title'
}

}
