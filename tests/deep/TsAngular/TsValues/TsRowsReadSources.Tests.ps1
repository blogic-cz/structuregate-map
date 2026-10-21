<#
    A TEMPLATE READ FOLLOWS EVERY VALUE A NAME CAN HANG FROM (rows 72, `TsTpl/TsTplReads.mjs::valueOf`, `TsTpl/TsTplOwners.mjs`): a loop variable
    and an `as` alias are typed by the value of their collection or condition - a call of an un-annotated method, a pipe over one, an `@for` -
    never by the first name met in it; a spread of a literal holds what it spreads; a generic alias binds its argument (`Frame<{ .. }>`); a type parameter reads as its constraint, and as its
    default where nothing is passed; an interface reads the members of its bases; `X & { .. }` and a list of one name both the class and the literal.
    The fixture's lines are pinned: `src.models.ts` declares each member on the line its case names. Its helpers are `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '../../TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

$script:TsSrcFiles = @{
    'apps/shop/src/src.models.ts' = @'
import { Observable, of } from 'rxjs';
export class AlphaDto { Id = 0; }
export class AlphaService {
  private all: AlphaDto[] = [];
  list() { return this.all; }
  first() { return this.all[0]; }
  stream() { const o: Observable<AlphaDto[]> = of(this.all); return o; }
  pick(): AlphaDto[] { return this.all; }
  tagged() { const t: Tagged[] = []; return t; }
}
export type Frame<TData> = { shown: boolean; data: TData };
export interface Row { selected: boolean; }
export class Bag { [key: string]: AlphaDto; }
export class Slot<TItem = AlphaDto> { Item!: TItem; }
export type Tagged = AlphaDto & { Extra?: number };
export interface BaseDoc { Code: string; }
export interface SavedDoc extends BaseDoc { Saved: boolean; }
export class Pair<TKey> { Key!: TKey; }
export interface Flags { On: boolean; }
export type Joined = Pair<AlphaDto> & { Key: { Flag?: Flags } };
export class Line { Declared!: BaseDoc; }
export type Lines = { Items: (Line & { open: boolean })[] };
export type Desc = Readonly<{ Title: string }>;
'@
    'apps/shop/src/src.component.ts' = @'
import { Component } from '@angular/core';
import { Observable, map, of } from 'rxjs';
import { AlphaDto, AlphaService, Bag, Desc, Frame, Joined, Lines, Row, SavedDoc, Slot, Tagged } from './src.models';
@Component({
  selector: 'app-src',
  standalone: true,
  template: `
<p *ngFor="let a of svc.list()">{{ a.Id }}</p>
<p *ngIf="svc.first() as one">{{ one.Id }}</p>
<p *ngFor="let s of svc.stream() | async">{{ s.Id }}</p>
<p *ngFor="let c of wrap(svc.pick())">{{ c.Id }}</p>
@for (f of svc.list(); track f.Id) { <b>{{ f.Id }}</b> }
<p>{{ frame.data.title }}</p>
<p *ngFor="let v of frame.data.values">{{ v.label }}</p>
<p>{{ (state$ | async)?.size }}</p>
<p>{{ (made$ | async)?.count }}</p>
<p>{{ (spread$ | async)?.count }}{{ (spread$ | async)?.extra }}</p>
<p>{{ bag[key].Id }}</p>
<p>{{ slots[key].Item.Id }}</p><p *ngFor="let sl of slotList">{{ sl.Item.Id }}</p>
<p>{{ tagged.Id }}{{ tagged.Extra }}</p>
<p *ngFor="let t of svc.tagged()">{{ t.Extra }}</p>
<p>{{ saved.Code }}</p><p>{{ desc.Title }}</p>
<p>{{ joined.Key.Flag }}</p>
<p *ngFor="let l of lines.Items">{{ l.Declared.Code }}{{ l.open }}</p>
`,
})
export class SrcComponent {
  constructor(public svc: AlphaService) {}
  key = '';
  frame!: Frame<{ title: string; values: { label: string }[] }>;
  state$!: Observable<{ size: number }>;
  initial = { count: 1 };
  made$ = of(this.initial);
  spread$ = this.made$.pipe(map((x) => ({ ...x, extra: 1 })));
  bag!: Bag;
  slots!: { [id: string]: Slot };
  tagged!: Tagged;
  saved!: SavedDoc;
  joined!: Joined;
  lines!: Lines;
  desc!: Desc;
  slotList!: Slot[];
  wrap(list: AlphaDto[]) { return list.map((x) => x.Id); }
}
@Component({
  selector: 'app-grid',
  standalone: true,
  template: `<p *ngFor="let r of rows">{{ r.selected }}</p><p>{{ one.selected }}</p>`,
})
export class GridComponent<T extends Row> {
  rows: T[] = [];
  one!: T;
}
'@
    'apps/shop/src/more.models.ts' = @'
export class BaseP { Code = ''; }
export class KindA extends BaseP { A = 1; }
export class KindB extends BaseP { B = 1; }
export type Dto = {
  keep: string;
  drop: number;
};
export interface Warn { label: string; }
export class Slot2<TItem = Warn> {
  Items!: { Row: TItem }[];
}
export type Settings = {
  pageSize: number;
};
export class Holder<T> {
  Input!: T;
}
export type HolderA = Holder<KindA>;
export type HolderB = Holder<KindB>;
export class Host {
  warnings!: Warn[];
}
'@
    'apps/shop/src/more.component.ts' = @'
import { Component } from '@angular/core';
import { Observable, map } from 'rxjs';
import { Dto, Host, HolderA, HolderB, KindA, KindB, Settings, Slot2, Warn } from './more.models';
@Component({
  selector: 'app-more',
  standalone: true,
  template: `
<p>{{ (picked$ | async)?.keep }}</p><p>{{ (picked$ | async)?.drop }}</p><p>{{ (picked$ | async)?.pageSize }}</p>
<p *ngFor="let w of warnings">{{ w.label }}</p>
<p>{{ slot.Items[0].Row.label }}</p>
<p>{{ (merged$ | async)?.pageSize }}</p>
<p>{{ either.Input }}</p><p>{{ either.Input.Code }}</p><p>{{ either.Input.A }}</p>
`,
})
export class MoreComponent {
  picked$!: Observable<Pick<Dto, 'keep'> & Pick<Settings, 'pageSize'>>;
  host!: Host;
  warnings: this['host']['warnings'] = [];
  slot!: Slot2;
  merged$ = this.picked$.pipe(map((x) => ({ ...x, column: 1 })));
  either!: HolderA | HolderB;
}
@Component({ selector: 'app-gen', standalone: true, template: `<p>{{ person.Code }}</p>` })
export class GenComponent<T extends KindA | KindB> {
  person!: T;
}
'@
}

