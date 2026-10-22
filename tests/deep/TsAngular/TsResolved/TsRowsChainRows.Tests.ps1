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
    'apps/shop/src/labels.ts' = "export const unusedLabels = 0;`n" + "export const hueLabels = { Alpha: 'A', Beta: 'B' };`n"
    'apps/shop/src/hue.enum.ts' = "export enum HueIDs { Alpha = 1 }`n"
    'apps/shop/src/argrefs.ts' = "import { HueIDs } from './hue.enum';`n" +
        "import { hueLabels } from './labels';`n" +
        "import { hueLabels as hl } from './labels';`n" +
        "export class Svc { convert(a: unknown, b: unknown, c?: unknown): void { void a; void b; void c; } }`n" +
        "export class Host {`n" +
        "  svc = new Svc();`n" +
        "  run() {`n" +
        "    const t = { Gamma: 'G' };`n" +
        "    this.svc.convert(HueIDs, hueLabels);`n" +
        "    this.svc.convert(1, hl);`n" +
        "    this.svc.convert('x', t);`n" +
        "  }`n" +
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
export function makeSlice() { return { DemoActions: { load: () => 1 }, DemoSelectors: { pick: () => 2 } }; }
export const { DemoActions, DemoSelectors } = makeSlice();
export const {
  a: { deep },
} = { a: { deep: { x: 1 } } };
'@
    'apps/shop/src/bind.use.ts' = @'
import { Alpha, Alpha as Aliased, Api, Beta, DemoActions, DemoSelectors, MODE, deep, project } from './bind.kinds';
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
export const demoUse = () => DemoActions.load() + DemoSelectors.pick() + deep.x;
'@
    'apps/shop/src/bind.loops.ts' = @'
import { Component, Input } from '@angular/core';
import { cloneDeep } from 'demo-lib';
import * as demo from 'demo-lib';
export class Looper {
  total(keys: string[], result: Record<string, number>): number {
    let sum = 0;
    for (const key of keys) { sum += result[key]; }
    for (const [name, qty] of Object.entries(result)) { sum += qty + name.length; }
    for (const idx in result) { sum += result[idx]; }
    try { sum += 1; } catch (err) { sum += String(err).length; }
    return sum;
  }
}
@Component({ selector: 'app-deco', template: '' })
export class Deco {
  @Input()
  label = 'x';
  @Input() other = 1;
  read(): string { return this.label + this.other; }
  copy(v: string[]): string[] { return cloneDeep(v); }
  both(v: string[]): number { return demo.cloneDeep(v).length; }
}
'@
    'apps/shop/src/bind.defaults.ts' = @'
export class Cell { name = ''; }
export const { alpha = 1, beta: gamma = 'x' } = { alpha: 2, beta: 'y' } as { alpha?: number; beta?: string };
export const [, second = new Cell()] = [new Cell()] as Cell[];
export function useArgs({ one = 1, two: dos = new Cell(), plain }: { one?: number; two?: Cell; plain: string }, [, third = 5]: number[]): number {
  const { p = 3, q: qq = 'z', zz } = { p: 1, q: 'w', zz: true } as { p?: number; q?: string; zz: boolean };
  const [, r = 2] = [1, 2] as number[];
  return one + p + third + r + dos.name.length + qq.length + plain.length + (zz ? 1 : 0);
}
export interface Holder { Input: { Params: { Tariff: number; Rate?: number } }; list: number[] }
export function pickTariff(x: Holder): number {
  const { Input: { Params: { Tariff: tariff, Rate = 2 } }, Input: simulation, list: [, first] } = x;
  return tariff + Rate + simulation.Params.Tariff + first;
}
'@
    'node_modules/demo-lib/package.json' = '{"name":"demo-lib","version":"1.0.0","types":"index.d.ts"}'
    'node_modules/demo-lib/index.d.ts' = @'
declare const _: _.DemoStatic;
declare namespace _ {
  interface DemoStatic {
    cloneDeep<T>(value: T): T;
  }
}
export = _;
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


