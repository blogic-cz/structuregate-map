<#
    A TYPESCRIPT-ONLY WORKSPACE, OPTED INTO: a tree with no `angular.json` whose shallowest `tsconfig.json` is governed by a
    `structuregate.ts.json` saying `"semantic": true` is mapped by the checker half (`lang = 'typescript'`, every pass but the
    templates, no `@angular/compiler` asked for), so a literal names the member it fills (#104) - while a script file outside
    that folder is still the plain half's. Without the opt-in the tree stays the plain half's, as before.

    Its helpers are its own - `-Only TsTypescriptOnly` runs this suite alone.
#>

$script:TsOnlyModules = Get-TypeScriptPath 'typescript@5'
if (-not $script:TsOnlyModules) { Write-Host '    (no node/npm-installed typescript 5 - the typescript-only cases are not run)' }

# The tree: a typed rule list under `web/`, a script beside it, and the opt-in when `$Semantic`.
function New-TsOnlyTree([bool]$Semantic) {
    $files = @{
        'web/tsconfig.json' = '{"compilerOptions":{"strict":true,"target":"ES2020"},"include":["src/**/*.ts"]}'
        'web/src/rule.ts'   = "export interface HueRule { Field: string; Message: string; }`n"
        'web/src/widget.ts' = "import { HueRule } from './rule';`nexport class Widget {`n  rules: HueRule[] = [{ Field: 'alpha', Message: 'hue.alpha.required' }];`n}`n"
        'tools/build.mjs'   = "export function build(x) { return x + 1; }`n"
    }
    if ($Semantic) { $files['structuregate.ts.json'] = '{"semantic": true}' }
    return Use-Tree $files
}

# `lang` of each file and the member each literal names, as lines.
function Get-TsOnlyFacts([string]$Tree) {
    $db = Join-Path $Tree 'map.sqlite'
    $run = Invoke-Gate --root $Tree --ext '.ts,.mjs' --map-sqlite $db --ts-node-modules $script:TsOnlyModules
    Assert-Exit $run 0
    $files = Invoke-Gate --map-query $db --width 0 --sql "SELECT 'f=' || path || '|' || lang AS f FROM files WHERE path LIKE '%.ts' OR path LIKE '%.mjs'"
    $tables = Invoke-Gate --map-query $db --width 0 --sql "SELECT 't=' || count(*) AS t FROM sqlite_master WHERE name = 'type_members'"
    $literals = if (@($tables.Lines | Where-Object { $_.Trim() -eq 't=1' }).Count) {
        Invoke-Gate --map-query $db --width 0 --sql ("SELECT 's=' || s.value || '|' || coalesce(m.name, '-') AS s FROM string_literals s " +
            "LEFT JOIN type_members m ON m.id = s.target_id WHERE s.value IN ('alpha', 'hue.alpha.required')")
    } else { [pscustomobject]@{ Lines = @('no type_members') } }
    return [pscustomobject]@{ Run = $run; Files = $files; Literals = $literals }
}

if ($script:TsOnlyModules) {

Test-Case 'tstypescriptonly: an opted-in tsconfig workspace is the checker half, a script beside it the plain half' {
    $tree = New-TsOnlyTree $true
    $facts = Get-TsOnlyFacts $tree
    Assert-Line $facts.Files 'f=src/widget.ts|typescript'
    Assert-Line $facts.Files 'f=tools/build.mjs|ts'
    # THE CHECKER'S ANSWER (#104): each literal names the member of `HueRule` it fills.
    Assert-Line $facts.Literals 's=hue.alpha.required|Message'
    Assert-Line $facts.Literals 's=alpha|Field'
    Assert-NoLine $facts.Run 'error'
    # A PARTIAL RUN after an edit keeps the checker's answer, and the new literal gets its own.
    Set-Content -LiteralPath (Join-Path $tree 'web/src/widget.ts') -NoNewline -Value ("import { HueRule } from './rule';`nexport class Widget {`n" +
        "  rules: HueRule[] = [{ Field: 'alpha', Message: 'hue.alpha.required' }, { Field: 'beta', Message: 'hue.beta.required' }];`n}`n")
    $again = Get-TsOnlyFacts $tree
    Assert-Line $again.Literals 's=hue.alpha.required|Message'
    $beta = Invoke-Gate --map-query (Join-Path $tree 'map.sqlite') --width 0 --sql ("SELECT 'b=' || m.name AS b FROM string_literals s JOIN type_members m ON m.id = s.target_id WHERE s.value = 'beta'")
    Assert-Line $beta 'b=Field'
}

Test-Case 'tstypescriptonly: without the opt-in the tree stays the plain half, with no member named' {
    $facts = Get-TsOnlyFacts (New-TsOnlyTree $false)
    Assert-Line $facts.Files 'f=web/src/widget.ts|ts'
    # NO CHECKER, NO DECLARED MEMBERS: the plain half writes no `type_members` at all.
    Assert-Line $facts.Literals 'no type_members'
}

}
