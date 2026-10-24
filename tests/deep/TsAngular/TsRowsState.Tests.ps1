<#
    The Angular half's state graph (`state_handlers`, `state_selectors`) and the input counts that a static
    attribute fills. See `src/TsRows/TsDerive/TsNgrx.mjs` and `TsInputUsage.mjs`. Helpers are `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '../TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

# A REDUCER'S HANDLERS MAY BE SPREAD IN from a list const (same file or imported), and a selector may be built
# by a factory function: both are how real stores are written, and both read as "no handlers / no selectors".
Test-Case 'tsrows: a spread list of handlers and a selector factory reach the state tables' {
    $files = @{
        'apps/shop/src/demo.actions.ts' = "import { createAction } from '@ngrx/store';`n" +
            "export const load = createAction('[Demo] load');`n" +
            "export const clear = createAction('[Demo] clear');`n"
        'apps/shop/src/demo.ons.ts' = "import { on, ReducerTypes, ActionCreator } from '@ngrx/store';`n" +
            "import * as demoActions from './demo.actions';`n" +
            "export interface S { n: number; items: { id: string }[] }`n" +
            "export const demoOns: ReducerTypes<S, readonly ActionCreator[]>[] = [on(demoActions.load, (s) => ({ ...s, n: 1 }))];`n"
        'apps/shop/src/demo.reducer.ts' = "import { createReducer, on } from '@ngrx/store';`n" +
            "import * as demoActions from './demo.actions';`n" +
            "import { S, demoOns } from './demo.ons';`n" +
            "const localOns = [on(demoActions.clear, (s: S) => ({ ...s, n: 0 }))];`n" +
            "export const demoReducer = createReducer({ n: 0, items: [] } as S, ...demoOns, ...localOns);`n"
        'apps/shop/src/demo.selectors.ts' = "import { createSelector } from '@ngrx/store';`n" +
            "import { S } from './demo.ons';`n" +
            "const getItems = (s: S) => s.items;`n" +
            "export const byId = (id: string) => createSelector(getItems, (items) => items.find((i) => i.id === id));`n" +
            "export function byIdFn(id: string) { return createSelector(getItems, (items) => items.find((i) => i.id === id)); }`n" +
            "export const DemoSelectors = { byKey: (id: string) => createSelector(getItems, (items) => items.find((i) => i.id === id)) };`n"
    }
    $made = New-TsRowsDb (New-TsRowsWorkspace $files)
    Assert-Exit $made.Result 0
    $h = Invoke-TsRowsQ $made.Db ("SELECT 'h=' || reducer_name || ' | ' || action_source FROM state_handlers " +
        "WHERE reducer_name = 'demoReducer'")
    Assert-Line $h 'h=demoReducer | demoActions.load'
    Assert-Line $h 'h=demoReducer | demoActions.clear'
    $s = Invoke-TsRowsQ $made.Db ("SELECT 's=' || name || ' | ' || input_count FROM state_selectors " +
        "WHERE factory IS NOT NULL")
    Assert-Line $s 's=byId | 1'
    Assert-Line $s 's=byIdFn | 1'
    # a factory held as a const object's property is named `<const>.<property>`
    Assert-Line $s 's=DemoSelectors.byKey | 1'
}

# AN INPUT IS ALSO SET BY A STATIC ATTRIBUTE: `<demo-beta title="x">` fills `title` exactly as `[title]="x"` does.
Test-Case 'tsrows: a static attribute that sets an input is counted as a binding of it' {
    $files = @{
        'apps/shop/src/demo.inputs.ts' = "import { Component, Input } from '@angular/core';`n" +
            "@Component({ selector: 'demo-beta', template: '', standalone: true })`n" +
            "export class BetaComponent { @Input() title = ''; @Input() plain = false; @Input() never = 0; }`n" +
            "@Component({ selector: 'demo-alpha', standalone: true, imports: [BetaComponent],`n" +
            "  template: '<demo-beta title=`"x`" plain class=`"c`"></demo-beta>' })`n" +
            "export class AlphaComponent {}`n"
    }
    $made = New-TsRowsDb (New-TsRowsWorkspace $files)
    Assert-Exit $made.Result 0
    $r = Invoke-TsRowsQ $made.Db ("SELECT 'i=' || member || ' | ' || bound_count FROM io " +
        "WHERE component = 'BetaComponent' AND kind = 'input'")
    Assert-Line $r 'i=title | 1'
    Assert-Line $r 'i=plain | 1'
    Assert-Line $r 'i=never | 0'
}