# (rows 75) a loop variable (`for..of`, `for..in`, destructured too) and a `catch` binding are `locals` rows with no initializer, and a read of one is that local - the place
# of its row - as a read of a `const` with a value is. The controls: a parameter stays a `param`, and a name written beside it stays what it was.
Test-Case 'tsrows: a read of a loop variable or a catch binding carries local, the place of its row' {
    $db = Get-TsBindDb
    Assert-Line (Get-TsBindLinks $db 'result[key]') 'bind=key=local@7/16'
    Assert-Line (Get-TsBindLinks $db 'result[idx]') 'bind=idx=local@9/16'
    $d = Get-TsBindLinks $db 'qty + name.length'
    Assert-Line $d 'bind=qty=local@8/16'
    Assert-Line $d 'bind=name=local@8/16'
    Assert-Line (Get-TsBindLinks $db 'String(err).length') 'bind=err=local@10/30'
    Assert-Line (Get-TsBindLinks $db 'result[key]') 'bind=result=param@5/25'
    $row = Invoke-TsRowsQ $db "SELECT 'row=' || l.name || '@' || l.line || '/' || l.col AS r FROM locals l JOIN files f ON f.id = l.file WHERE f.path LIKE '%bind.loops.ts' AND l.name IN ('key', 'err')"
    Assert-Line $row 'row=key@7/16'
    Assert-Line $row 'row=err@10/30'
}

# (rows 75) a decorated member's target is its NAME's line - the line of its `members` row - not the decorator's, as a class's target already is.
Test-Case 'tsrows: a read of a decorated member targets its name line, not its decorator' {
    $db = Get-TsBindDb
    $r = Get-TsBindLinks $db 'this.label + this.other'
    Assert-Line $r 'bind=label=target@17'
    Assert-Line $r 'bind=other=target@18'
    Assert-Line (Invoke-TsRowsQ $db "SELECT 'member=' || m.name || '@' || m.line AS r FROM members m JOIN files f ON f.id = m.file WHERE f.path LIKE '%bind.loops.ts' AND m.name = 'label'") 'member=label@17'
    # Every target of that file's code is a place some row sits at: a `members` row or a `classes` row.
    $sql = "SELECT 'orphans=' || count(*) AS r FROM expressions x JOIN files xf ON xf.id = x.file JOIN expressions__ast t ON t.owner = x.id AND t.role = 'target' " +
        "JOIN expressions__ast tf ON tf.owner = x.id AND tf.parent = t.n AND tf.role = 'file' JOIN expressions__ast tl ON tl.owner = x.id AND tl.parent = t.n AND tl.role = 'line' " +
        "JOIN files df ON df.abs = tf.value WHERE xf.path LIKE '%bind.loops.ts' AND df.path LIKE '%bind.loops.ts' " +
        "AND NOT EXISTS (SELECT 1 FROM members m WHERE m.file = df.id AND m.line = tl.value) AND NOT EXISTS (SELECT 1 FROM classes c WHERE c.file = df.id AND c.line = tl.value)"
    Assert-Line (Invoke-TsRowsQ $db $sql) 'orphans=0'
}

