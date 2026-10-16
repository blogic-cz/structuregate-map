<#
    A PARTIAL RUN KEEPS THE CLOSURE IT STORED when nothing the closure read moved (`rust/fbtcore/src/rows/partial/unchanged.rs`):
    a comment keeps it, and it is the closure a full run of the edited tree derives; an edit to a condition a key is
    chosen under moves it, and it is derived again.
#>

. (Join-Path $PSScriptRoot '..\TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

# Every key and what always restricts it, plus the size of two closure tables: what a kept closure has to match.
$script:TsKeptSql = "SELECT k.key || ' | ' || coalesce($(Get-TsFieldSql 'key_reach__always_values' 'v' 'dimension'), '-') || ' ' || " +
    "coalesce($(Get-TsFieldSql 'key_reach__always_values' 'v' 'op'), '-') || ' ' || coalesce($(Get-NodeListSql 'key_reach__always_values' 'v' 'values'), '-') AS reach " +
    "FROM key_reach k LEFT JOIN key_reach__always_values v ON v.owner = '#' || k.rowid AND v.parent = 0 WHERE k.key LIKE 'kp.%' " +
    "UNION ALL SELECT 'paths=' || (SELECT count(*) FROM render_path) || ' keys=' || (SELECT count(*) FROM key_reach) ORDER BY 1"

$script:TsKeptFiles = @{
    'apps/shop/src/kp.component.html' = "<b>{{ label }}</b>`n"
    'apps/shop/src/kp.component.ts' = "import { Component, Injectable } from '@angular/core';`n" +
        "export enum Tier { Alpha = 1, Beta = 2, Gamma = 3 }`n" +
        "@Injectable({ providedIn: 'root' })`n" +
        "export class Words {`n  instant(key: string): string {`n    return key;`n  }`n}`n" +
        "@Component({ selector: 'app-kp', templateUrl: './kp.component.html', standalone: true })`n" +
        "export class KpComponent {`n" +
        "  tier: Tier = Tier.Alpha;`n  label = '';`n" +
        "  constructor(private words: Words) {}`n" +
        "  build(): void {`n" +
        "    this.label = this.tier === Tier.Beta ? this.words.instant('kp.a') : this.words.instant('kp.b');`n" +
        "    this.words.instant('kp.c');`n" +
        "  }`n" +
        "}`n"
    'apps/shop/src/assets/locales/en.json' = '{"shop":{"title":"Shop","cart":{"empty":"Empty"}},"kp":{"a":"1","b":"2","c":"3"}}'
    'structuregate.ts.json' = '{"locales":["apps/shop/src/assets/locales"],"i18nCalls":["instant"]}'
}

# The same tree mapped whole after the same edit: what a partial run's closure must equal.
function Get-TsKeptFull([scriptblock]$Edit) {
    $fresh = New-TsRowsWorkspace $script:TsKeptFiles
    & $Edit (Join-Path $fresh 'apps/shop/src/kp.component.ts')
    $made = New-TsRowsDb $fresh
    Assert-Exit $made.Result 0
    return (Invoke-Gate --map-query $made.Db --width 0 --sql $script:TsKeptSql)
}

Test-Case 'tsrows: a comment keeps the stored closure, and it is the closure a full run of the edited tree derives' {
    $tree = New-TsRowsWorkspace $script:TsKeptFiles
    Assert-Exit (New-TsRowsDb $tree).Result 0
    $comment = { param($ts) Add-Content -LiteralPath $ts -Value '// demo: a remark' }
    & $comment (Join-Path $tree 'apps/shop/src/kp.component.ts')
    $again = New-TsRowsDb $tree
    Assert-Exit $again.Result 0
    Assert-Line $again.Result 'put back'
    Assert-Line $again.Result 'kept the closure it stored'
    $kept = Invoke-Gate --map-query $again.Db --width 0 --sql $script:TsKeptSql
    Assert-Line $kept 'kp.a | tier in ["Beta"]'
    $full = Get-TsKeptFull $comment
    Assert-Equal $kept.Text $full.Text "the kept closure against a full run of the edited tree"
}

Test-Case 'tsrows: an edited condition moves the closure, which a partial run derives again' {
    $tree = New-TsRowsWorkspace $script:TsKeptFiles
    Assert-Exit (New-TsRowsDb $tree).Result 0
    $edit = { param($ts) Set-Content -LiteralPath $ts -Value ([IO.File]::ReadAllText($ts).Replace('Tier.Beta ?', 'Tier.Gamma ?')) }
    & $edit (Join-Path $tree 'apps/shop/src/kp.component.ts')
    $again = New-TsRowsDb $tree
    Assert-Exit $again.Result 0
    Assert-Line $again.Result 'derives its closure again'
    Assert-NoLine $again.Result 'kept the closure it stored'
    $moved = Invoke-Gate --map-query $again.Db --width 0 --sql $script:TsKeptSql
    Assert-Line $moved 'kp.a | tier in ["Gamma"]'
    Assert-NoLine $moved 'Beta'
    $full = Get-TsKeptFull $edit
    Assert-Equal $moved.Text $full.Text "the derived closure against a full run of the edited tree"
}

# DERIVED AGAIN, IT IS WRITTEN AS WHAT CHANGED (`rust/fbtcore/src/rows/partial/delta.rs`): the row of a key the edit did not
# move is the row that was there - the same rowid - and the row of one it moved is a new one, above every row there was.
# Deleted and written whole, the table numbers its rows from 1 again and the moved key sits where it sat.
Test-Case 'tsrows: a closure derived again rewrites the rows that moved and keeps the rest' {
    $tree = New-TsRowsWorkspace $script:TsKeptFiles
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    $where = "SELECT key || '=' || rowid AS r FROM key_reach WHERE key IN ('kp.a', 'kp.c') ORDER BY key"
    $top = [long](Invoke-TsRowsQ $made.Db "SELECT 'top=' || max(rowid) AS r FROM key_reach").Lines.Where({ $_.Trim().StartsWith('top=') })[0].Trim().Substring(4)
    $before = Invoke-Gate --map-query $made.Db --width 0 --sql $where
    $ts = Join-Path $tree 'apps/shop/src/kp.component.ts'
    Set-Content -LiteralPath $ts -Value ([IO.File]::ReadAllText($ts).Replace('Tier.Beta ?', 'Tier.Gamma ?'))
    $again = New-TsRowsDb $tree
    Assert-Exit $again.Result 0
    Assert-Line $again.Result 'derives its closure again'
    $after = Invoke-Gate --map-query $again.Db --width 0 --sql $where
    $was = @($before.Lines | ForEach-Object { $_.Trim() } | Where-Object { $_.StartsWith('kp.') })
    $is = @($after.Lines | ForEach-Object { $_.Trim() } | Where-Object { $_.StartsWith('kp.') })
    Assert-Equal $was.Count 2 'the two keys before the edit'
    Assert-Equal $is.Count 2 'the two keys after it'
    Assert-Equal $is[1] $was[1] 'kp.c, which the edit did not move, keeps its row'
    $moved = [long]$is[0].Substring('kp.a='.Length)
    if ($moved -le $top) { throw "kp.a moved and its row is not a new one: $($is[0]), the old rows ran to $top" }
}

# A TEMPLATE TWO COMPONENTS SHARE is walked once for each, both copies of its nodes and bindings under the one `.html`:
# no key without the template row told the copies apart, so neither table was claimed, a comment renumbered both, and
# the closure was derived again on every partial run of a large workspace.
Test-Case 'tsrows: a comment beside a template two components share keeps the stored closure' {
    $files = $script:TsKeptFiles.Clone()
    $files['apps/shop/src/twin.component.html'] = "<p [title]=""label"">{{ label }}</p>`n<i>{{ label }}</i>`n"
    $files['apps/shop/src/twin.component.ts'] = "import { Component } from '@angular/core';`n" +
        "@Component({ selector: 'app-left', templateUrl: './twin.component.html', standalone: true })`n" +
        "export class LeftComponent {`n  label = 'l';`n}`n" +
        "@Component({ selector: 'app-right', templateUrl: './twin.component.html', standalone: true })`n" +
        "export class RightComponent {`n  label = 'r';`n}`n"
    $tree = New-TsRowsWorkspace $files
    Assert-Exit (New-TsRowsDb $tree).Result 0
    Add-Content -LiteralPath (Join-Path $tree 'apps/shop/src/twin.component.ts') -Value '// demo: a remark'
    $again = New-TsRowsDb $tree
    Assert-Exit $again.Result 0
    Assert-Line $again.Result 'kept the closure it stored'
}

}