# ONE MAP PER SUITE PROCESS (`-Shared`): every case reads the same tree.
function Get-TsSrcDb {
    $made = New-TsRowsDb (New-TsRowsWorkspace $script:TsSrcFiles) -Shared
    Assert-Exit $made.Result 0
    return $made.Db
}

# Every Read and SafeRead of the expressions whose source holds `$Like`: `read=<source>|<name>=<row prefix>@<file>:<line>` of the declaration its `target`
# names, or `none` - the row prefix says which table (`m` member, `tm` type member, `sm` shape member), the place which declaration.
function Get-TsSrcTargets([string]$Like) {
    $r = Invoke-Gate --map-query (Get-TsSrcDb) --width 0 --limit 0 --sql ("SELECT 'read=' || x.source || '|' || nm.value || '=' || (CASE WHEN rw.value IS NULL THEN 'none' ELSE " +
        "substr(rw.value, 1, instr(rw.value, ':') - 1) || '@' || tf.value || ':' || tl.value END) AS r " +
        "FROM expressions x JOIN expressions__ast k ON k.owner = x.id AND k.role = 'k' AND k.value IN ('Read', 'SafeRead') " +
        "JOIN expressions__ast nm ON nm.owner = x.id AND nm.parent = k.parent AND nm.role = 'name' " +
        "LEFT JOIN expressions__ast t ON t.owner = x.id AND t.parent = k.parent AND t.role = 'target' " +
        "LEFT JOIN expressions__ast rw ON rw.owner = x.id AND rw.parent = t.n AND rw.role = 'row' " +
        "LEFT JOIN expressions__ast tf ON tf.owner = x.id AND tf.parent = t.n AND tf.role = 'file' " +
        "LEFT JOIN expressions__ast tl ON tl.owner = x.id AND tl.parent = t.n AND tl.role = 'line' " +
        "WHERE x.source LIKE '$Like' ORDER BY r")
    Assert-Exit $r 0
    return $r
}


