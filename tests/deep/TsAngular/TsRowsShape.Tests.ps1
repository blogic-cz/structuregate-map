<#
    An edit to a file that declares what a template resolves (a selector, a pipe, a directive, an NgModule) goes
    the short way while that file's SHAPE - every token outside a function body - is the one recorded; one that
    moves it still reads everything. See `src/TsRows/TsSetup/TsShape.mjs`. Its helpers are `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '../TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

$script:TsShapeFull = 'reading everything: 1 changed file(s) can move what a template resolves'

# One literal edit in one fixture file, then the map again over the same database.
function Invoke-TsShapeEdit([string]$File, [string]$Old, [string]$New) {
    $tree = New-TsRowsWorkspace
    Assert-Exit (New-TsRowsDb $tree).Result 0
    $path = Join-Path $tree "apps/shop/src/$File"
    $text = [IO.File]::ReadAllText($path)
    if (-not $text.Contains($Old)) { throw "the fixture has no '$Old' in $File" }
    [IO.File]::WriteAllText($path, $text.Replace($Old, $New))
    $again = New-TsRowsDb $tree
    Assert-Exit $again.Result 0
    return $again.Result
}

Test-Case 'tsrows: a comment in a component goes the short way' {
    $tree = New-TsRowsWorkspace
    Assert-Exit (New-TsRowsDb $tree).Result 0
    Add-Content -LiteralPath (Join-Path $tree 'apps/shop/src/cart.component.ts') -Value '// moved'
    $again = New-TsRowsDb $tree
    Assert-Exit $again.Result 0
    Assert-NoLine $again.Result 'reading everything'
    Assert-Line $again.Result 'put back'
}

Test-Case 'tsrows: a changed selector reads everything' {
    $r = Invoke-TsShapeEdit 'cart.component.ts' "selector: 'app-cart'" "selector: 'app-cart2'"
    Assert-Line $r $script:TsShapeFull
}

# A CONST A DECORATOR NAMES is outside every function body, so it moves the shape of its own file.
Test-Case 'tsrows: a changed const an NgModule exports reads everything' {
    $r = Invoke-TsShapeEdit 'basket.component.ts' 'const GROUP = [HighlightDirective, MoneyPipe];' 'const GROUP = [HighlightDirective];'
    Assert-Line $r $script:TsShapeFull
}

Test-Case 'tsrows: a changed method body in a component goes the short way' {
    $r = Invoke-TsShapeEdit 'cart.component.ts' 'greet(name: string): string { return name; }' "greet(name: string): string { return name + '!'; }"
    Assert-NoLine $r 'reading everything'
    Assert-Line $r 'put back'
    Assert-Line $r 'read 2 file(s) again for 1 changed: 1 surface(s) looked at, 0 moved'
}

}