# AN NGRX EFFECT IS A MEMBER HOLDING `createEffect(() => ...)`: the actions it listens to are the `ofType` arguments inside
# that callback, which a property initializer that is not itself a function used to leave unwalked.
Test-Case 'tsrows: a createEffect member names the actions it listens to, whether it dispatches and what it emits' {
    $files = @{
        'apps/shop/src/fx.actions.ts' = "import { createAction } from '@ngrx/store';`n" +
            "export const load = createAction('[Demo] load');`n" +
            "export const clear = createAction('[Demo] clear');`n" +
            "export const done = createAction('[Demo] done');`n"
        'apps/shop/src/fx.effects.ts' = "import { Injectable, inject } from '@angular/core';`n" +
            "import { Actions, createEffect, ofType } from '@ngrx/effects';`n" +
            "import { map, tap } from 'rxjs';`n" +
            "import * as demoActions from './fx.actions';`n" +
            "@Injectable() export class DemoEffects {`n" +
            "  private actions`$ = inject(Actions);`n" +
            "  loadAlpha`$ = createEffect(() => this.actions`$.pipe(ofType(demoActions.load, demoActions.clear), map(() => demoActions.done())));`n" +
            "  logAlpha`$ = createEffect(() => this.actions`$.pipe(ofType(demoActions.done), tap(() => {})), { dispatch: false });`n" +
            "}`n"
    }
    $made = New-TsRowsDb (New-TsRowsWorkspace $files)
    Assert-Exit $made.Result 0
    $e = Invoke-Gate --map-query $made.Db --width 0 --sql ("SELECT 'e=' || name || ' | ' || $(Get-NodeRootListSql 'state_effects__actions' 'state_effects.id' ', ') || ' | ' || dispatch || ' | ' || coalesce($(Get-NodeRootListSql 'state_effects__emits' 'state_effects.id' ', '),'') AS r FROM state_effects")
    Assert-Line $e 'e=loadAlpha$ | ["demoActions.load", "demoActions.clear"] | 1 | ["demoActions.done"]'
    Assert-Line $e 'e=logAlpha$ | ["demoActions.done"] | 0 | []'
    # The effect's callback is a function of its own (line 7), its calls under it - `state_effects` reads them through it.
    $fx = Invoke-TsRowsQ $made.Db ("SELECT 'fx=' || group_concat(f.line || ':' || f.col, ',') AS r FROM functions f JOIN files p ON p.id = f.file " +
        "WHERE p.path = 'apps/shop/src/fx.effects.ts' AND f.inline = 1 AND f.line = 7 AND f.col < 30")
    Assert-Line $fx 'fx=7:'
}

