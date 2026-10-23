<#
    A LITERAL IN AN OBJECT LITERAL NAMES THE MEMBER IT FILLS (#104, `src/TsRows/TsDecls/TsExpr/TsReadRefs.mjs::fieldTarget`,
    `TsRefs.mjs::rollupRefIds`): `string_literals.target` / `number_literals.target` is `{name, file, line}` where the checker's
    contextual type declares the property (the NAME's line), and `target_id` is the `type_members` row there. An object with no
    contextual type, a property the type does not declare and a union whose arms declare it in SEVERAL places name none.
    Read from the node rows (`string_literals__target`). Its helpers are `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '../../TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

$script:TsFieldFiles = @{
    'apps/shop/src/rule.ts' = "export interface HueRule {`n" +
        "  Field: string;`n" +
        "  Message: string;`n" +
        "  Weight?: number;`n" +
        "}`n" +
        "export interface HueOther {`n" +
        "  Message: string;`n" +
        "}`n" +
        "export function check(r: HueRule): void { void r; }`n" +
        "export function checkEither(r: HueRule | HueOther): void { void r; }`n"
    'apps/shop/src/use.ts' = "import { HueRule, check, checkEither } from './rule';`n" +
        "export class HueForm {`n" +
        "  rules: HueRule[] = [{ Field: 'alpha', Message: 'hue.alpha.required', Weight: 7 }];`n" +
        "  loose = { Field: 'beta', Message: 'hue.beta.loose', Weight: 8 };`n" +
        "  run(): void {`n" +
        "    check({ Field: 'gamma', Message: 'hue.gamma.arg' });`n" +
        "    checkEither({ Message: 'hue.delta.either' });`n" +
        "    const x: HueRule = { Field: 'epsilon', Message: 'hue.eps', Extra: 'hue.extra' };`n" +
        "    void x;`n" +
        "  }`n" +
        "}`n"
}

# `value=name@line:file:id`: the target a literal carries, the file as `rule.ts`, and the row `target_id` reaches.
function Get-TsFieldTargetSql([string]$Table, [string]$Column) {
    $f = { param($role) Get-TsRootFieldSql "${Table}__target" 's.id' $role }
    $file = "(CASE WHEN $(& $f 'file') LIKE '%/apps/shop/src/rule.ts' THEN 'rule.ts' ELSE coalesce($(& $f 'file'), '-') END)"
    return "SELECT s.$Column || '=' || coalesce($(& $f 'name'), '-') || '@' || coalesce($(& $f 'line'), '-') || ':' || $file || ':' || " +
        "coalesce((SELECT 'tm ' || t.name || '@' || t.line FROM type_members t WHERE t.id = s.target_id), '-') AS v FROM $Table s " +
        "JOIN files f ON f.id = s.file AND f.path LIKE '%use.ts'"
}

Test-Case 'tsrows: a string in an object literal names the member of its contextual type (#104)' {
    $made = New-TsRowsDb (New-TsRowsWorkspace $script:TsFieldFiles) -Shared
    Assert-Exit $made.Result 0
    $r = Invoke-TsRowsQ $made.Db (Get-TsFieldTargetSql 'string_literals' 'value')
    # the issue: an element of a typed array
    Assert-Line $r 'hue.alpha.required=Message@3:rule.ts:tm Message@3'
    Assert-Line $r 'alpha=Field@2:rule.ts:tm Field@2'
    # a call argument, and a typed local
    Assert-Line $r 'hue.gamma.arg=Message@3:rule.ts:tm Message@3'
    Assert-Line $r 'hue.eps=Message@3:rule.ts:tm Message@3'
    # control: an object with no contextual type, and a property the type does not declare, name none
    Assert-Line $r 'hue.beta.loose=-@-:-:-'
    Assert-Line $r 'hue.extra=-@-:-:-'
    # a union whose arms declare `Message` in two places names none
    Assert-Line $r 'hue.delta.either=-@-:-:-'
}

