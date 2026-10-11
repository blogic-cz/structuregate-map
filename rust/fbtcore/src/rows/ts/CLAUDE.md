# The render closure and the gate readers (Angular half, rust side)

How the closure runs inside the store (partial runs, `Cell`, memory) is
[../../../../../.claude/rules/typescript-half.md](../../../../../.claude/rules/typescript-half.md); the rows
it reads are [../../../../../src/TsRows/CLAUDE.md](../../../../../src/TsRows/CLAUDE.md). This file is WHAT the
four derived tables mean and which restrictions a gate is read for. Black-box cases:
`tests/deep/TsRowsGates.Tests.ps1`, `tests/deep/TsRowsBranches.Tests.ps1`, `tests/deep/TsKeys/` and
`tests/deep/TsAngular/` (gates in `TsGates/`: `TsRowsGateThrough`, `TsRowsGateParents`, `TsRowsGateSpread`).

## The closure (`closure.rs`, `key/keyreach.rs`, `value_folds.rs`)

`render_path`, `key_reach`, `gate_values` and `gate_features` are derived from EVERY other table, so they run
LAST, over `Store` read back from SQLite. They answer what no single row can: every way in to a component from
a routed root, where a translation key can be travelled to, what gates every way, which capability a gate
requires and which enum members it still permits.

- **THE UNIT IS (REF, PATH)**: a ref's own in-template gates joined to the gates of THAT path, never unioned
  into every path of the component - that made `always_gates` claim conditions that do not always hold.
- **`render_path`'s unique key is `(component, edges)`**, never `(component, hops)`: one parent rendering a
  child at two sites is two real paths with one component chain. `path_index` is assigned by SORTING on the
  key, and every id list in `key_reach` (`routes`, `maybe_gates`, `components`, `roots`, `modules`) is sorted
  too: walk order reads as thousands of changed rows with no fact different.