# A REDUCER ENTRY IS `on` OF `@ngrx/store` OR `immerOn` OF `ngrx-immer` (by the declaration the checker reached), and its HANDLER is the
# LAST argument: every one before it is an action, so `on(A, B, fn)` is two handlers. An immer handler returns nothing - it writes by
# MUTATING its draft - so its `writes` are the paths it assigns, pushes into or reaches through a local (`list.active`); a handler that
# returns a new state names its keys as before.
Test-Case 'tsrows: immerOn and a multi-action on are one state_handlers row per action, an immer handler writes its draft paths' {
    $files = @{
        'node_modules/ngrx-immer/package.json' = '{"name":"ngrx-immer","version":"1.0.0"}'
        'node_modules/ngrx-immer/store/index.d.ts' = @'
import { ActionCreator, ReducerTypes } from '@ngrx/store';
export declare function immerOn<State, Creators extends readonly ActionCreator[]>(...args: [...creators: Creators, reducer: (state: State, action: any) => void]): ReducerTypes<State, Creators>;
'@
        'apps/shop/src/cart.actions.ts' = @'
import { createAction } from '@ngrx/store';
export const add = createAction('[Demo] add');
export const remove = createAction('[Demo] remove');
export const reset = createAction('[Demo] reset');
export const clear = createAction('[Demo] clear');
'@
        'apps/shop/src/cart.reducer.ts' = @'
import { createReducer, on } from '@ngrx/store';
import { immerOn } from 'ngrx-immer/store';
import * as cart from './cart.actions';
export interface Line { id: number; active: boolean; }
export interface S { n: number; list: Line[]; flag: { on: boolean }; }
const initial: S = { n: 0, list: [], flag: { on: false } };
export const cartReducer = createReducer(initial,
  immerOn(cart.add, (s) => { s.n = 1; s.list.push({ id: 1, active: true }); }),
  immerOn(cart.remove, cart.reset, (s) => { const t = s.list.find((l) => l.id === 1); if (t) { t.active = false; } s.flag.on = true; }),
  on(cart.clear, (s) => ({ ...s, n: 0 })));
export const plainReducer = createReducer(initial,
  on(cart.add, cart.remove, (s) => ({ ...s, n: 2 })));
'@
    }
    $made = New-TsRowsDb (New-TsRowsWorkspace $files)
    Assert-Exit $made.Result 0
    $h = Invoke-TsRowsQ $made.Db ("SELECT 'h=' || h.reducer_name || ' | ' || h.action_source || ' | ' || coalesce((SELECT group_concat(w.value) " +
        "FROM state_handlers__writes w WHERE w.owner = h.id AND w.value IS NOT NULL), '-') AS r FROM state_handlers h")
    Assert-Line $h 'h=cartReducer | cart.add | n,list'
    Assert-Line $h 'h=cartReducer | cart.remove | list.active,flag.on'
    Assert-Line $h 'h=cartReducer | cart.reset | list.active,flag.on'
    # the control: `on` of `@ngrx/store` returning a new state, with one action and with two
    Assert-Line $h 'h=cartReducer | cart.clear | n'
    Assert-Line $h 'h=plainReducer | cart.add | n'
    Assert-Line $h 'h=plainReducer | cart.remove | n'
    $n = Invoke-TsRowsQ $made.Db "SELECT 'n=' || count(*) AS r FROM state_handlers WHERE reducer_name IN ('cartReducer', 'plainReducer')"
    Assert-Line $n 'n=6'
}

