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
# A FUNCTION IN AN OBJECT LITERAL NAMES THE MEMBER IT FILLS (rows 83, #105, `TsWalk.mjs::declareInline`): an arrow, a function expression and a method
# written in the object are `functions` rows (`inline`) whose `target` is the member and `target_id` its `type_members` row, as a literal's are - so what
# an arrow RETURNS (`returns.member`) joins that member through its function. Parentheses and `as const` around a literal are stepped through (`fieldTarget`).
# Control: a function in an object with no contextual type names none.
$script:TsFieldFns = @{
    'apps/shop/src/rule.ts' = "export interface HueRule {`n" +
        "  Field: string;`n" +
        "  Message: string | (() => string);`n" +
        "  Check?: (v: string) => boolean;`n" +
        "}`n"
    'apps/shop/src/use.ts' = "import { HueRule } from './rule';`n" +
        "export class HueForm {`n" +
        "  strict = false;`n" +
        "  rules: HueRule[] = [`n" +
        "    { Field: ('alpha'), Message: () => (this.strict ? 'hue.alpha.strict' : 'hue.alpha.required') },`n" +
        "    { Field: 'beta' as const, Message: function () { return 'hue.beta'; }, Check(v) { return v.length > 0; } },`n" +
        "  ];`n" +
        "  loose = { Message: () => 'hue.loose' };`n" +
        "}`n"
}

Test-Case 'tsrows: a function in an object literal names the member of its contextual type (#105)' {
    $made = New-TsRowsDb (New-TsRowsWorkspace $script:TsFieldFns) -Shared
    Assert-Exit $made.Result 0
    $f = { param($role) Get-TsRootFieldSql 'functions__target' 'fn.id' $role }
    $r = Invoke-TsRowsQ $made.Db ("SELECT 'fn=' || fn.form || '@' || fn.line || '=' || coalesce($(& $f 'name'), '-') || '@' || coalesce($(& $f 'line'), '-') || ':' || " +
        "coalesce((SELECT 'tm ' || t.name || '@' || t.line FROM type_members t WHERE t.id = fn.target_id), '-') AS v FROM functions fn " +
        "JOIN files f ON f.id = fn.file AND f.path LIKE '%use.ts' WHERE fn.inline = 1")
    Assert-Line $r 'fn=arrow@5=Message@3:tm Message@3'
    Assert-Line $r 'fn=function-expression@6=Message@3:tm Message@3'
    Assert-Line $r 'fn=method@6=Check@4:tm Check@4'
    Assert-Line $r 'fn=arrow@8=-@-:-'
    # THE ISSUE'S JOIN: what a function returns (`returns.member` is the function) reaches the member it fills.
    $j = Invoke-TsRowsQ $made.Db ("SELECT 'ret=' || r.source || '>' || t.name AS v FROM returns r JOIN functions fn ON fn.id = r.member " +
        "JOIN type_members t ON t.id = fn.target_id")
    Assert-Line $j "ret=(this.strict ? 'hue.alpha.strict' : 'hue.alpha.required')>Message"
    Assert-Line $j "ret='hue.beta'>Message"
    Assert-NoLine $j 'hue.loose'
    $l = Invoke-TsRowsQ $made.Db (Get-TsFieldTargetSql 'string_literals' 'value')
    Assert-Line $l 'alpha=Field@2:rule.ts:tm Field@2'
    Assert-Line $l 'beta=Field@2:rule.ts:tm Field@2'
}
# (rows 87, #106) AN UN-ANNOTATED CONST TAKES ITS TYPE LATER (`TsReadRefs.mjs::laterTypes`): `const list = [..]; return list;` in a method typed `HueRule[]`, a const handed to a parameter
# typed `HueRule`, and a const that only another const aliases before the return - each object's literal and function name the member they fill, as with an annotation.
# THE CONTROL: a const pushed into another un-annotated array takes that array's INFERRED literal type, whose `Message` is the other literal's own property - no member, so none.
$script:TsFieldLater = @{
    'apps/shop/src/rule.ts' = "export interface HueRule {`n" +
        "  Field: string;`n" +
        "  Message: string | (() => string);`n" +
        "}`n" +
        "export function check(r: HueRule): void { void r; }`n"
    'apps/shop/src/use.ts' = "import { HueRule, check } from './rule';`n" +
        "export class Widget {`n" +
        "  strict = false;`n" +
        "  rules(): HueRule[] {`n" +
        "    const list = [`n" +
        "      { Field: 'alpha', Message: 'hue.alpha.required' },`n" +
        "      { Field: 'beta', Message: () => (this.strict ? 'hue.beta.strict' : 'hue.beta.required') },`n" +
        "    ];`n" +
        "    return list;`n" +
        "  }`n" +
        "  run(): void {`n" +
        "    const one = { Field: 'gamma', Message: 'hue.gamma.arg' };`n" +
        "    check(one);`n" +
        "  }`n" +
        "  nested(): HueRule[][] {`n" +
        "    const grid = [[{ Field: 'delta', Message: 'hue.delta' }]];`n" +
        "    const same = grid;`n" +
        "    return same;`n" +
        "  }`n" +
        "  pushed(): HueRule[] {`n" +
        "    const out = [];`n" +
        "    const rule = { Field: 'theta', Message: 'hue.theta.pushed' };`n" +
        "    out.push(rule);`n" +
        "    out.push({ Field: 'iota', Message: () => 'hue.iota.pushed' });`n" +
        "    return out;`n" +
        "  }`n" +
        "  loose(): number {`n" +
        "    const free = { Field: 'eps', Message: 'hue.eps.loose' };`n" +
        "    const other = [{ Field: 'zeta', Message: 'hue.zeta.loose' }];`n" +
        "    other.push(free);`n" +
        "    return other.length;`n" +
        "  }`n" +
        "}`n"
}

Test-Case 'tsrows: a literal in an un-annotated const names the member of the type it takes later (#106)' {
    $made = New-TsRowsDb (New-TsRowsWorkspace $script:TsFieldLater) -Shared
    Assert-Exit $made.Result 0
    $r = Invoke-TsRowsQ $made.Db (Get-TsFieldTargetSql 'string_literals' 'value')
    Assert-Line $r 'hue.alpha.required=Message@3:rule.ts:tm Message@3'
    Assert-Line $r 'alpha=Field@2:rule.ts:tm Field@2'
    Assert-Line $r 'hue.gamma.arg=Message@3:rule.ts:tm Message@3'
    Assert-Line $r 'hue.delta=Message@3:rule.ts:tm Message@3'
    Assert-Line $r 'hue.eps.loose=-@-:-:-'
    Assert-Line $r 'hue.zeta.loose=-@-:-:-'
    $fn = Invoke-TsRowsQ $made.Db ("SELECT 'fn=' || fn.line || ':' || coalesce((SELECT t.name || '@' || t.line FROM type_members t WHERE t.id = fn.target_id), '-') AS v FROM functions fn " +
        "JOIN files f ON f.id = fn.file AND f.path LIKE '%use.ts' WHERE fn.inline = 1")
    Assert-Line $fn 'fn=7:Message@3'
    # (rows 88) PUSHED INTO AN UN-ANNOTATED ARRAY that is returned typed: a const object and a literal handed to `out.push(..)` take `HueRule`, one level below `out`'s `HueRule[]`.
    Assert-Line $r 'hue.theta.pushed=Message@3:rule.ts:tm Message@3'
    Assert-Line $r 'theta=Field@2:rule.ts:tm Field@2'
    Assert-Line $r 'iota=Field@2:rule.ts:tm Field@2'
    Assert-Line $fn 'fn=24:Message@3'
}
}
