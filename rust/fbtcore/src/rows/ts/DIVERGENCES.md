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

**#24 - a key a class instance carries reaches the field that reads it** (signed off; #19-#23's readers stopped at the class a receiver is declared by): `this.c = this.btn.text` and `this.c = this.strat.tip()` read the member of the receiver's class, and of every subclass or implementer a `new` builds (`key/fields/key_classes.rs`; an unbuilt one contributes nothing), a constructor's `this.<member> = ..` write included; no callee branch carried. A call through a receiver is now entered by the class the checker declares it in (`$owner`), where two members of one name in a file read as none. Tables that move: `key_reach` for keys behind these shapes, `value` JSON of `assignments`/`returns`/`locals`/`calls`/`members` (`$owner` on a call, `$read` on a property read through a receiver; `rows` 43).
