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


# WHAT EACH BARE NAME AND EACH LINK IN CODE IS BOUND TO (rows 74, `TsExpr/TsReadRefs.mjs`, `TsExprNorm.mjs`): a class a module declares is a `target` (marked
# `kind: class`, at its NAME's line, never its decorator's) and a parameter is a `param` - the place of its name, which a `params` item carries - as a local is a
# `local`. A shorthand property (`{ client }`) is a read of the variable it names. The sources are in the here-strings; their lines and columns are pinned.
$script:TsBindFiles = @{
    'apps/shop/src/bind.kinds.ts' = @'
import { Injectable } from '@angular/core';
export class Alpha { Id = 1; static Tag = 'a'; static Make(): Alpha { return new Alpha(); } }
@Injectable({ providedIn: 'root' })
export class Beta { static Parse(x: string): number { return x.length; } }
export interface Req { client: string; }
export interface Resp { client: string; }
export class Api { client = 'x'; post(b: Req): Resp { return { client: b.client }; } }
export function project(f: () => { currencyID: string }): string { return f().currencyID; }
export const MODE = 1;
'@
    'apps/shop/src/bind.use.ts' = @'
import { Alpha, Alpha as Aliased, Api, Beta, MODE, project } from './bind.kinds';
export class Eff {
  client = 1;
  api = new Api();
  send = ([_, client]: [number, string]) => this.api.post({ client });
  save(resp: { client: string }): string { const client = 'x'; return resp.client + client; }
  make(): Alpha { return Alpha.Make() ?? new Alpha(); }
  tag(): string { return Aliased.Tag + Alpha.Tag; }
  parse(s: string): number { return Beta.Parse(s) + MODE; }
  plain(p: string, q: number): string { return p + q; }
  pair({ x, y: z }: { x: string; y: string }, [h]: number[], ...rest: string[]): string { return x + z + h + rest.length; }
  ctx(): string {
    const currencyID = 'c';
    return project(() => ({ currencyID }));
  }
}
export function top(a: number, b = a): number { return a + b; }
export const arrow = (s: string) => s.length;
export const obj = { m(n: number) { return n + 1; } };
export function outer(k: number) { function inner(j: number) { return j + k; } return inner(k); }
export class Holder { constructor(private dep: Api, id: number) { void id; } }
'@
}

function Get-TsBindDb { return (New-TsRowsDb (New-TsRowsWorkspace $script:TsBindFiles) -Shared).Db }

# `bind=<name>=<what its Read is bound to>` for every Read and SafeRead of the CODE expression whose source is `$Source`: `target@<line>[:class]`, `local@<line>/<col>`,
# `param@<line>/<col>`, or `none`.
function Get-TsBindLinks([string]$Db, [string]$Source) {
    $part = { param($role, $field) "LEFT JOIN expressions__ast ${role}_$field ON ${role}_$field.owner = x.id AND ${role}_$field.parent = $role.n AND ${role}_$field.role = '$field' " }
    return Invoke-TsRowsQ $Db ("SELECT 'bind=' || nm.value || '=' || CASE " +
        "WHEN target_line.value IS NOT NULL THEN 'target@' || target_line.value || ifnull(':' || target_kind.value, '') " +
        "WHEN local_line.value IS NOT NULL THEN 'local@' || local_line.value || '/' || local_col.value " +
        "WHEN param_line.value IS NOT NULL THEN 'param@' || param_line.value || '/' || param_col.value ELSE 'none' END AS r " +
        "FROM expressions x JOIN expressions__ast k ON k.owner = x.id AND k.role = 'k' AND k.value IN ('Read', 'SafeRead') " +
        "JOIN expressions__ast nm ON nm.owner = x.id AND nm.parent = k.parent AND nm.role = 'name' " +
        "LEFT JOIN expressions__ast target ON target.owner = x.id AND target.parent = k.parent AND target.role = 'target' " +
        "LEFT JOIN expressions__ast local ON local.owner = x.id AND local.parent = k.parent AND local.role = 'local' " +
        "LEFT JOIN expressions__ast param ON param.owner = x.id AND param.parent = k.parent AND param.role = 'param' " +
        (& $part 'target' 'line') + (& $part 'target' 'kind') + (& $part 'local' 'line') + (& $part 'local' 'col') + (& $part 'param' 'line') + (& $part 'param' 'col') +
        "WHERE x.source = '$Source' ORDER BY nm.n")
}

# C1 (audit): a name that is ALSO a local or a parameter binds to the checker's symbol at THAT identifier - the property link `resp.client` to the property, the bare
# `client` to the local - and a shorthand `{ client }` is the parameter it reads, never the property a class of the same name declares.
Test-Case 'tsrows: a property link and a bare name spelled alike each bind to their own declaration' {
    $db = Get-TsBindDb
    $r = Get-TsBindLinks $db 'resp.client + client'
    Assert-Line $r 'bind=resp=param@6/8'
    Assert-Line $r 'bind=client=target@6'
    Assert-Line $r 'bind=client=local@6/50'
    $s = Get-TsBindLinks $db 'this.api.post({ client })'
    Assert-Line $s 'bind=post=target@7'
    Assert-Line $s 'bind=api=target@4'
    Assert-Line $s 'bind=client=param@5/15'
}