# A: THE VALUE OF A CALL, not the first name met in the collection. `list()` has no annotation, so its type is only what it returns.
Test-Case 'tsrows: a loop variable or alias over a call of an un-annotated method is typed by what the method returns' {
    # The control: the call's own hop resolved before, and a later hop of the same chain.
    Assert-Line (Get-TsSrcTargets 'svc.list()') 'read=svc.list()|list=m@apps/shop/src/src.models.ts:5'
    Assert-Line (Get-TsSrcTargets '{{ a.Id }}') 'read={{ a.Id }}|Id=m@apps/shop/src/src.models.ts:2'
    Assert-Line (Get-TsSrcTargets '{{ f.Id }}') 'read={{ f.Id }}|Id=m@apps/shop/src/src.models.ts:2'
    # An `as` alias over a call, and a loop over a pipe over a call (`async` unwraps the observable the method returns).
    Assert-Line (Get-TsSrcTargets '{{ one.Id }}') 'read={{ one.Id }}|Id=m@apps/shop/src/src.models.ts:2'
    Assert-Line (Get-TsSrcTargets '{{ s.Id }}') 'read={{ s.Id }}|Id=m@apps/shop/src/src.models.ts:2'
}

Test-Case 'tsrows: a loop over a call is typed by the call, never by an argument of it' {
    # `wrap(list)` returns numbers; its argument `svc.pick()` holds `AlphaDto[]`, which must not type `c`.
    Assert-Line (Get-TsSrcTargets 'wrap(svc.pick())') 'read=wrap(svc.pick())|pick=m@apps/shop/src/src.models.ts:8'
    Assert-Line (Get-TsSrcTargets '{{ c.Id }}') 'read={{ c.Id }}|Id=none'
}

# B: A GENERIC ALIAS BINDS ITS ARGUMENT, here an anonymous type.
Test-Case 'tsrows: a generic alias instantiated with a type literal reads through the literal' {
    # The control: the alias's own member.
    Assert-Line (Get-TsSrcTargets '{{ frame.data.title }}') 'read={{ frame.data.title }}|data=tm@apps/shop/src/src.models.ts:11'
    Assert-Line (Get-TsSrcTargets '{{ frame.data.title }}') 'read={{ frame.data.title }}|title=sm@apps/shop/src/src.component.ts:30'
    # The loop over a member of the literal: its element is another literal.
    Assert-Line (Get-TsSrcTargets '{{ v.label }}') 'read={{ v.label }}|label=sm@apps/shop/src/src.component.ts:30'
}

# C: A PIPE OVER AN OBSERVABLE OF AN ANONYMOUS TYPE.
Test-Case 'tsrows: an async pipe over an observable of an anonymous object type reads through it' {
    Assert-Line (Get-TsSrcTargets '{{ (state$ | async)?.size }}') 'read={{ (state$ | async)?.size }}|state$=m@apps/shop/src/src.component.ts:31'
    Assert-Line (Get-TsSrcTargets '{{ (state$ | async)?.size }}') 'read={{ (state$ | async)?.size }}|size=sm@apps/shop/src/src.component.ts:31'
    # An object literal the compiler infers as the observable's type.
    Assert-Line (Get-TsSrcTargets '{{ (made$ | async)?.count }}') 'read={{ (made$ | async)?.count }}|count=sm@apps/shop/src/src.component.ts:32'
    # A literal that spreads another (`map((x) => ({ ...x, extra: 1 }))`): the names of the spread, and the literal's own.
    Assert-Line (Get-TsSrcTargets '{{ (spread$ | async)?.count }}%') '|count=sm@apps/shop/src/src.component.ts:32'
    Assert-Line (Get-TsSrcTargets '{{ (spread$ | async)?.count }}%') '|extra=sm@apps/shop/src/src.component.ts:34'
}

