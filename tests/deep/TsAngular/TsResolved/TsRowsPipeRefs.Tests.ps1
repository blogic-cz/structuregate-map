<#
    A template's pipe usage carries the pipe Angular RUNS (#86, `TsTpl/TsPipeRefs.mjs`): `pipe_refs` beside the name
    list, resolved by the scope of the template's component - its NgModule's declarations and what its imports export, or a
    standalone component's own `imports` - never by the name alone. A library pipe (`date` through `CommonModule` or a
    standalone `DatePipe`) is `external`, two of one name in scope are `ambiguous`. Its helpers are `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '../../TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

# TWO MODULES, EACH DECLARING ITS OWN `demoFormat`: by name alone either template could run either pipe.
$script:TsPipeFiles = @{
    'apps/shop/src/alpha/alpha-format.pipe.ts' = "import { Pipe, PipeTransform } from '@angular/core';`n" +
        "@Pipe({ name: 'demoFormat' })`nexport class AlphaFormatPipe implements PipeTransform {`n" +
        "  transform(v: string): string { return v; }`n}`n"
    'apps/shop/src/alpha/alpha.component.ts' = "import { Component } from '@angular/core';`n" +
        "@Component({ selector: 'demo-alpha', template: '<b>{{ x | demoFormat }}</b><i>{{ when | date }}</i><u>{{ \'a.b\' | demoFormat }}</u>' })`n" +
        "export class AlphaComponent {`n  x = 'a';`n  when = new Date();`n}`n"
    'apps/shop/src/alpha/alpha.module.ts' = "import { NgModule } from '@angular/core';`n" +
        "import { CommonModule } from '@angular/common';`n" +
        "import { AlphaFormatPipe } from './alpha-format.pipe';`n" +
        "import { AlphaComponent } from './alpha.component';`n" +
        "@NgModule({ imports: [CommonModule], declarations: [AlphaFormatPipe, AlphaComponent], exports: [AlphaFormatPipe] })`n" +
        "export class AlphaModule {}`n"
    'apps/shop/src/beta/beta-format.pipe.ts' = "import { Pipe, PipeTransform } from '@angular/core';`n" +
        "@Pipe({ name: 'demoFormat', standalone: true })`nexport class BetaFormatPipe implements PipeTransform {`n" +
        "  transform(v: string): string { return v + '!'; }`n}`n"
    'apps/shop/src/beta/beta.component.ts' = "import { Component } from '@angular/core';`n" +
        "@Component({ selector: 'demo-beta', template: '<b>{{ y | demoFormat }}</b><i>{{ y | missingFormat }}</i>' })`n" +
        "export class BetaComponent {`n  y = 'b';`n}`n"
    'apps/shop/src/beta/beta.module.ts' = "import { NgModule } from '@angular/core';`n" +
        "import { BetaFormatPipe } from './beta-format.pipe';`n" +
        "import { BetaComponent } from './beta.component';`n" +
        "@NgModule({ imports: [BetaFormatPipe], declarations: [BetaComponent] })`n" +
        "export class BetaModule {}`n"
    'apps/shop/src/order.component.ts' = "import { Component } from '@angular/core';`n" +
        "import { DatePipe } from '@angular/common';`n" +
        "import { BetaFormatPipe as Format } from './beta/beta-format.pipe';`n" +
        "@Component({ selector: 'demo-order', standalone: true, imports: [Format, DatePipe], template: '<b>{{ z | demoFormat }}</b><i>{{ at | date }}</i>' })`n" +
        "export class OrderComponent {`n  z = 'o';`n  at = new Date();`n}`n"
    'apps/shop/src/mix.component.ts' = "import { Component } from '@angular/core';`n" +
        "import { AlphaModule } from './alpha/alpha.module';`n" +
        "import { BetaFormatPipe } from './beta/beta-format.pipe';`n" +
        "@Component({ selector: 'demo-mix', standalone: true, imports: [AlphaModule, BetaFormatPipe], template: '<b>{{ m | demoFormat }}</b>' })`n" +
        "export class MixComponent {`n  m = 'm';`n}`n"
}

# Each usage as `<component>:<name>:<pipes row's class or ->:<external>:<ambiguous>:<library class or ->`, from the node rows.
function Get-TsPipeRefs([string]$Db, [string]$Table = 'expressions') {
    $nodes = "${Table}__pipe_refs"
    return Invoke-TsRowsQ $Db ("SELECT 'ref=' || k.name || ':' || $(Get-TsFieldSql $nodes 'r' 'name') || ':' || " +
        "coalesce((SELECT p.name FROM pipes p WHERE p.id = $(Get-TsFieldSql $nodes 'r' 'id')), '-') || ':' || " +
        "coalesce($(Get-TsFieldSql $nodes 'r' 'external'), 0) || ':' || coalesce($(Get-TsFieldSql $nodes 'r' 'ambiguous'), 0) || " +
        "':' || coalesce($(Get-TsFieldSql $nodes 'r' 'class'), '-') AS r " +
        "FROM $Table e JOIN components k ON k.id = e.component JOIN $nodes r ON r.owner = e.id AND r.parent = 0")
}

