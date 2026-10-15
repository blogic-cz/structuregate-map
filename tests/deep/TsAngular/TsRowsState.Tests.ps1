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
}
}
