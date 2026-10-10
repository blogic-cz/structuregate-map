# The render closure and the gate readers (Angular half, rust side)

How the closure runs inside the store (partial runs, `Cell`, memory) is
[../../../../../.claude/rules/typescript-half.md](../../../../../.claude/rules/typescript-half.md); the rows
it reads are [../../../../../src/TsRows/CLAUDE.md](../../../../../src/TsRows/CLAUDE.md). This file is WHAT the
four derived tables mean and which restrictions a gate is read for. Black-box cases:
`tests/deep/TsRowsGates.Tests.ps1`, `tests/deep/TsRowsBranches.Tests.ps1`, `tests/deep/TsKeys/` and
`tests/deep/TsAngular/` (`TsRowsGateThrough.Tests.ps1`, `TsRowsGateParents.Tests.ps1`).

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
  so a long `in` is published as its `not_in` complement. `set = 1` marks a dimension several members can
  hold at once (a collection): a second `in` on it does not intersect.
- **`listed_json`** (divergence #7) is what a directive's CONFIG OBJECT lists for the dimension, resolved to
  members whatever the `op`; null when no config lists it, `[]` for an empty list. An exclusion list tested
  under a `!` stays `unknown` (hide or disable cannot be told apart), its members in `listed_json`. A
  conditional config (`c ? {...} : {...}`) is read per branch: a member every branch lists permits their
  union, one a branch omits is `unknown`, one no branch writes has no row.
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
  `expressions` id, `!`-prefixed when negated. A GETTER's returns are its writes; a method's are not (its
  branches may test parameters). A translation call (`ts_call`) takes its own row's chain; a literal the chain
  of the innermost assignment, `return` or argument holding it; a PIPE's `transform` key renders at every node
  applying the pipe (route `pipe`). `key_reach` adds `always_branches`/`maybe_branches` and `branch_senses`
  (`then`/`else`/`case`); `always_gates` still names `gates` rows only.
- **A KEY A TEMPLATE LITERAL BUILDS** (`key/key_built.rs`) is every locale key its pieces spell, with a
  `tpl:<tree>=<holes>` link. A hole `Enum[x]` is the member's NAME, one typed by an enum its VALUE; text no
  member spells is a way that cannot happen. A `const` local of the member resolves in a condition.
- **A BARE BOOLEAN PROPERTY** in truth position is what defines it (`gate/ts/gate_props.rs`): a getter of one
  unconditional return is its expression both ways, and one of `if (C) { return E; } return false;` (one return
  under a top-level `then`, every other the literal `false`) is `C && E` both ways; a property written once, under no branch and after no
  `return`, in its own class's constructor or `ngOnInit` (not an `@Input`) is read both ways; written once
  anywhere else it proves only the true side; INITIALISED with a decision and never written it is that
  expression. A read compared by value (`v === V.First`) is never replaced.
- **A CALL OF ONE RETURN** (#10, `gate/ts/gate_calls.rs`): a method, function, arrow property or arrow `const`
  of one unconditional return - or a property holding one - is that return with parameters replaced by
  arguments, a service's dimensions then the service's. Not inlined: a second return, a `Promise`/`Observable`
  body, a parameter a callback rebinds, a different argument count, a callee `gate_predicates` or `gate_lists`
  already reads. An arrow property returns like a METHOD (#8: the extractor writes its implicit `returns`).
- **AN OR IS THE UNION OF ITS SIDES** (#10, `gate/gate_collapse.rs::either`); `!(a && b)` is `!a || !b`. A
  dimension one side says nothing of is no row, `unknown` on either side is `unknown`, a SET dimension unions
  only two `in`s, and `gate_features` reads nothing from an OR.
- **A STRUCTURAL DIRECTIVE THE TEMPLATE HANDS A CONSTANT** (#9, `gate/ts/gate_directive_inputs.rs`):
  `*whenMode="Modes.A"` over an `@Input` (or a setter copying its one parameter into a field) compared with a
  member of the same enum - or with a field declared a bare `number`/`string` the member can equal, an injected
  config value - is a row on THAT member. The `op` is what every `createEmbeddedView` in the class
  chain requires - its branches, a one-level `return` before it, what every `this.` call into its method
  requires - so a render under the `else` of `===` is `not_in`. `unknown`: a field written by anything but
  the copy, a copy under an `if`, two comparisons, an unguarded render, a value that is not the enum's constant.
- **A DIRECTIVE RENDERING WHEN ITS CONFIG'S VALUE IS IN ITS CONFIG'S LIST** (`gate/ts/gate_guards.rs`,
  `gate/ts/gate_guard_eval.rs`): the render path is EVALUATED from the input setter through `extends` to every
  `createEmbeddedView`, under its branches and the negation of each earlier `return`. A field copied from
  `config.M` holds `M`, an omitted member is `undefined`, `L.find((x) => x === S) === undefined` over a constant
  list is membership. A render that can run but the walk cannot reach (a callback, an `ng*` hook, a decorated
  member) refuses the directive.
- **A LIST HANDED TO A DIRECTIVE** (#10, `gate/ts/gate_directive_lists.rs`): `*whenKind="[Kind.A]"` tested by
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
  element the child renders at binds it to one tree and the child never writes it; else unread.
- **A KEY CHOSEN IN A CONDITIONAL ARM takes the arm** (#11): the extractor stamps `choices` on each statement
  row inside an arm; the i18n ref joins it through `i18n_refs.call` and `key_reach` folds it like a branch.
- **A COMPONENT A FACTORY RETURNS UNDER A `case` OR `if` RENDERS UNDER IT** (#13, `key/key_returned.rs`): a
  dynamic render's `renders.return_ways` lists the `returns` rows each way the class came through; the edge
  holds the branches and cases EVERY way holds (two cases returning one class, or one case and no case, hold
  nothing). They ride `render_path.branches` - never `gates` - and `key_reach` folds them with the key's own,
  so `always_branches`/`always_values` gain the case. `path_always_gates` stays gates alone.

**Not read yet**: a curried predicate (`const isPicked = isA([...])`), a set held in a local
(`active?.some(...)`), a property written more than once.

## Divergences, and the rule that made them

The half replaced a predecessor 1:1, compared by NATURAL IDENTITY (what a row says about the source, never its
id) with every carried field equal. A row the predecessor got wrong was reproduced, and changing it is a
SIGNED-OFF divergence - a port that improves rows cannot be told from one that breaks them. The numbers above
(#1-#11, #13) are those decisions - #13 adds `renders.return_ways` and `render_path.branches`, and moves
`key_reach` only for keys behind such a render; #2 (the atlas reports `routes.children`, `rust/fbtcore/src/atlas.rs`) and #5
(an aliased dependency NgModule, the extractor's) and #12 (`render_graph` leaves out an `out_of_scope` render,
`src/TsRows/TsDerive/TsIndexes.mjs`) live elsewhere. A new restriction is a new divergence: say
which tables gain rows and that no other table moves, and bump the extractor's `rows` (it keys an unchanged tree).