# A `createEffect` IN A CONSTRUCTOR is an effect: a class lists several there without a property each. The row is the CALL (by its
# `@ngrx/effects` declaration), `member` the constructor's row, `name` `constructor@line:col`; the actions it listens to and what it
# emits come from the callback THAT call holds, never from the constructor as a whole. A property effect beside it is as before.
Test-Case 'tsrows: a createEffect in a constructor body is a state_effects row of its own' {
    $files = @{
        'apps/shop/src/fx.actions.ts' = @'
import { createAction } from '@ngrx/store';
export const add = createAction('[Demo] add');
export const remove = createAction('[Demo] remove');
export const clear = createAction('[Demo] clear');
export const reset = createAction('[Demo] reset');
'@
        'apps/shop/src/fx.effects.ts' = @'
import { Injectable } from '@angular/core';
import { Actions, createEffect, ofType } from '@ngrx/effects';
import { map } from 'rxjs';
import * as cart from './fx.actions';
@Injectable() export class CartEffects {
  constructor(private actions$: Actions) {
    [createEffect(() => this.actions$.pipe(ofType(cart.add), map(() => cart.reset()))),
     createEffect(() => this.actions$.pipe(ofType(cart.remove, cart.clear)), { dispatch: false })];
  }
}
export class ListEffects {
  member$ = createEffect(() => this.actions$.pipe(ofType(cart.add)), { dispatch: false });
  constructor(private actions$: Actions) {}
}
'@
    }
    $made = New-TsRowsDb (New-TsRowsWorkspace $files)
    Assert-Exit $made.Result 0
    $e = Invoke-TsRowsQ $made.Db ("SELECT 'e=' || (SELECT k.name FROM classes k WHERE k.id = e.class) || '.' || e.name || ' | ' || coalesce((SELECT group_concat(a.value) FROM state_effects__actions a " +
        "WHERE a.owner = e.id AND a.value IS NOT NULL), '-') || ' | ' || e.dispatch || ' | ' || coalesce((SELECT group_concat(a.value) FROM state_effects__emits a " +
        "WHERE a.owner = e.id AND a.value IS NOT NULL), '-') AS r FROM state_effects e")
    Assert-Line $e 'e=ListEffects.member$ | cart.add | 0 | -'
    Assert-Line $e 'e=CartEffects.constructor@7:6 | cart.add | 1 | cart.reset'
    Assert-Line $e 'e=CartEffects.constructor@8:6 | cart.remove,cart.clear | 0 | -'
    $n = Invoke-TsRowsQ $made.Db "SELECT 'n=' || count(*) AS r FROM state_effects"
    Assert-Line $n 'n=3'
}
# (rows 99) AN IMMER HANDLER WRITES THROUGH THE HELPERS IT CALLS (`TsExprDescribe.mjs::draftWrites`): `setLine((g) => (g.active = x), id, s.cart)` hands the draft's `cart`
# to a helper (another file) that finds a line under its `bag.lines` and calls the callback with it - so the handler writes `cart.lines.active`; a helper handing
# a callback on to a second helper (`viaPlan`) is walked the same way, and a helper that mutates its parameter directly (`bump`) writes `cart.count`.
# THE CONTROL: a helper given no draft path (`log(1)`) writes nothing, and one that only reads it (`peek(s.cart)`) writes nothing.
Test-Case 'tsrows: an immer handler writes through the helpers it hands its draft to' {
    $files = @{
        'node_modules/ngrx-immer/package.json' = '{"name":"ngrx-immer","version":"1.0.0"}'
        'node_modules/ngrx-immer/store/index.d.ts' = @'
import { ActionCreator, ReducerTypes } from '@ngrx/store';
export declare function immerOn<State, Creators extends readonly ActionCreator[]>(...args: [...creators: Creators, reducer: (state: State, action: any) => void]): ReducerTypes<State, Creators>;
'@
        'apps/shop/src/bag.actions.ts' = @'
import { createAction } from '@ngrx/store';
export const tick = createAction('[Demo] tick');
export const plan = createAction('[Demo] plan');
export const bumped = createAction('[Demo] bumped');
export const quiet = createAction('[Demo] quiet');
'@
        'apps/shop/src/bag.helpers.ts' = @'
export interface Line { id: number; active: boolean; }
export interface Bag { lines: Line[]; count: number; }
export const setLine = (callback: (g: Line) => void, id: number, bag: Bag | null) => {
  if (bag) { const line = bag.lines.find((l) => l.id === id); if (line) { callback(line); } }
};
export function viaPlan(callback: (b: Bag) => void, bag: Bag) { callback(bag); }
export function bump(bag: Bag) { bag.count++; }
export function peek(bag: Bag) { return bag.count; }
export function log(n: number) { return n; }
'@
        'apps/shop/src/bag.reducer.ts' = @'
import { createReducer } from '@ngrx/store';
import { immerOn } from 'ngrx-immer/store';
import * as bag from './bag.actions';
import { Bag, bump, log, peek, setLine, viaPlan } from './bag.helpers';
export interface S { cart: Bag; }
const initial: S = { cart: { lines: [], count: 0 } };
export const bagReducer = createReducer(initial,
  immerOn(bag.tick, (s) => { setLine((g) => (g.active = true), 1, s.cart); }),
  immerOn(bag.plan, (s) => { viaPlan((b) => setLine((g) => (g.active = false), 2, b), s.cart); }),
  immerOn(bag.bumped, (s) => { bump(s.cart); }),
  immerOn(bag.quiet, (s) => { log(1); peek(s.cart); }));
'@
    }
    $made = New-TsRowsDb (New-TsRowsWorkspace $files)
    Assert-Exit $made.Result 0
    $h = Invoke-TsRowsQ $made.Db ("SELECT 'h=' || h.action_source || ' | ' || coalesce((SELECT group_concat(w.value) " +
        "FROM state_handlers__writes w WHERE w.owner = h.id AND w.value IS NOT NULL), '-') AS r FROM state_handlers h")
    Assert-Line $h 'h=bag.tick | cart.lines.active'
    Assert-Line $h 'h=bag.plan | cart.lines.active'
    Assert-Line $h 'h=bag.bumped | cart.count'
    Assert-Line $h 'h=bag.quiet | -'
}
}