# (rows 75) a bare name a LIBRARY declares as anything but a top-level function - here a method of an interface (`import { cloneDeep } from 'lodash'`) - targets that
# declaration in the library's file, as the property link `demo.cloneDeep` always did. Controls: the same method through its namespace import, and the namespace import itself.
Test-Case 'tsrows: a bare name a library declares as an interface method has a target in the library' {
    $db = Get-TsBindDb
    Assert-Line (Get-TsBindLinks $db 'cloneDeep(v)') 'bind=cloneDeep=target@4'
    $c = Get-TsBindLinks $db 'demo.cloneDeep(v).length'
    Assert-Line $c 'bind=cloneDeep=target@4'
    Assert-Line $c 'bind=demo=target@1'
    # The file is the library's, not the tree's.
    $f = Invoke-TsRowsQ $db ("SELECT 'file=' || substr(tf.value, instr(tf.value, 'node_modules')) AS r FROM expressions x JOIN expressions__ast k ON k.owner = x.id AND k.role = 'k' " +
        "JOIN expressions__ast nm ON nm.owner = x.id AND nm.parent = k.parent AND nm.role = 'name' AND nm.value = 'cloneDeep' " +
        "JOIN expressions__ast t ON t.owner = x.id AND t.parent = k.parent AND t.role = 'target' JOIN expressions__ast tf ON tf.owner = x.id AND tf.parent = t.n AND tf.role = 'file' " +
        "WHERE x.source = 'cloneDeep(v)'")
    Assert-Line $f 'file=node_modules/demo-lib/index.d.ts'
}

# (rows 76) a bare name a TOP-LEVEL DESTRUCTURING `const` binds (`export const { DemoActions } = makeSlice()`, a nested `{ a: { deep } }`) is a module declaration as a plain
# `const` is: its read has a `target` at the BOUND NAME's line, and a `consts` row sits there for that name, value-less (the initializer is not the name's value). Control: `MODE`.
Test-Case 'tsrows: a bare name a top-level destructuring const binds has a target that joins a consts row' {
    $db = Get-TsBindDb
    $r = Get-TsBindLinks $db 'DemoActions.load() + DemoSelectors.pick() + deep.x'
    Assert-Line $r 'bind=DemoActions=target@11'
    Assert-Line $r 'bind=DemoSelectors=target@11'
    Assert-Line $r 'bind=deep=target@13'
    Assert-Line (Get-TsBindLinks $db 'Beta.Parse(s) + MODE') 'bind=MODE=target@9'
    $k = Invoke-TsRowsQ $db "SELECT 'const=' || name || '@' || line || '/' || ifnull(destructured, 0) || '/' || ifnull(value, 'none') AS r FROM consts WHERE name IN ('DemoActions', 'DemoSelectors', 'deep', 'MODE')"
    Assert-Line $k 'const=DemoActions@11/1/none'
    Assert-Line $k 'const=DemoSelectors@11/1/none'
    Assert-Line $k 'const=deep@13/1/none'
    Assert-Line $k 'const=MODE@9/0/1'
}

# (rows 77) A NAME A DESTRUCTURING BINDS has its `default` (the evaluated `= 1`), its `type` (the checker's text for the name) and, in an ARRAY pattern, its `index` - a hole (`[, x]`)
# is skipped yet counts - on a `locals` row's `bindings`, a parameter's `bindings` items and the per-name `consts` rows of a top-level `const`. Control: a name with no default has none.
# One line per binding: `<name>|default=..|type=..|index=..`, `none` where the row has no such member.
function Get-TsBindingFacts([string]$Table, [string]$Owner, [string]$Where, [string]$Name, [string]$Chain) {
    $leaf = { param($key) "LEFT JOIN $Table k_$key ON k_$key.owner = it.owner AND k_$key.parent = it.n AND k_$key.role = '$key' " }
    return Invoke-TsRowsQ (Get-TsBindDb) ("SELECT nm.value || '|default=' || ifnull(k_default.value, 'none') || '|type=' || ifnull(k_type.value, 'none') || '|index=' || ifnull(k_index.value, 'none') AS r " +
        "FROM $Owner o JOIN files f ON f.id = o.file $Chain JOIN $Table nm ON nm.owner = it.owner AND nm.parent = it.n AND nm.role = '$Name' " +
        (& $leaf 'default') + (& $leaf 'type') + (& $leaf 'index') + "WHERE f.path LIKE '%bind.defaults.ts' AND $Where")
}