# D: A TYPE PARAMETER READS AS ITS CONSTRAINT.
Test-Case 'tsrows: a member typed by a type parameter reads as the parameter''s constraint' {
    Assert-Line (Get-TsSrcTargets '{{ one.selected }}') 'read={{ one.selected }}|one=m@apps/shop/src/src.component.ts:52'
    Assert-Line (Get-TsSrcTargets '{{ one.selected }}') 'read={{ one.selected }}|selected=tm@apps/shop/src/src.models.ts:12'
    # The element of a list of the parameter.
    Assert-Line (Get-TsSrcTargets '{{ r.selected }}') 'read={{ r.selected }}|selected=tm@apps/shop/src/src.models.ts:12'
}

# E: A TYPE ALIAS OF AN INTERSECTION, and a loop over a list of it.
Test-Case 'tsrows: a read through an alias of an intersection and through a list of it' {
    Assert-Line (Get-TsSrcTargets 'svc.tagged()') 'read=svc.tagged()|tagged=m@apps/shop/src/src.models.ts:9'
    Assert-Line (Get-TsSrcTargets '{{ tagged.Id }}%') '|Id=tm@apps/shop/src/src.models.ts:2'
    Assert-Line (Get-TsSrcTargets '{{ tagged.Id }}%') '|Extra=tm@apps/shop/src/src.models.ts:15'
    Assert-Line (Get-TsSrcTargets '{{ t.Extra }}') 'read={{ t.Extra }}|Extra=tm@apps/shop/src/src.models.ts:15'
}

# F: A KEYED READ, over a class with an index signature, and over a class whose parameter nothing passes (its default).
Test-Case 'tsrows: a keyed read holds what the index signature or the default of an unpassed parameter names' {
    Assert-Line (Get-TsSrcTargets '{{ bag[key].Id }}') 'read={{ bag[key].Id }}|bag=m@apps/shop/src/src.component.ts:35'
    Assert-Line (Get-TsSrcTargets '{{ bag[key].Id }}') 'read={{ bag[key].Id }}|Id=m@apps/shop/src/src.models.ts:2'
    Assert-Line (Get-TsSrcTargets '{{ slots[key].Item.Id }}') 'read={{ slots[key].Item.Id }}|Item=m@apps/shop/src/src.models.ts:14'
    Assert-Line (Get-TsSrcTargets '{{ slots[key].Item.Id }}') 'read={{ slots[key].Item.Id }}|Id=m@apps/shop/src/src.models.ts:2'
    # A list of the class: its element names the class with no argument at all, so the parameter is the default.
    Assert-Line (Get-TsSrcTargets '{{ sl.Item.Id }}') 'read={{ sl.Item.Id }}|Id=m@apps/shop/src/src.models.ts:2'
}

# G: AN INTERFACE'S BASES, and the element of an unnamed intersection.
Test-Case 'tsrows: an interface reads the members of its bases' {
    Assert-Line (Get-TsSrcTargets '{{ saved.Code }}') 'read={{ saved.Code }}|saved=m@apps/shop/src/src.component.ts:38'
    Assert-Line (Get-TsSrcTargets '{{ saved.Code }}') 'read={{ saved.Code }}|Code=tm@apps/shop/src/src.models.ts:16'
}

Test-Case 'tsrows: a member typed by an alias of a library generic reads through the alias' {
    # `type Desc = Readonly<{ Title: string }>`: the checker's type is `Readonly`, the alias written is `Desc`, whose computed members answer.
    Assert-Line (Get-TsSrcTargets '{{ desc.Title }}') 'read={{ desc.Title }}|desc=m@apps/shop/src/src.component.ts:41'
    Assert-Line (Get-TsSrcTargets '{{ desc.Title }}') 'read={{ desc.Title }}|Title=tm@apps/shop/src/src.models.ts:23'
}