Test-Case 'tsrows: a template pipe usage carries the pipe its own scope resolves, never another of the same name' {
    $made = New-TsRowsDb (New-TsRowsWorkspace $script:TsPipeFiles) -Shared
    Assert-Exit $made.Result 0
    $refs = Get-TsPipeRefs $made.Db
    # The NgModule's own declaration; the standalone pipe a module IMPORTS; a standalone component's own `imports` (an alias).
    Assert-Line $refs 'ref=AlphaComponent:demoFormat:AlphaFormatPipe:0:0:-'
    Assert-Line $refs 'ref=BetaComponent:demoFormat:BetaFormatPipe:0:0:-'
    Assert-Line $refs 'ref=OrderComponent:demoFormat:BetaFormatPipe:0:0:-'
    Assert-NoLine $refs 'ref=AlphaComponent:demoFormat:BetaFormatPipe'
    Assert-NoLine $refs 'ref=BetaComponent:demoFormat:AlphaFormatPipe'
    # A string through the pipe is a template string carrying the same resolution.
    Assert-Line (Get-TsPipeRefs $made.Db 'template_strings') 'ref=AlphaComponent:demoFormat:AlphaFormatPipe:0:0:-'
    # The name list a consumer reads is unchanged.
    $names = Invoke-TsRowsQ $made.Db ("SELECT 'names=' || $(Get-NodeRootListSql 'expressions__pipes' 'e.id') AS r FROM expressions e " +
        "JOIN components k ON k.id = e.component WHERE k.name = 'OrderComponent' AND e.source LIKE '%demoFormat%'")
    Assert-Line $names 'names=["demoFormat"]'
}

Test-Case 'tsrows: a library pipe is external, two of one name in scope are ambiguous, a missing one is a diagnostic' {
    $made = New-TsRowsDb (New-TsRowsWorkspace $script:TsPipeFiles) -Shared
    Assert-Exit $made.Result 0
    $refs = Get-TsPipeRefs $made.Db
    # `date` through `CommonModule`'s exports, and through a standalone component's own `DatePipe`.
    Assert-Line $refs 'ref=AlphaComponent:date:-:1:0:DatePipe'
    Assert-Line $refs 'ref=OrderComponent:date:-:1:0:DatePipe'
    # AlphaModule exports its `demoFormat` and the standalone BetaFormatPipe is imported beside it: no id is picked.
    Assert-Line $refs 'ref=MixComponent:demoFormat:-:0:1:-'
    $diag = Invoke-TsRowsQ $made.Db "SELECT 'dg=' || kind || ':' || name || ':' || coalesce(reason, '-') AS r FROM diagnostics WHERE kind LIKE 'pipe_%'"
    Assert-Line $diag 'dg=pipe_ambiguous_in_scope:demoFormat:-'
    Assert-Line $diag 'dg=pipe_unresolved_in_scope:missingFormat:not_in_scope'
    Assert-NoLine $diag 'dg=pipe_unresolved_in_scope:date'
}

# A PARTIAL RUN keeps the resolution: the component's file is read again, its expressions are made again and resolved
# against pipes it was handed back; the pipe's file read again keeps the id a kept expression names.
Test-Case 'tsrows: a partial run keeps every pipe usage resolved to the same pipe row' {
    $tree = New-TsRowsWorkspace $script:TsPipeFiles
    $first = New-TsRowsDb $tree
    Assert-Exit $first.Result 0
    $before = Invoke-TsRowsQ $first.Db ("SELECT 'id=' || k.name || ':' || $(Get-TsFieldSql 'expressions__pipe_refs' 'r' 'id') AS r " +
        "FROM expressions e JOIN components k ON k.id = e.component JOIN expressions__pipe_refs r ON r.owner = e.id AND r.parent = 0 " +
        "WHERE $(Get-TsFieldSql 'expressions__pipe_refs' 'r' 'id') IS NOT NULL")
    foreach ($edit in @('apps/shop/src/alpha/alpha.component.ts', 'apps/shop/src/beta/beta-format.pipe.ts')) {
        $path = Join-Path $tree $edit
        Set-Content -LiteralPath $path -Value ((Get-Content -LiteralPath $path -Raw) + "// edited`n") -NoNewline
        $again = New-TsRowsDb $tree
        Assert-Exit $again.Result 0
        Assert-Line $again.Result 'put back'
        $after = Invoke-TsRowsQ $again.Db ("SELECT 'id=' || k.name || ':' || $(Get-TsFieldSql 'expressions__pipe_refs' 'r' 'id') AS r " +
            "FROM expressions e JOIN components k ON k.id = e.component JOIN expressions__pipe_refs r ON r.owner = e.id AND r.parent = 0 " +
            "WHERE $(Get-TsFieldSql 'expressions__pipe_refs' 'r' 'id') IS NOT NULL")
        foreach ($line in @($before.Lines | Where-Object { $_.Trim().StartsWith('id=') })) { Assert-Line $after $line.Trim() }
        $refs = Get-TsPipeRefs $again.Db
        Assert-Line $refs 'ref=AlphaComponent:demoFormat:AlphaFormatPipe:0:0:-'
        Assert-Line $refs 'ref=BetaComponent:demoFormat:BetaFormatPipe:0:0:-'
        Assert-Line $refs 'ref=AlphaComponent:date:-:1:0:DatePipe'
    }
}

}