# C2 (audit): a bare name that is a CLASS - a receiver of a static member, of `new`, through an import alias - has a target at the class's NAME line.
Test-Case 'tsrows: a bare name that is a class has a target marked class, at the name line' {
    $db = Get-TsBindDb
    $r = Get-TsBindLinks $db 'Alpha.Make() ?? new Alpha()'
    Assert-Line $r 'bind=Make=target@2'
    Assert-Line $r 'bind=Alpha=target@2:class'
    Assert-Line (Get-TsBindLinks $db 'Aliased.Tag + Alpha.Tag') 'bind=Aliased=target@2:class'
    # The decorator is on line 3 and the class name on line 4: the target is the `classes` row's own line. The const `MODE` stays a target with no mark.
    $b = Get-TsBindLinks $db 'Beta.Parse(s) + MODE'
    Assert-Line $b 'bind=Beta=target@4:class'
    Assert-Line $b 'bind=MODE=target@9'
    $line = Invoke-TsRowsQ $db "SELECT 'class=' || name || '@' || line AS r FROM classes WHERE name = 'Beta'"
    Assert-Line $line 'class=Beta@4'
}

# C3 (audit): a read of a parameter names where its name is declared, in every function-like that has a row, destructured names included.
Test-Case 'tsrows: a read of a parameter carries param, the place of the parameter name' {
    $db = Get-TsBindDb
    $p = Get-TsBindLinks $db 'p + q'
    Assert-Line $p 'bind=p=param@10/9'
    Assert-Line $p 'bind=q=param@10/20'
    $d = Get-TsBindLinks $db 'x + z + h + rest.length'
    Assert-Line $d 'bind=x=param@11/10'
    Assert-Line $d 'bind=z=param@11/16'
    Assert-Line $d 'bind=h=param@11/48'
    Assert-Line $d 'bind=rest=param@11/65'
    Assert-Line (Get-TsBindLinks $db 'Beta.Parse(s) + MODE') 'bind=s=param@9/9'
    Assert-Line (Get-TsBindLinks $db 'a + b') 'bind=a=param@17/21'
    Assert-Line (Get-TsBindLinks $db 'a + b') 'bind=b=param@17/32'
    Assert-Line (Get-TsBindLinks $db '(s: string) => s.length') 'bind=s=param@18/23'
    Assert-Line (Get-TsBindLinks $db 'n + 1') 'bind=n=param@19/24'
    Assert-Line (Get-TsBindLinks $db 'void id') 'bind=id=param@21/53'
    Assert-Line (Get-TsBindLinks $db 'j + k') 'bind=j=param@20/51'
    Assert-Line (Get-TsBindLinks $db 'j + k') 'bind=k=param@20/23'
    # The control: a name that is no parameter has none (a local, a class).
    Assert-NoLine (Get-TsBindLinks $db 'resp.client + client') 'bind=client=param'
    Assert-NoLine (Get-TsBindLinks $db 'Alpha.Make() ?? new Alpha()') 'bind=Alpha=param'
}

# A CODE read's `param` is a place, the template's `param: true` a mark the flat summary skips: a function's own parameters stay in `reads`.
Test-Case 'tsrows: the flat reads of a code expression still list the parameters it reads' {
    $db = Get-TsBindDb
    $r = Invoke-TsRowsQ $db ("SELECT 'read=' || v.value AS r FROM expressions x JOIN expressions__reads v ON v.owner = x.id WHERE x.source = 'p + q'")
    Assert-Line $r 'read=p'
    Assert-Line $r 'read=q'
}

Test-Case 'tsrows: every param of a read is the line and col of a params item or one of its bindings' {
    $db = Get-TsBindDb
    $join = { param($table, $owner) "EXISTS (SELECT 1 FROM $table l JOIN $table c ON c.owner = l.owner AND c.parent = l.parent AND c.role = 'col' " +
        "JOIN $owner o ON o.id = l.owner WHERE l.role = 'line' AND l.value = pl.value AND c.value = pc.value AND o.file = fl.id)" }
    $sql = "SELECT 'reads=' || count(*) || ' orphans=' || sum(CASE WHEN " + (& $join 'members__params' 'members') + " OR " + (& $join 'functions__params' 'functions') +
        " THEN 0 ELSE 1 END) AS r FROM expressions x JOIN expressions__ast pa ON pa.owner = x.id AND pa.role = 'param' " +
        "JOIN expressions__ast pf ON pf.owner = x.id AND pf.parent = pa.n AND pf.role = 'file' JOIN files fl ON fl.abs = pf.value " +
        "JOIN expressions__ast pl ON pl.owner = x.id AND pl.parent = pa.n AND pl.role = 'line' " +
        "JOIN expressions__ast pc ON pc.owner = x.id AND pc.parent = pa.n AND pc.role = 'col' WHERE fl.path LIKE '%bind.use.ts'"
    $r = Invoke-TsRowsQ $db $sql
    Assert-Line $r 'reads=18 orphans=0'
}

# C4 (audit): a shorthand property is a read of the variable it names - the local - not an unbound name, and the control is the same name written out.
Test-Case 'tsrows: a shorthand property in an object binds the local it reads' {
    $db = Get-TsBindDb
    Assert-Line (Get-TsBindLinks $db '() => ({ currencyID })') 'bind=currencyID=local@13/11'
    Assert-Line (Get-TsBindLinks $db 'project(() => ({ currencyID }))') 'bind=currencyID=local@13/11'
}

}
