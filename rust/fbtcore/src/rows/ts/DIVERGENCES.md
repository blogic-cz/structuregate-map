# Signed-off divergences of the Angular half

Split out of [CLAUDE.md](CLAUDE.md), which states what each derived table means.

## The rule that made them

The half replaced a predecessor 1:1, compared by NATURAL IDENTITY (what a row says about the source, never its
id) with every carried field equal. A row the predecessor got wrong was reproduced, and changing it is a
SIGNED-OFF divergence - a port that improves rows cannot be told from one that breaks them. The numbers above
(#1-#11, #13-#15) are those decisions - #13 adds `renders.return_ways` and `render_path.branches`, and moves
`key_reach` only for keys behind such a render; #2 (the atlas reports `routes.children`, `rust/fbtcore/src/atlas.rs`) and #5
(an aliased dependency NgModule, the extractor's) and #12 (`render_graph` leaves out an `out_of_scope` render,
`src/TsRows/TsDerive/TsIndexes.mjs`) live elsewhere. A new restriction is a new divergence: say
which tables gain rows and that no other table moves, and bump the extractor's `rows` (it keys an unchanged tree).


**#21 - a twin enum, a number and a getter's `this.` alias are read** (signed off). Tables that move: `gate_values` (`unknown` becomes `in`/`not_in`, or `in []` for a number no member has) and, for keys under such gates, `key_reach` `always_values` and `unreadable_values`. No other table moves.

**#14 - an input flag the parents bind differently is read per render edge** (signed off; replaces "else unread"
of #11). A way whose edge leaves the flag at a literal `false` is no way. Tables that move: `key_reach`
`always_values`, `always_branches`, `maybe_branches`, `n_paths` and `roots`, for keys under such gates. `gate_values`
does not move.

**#15 - several cases returning one class are either of them** (signed off; replaces "two cases hold nothing" of #13).
Tables that move: `render_path.branches` (an `or:` id) and `key_reach` `always_branches`, `always_values` and
`branch_senses`, for keys behind such a render. `renders` and `gate_values` do not move.

**#16 - a key a template expression chooses holds its arm** (signed off; the template half of #11). Tables that
move: `key_reach` `always_values`, `unreadable_values`, `always_features`, `maybe_features`, `always_branches`,
`maybe_branches`, `branch_senses` and `n_paths` (a key in both arms is two ways), for keys chosen by a ternary, `&&`
or `||` inside a template expression. `gate_values` and `render_path` do not move.

**#17 - an out-of-scope render is no way in** (signed off; #12 for the closure, `closure.rs::load_edges`). A tag
matched to a component the host's NgModule cannot see is no edge, as `render_graph` and `component_reach` already
read it; `unknown` stays an edge. Tables that move: `render_path` (no path through such an edge) and `key_reach`
`roots`, `routes`, `components`, `modules`, `n_paths` and every `always_*`/`maybe_*` column, for keys behind a
same-selector component. A component a module exports from its own file is `imported`, no longer `out_of_scope`
(`renders.scope`, `src/TsRows/TsDerive/TsModuleScope.mjs`).

**#18 - a stronger route never drops another component's site** (signed off; `key/routes/key_routes.rs::put_ways`). The
`route` column stays the strongest route present, but the sites are kept PER COMPONENT, each from the strongest route
that reaches IT: a `literal` or `ts_call` component beside another component's `field_binding` (or a carrier) stays
listed, where the stronger route used to replace the whole site set. A weaker site in a component a stronger route
already reaches is still dropped, so a bare literal's empty gate list never dilutes a position's own `always_gates`.
Tables that move: `key_reach` `components`, `roots`, `routes`, `modules`, `n_paths`, `n_refs` and every `always_*`/`maybe_*`
column, for keys with sites of more than one route in different components; nothing else moves.

**#19 - a method a template calls hands the template the keys it returns** (signed off; the method form of #18's getter).
`{{ label() }}`, `[title]="tip('a')"` read the method's `returns` like a getter's, at the node that calls it (the
receiver's checker-resolved row, never the name), under the node's own `gate_chain`. A method with parameters carries
no branch; one without carries its `if`/`case`. A string literal or ternary of strings argument spells its hole alone.
Tables that move: `key_reach` for keys a template-called method returns (a `ts_call`/`literal` gains the
`field_binding` route and its components); nothing else moves.

**#20 - a dictionary call references every key under its prefix** (signed off; config `i18nDictionaryCalls`): an `i18n_refs` row with `prefix`, no `key`, expanded in `key_routes.rs` against the locale keys under `<prefix>.` (`ts_call`); a field, getter or return HOLDING the call reads as `<prefix>.${rest}` (`key_follow::dictionary`, `field_binding`); a converter's argument is not followed. Moves: `i18n_refs` (new), `key_reach`.

**#22 - a key behind a pushed list, a template hole, a chained string method or a dynamic index reaches the field** (signed off; #19's readers skipped them): no condition carried, a field list's push carries its branches. Tables that move: `key_reach` for keys behind these shapes, `i18n_refs` (a call indexed so claims every value), `value` JSON of `returns`/`locals`/`assignments`/`calls` (`$hole_values`, `$receiver_value`, `$indexed`; `rows` 41).

**#23 - more shapes reach the field** (signed off; #22's readers skipped them): a `+=` on a returned local, an `await <call>`, a field a `.then` callback writes its parameter to, a list copied by `slice`/`map` (parameter kept), a const ARRAY read by an unknown index, an arrow property a template calls, a list every parent binds to a child's `@Input`. Tables that move: `key_reach` for keys behind these shapes, `value` JSON of `returns`/`locals`/`assignments`/`calls` (`$indexed` on arrays; `rows` 42).

**#24 - a key a class instance carries reaches the field that reads it** (signed off; #19-#23's readers stopped at the class a receiver is declared by): `this.c = this.btn.text` and `this.c = this.strat.tip()` read the member of the receiver's class, and of every subclass or implementer a `new` builds (`key/fields/key_classes.rs`; an unbuilt one contributes nothing), a constructor's `this.<member> = ..` write included; no callee branch carried. A call through a receiver is now entered by the class the checker declares it in (`$owner`), where two members of one name in a file read as none. Tables that move: `key_reach` for keys behind these shapes, `value` JSON of `assignments`/`returns`/`locals`/`calls`/`members` (`$owner` on a call, `$read` on a property read through a receiver; `rows` 43). It reaches lists of instances too: pushed with `new X(..)` in a parent (a local handed to the field, a field push, either inside a `.then` callback) and handed to a child's `@Input`, read in the child's `*ngFor` as `b.text` or `b.tooltip.text(row)`; a `.push` inside a callback now counts for the member's class (`key_pushes`), and a template call on an interface or base method reads every built subtype's. Moves also `key_reach` for keys behind lists filled by a push in a callback.

**#25 - a list of instances reaches the child's table through the shapes a real one is built and handed with** (signed off; #24's readers stopped at a field holding the list): a callee's returned local and its pushes (in its callbacks too), a pushed `const` of a `new`, `push(...call())`, a constructor write inside a callback, every return a held function spells, an `@Input` setter copying into a field read through its getter, an input bound to a call, a getter or a property of a field (`cfg.list`), a method the template hands the loop item to (`$reads` off its parameter), a template read of a base's or an interface's property, and a field an rxjs `subscribe` (through value-passing `pipe` operators) or a `Promise.resolve(x).then` callback writes from its parameter, and a `this.<x>` write two classes of one file both declare `x` for (resolved through the writing class). A key a list element holds through a call (a callee's list of `{ label: t('k') }`) now keeps the element's own chain - the write's branches, the edge - where it used to drop it. A CHILD'S INPUT IS NOW READ PER RENDER EDGE (`edge:<render>`): #23 required every parent to bind a known list, and one parent rendering the child without it left every key of every parent unreached. Tables that move: `key_reach` for keys behind these shapes, `value` JSON of `calls` (`$spread` on a spread argument) and of `assignments`/`members` (`$fn_id` on a function held in place; `assignments.holder`; `assignments.target_id` on such a write; `rows` 44).

**#26 - a callee's branch on its parameter restricts a key by the caller's argument** (signed off; #19-#25 carried no condition of a callee): a field or getter holding `this.svc.f(this.item.kind)` gets `arg:<branch>@<call>` on each key `f` returns under its own `if`, the condition being the branch with `f`'s parameters replaced by the call's arguments (`key/fields/key_args.rs`, sense of the branch); a method a template calls holds `arg:<branch>@<call>@<template expression>`, its parameters replaced by the template's arguments at that call. A local arrow or handed-in function is now entered before its call is read as an `i18nCalls` call - a local `t` wrapping the translate call no longer reads as one. Tables that move: `key_reach` (`always_values`, `always_branches`, `maybe_branches`, `branch_senses`, `n_paths`) for keys a service returns under a branch on a parameter; keys behind a local arrow named like a translate method.

**#27 - extractor rows the original tool left out** (not yet signed off; `rows` 45): `on()` handlers spread into `createReducer` from a const array (same file or imported) and selector factories (`state_selectors.factory`); `di` rows for `inject()` fields and the token of `@Inject(TOKEN)` (`token`, `token_ref`, `token_id`); an `export { A as B }` alias resolved to `A` (`routes.component_id`, `ng_modules` refs' `id`, `component_reach`); a static attribute filling an input (`input_usage` with `expression` null, `io.bound_count`); `imports` rows for a bare `import()` / `require()` (`dynamic`); interfaces and type aliases inside a module block (`in_module`), and a non-ambient namespace's classes, functions and consts. Tables that move: those named; nothing else.

**#28 - a reference through an alias the module file imports, and where an import lands** (not yet signed off; `rows` 46): an `ng_modules` ref or a route's `component` that names an `import { A as B }` alias, or that the evaluator left unresolved (`{expr}`), joins the class through the file's own `import_names` row (`id`, `component_id`); `imports.in_tree`; `in_module` on a namespace's classes, functions, consts and enums. In the key closure: a bare parameter a callee hands on (`this.home(name)`) carries the caller's string, and a caller's plain `const` local in a carried condition is the chain it holds. A shorthand property (`{ template }`) evaluates to its const's value (`class_decorators.args`, every object literal), and a same-file const-held inline template takes the const's line (`components.inline_template_line`; an imported const keeps the decorator's). Tables that move: those named, and `key_reach` for keys behind these shapes.

**#29 - `[hidden]` hides, so it gates on its negation** (not yet signed off; `rows` 46): `[hidden]="c"` / `[attr.hidden]="c"` is a `gates` row with `gate_kind = 'hidden'` whose `expression` is a row holding `!(c)`; a key in its element or subtree is restricted to the members `c` excludes (`not_in`). Before, it was no gate, or a `gateInputs` `declared_input` read as `in` - the wrong side. Tables that move: `gates`, `expressions` (one row per `[hidden]`), `gate_values`, `key_reach` for keys under one.

**#30 - a class gate restricts only when a stylesheet proves its class hides, and then on the false side** (not yet signed off): `[class.x]="c"` whose class has a `class_hides` row (scope `own`/`shared`) is read as `!c` (the element shows while `c` is false); a class no rule proves hiding is no visibility condition and gives no `gate_values`/`gate_features` row (it stays in `gates`/`always_gates`). The stylesheets are now read before the closure (`tsapply.rs`). Before, every class gate was read as `c`: the wrong side for a hiding class and a false restriction for a styling one. Tables that move: `gate_values`, `gate_features`, `key_reach` for keys under a class gate.

**#31 - a literal inside a component class is a site of that component only** (not yet signed off): a bare literal equal to a key, written inside a component's class, is a `literal` site of that component, not of every component in its file; one outside any class or in a non-component class still is. Before, a child's own `key = 'a.b'` in a file shared with its parents was an ungated site of each parent, and the child's template gate no longer restricted the key. Tables that move: `key_reach` for such keys.

**#32 - `state_effects`, and a `createEffect` callback's body is walked** (not yet signed off; `rows` 46): the arrow handed to `createEffect` was never walked (a property is walked only when it is itself a function), so `ofType(a, b)` existed as `$fn` text only. Its calls, returns and nested arrows are now rows under the effect member, and `state_effects` names the actions each effect listens to, whether it dispatches and what its callbacks return. Tables that move: `state_effects` (new), `calls`/`functions`/`returns`/`locals`/... for effect bodies.

**#33 - an input's dimension is the parent's expression on a way through its binding** (not yet signed off): On a way through a render edge that binds a child `@Input` to a plain, non-constant read chain the child never writes, the two spellings (`vm.kind`, `item.kind`) are one variable ON THAT WAY: every row keeps its own text, and rows present under both intersect. Rows under aliased texts that come out equal on every way are published once, labelled by both. Renaming either spelling into the other split one variable wherever only some rows were renamed: renaming the child up (v2.1.13) and renaming the parent down (not released) each lost keys a real tree restricts. A constant (a literal, or a chain through a `typeof X` alias field) aliases nothing (#49). Tables that move: `key_reach` `always_values`/`unreadable_values` for keys under such edges.

**#34 - a callee branch carried through a call is one dimension of the callee's parameter** (not yet signed off): a callee branch carried through a call (`arg:`) restricts the callee's PARAMETER: when its condition reads one parameter, its rows are one dimension on every way, keyed by (callee, parameter), and published under the caller spellings joined. Before, each spelling was its own dimension, so ways that spelled the argument differently left none common and the fold widened it (#48). Tables that move: `key_reach` `always_values`/`unreadable_values` for keys carried through a call reached by differently spelled arguments.

**#35 - a parent that only hands a list to a child's input is no site of its keys** (not yet signed off; `key/routes/key_routes.rs::Routes::claim`): a key held by a list element that reached a child's `@Input` through a render edge (`edge:<render>`) drops the edge parent's weaker (`ts_call`, `literal`, `route_path`) site of that key; a site of the parent's own at the template rank stays. Tables that move: `key_reach` `components`, `roots`, `modules`, `routes`, `n_paths`, `n_refs`, `min_depth` and every `always_*`/`maybe_*` column, for keys built in a parent and displayed through a child input; nothing else moves.

**#36 - a key inside an `ng-template` only outlets render takes the outlets' gates** (not yet signed off; `key/routes/key_outlets.rs`, `key_routes.rs::put_sited`): a site inside `<ng-template #ref>` whose every read is an outlet binding (`[ngTemplateOutlet]`, `*ngTemplateOutlet`, `then`) holds one way per outlet, the outlet's chain without its hand-over gates; before, it held only its lexical chain and an outlet's `*ngIf` restricted nothing. Tables that move: `key_reach` `always_gates`, `maybe_gates`, `always_values`, `unreadable_values`, `always_branches`/`maybe_branches` and `n_paths` (one per outlet way), for keys inside such templates; nothing else moves.

**#37 - a constant `false` drops a way only through a gate that hides** (not yet signed off; `key/key_bound.rs`, `key_dead::renders_at_false`): an edge whose binding (or literal initializer) folds a child's gate to `false` is no way only when that gate is `*ngIf`, an `@if` branch, `[hidden]`/`[attr.hidden]` or a class a stylesheet proves hiding. Before, any gate did, so `[disabled]="isDisabled"` (a `gateInputs` `declared_input`) with a parent binding `[isDisabled]="false"` removed that parent's way and restricted the key to the other ways' values. Tables that move: `key_reach` `n_paths`, `roots`, `min_depth`, `always_*` (shrink) and `maybe_*` for keys behind such a gate; `gates`, `gate_values` and `render_path` do not move.

**#38 - `key_reach.paths` names each way** (not yet signed off; `key/keyreach.rs::ways_of`, `closure.rs::Paths::first_id`): one `{path, own}` per way, `n_paths` of them, `path` a `render_path.id` and `own` what the way adds to that row's `gates`/`branches`. Tables that move: `key_reach` gains `paths`; no other column or table moves, and no restriction changes.
