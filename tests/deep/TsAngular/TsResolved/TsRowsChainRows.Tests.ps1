<#
    FOUR ROWS THAT SAID LESS, OR MORE, THAN THE SOURCE (rows 70): a concise arrow's implicit `returns` is anchored at the START of its
    body (`TsNodes.mjs::locationOf`), a flat `reads`/`calls` summary records no chain tail as a root (`TsExpr/TsExprSummary.mjs`) and keeps
    `''` in `strings`, a call of an overloaded method names its implementation (`TsDecls/TsRefs.mjs`), and a side-effect import of a script
    resolves (`TsImports.mjs::importRow`). Its helpers are `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '../../TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

$script:TsChainFiles = @{
    'apps/shop/src/chain.component.ts' = "import { Component } from '@angular/core';`n" +
        "import './flags';`n" +
        "export const pick = (o: { name: string }) => o.name;`n" +
        "export const deep = (o: { a: { b: string } }) =>`n" +
        "  o.a`n" +
        "    .b;`n" +
        "export class Log {`n" +
        "  error(msg: string): void;`n" +
        "  error(msg: string, t: string): void;`n" +
        "  error(msg: string, t?: string): void { void msg; void t; }`n" +
        "  plain(): void {}`n" +
        "}`n" +
        "function foo(): { bar: string } { return { bar: '' }; }`n" +
        "@Component({ selector: 'app-chain', template: '<p>{{ (user`$ | async)?.name }}</p><p>{{ label ?? `"`" }}</p>' })`n" +
        "export class ChainComponent {`n" +
        "  user`$: any; label = 'x';`n" +
        "  log = new Log();`n" +
        "  run() { this.log.error('a'); this.log.plain(); return foo().bar; }`n" +
        "}`n"
    'apps/shop/src/flags.ts' = "(globalThis as any).alphaFlag = true;`n"
    'apps/shop/src/links.ts' = "export class Inner { Id: number = 0; }`n" +
        "export class Holder { Inner: Inner = new Inner(); }`n" +
        "export interface Tail { c: number }`n" +
        "export interface Mid { b?: Tail }`n" +
        "export class Widget {`n" +
        "  holder: Holder = new Holder();`n" +
        "  a?: Mid;`n" +
        "  check() { return this.holder.Inner.Id === 1; }`n" +
        "  safe() { return this.a?.b?.c === 1; }`n" +
        "  call(f: (n: number) => number) { return f(this.holder.Inner.Id) === 1; }`n" +
        "}`n"
}

function Get-TsChainDb {
    $made = New-TsRowsDb (New-TsRowsWorkspace $script:TsChainFiles)
    Assert-Exit $made.Result 0
    return $made.Db
}

Test-Case 'tsrows: an implicit return of a concise arrow is anchored at its body start, over one line or several' {
    $db = Get-TsChainDb
    $r = Invoke-TsRowsQ $db ("SELECT 'ret=' || f.name || ':' || r.line || ':' || r.col AS r FROM returns r " +
        "JOIN functions f ON f.id = r.member WHERE r.implicit = 1 AND f.name IN ('pick', 'deep') ORDER BY f.name")
    Assert-Line $r 'ret=pick:3:46'
    Assert-Line $r 'ret=deep:5:3'
}

Test-Case 'tsrows: a chain tail is no root in the flat reads, a call chain neither, and an empty string stays in strings' {
    $db = Get-TsChainDb
    $r = Invoke-TsRowsQ $db ("SELECT 'read=' || x.source || '=' || ifnull((SELECT group_concat(v.value, ',') FROM expressions__reads v WHERE v.owner = x.id), '') AS r " +
        "FROM expressions x WHERE x.source LIKE '%async%' OR x.source LIKE '%??%'")
    Assert-Line $r 'read={{ (user$ | async)?.name }}=user$'
    # The control: a plain read still lists its dotted path.
    Assert-Line $r 'read={{ label ?? "" }}=label'
    $s = Invoke-TsRowsQ $db "SELECT 'str=' || count(*) AS r FROM expressions x JOIN expressions__strings s ON s.owner = x.id WHERE x.source LIKE '%??%' AND s.value = ''"
    Assert-Line $s 'str=1'
}

Test-Case 'tsrows: a call of an overloaded method has its target_id, a plain one still has it' {
    $db = Get-TsChainDb
    $r = Invoke-TsRowsQ $db ("SELECT 'call=' || c.callee || ':' || (c.target_id IS NOT NULL) || ':' || ifnull(m.line, '-') AS r FROM calls c " +
        "LEFT JOIN members m ON m.id = c.target_id WHERE c.callee IN ('this.log.error', 'this.log.plain') ORDER BY c.callee")
    Assert-Line $r 'call=this.log.error:1:10'
    Assert-Line $r 'call=this.log.plain:1:11'
}

Test-Case 'tsrows: a side-effect import of a script resolves to its file' {
    $db = Get-TsChainDb
    $r = Invoke-TsRowsQ $db ("SELECT 'imp=' || i.module || ':' || ifnull(i.resolved, '-') || ':' || (i.resolved_file IS NOT NULL) || ':' || i.in_tree AS r " +
        "FROM imports i WHERE i.module = './flags'")
    Assert-Line $r 'imp=./flags:apps/shop/src/flags.ts:1:1'
    # Resolved by its PATH, so it is recorded as a read: a row naming a file its owner did not read fails the reads proof.
    Assert-Line (Invoke-TsRowsQ $db "SELECT 'reads=' || value AS r FROM _meta WHERE key = 'reads:typescript'") 'reads=proven'
}

# Every Read and SafeRead of the CODE expression whose source is `$Source`: `link=<name>@<line of the declaration its target names, or none>`.
function Get-TsChainLinks([string]$Db, [string]$Source) {
    return Invoke-TsRowsQ $Db ("SELECT 'link=' || nm.value || '@' || ifnull(tl.value, 'none') AS r " +
        "FROM expressions x JOIN expressions__ast k ON k.owner = x.id AND k.role = 'k' AND k.value IN ('Read', 'SafeRead') " +
        "JOIN expressions__ast nm ON nm.owner = x.id AND nm.parent = k.parent AND nm.role = 'name' " +
        "LEFT JOIN expressions__ast t ON t.owner = x.id AND t.parent = k.parent AND t.role = 'target' " +
        "LEFT JOIN expressions__ast tl ON tl.owner = x.id AND tl.parent = t.n AND tl.role = 'line' " +
        "WHERE x.source = '$Source' ORDER BY nm.n")
}

# EVERY LINK OF A PROPERTY CHAIN IN CODE IS RESOLVED (rows 73, #102, `TsExpr/TsExprNorm.mjs`): `this.holder.Inner.Id` bound only `Id`.
Test-Case 'tsrows: every link of a property chain in a code expression names its declaration' {
    $db = Get-TsChainDb
    $r = Get-TsChainLinks $db 'this.holder.Inner.Id === 1'
    Assert-Line $r 'link=holder@6'
    Assert-Line $r 'link=Inner@2'
    Assert-Line $r 'link=Id@1'
}

Test-Case 'tsrows: a safe chain and a chain under a call argument name every link too' {
    $db = Get-TsChainDb
    $s = Get-TsChainLinks $db 'this.a?.b?.c === 1'
    Assert-Line $s 'link=a@7'
    Assert-Line $s 'link=b@4'
    Assert-Line $s 'link=c@3'
    $c = Get-TsChainLinks $db 'f(this.holder.Inner.Id) === 1'
    Assert-Line $c 'link=holder@6'
    Assert-Line $c 'link=Inner@2'
    Assert-Line $c 'link=Id@1'
}

}