Test-Case 'tsrows: a number in an object literal names the member of its contextual type (#104)' {
    $made = New-TsRowsDb (New-TsRowsWorkspace $script:TsFieldFiles) -Shared
    Assert-Exit $made.Result 0
    $r = Invoke-TsRowsQ $made.Db (Get-TsFieldTargetSql 'number_literals' 'value')
    Assert-Line $r '7=Weight@4:rule.ts:tm Weight@4'
    Assert-Line $r '8=-@-:-:-'
}

# `target_id` IS THE DECLARATION'S OWN ROW. `a-wide.ts` flattens a library's `Dataset` into its alias (`via = 'computed'` rows it owns, about the
# library's declaration), and `c-use.ts` meets `Dataset` only as a library function's parameter type: its literal keeps its `target` (the library's
# file) and has NO `target_id` - binding to the alias's copy made it name a file it never read, and the reads `unproven` for the whole map.
# Control: an in-tree interface's member still binds its own row (`e-lib.ts`), and so does a call (`d-call.ts`).
$script:TsFieldReach = @{
    'node_modules/demo-lib/package.json' = '{"name":"demo-lib","version":"1.0.0","types":"index.d.ts"}'
    'node_modules/demo-lib/index.d.ts' = "export interface Dataset {`n  Label: string;`n  Pick: () => number;`n}`n" +
        "export declare function take(d: Dataset): void;`n"
    'apps/shop/src/a-wide.ts' = "import { Dataset } from 'demo-lib';`n" +
        "export type Wide = Dataset & { Extra: number };`n"
    'apps/shop/src/c-use.ts' = "import { take } from 'demo-lib';`n" +
        "import { Local, put } from './e-lib';`n" +
        "export function run(): void {`n" +
        "  take({ Label: 'hue.reach', Pick: () => 1 });`n" +
        "  put({ Name: 'hue.own' });`n" +
        "}`n"
    'apps/shop/src/d-call.ts' = "import { kit } from './e-lib';`n" +
        "export const made = kit.Pick();`n"
    'apps/shop/src/e-lib.ts' = "export const kit = { Pick: () => 1 };`n" +
        "export interface Local {`n  Name: string;`n}`n" +
        "export function put(l: Local): void { void l; }`n"
}

Test-Case 'tsrows: a library member has a target and no target_id, an in-tree one binds its own row (#104)' {
    $made = New-TsRowsDb (New-TsRowsWorkspace $script:TsFieldReach) -Shared
    Assert-Exit $made.Result 0
    $lit = { param($value) "SELECT 'v=' || coalesce((SELECT CASE WHEN value LIKE '%/node_modules/demo-lib/index.d.ts' THEN 'lib' ELSE 'other' END FROM string_literals__target WHERE owner = s.id AND role = 'file'), '-') || ' id=' || coalesce(s.target_id, '-') AS r FROM string_literals s WHERE s.value = '$value'" }
    Assert-Line (Invoke-TsRowsQ $made.Db (& $lit 'hue.reach')) 'v=lib id=-'
    $own = Invoke-TsRowsQ $made.Db "SELECT 'own=' || t.name || '@' || (SELECT f.path FROM files f WHERE f.id = t.file) AS r FROM string_literals s JOIN type_members t ON t.id = s.target_id WHERE s.value = 'hue.own'"
    Assert-Line $own 'own=Name@apps/shop/src/e-lib.ts'
    $c = Invoke-TsRowsQ $made.Db "SELECT 'call=' || t.name || '@' || (SELECT f.path FROM files f WHERE f.id = t.file) AS r FROM calls c JOIN shape_members t ON t.id = c.target_id JOIN files f ON f.id = c.file AND f.path LIKE '%d-call.ts'"
    Assert-Line $c 'call=Pick@apps/shop/src/e-lib.ts'
    Assert-NoLine $made.Result 'recorded reads miss'
    Assert-Line (Invoke-TsRowsQ $made.Db "SELECT 'reads=' || value AS r FROM _meta WHERE key = 'reads:typescript'") 'reads=proven'
}
}