Test-Case 'tsrows: a name a local destructuring binds has its default, its type and its index in an array pattern' {
    $r = Get-TsBindingFacts 'locals__bindings' 'locals' 'o.destructured = 1' 'local' 'JOIN locals__bindings it ON it.owner = o.id AND it.parent = 0'
    Assert-Line $r 'p|default=3|type=number|index=none'
    Assert-Line $r 'qq|default=z|type=string|index=none'
    # THE CONTROL: no default, no `default` - the name still has its type.
    Assert-Line $r 'zz|default=none|type=boolean|index=none'
    # THE HOLE: `[, r = 2]` binds one name, and it is the SECOND element.
    Assert-Line $r 'r|default=2|type=number|index=1'
}

Test-Case 'tsrows: a name a destructured parameter binds has its default, its type and its index in an array pattern' {
    $r = Get-TsBindingFacts 'functions__params' 'functions' "o.name = 'useArgs'" 'name' ("JOIN functions__params pi ON pi.owner = o.id AND pi.parent = 0 " +
        "JOIN functions__params bl ON bl.owner = pi.owner AND bl.parent = pi.n AND bl.role = 'bindings' JOIN functions__params it ON it.owner = bl.owner AND it.parent = bl.n")
    Assert-Line $r 'one|default=1|type=number|index=none'
    Assert-Line $r 'dos|default=none|type=Cell|index=none'
    Assert-Line $r 'plain|default=none|type=string|index=none'
    Assert-Line $r 'third|default=5|type=number|index=1'
}

Test-Case 'tsrows: the per-name consts rows of a top-level destructuring have their default, their type and their index in an array pattern' {
    $r = Invoke-TsRowsQ (Get-TsBindDb) ("SELECT c.name || '|default=' || ifnull(c.""default"", 'none') || '|type=' || ifnull(c.type, 'none') || '|index=' || ifnull(c.""index"", 'none') AS r " +
        "FROM consts c JOIN files f ON f.id = c.file WHERE f.path LIKE '%bind.defaults.ts' AND c.destructured = 1")
    Assert-Line $r 'alpha|default=1|type=number|index=none'
    Assert-Line $r 'gamma|default=x|type=string|index=none'
    Assert-Line $r 'second|default=none|type=Cell|index=1'
    # The object default is a structure: `consts__default` holds its nodes, and the name's own `value` stays none.
    $d = Invoke-TsRowsQ (Get-TsBindDb) "SELECT 'default nodes=' || (count(*) > 0) AS r FROM consts c JOIN consts__default d ON d.owner = c.id WHERE c.name = 'second'"
    Assert-Line $d 'default nodes=1'
}

# (rows 78) A NESTED LOCAL DESTRUCTURING BINDS EVERY NAME, one `bindings` item per name at any depth (`TsBody.mjs::emitLocal`): `property` is the PATH to it (`Input.Params.Tariff`), as a parameter's or a
# top-level const's nested names are each their own. The outer `Input` listed once for the whole pattern, with the pattern's text for its `local`, hid `tariff`. Control: a plain top-level name keeps its key.
Test-Case 'tsrows: a nested local destructuring has one binding per bound name, with the path to it' {
    $r = Invoke-TsRowsQ (Get-TsBindDb) ("SELECT nm.value || '|property=' || ifnull(pr.value, 'none') || '|default=' || ifnull(df.value, 'none') || '|index=' || ifnull(ix.value, 'none') AS r " +
        "FROM locals o JOIN files f ON f.id = o.file JOIN locals__bindings it ON it.owner = o.id AND it.parent = 0 " +
        "JOIN locals__bindings nm ON nm.owner = it.owner AND nm.parent = it.n AND nm.role = 'local' " +
        "LEFT JOIN locals__bindings pr ON pr.owner = it.owner AND pr.parent = it.n AND pr.role = 'property' " +
        "LEFT JOIN locals__bindings df ON df.owner = it.owner AND df.parent = it.n AND df.role = 'default' " +
        "LEFT JOIN locals__bindings ix ON ix.owner = it.owner AND ix.parent = it.n AND ix.role = 'index' " +
        "WHERE f.path LIKE '%bind.defaults.ts' AND o.destructured = 1 AND o.line = 11")
    Assert-Line $r 'tariff|property=Input.Params.Tariff|default=none|index=none'
    Assert-Line $r 'Rate|property=Input.Params.Rate|default=2|index=none'
    Assert-Line $r 'simulation|property=Input|default=none|index=none'
    Assert-Line $r 'first|property=list|default=none|index=1'
    Assert-Line (Invoke-TsRowsQ (Get-TsBindDb) "SELECT 'items=' || count(*) AS r FROM locals o JOIN locals__bindings it ON it.owner = o.id AND it.parent = 0 WHERE o.line = 11") 'items=4'
    # THE CONTROL: a pattern with no nesting keeps its names and keys.
    $c = Get-TsBindingFacts 'locals__bindings' 'locals' 'o.destructured = 1' 'local' 'JOIN locals__bindings it ON it.owner = o.id AND it.parent = 0'
    Assert-Line $c 'p|default=3|type=number|index=none'
}

