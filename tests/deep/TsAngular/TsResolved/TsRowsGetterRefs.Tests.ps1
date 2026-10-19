<#
    A GETTER WITHOUT AN ANNOTATION HAS THE TYPE THE CHECKER INFERS (#95, `TsDecls/TsClassRows.mjs::memberRow`): its `members`
    row carries `type` and `type_ref` as an un-annotated property's does, so a template read chained off it resolves past it.
    Its helpers are `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '../../TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

$script:TsGetterFiles = @{
    'apps/shop/src/vm.component.ts' = "import { Component } from '@angular/core';`n" +
        "export interface Inner { Flag: boolean; }`n" +
        "export interface Outer { Inner: Inner; }`n" +
        "class Holder { get<T>(): T { return {} as T; } }`n" +
        "@Component({ selector: 'app-vm', template: '<p>{{ vm?.Inner.Flag }}</p><p>{{ pair.Inner.Flag }}</p><p>{{ named.Inner.Flag }}</p>' })`n" +
        "export class VmComponent {`n" +
        "  holder = new Holder();`n" +
        "  private outer!: Outer;`n" +
        "  get vm() { return this.holder.get<Outer>(); }`n" +
        "  get pair() { return this.outer; }`n" +
        "  set pair(v: Outer) { this.outer = v; }`n" +
        "  get named(): Outer { return this.outer; }`n" +
        "}`n"
}

# The names an expression's `Read` nodes resolved, in order of their node number.
function Get-TsGetterTargets([string]$Db, [string]$Source) {
    $r = Invoke-TsRowsQ $Db ("SELECT 'targets=' || group_concat(n.value, ',') AS r FROM (SELECT j.owner, j.n FROM expressions x " +
        "JOIN expressions__ast j ON j.owner = x.id AND j.role = 'target' WHERE x.source = '$Source' ORDER BY j.n) t " +
        "JOIN expressions__ast n ON n.owner = t.owner AND n.parent = t.n AND n.role = 'name'")
    return $r
}

Test-Case 'tsrows: a getter without an annotation is the type the checker infers, and template reads resolve past it (#95)' {
    $made = New-TsRowsDb (New-TsRowsWorkspace $script:TsGetterFiles)
    Assert-Exit $made.Result 0
    $types = Invoke-TsRowsQ $made.Db ("SELECT 'type=' || m.name || ':' || m.kind || ':' || coalesce(m.type, '-') AS r FROM members m " +
        "WHERE m.name IN ('vm', 'pair', 'named') ORDER BY m.name, m.kind")
    # Inferred, never a function type: an accessor's symbol is the property's.
    Assert-Line $types 'type=vm:getter:Outer'
    Assert-Line $types 'type=pair:getter:Outer'
    Assert-Line $types 'type=named:getter:Outer'
    Assert-Line (Get-TsGetterTargets $made.Db '{{ vm?.Inner.Flag }}') 'targets=vm,Inner,Flag'
    Assert-Line (Get-TsGetterTargets $made.Db '{{ pair.Inner.Flag }}') 'targets=pair,Inner,Flag'
    # The annotated getter resolved before the fix too - the control.
    Assert-Line (Get-TsGetterTargets $made.Db '{{ named.Inner.Flag }}') 'targets=named,Inner,Flag'
}

}