- **NO PATH CAP** (signed-off divergence #1). A per-component cap hid real paths on one tree and most of the
  map on another. `truncated` stays because a consumer filters on it, and is always 0. `always_*` are
  intersections and only shrink as paths are added; the cap only under-reported the maybe side.
- **A WAY THROUGH A LITERAL `false` IS NO WAY** (`key/key_dead.rs`): an `*ngIf` or `@if` whose condition is the
  literal names no member, so no `gate_values` row can carry it. `ways_of` drops every (ref, path) through one
  before anything folds: no live way left is `n_paths 0` - the key renders for nobody - and its roots go too.
- **THE CLOSURE TABLES DECLARE UNIQUE KEYS**, so a pass that files a row twice fails the insert instead of
  publishing a plausible duplicate.
- **NO REGEX: the dead-key trace is character walks** (`sourcescan.rs`, `sourcetrace.rs`). It decides whether
  a key nothing references is really dead; a pattern matching inside a comment or a longer identifier is how a
  live key gets called dead. **A file whose path escapes the workspace (`..`) is never scanned**: the
  compiler's `lib.dom.d.ts` is a `files` row, and scanning it called absent keys `leaf`.

## `gate_values` - one row per (gate, enum, dimension)

- **`op`**: `in` (renders only for these members), `not_in` (for every member but these), `unknown` (restricts
  the dimension in a way the map cannot read; `values_json` is `[]`). `values_json` is always the SMALLER side,
  so a long `in` is published as its `not_in` complement - except on a SET, whose sides are not complements ("holds some of A, B" is not "holds no C"): it keeps the side it was written on. `set = 1` marks a dimension several members can
  hold at once (a collection): a second `in` on it does not intersect.
- **`listed_json`** (divergence #7) is what a directive's CONFIG OBJECT lists for the dimension, resolved to
  members whatever the `op`; null when no config lists it, `[]` for an empty list. An exclusion list tested
  under a `!` stays `unknown` (hide or disable cannot be told apart), its members in `listed_json`. A
  conditional config (`c ? {...} : {...}`) is read per branch: a member every branch lists permits their
  union, one a branch omits is `unknown`, one no branch writes has no row. **A CONFIG READING AN `*ngFor` ITEM'S ENUM PROPERTY** (`{ kinds: [t.kind] }` over a list of literal objects, the list typed in place or by a named interface, `type` or class in any file, the list typed in place or by a named interface, `type` or class in any file, `key/key_loops.rs::Loops::expand`) is one branch PER ELEMENT: the gate permits their union, and a key rendered from element i carries `loop:<gate>#i`, whose own rows (`config_restrictions_all`) give it element i's value alone.
- **ONE ENUM TESTED THROUGH DIFFERENTLY WRITTEN EXPRESSIONS FOLDS AS THE ENUM** (`value_folds.rs::fold_enums`): when no
  dimension of an enum is published, each way permits the union of its own dimensions of it (none: the domain), and
  a smaller union is one row whose `dimension` joins the texts (`item.kind | kind`).
- **NOTHING PERMITTED ON EVERY WAY IS `in []`, AND IT IS THE ANSWER** (`value_folds.rs`): two nested config
  lists sharing no member, or a built key whose hole names no member, render the key for NOBODY. Moving it to
  `unreadable_values` made the key read as unrestricted.
- **A gate's dimension resolves through ANY declaration of a union's property** (divergence #6, `row_of` in
  `gate/ts/gate_tsrows.rs`): every declaration in path and line order, because the checker lists them in the
  order it was asked and a full and a partial run disagreed. The extracted `target`/`also` order stays as
  written - sorting THAT moved thousands of rows.

## What a gate is read for - always from the SOURCE that proves it, never a name

- **A MEMBERSHIP PREDICATE** (#3, `gate/gate_predicates.rs`): `isA([Kind.A])` restricts when the method's OWN
  BODY proves it - one enum-typed parameter, one unconditional return testing it for membership in a collection
  the class holds, or a KEYED LOOKUP (`list.find((x) => x.F === key)`). A lookup proves true only (an item may be
  present and switched off), so a negated call is `unknown`. The dimension is the collection (`set = 1`).
- **A FIXED LIST OF CONSTANTS** (`gate/ts/gate_lists.rs`): `LIST.includes(v)`, `[A, B].some((x) => x === v)`,
  `indexOf !== -1`, and a function whose one return is such a test - exact both ways. A bare local is no dimension.
- **A KEY ASSIGNED IN TYPESCRIPT CARRIES ITS `if` AND `case`** (#4, `key/key_branches.rs`,
  `gate/ts/gate_cases.rs`): each write of a key (`field_binding`, `literal`) is a WAY IN joined to its chain of
  branches; an `else` holds the condition negated; a `case` group is one `in`, a `default` a `not_in` of its
  siblings; a write outside any branch empties the intersection. A key CHOSEN in a value is a link: `c ? 'a' :
  'b'` holds `c`/`!c`, `x && 'k'` holds `x`, `x || 'k'` holds `!x`, `??` nothing - the link is the condition's
  `expressions` id, `!`-prefixed when negated. A GETTER's returns are its writes, and a getter RETURNING A CALL follows it as a field holding the call does (its own `if`/`case` carried, the callee's not); a METHOD's are too, but only at a node whose template CALLS it (#19, `key/fields/key_methods.rs`: a method WITH parameters carries no branch - they may test parameters - and a string or ternary of strings handed as an argument is bound into the holes naming the parameter, any other argument leaves the hole open); otherwise a method's are not, except that a field (or such a getter) HOLDING ITS CALL reads the method's ONE plain return (`key/fields/key_fields.rs`, `key/fields/key_follow.rs`; no branch, no
  second return, its `tpl:` links dropped; a return that is a call of a configured `i18nCalls` method holds its arguments' keys; a return is followed through a call of another method - every return of one with several, no branch carried, never a key such a return spells itself - and either side of `??`/`||` or a ternary's arms, no condition carried; a translate call WRAPPED in an object's property values, a list's non-object elements, a `+` chain or `Promise.resolve(..)` is held too (never the argument of another call, and a template literal's hole or a method chained on the call is unread); a return of a bare `let` local reads every later `=` of it too, its own or its enclosing method's, no branch carried; a call of the method's own parameter runs the function the call handed in, a call of a `const` local the arrow it holds, a call of a FREE function (a `function` or `const` arrow of no class, found by the call's `$target` when no member of that name is) is entered like a method - and a hole naming a parameter handed a string is that string (`t('a')` spells `x.a` alone), a string enum member's value, or a ternary of those (`t(c ? 'a' : 'b')` spells both, no condition carried, up to 16 combinations)). **A KEY IN A LIST OF OBJECTS** (`tips = [{ tooltip: 'k' }]`, a call of a method or free function returning one from every return (`<list>.filter(..)` over one counts; `key/key_lists.rs`) - a property's value may be a translate call, its arguments' keys, or a call read as the callee reads its own (a local arrow, a hole handed a string), no condition carried - a `...` spread of such a call, or the union of every plain `=` the field's own class writes when it is initialised with nothing or `[]` - each element under its write's branches, a callee's branches not carried) is a site only where an `*ngFor` over the field has its loop variable's property read (`t.tooltip`, `key/key_loops.rs`), at THAT node with ITS `gate_chain`; the whole field's read would drop every condition inside the loop from `always_gates`, and a list no `*ngFor` reads stays a `literal`. A translation call (`ts_call`) takes its own row's chain; a literal the chain
  of the innermost assignment, `return` or argument holding it; a PIPE's `transform` key renders at every node
  applying the pipe (route `pipe`). `key_reach` adds `always_branches`/`maybe_branches` and `branch_senses`
  (`then`/`else`/`case`/`loop`/`bind`/`either`; `always_branches`/`maybe_branches` can carry `loop:`, `bind:` and `or:` ids); `always_gates` still names `gates` rows only.
- **A KEY A TEMPLATE LITERAL BUILDS** (`key/key_built.rs`) is every locale key its pieces spell, with a
  `tpl:<tree>=<holes>` link. A hole `Enum[x]` is the member's NAME, one typed by an enum its VALUE; text no
  member spells is a way that cannot happen. A `const` local of the member resolves in a condition.
  **A LOCAL `const` ARROW WHOSE EVERY CALL SPELLS ITS PARAMETERS IS READ PER CALL, NOT AS A WILDCARD**
  (`key_branches::literal_ways`, `key_follow::spelled_arrows`): its own body's template is bound to each call's
  arguments - a string, a ternary of strings, or a STRING enum member's value (`key_follow::spell`) - so `k('lit')` over
  `(n) => t(`p.${n}`)` leaves `p.other` unlinked. One call handing a variable, a property, a numeric member or a
  callback's item, or no call found, keeps the wildcard: nothing says which keys it spells. A METHOD's, a free
  function's or a callback's parameter always spells every key its pieces match: its callers are not all known.
- **A BARE BOOLEAN PROPERTY** in truth position is what defines it (`gate/ts/gate_props.rs`): a getter of one
  unconditional return is its expression both ways, and one of `if (C) { return E; } return false;` (one return
  under a top-level `then`, every other the literal `false`) is `C && E` both ways; a property written once, under no branch and after no
  `return`, in its own class's constructor or `ngOnInit` (not an `@Input`) is read both ways; written once
  anywhere else it proves only the true side; INITIALISED with a decision and never written it is that
  expression. A read compared by value (`v === V.First`) is never replaced.
- **A CALL OF ONE RETURN** (#10, `gate/ts/gate_calls.rs`): a method, function, arrow property or arrow `const`
  of one unconditional return - or a property holding one (an arrow, or a `function` declaration by its `$ref`) - is that return with parameters replaced by
  arguments, a service's dimensions then the service's. Not inlined: a second return, a `Promise`/`Observable`
  body, a parameter a callback rebinds, a different argument count, a callee `gate_predicates` or `gate_lists`
  already reads. An arrow property returns like a METHOD (#8: the extractor writes its implicit `returns`).
- **A PREDICATE A FACTORY BUILDS** (`gate/ts/gate_curried.rs`): `const isA = make([Kind.A])` over `make = (ids) =>
  (k) => ids.includes(k)` - the factory found by the call's `$target`, its one unconditional return an arrow whose one
  return tests the INNER parameter in the OUTER one's list - or a REST parameter's `rest.some((x) => ids.includes(x))`, `ids.includes(rest[0])`, or `rest.m(ids)` over an `Array.prototype.m = function (o) { return this.some((x) => o.includes(x)); }` (or `o.some((x) => this.includes(x))`; a `this` parameter is a type; assigned under `if (!..)` or by `Object.defineProperty` too; a method defined twice is read only when every body proves it) (joined by the call's `target`: the extractor files the assignment as a `prototype-method` function row; never by the name) -
  is `in [A]` (`not_in` negated), the list being the call's constant argument (`$args`). A variable or a spread handed over, a second parameter or a second return is unread. A CALL handed a spread, `isA(...this.xs)`, over a test of EVERY element of the rest (`rest.some(..)` or such a prototype method, never `rest[0]`) is `xs.some(..)`: a SET row on `xs`, `in` the list and `not_in` negated, exact both ways.
- **AN OR IS THE UNION OF ITS SIDES** (#10, `gate/gate_collapse.rs::either`); `!(a && b)` is `!a || !b`. A
  dimension one side says nothing of is no row, `unknown` on either side is `unknown`, a SET dimension unions
  only two `in`s, and `gate_features` reads nothing from an OR.
- **A STRUCTURAL DIRECTIVE THE TEMPLATE HANDS A CONSTANT** (#9, `gate/ts/directive/gate_directive_inputs.rs`):
  `*whenMode="Modes.A"` over an `@Input` (or a setter copying its one parameter into a field) compared with a
  member of the same enum - or with a field declared a bare `number`/`string` the member can equal, an injected
  config value - is a row on THAT member. The `op` is what every `createEmbeddedView` in the class
  chain requires - its branches, a one-level `return` before it, what every `this.` call into its method
  requires - so a render under the `else` of `===` is `not_in`. `unknown`: a field written by anything but
  the copy, a copy under an `if`, two comparisons, an unguarded render, a value that is not the enum's constant.
- **A DIRECTIVE RENDERING WHEN ITS CONFIG'S VALUE IS IN ITS CONFIG'S LIST** (`gate/ts/directive/gate_guards.rs`,
  `gate/ts/directive/gate_guard_eval.rs`): the render path is EVALUATED from the input setter through `extends` to every
  `createEmbeddedView`, under its branches and the negation of each earlier `return`. A field copied from
  `config.M` holds `M`, an omitted member is `undefined`, `L.find((x) => x === S) === undefined` over a constant
  list is membership. A render that can run but the walk cannot reach (a callback, an `ng*` hook, a decorated
  member) refuses the directive.
- **A LIST HANDED TO A DIRECTIVE** (#10, `gate/ts/directive/gate_directive_lists.rs`): `*whenKind="[Kind.A]"` tested by
  `includes`, `indexOf(x) !== -1` or `some((e) => e === x)` is a SET row (`items.kind in ["A"]`), or a value row
  when tested for a class member. A `const` local is its initializer; an `||` side needing truthy a field no
  template can set is its other side. **A LIST THE SETTER HANDS ON** (#11) is the method parameter it lands in,
  a `createEmbeddedView` in a callback renders for the method around it, and every other call must hand the
  same thing - a call from the class chain only: a same-named method of an unrelated class is not this one.
  `unknown`: not the enum's constants, empty, two tests of the field, that other input bound anywhere.
- **AN ASYNC SELECTOR BUILT FOR ONE MEMBER** (`gate/ts/gate_selectors.rs`): `x$ | async` over a property initialised
  once as the ngrx `store.select(F(E.M))`, where `F` (a `const` arrow or a key of a `const` object) has one
  enum-typed parameter and returns `createSelector(..., (items) => items.some((i) => i.F === p))` - or hands
  `items.filter((i) => i.F === p)` to a helper whose one return is `list.some(...)` (`?? false` allowed) over THAT
  parameter - is a SET row `F.F in [M]`, true side only: `async` is null before the first emit, so a negated one is no row.
- **AN `@Input` FLAG IS WHAT ITS PARENT BINDS** (#11, `gate/ts/gate_input_flags.rs`), both ways, when EVERY
  element the child renders at binds it to one tree and the child never writes it: that is the `gate_values` row.
  **PARENTS THAT DISAGREE ARE READ PER RENDER EDGE** (#14, `key/key_bound.rs`): no `gate_values` row, but each
  edge into the child resolves the flag by its own binding, or by the input's literal initializer when it binds
  nothing, and the way through it holds `bind:<render>#<gate>` (sense `bind`). An edge on which the gate's
  condition becomes the literal `false` is no way. The child writing the flag itself is still unread.
- **A KEY CHOSEN IN A CONDITIONAL ARM takes the arm** (#11): the extractor stamps `choices` on each statement
  row inside an arm; the i18n ref joins it through `i18n_refs.call` and `key_reach` folds it like a branch.
  A key a TEMPLATE expression chooses (`[text]="c ? 'a' : 'b'"`, `x && 'k'`, `x || 'k'`, inside a pipe or a
  call too) holds its arm as `arm:<expression>#<n>` (#16, `key/key_arms.rs`): the condition's
  subtree is read as a `gates` tree is, so `c` restricts exactly as `*ngIf="c"` does.
- **A COMPONENT A FACTORY RETURNS UNDER A `case` OR `if` RENDERS UNDER IT** (#13, `key/key_returned.rs`): a
  dynamic render's `renders.return_ways` lists the `returns` rows each way the class came through; the edge
  holds the branches and cases EVERY way holds (one case and no case hold nothing; two or more cases, each alone
  on its way, hold `or:<case>|<case>`, the union of their values - #15). They ride `render_path.branches` - never `gates` - and `key_reach` folds them with the key's own,
  so `always_branches`/`always_values` gain the case. `path_always_gates` stays gates alone.

A `const` local holding a DECISION (a comparison, `&&`/`||`, `!`, `includes`/`some`) is its initializer wherever a gate
reads it (`gate/ts/gate_tsrows.rs`), a bare `this.f = local` included.

**Not read yet**: a set held in a local (`active?.some(...)`), a property written more than once.

## Divergences, and the rule that made them

The half replaced a predecessor 1:1, compared by NATURAL IDENTITY (what a row says about the source, never its
id) with every carried field equal. A row the predecessor got wrong was reproduced, and changing it is a
SIGNED-OFF divergence - a port that improves rows cannot be told from one that breaks them. The numbers above
(#1-#11, #13-#15) are those decisions - #13 adds `renders.return_ways` and `render_path.branches`, and moves
`key_reach` only for keys behind such a render; #2 (the atlas reports `routes.children`, `rust/fbtcore/src/atlas.rs`) and #5
(an aliased dependency NgModule, the extractor's) and #12 (`render_graph` leaves out an `out_of_scope` render,
`src/TsRows/TsDerive/TsIndexes.mjs`) live elsewhere. A new restriction is a new divergence: say
which tables gain rows and that no other table moves, and bump the extractor's `rows` (it keys an unchanged tree).


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