Test-Case 'tsrows: a loop over a list of an intersection names the class and the literal' {
    Assert-Line (Get-TsSrcTargets 'lines.Items') 'read=lines.Items|Items=tm@apps/shop/src/src.models.ts:22'
    Assert-Line (Get-TsSrcTargets '{{ l.Declared.Code }}%') '|Declared=m@apps/shop/src/src.models.ts:21'
    Assert-Line (Get-TsSrcTargets '{{ l.Declared.Code }}%') '|open=sm@apps/shop/src/src.models.ts:22'
}

# R3: THE LITERAL OF AN INTERSECTION IS ITS MEMBERS TOO, in a member typed by one.
Test-Case 'tsrows: the literal an intersection drops from a member''s type still answers its names' {
    Assert-Line (Get-TsSrcTargets '{{ joined.Key.Flag }}') 'read={{ joined.Key.Flag }}|Key=tm@apps/shop/src/src.models.ts:18'
    Assert-Line (Get-TsSrcTargets '{{ joined.Key.Flag }}') 'read={{ joined.Key.Flag }}|Flag=sm@apps/shop/src/src.models.ts:20'
}

# H: WHAT NO DECLARATION NAMES - a mapped type, a type parameter bounded by a union, `this[..]`, a literal in a generic class, a spread of an intersection.
Test-Case 'tsrows: a read past a mapped type reaches only the members the mapping kept' {
    Assert-Line (Get-TsSrcTargets '{{ (picked$ | async)?.keep }}') 'read={{ (picked$ | async)?.keep }}|keep=tm@apps/shop/src/more.models.ts:5'
    Assert-Line (Get-TsSrcTargets '{{ (picked$ | async)?.pageSize }}') 'read={{ (picked$ | async)?.pageSize }}|pageSize=tm@apps/shop/src/more.models.ts:13'
    Assert-Line (Get-TsSrcTargets '{{ (picked$ | async)?.drop }}') 'read={{ (picked$ | async)?.drop }}|drop=none'
}

Test-Case 'tsrows: a literal spreading an intersection answers the names of its parts' {
    Assert-Line (Get-TsSrcTargets '{{ (merged$ | async)?.pageSize }}') 'read={{ (merged$ | async)?.pageSize }}|pageSize=tm@apps/shop/src/more.models.ts:13'
}

Test-Case 'tsrows: a type parameter bounded by a union reads as what the members of the union name' {
    Assert-Line (Get-TsSrcTargets '{{ person.Code }}') 'read={{ person.Code }}|Code=m@apps/shop/src/more.models.ts:1'
}

Test-Case 'tsrows: a member typed by an indexed access on this reads as the property''s own type' {
    Assert-Line (Get-TsSrcTargets '{{ w.label }}') 'read={{ w.label }}|label=tm@apps/shop/src/more.models.ts:8'
}

Test-Case 'tsrows: a literal in a generic class reads its members with the bindings of the class' {
    Assert-Line (Get-TsSrcTargets '{{ slot.Items[0].Row.label }}') 'read={{ slot.Items[0].Row.label }}|label=tm@apps/shop/src/more.models.ts:8'
}

Test-Case 'tsrows: a union of two aliases of one generic reaches the member they share, and what the arms agree on' {
    # `Input` is declared once, at the generic class; each alias carries its own row for it, at that place.
    Assert-Line (Get-TsSrcTargets '{{ either.Input }}') 'read={{ either.Input }}|Input=tm@apps/shop/src/more.models.ts:16'
    Assert-Line (Get-TsSrcTargets '{{ either.Input.Code }}') 'read={{ either.Input.Code }}|Code=m@apps/shop/src/more.models.ts:1'
    # A name only ONE arm's type declares is that arm's (#97).
    Assert-Line (Get-TsSrcTargets '{{ either.Input.A }}') 'read={{ either.Input.A }}|A=m@apps/shop/src/more.models.ts:2'
}

}