# (rows 79, #103) A CALL ARGUMENT THAT NAMES A VARIABLE KEEPS ITS DECLARATION: `calls.args` holds the const's evaluated value, `arg_refs` one item per such argument - its `position`, the
# `$ref` name declared, the declaring file and the line of its NAME, which a join to `consts` (module level) or `locals` (a body's) finds. An enum has its `$ref` in the value, and a literal names nothing.
Test-Case 'tsrows: a call argument naming a const has an arg_refs item joining the declaration, an alias names the declared const' {
    $db = Get-TsChainDb
    $items = "FROM calls c JOIN files cf ON cf.id = c.file JOIN calls__arg_refs i ON i.owner = c.id AND i.parent = 0 " +
        "JOIN calls__arg_refs a ON a.owner = i.owner AND a.parent = i.n AND a.role = 'position' " +
        "JOIN calls__arg_refs r ON r.owner = i.owner AND r.parent = i.n AND r.role = '`$ref' " +
        "JOIN calls__arg_refs w ON w.owner = i.owner AND w.parent = i.n AND w.role = 'name' " +
        "JOIN calls__arg_refs k ON k.owner = i.owner AND k.parent = i.n AND k.role = 'kind' " +
        "JOIN calls__arg_refs l ON l.owner = i.owner AND l.parent = i.n AND l.role = 'line' " +
        "JOIN calls__arg_refs d ON d.owner = i.owner AND d.parent = i.n AND d.role = 'file' " +
        "WHERE cf.path LIKE '%argrefs.ts'"
    $r = Invoke-TsRowsQ $db ("SELECT 'item=' || c.line || '|' || a.value || '|' || w.value || '|' || r.value || '|' || k.value || '|' || l.value || '|' || " +
        "(SELECT count(*) FROM consts q JOIN files qf ON qf.id = q.file WHERE qf.abs = d.value AND q.line = l.value AND q.name = r.value) || '|' || " +
        "(SELECT count(*) FROM locals q JOIN files qf ON qf.id = q.file WHERE qf.abs = d.value AND q.line = l.value AND q.name = r.value) AS r $items ORDER BY c.line")
    Assert-Line $r 'item=9|1|hueLabels|hueLabels|const|2|1|0'
    Assert-Line $r 'item=10|1|hl|hueLabels|const|2|1|0'
    Assert-Line $r 'item=11|1|t|t|const|8|0|1'
    # THE CONTROLS: the enum, the literals and the number name no variable - three items in all - and the value of the argument is still the evaluated const.
    Assert-Line (Invoke-TsRowsQ $db "SELECT 'items=' || count(*) AS r $items") 'items=3'
    $v = Invoke-TsRowsQ $db ("SELECT 'value=' || $(Get-TsItemFieldSql 'calls__args' 'c.id' 1 'Alpha') AS r FROM calls c JOIN files cf ON cf.id = c.file WHERE cf.path LIKE '%argrefs.ts' AND c.line = 9")
    Assert-Line $v 'value=A'
}

}
