<#
    A CONTROL'S LIST HELD IN THE NGRX STORE (#117): the template reads a property of a DECLARED type - a signature, with no value of its own -
    so the list is what writes it. A field written with `store.select(..)` holds what the selector's projector returns: a factory's
    `createSelector` called in an arrow included (`shape_select::selected_values`). A state's signature holds every literal property the
    checker types by it (`initialState: State = { .. }`) and every plain `=` naming its place (a reducer's `state.form.list = [..]`), their
    elements together (`shape_select::filled`). Its helpers are `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '../../TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

Test-Case 'tsrows: a control offers the keys a store selector or a state signature holds (#117)' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/radio.component.ts' = "import { Component, EventEmitter, Input, Output } from '@angular/core';`n" +
            "import { NgFor } from '@angular/common';`n" +
            "@Component({ selector: 'radio-list', standalone: true, imports: [NgFor], template: '<label *ngFor=`"let i of items`">{{ i.label }}</label>' })`n" +
            "export class RadioListComponent {`n  @Input() items: { label: string }[] = [];`n  @Input() value = '';`n" +
            "  @Output() valueChange = new EventEmitter<string>();`n}`n"
        'apps/shop/src/pick.state.ts' = "import { createFeatureSelector, createReducer, createSelector, on, createAction } from '@ngrx/store';`n" +
            "export interface Opt { label: string }`n" +
            "export interface PickState { form: { pick: { options: Opt[]; spare: Opt[] } } }`n" +
            "export const grow = createAction('[Pick] grow');`n" +
            "export const initialState: PickState = { form: { pick: { options: [{ label: 'opt.alpha' }, { label: 'opt.beta' }], spare: [] } } };`n" +
            "export const reducer = createReducer(initialState, on(grow, (state) => {`n" +
            "  state.form.pick.options = [{ label: 'opt.gamma' }];`n  return state;`n}));`n" +
            "export const selectPick = createFeatureSelector<PickState>('pick');`n" +
            "export const selectForm = createSelector(selectPick, (s) => s.form);`n" +
            "export function yesNo(): Opt[] { return [{ label: 'answer.yes' }, { label: 'answer.no' }]; }`n" +
            "export const pickVm = (id: string) => createSelector(selectPick, (s) => ({ criteria: { answers: yesNo(), unset: [] as Opt[] } }));`n"
        # THE STATE: `vm.pick.options` lands on `PickState`'s signature; `initialState` and the reducer's write fill it.
        'apps/shop/src/form.component.ts' = "import { Component } from '@angular/core';`nimport { NgIf, AsyncPipe } from '@angular/common';`n" +
            "import { Store } from '@ngrx/store';`nimport { RadioListComponent } from './radio.component';`nimport { selectForm } from './pick.state';`n" +
            "@Component({ selector: 'app-form', standalone: true, imports: [RadioListComponent, NgIf, AsyncPipe],`n" +
            "  template: '<ng-container *ngIf=`"vm`$ | async as vm`"><radio-list [items]=`"vm.pick.options`" [(value)]=`"model.chosen`"></radio-list></ng-container>' })`n" +
            "export class FormComponent {`n  model = { chosen: '' };`n  vm`$ = this.store.select(selectForm);`n  constructor(private store: Store) {}`n}`n"
        # THE SELECTOR FACTORY: a field declared by a type literal, written in `ngOnInit` with an arrow calling the factory's selector.
        # THE CONTROL: `vm.criteria.unset` - the projector writes an empty list there - offers nothing.
        'apps/shop/src/proposal.component.ts' = "import { Component, OnInit } from '@angular/core';`nimport { NgIf, AsyncPipe } from '@angular/common';`n" +
            "import { Observable } from 'rxjs';`nimport { Store } from '@ngrx/store';`nimport { RadioListComponent } from './radio.component';`n" +
            "import { Opt, pickVm } from './pick.state';`n" +
            "@Component({ selector: 'app-proposal', standalone: true, imports: [RadioListComponent, NgIf, AsyncPipe],`n" +
            "  template: '<div *ngIf=`"vm`$ | async as vm`"><radio-list [items]=`"vm.criteria.answers`" [(value)]=`"model.answer`"></radio-list>' +`n" +
            "    '<radio-list [items]=`"vm.criteria.unset`" [(value)]=`"model.other`"></radio-list></div>' })`n" +
            "export class ProposalComponent implements OnInit {`n  id = 'a';`n  model = { answer: '', other: '' };`n" +
            "  vm`$: Observable<{ criteria: { answers: Opt[]; unset: Opt[] } }>;`n  constructor(private store: Store) {}`n" +
            "  ngOnInit(): void {`n    this.vm`$ = this.store.select((x) => pickVm(this.id)(x));`n  }`n}`n"
        'apps/shop/src/assets/locales/en.json' = '{"opt":{"alpha":"A","beta":"B","gamma":"G"},"answer":{"yes":"Y","no":"N"}}'
    }
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    $r = Invoke-Gate --map-query $made.Db --width 0 --limit 0 --sql "SELECT 'opt=' || value || '|' || key AS o FROM control_options ORDER BY 1"
    Assert-Exit $r 0
    Assert-Line $r 'opt=model.chosen|opt.alpha'
    Assert-Line $r 'opt=model.chosen|opt.beta'
    Assert-Line $r 'opt=model.chosen|opt.gamma'
    Assert-Line $r 'opt=model.answer|answer.yes'
    Assert-Line $r 'opt=model.answer|answer.no'
    Assert-NoLine $r 'model.other'
}

}
