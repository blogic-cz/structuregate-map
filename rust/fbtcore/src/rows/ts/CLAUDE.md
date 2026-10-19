# The render closure and the gate readers (Angular half, rust side)

How the closure runs inside the store (partial runs, `Cell`, memory) is
[../../../../../.claude/rules/typescript-half.md](../../../../../.claude/rules/typescript-half.md); the rows
it reads are [../../../../../src/TsRows/CLAUDE.md](../../../../../src/TsRows/CLAUDE.md). This file is WHAT the
four derived tables mean and which restrictions a gate is read for. Black-box cases:
`tests/deep/TsRowsGates.Tests.ps1`, `tests/deep/TsRowsBranches.Tests.ps1`, `tests/deep/TsKeys/` and
`tests/deep/TsAngular/` (gates in `TsGates/`: `TsRowsGateThrough`, `TsRowsGateParents`, `TsRowsGateSpread`, `TsRowsGateOutlets`).

## The closure (`closure.rs`, `key/keyreach.rs`, `value_folds.rs`)

`render_path`, `key_reach`, `gate_values` and `gate_features` are derived from EVERY other table, so they run
LAST, over `Store` read back from SQLite. They answer what no single row can: every way in to a component from
a routed root, where a translation key can be travelled to, what gates every way, which capability a gate
requires and which enum members it still permits.

- **THE UNIT IS (REF, PATH)**: a ref's own in-template gates joined to the gates of THAT path, never unioned
  into every path of the component - that made `always_gates` claim conditions that do not always hold.
- **`key_reach.paths` NAMES THE WAYS** (#53): one `{path, own}` per way, `n_paths` of them, sorted by `(path, own)`. `path` is the `render_path.id` the way runs over, repeated when one row carries several ways (two sites, the outlets of one `ng-template`). `own` holds the ids the way adds to that row's `gates`/`branches`: the site's own chain and its `bind:`/`arg:`/`loop:` links. A way is `gates ∪ branches ∪ own`, and `always_*`/`maybe_*` intersect and union exactly those. An `edge:` filter only picks which rows pair with a chain; it never narrows a row. `components` cannot stand in for it, because a parent that only hands a list on (#50) is not listed.
- **`render_path`'s unique key is `(component, edges)`**, never `(component, hops)`: one parent rendering a
  child at two sites is two real paths with one component chain. `path_index` is assigned by SORTING on the
  key, and every id list in `key_reach` (`routes`, `maybe_gates`, `components`, `roots`, `modules`, `paths`) is sorted
  too: walk order reads as thousands of changed rows with no fact different.
- **NO PATH CAP** (signed-off divergence #1). A per-component cap hid real paths on one tree and most of the
  map on another. `truncated` stays because a consumer filters on it, and is always 0. `always_*` are
  intersections and only shrink as paths are added; the cap only under-reported the maybe side.
- **A WAY THROUGH A LITERAL `false` IS NO WAY** (`key/key_dead.rs`): an `*ngIf` or `@if` whose condition is the
  literal names no member, so no `gate_values` row can carry it. `ways_of` drops every (ref, path) through one
  before anything folds: no live way left is `n_paths 0` - the key renders for nobody - and its roots go too. A style
  gate whose value hides on every side (`style_hides.when = always`, `[style.display]="'none'"`) is dead the same way (#54).
- **THE CLOSURE TABLES DECLARE UNIQUE KEYS**, so a pass that files a row twice fails the insert instead of
  publishing a plausible duplicate.
- **NO REGEX: the dead-key trace is character walks** (`sourcescan.rs`, `sourcetrace.rs`). It decides whether
  a key nothing references is really dead; a pattern matching inside a comment or a longer identifier is how a
  live key gets called dead. **A file whose path escapes the workspace (`..`) is never scanned**: the
  compiler's `lib.dom.d.ts` is a `files` row, and scanning it called absent keys `leaf`.

## `gate_values` - one row per (gate, enum, dimension)

- **`op`**: `in` (renders only for these members), `not_in` (for every member but these), `unknown` (restricts
  the dimension in a way the map cannot read; `values_json` has no item rows). `values_json` (child table `gate_values__values_json`) is always the SMALLER side,
  so a long `in` is published as its `not_in` complement - except on a SET, whose sides are not complements ("holds some of A, B" is not "holds no C"): it keeps the side it was written on, and so does `key_reach.always_values` (`value_folds::fold_sided`: the side every way restricting the set wrote; ways on different sides keep the smaller one). `set = 1` marks a dimension several members can
  hold at once (a collection): a second `in` on it does not intersect.
- **`listed_json`** (divergence #7) is what a directive's CONFIG OBJECT lists for the dimension, resolved to
  members whatever the `op`; no node rows when no config lists it, a root with no child for an empty list. An exclusion list tested
  under a `!` stays `unknown` (hide or disable cannot be told apart), its members in `listed_json`. A
  conditional config (`c ? {...} : {...}`) is read per branch: a member every branch lists permits their
  union, one a branch omits is `unknown`, one no branch writes has no row. **A CONFIG READING AN `*ngFor` ITEM'S ENUM PROPERTY** (`{ kinds: [t.kind] }` over a list of literal objects, the list typed in place or by a named interface, `type` or class in any file, the list typed in place or by a named interface, `type` or class in any file, `key/key_loops.rs::Loops::expand`) is one branch PER ELEMENT: the gate permits their union, and a key rendered from element i carries `loop:<gate>#i`, whose own rows (`config_restrictions_all`) give it element i's value alone.
- **ONE ENUM TESTED THROUGH DIFFERENTLY WRITTEN EXPRESSIONS FOLDS AS THE ENUM** (`value_folds.rs::fold_enums`): when no
  dimension of an enum is published, each way permits the union of its own dimensions of it (none: the domain), and
  a smaller union is one row whose `dimension` joins the texts (`item.kind | kind`).
- **A CHILD'S INPUT IS THE PARENT'S EXPRESSION ON A WAY THROUGH THE EDGE THAT BINDS IT** (#33, `key/key_scopes.rs`, `value_folds.rs`): every row keeps its own text; on a way, `[item]="vm"` makes `vm.kind` and `item.kind` ONE variable (`pairs`), and after the way is keyed the present keys of one enum in one alias class all get the intersection of their sets (`unify_present`); published rows equal on every way under aliased texts are one row labelled by both (`merge_aliased`). Renaming either spelling into the other split one variable wherever only some rows were renamed (child up lost keys, parent down lost others). The forward rename (`renamed`) only labels `arg:` rows. Only a plain read chain the child never writes (no `this.item =` assignment, no setter) aliases. A literal, or a chain through the parent's `typeof X` alias field, is a constant and aliases nothing (#49).
- **AN `arg:` ROW READING ONE CALLEE PARAMETER IS ONE DIMENSION ON EVERY WAY** (#34, `value_folds.rs::per_way_keyed`, `key/fields/key_args.rs`): keyed by (callee, parameter) and published under the caller spellings joined (`relabel`), so ways that spelled the argument differently still share it; any other row is keyed by its text.
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
- **TWO DECLARATIONS OF ONE ENUM, A NUMBER, AND A `this.` ALIAS** (#21, `gate/ts/gate_same_enum.rs`, `gate/ts/gate_tsrows.rs`): an enum of the same name, members and values declared in two files is one enum for a comparison (a different member or value stays `unknown`); a number compared with an enum-typed value is the member it equals (`in []` when none has it, `>`/`<` stay `unknown`); `this.field.Member` through a field initialised to the enum object (`field = Kind`) reads as the template's alias does.
- **`[hidden]` HIDES, SO IT GATES ON ITS NEGATION** (#29): a `gates` row with `gate_kind = 'hidden'` whose `expression` is a row holding `!(c)`, read by the same `Not` handling as any gate; a key under it is restricted to the members `c` excludes. Never `declared_input`, whatever `gateInputs` lists.
- **A CLASS GATE IS A VISIBILITY CONDITION ONLY WHEN A STYLESHEET PROVES ITS CLASS HIDES** (#30, `gate_values::class_polarity`): `[class.x]="c"` with a `class_hides` row (scope `own`/`shared`; a rule hiding as a whole - `height: 0` + `overflow: hidden`, `opacity: 0` + `pointer-events: none` - counts too, #51, `cssmap/collapse.rs`) is read as `!c`; a class no rule proves hiding gives no `gate_values`/`gate_features` row and no `bind:` condition (it stays in `gates`/`always_gates`), and a `[style.x]` gate is likewise no condition (#52), except that a `[style.display]`/`[style.visibility]` ternary of literals whose one arm hides is read on its showing side (`style_hides.when`, #53). `tsapply.rs` reads the stylesheets BEFORE the closure for this.
- **A FIXED LIST OF CONSTANTS** (`gate/ts/gate_lists.rs`): `LIST.includes(v)`, `[A, B].some((x) => x === v)`,
  `indexOf !== -1`, and a function whose one return is such a test - exact both ways. A bare local is no dimension.
- **A KEY ASSIGNED IN TYPESCRIPT CARRIES ITS `if` AND `case`** (#4, `key/key_branches.rs`,
  `gate/ts/gate_cases.rs`): each write of a key (`field_binding`, `literal`) is a WAY IN joined to its chain of
  branches; an `else` holds the condition negated; a `case` group is one `in`, a `default` a `not_in` of its
  siblings; a write outside any branch empties the intersection. A key CHOSEN in a value is a link: `c ? 'a' :
  'b'` holds `c`/`!c`, `x && 'k'` holds `x`, `x || 'k'` holds `!x`, `??` nothing - the link is the condition's
  `expressions` id, `!`-prefixed when negated. A GETTER's returns are its writes, and a getter RETURNING A CALL follows it as a field holding the call does (its own `if`/`case` carried, the callee's not); a METHOD's - or an ARROW PROPERTY's (`label = () => t('k')`) - are too, but only at a node whose template CALLS it (#19, `key/fields/key_methods.rs`: a method WITH parameters carries its callee's branches only as `arg:` links (#47, below), and a string or ternary of strings handed as an argument is bound into the holes naming the parameter, any other argument leaves the hole open); otherwise a method's are not, except that a field (or such a getter) HOLDING ITS CALL reads the method's ONE plain return (`key/fields/key_fields.rs`, `key/fields/key_follow.rs`; no branch, no
  second return, its `tpl:` links dropped; a return that is a call of a configured `i18nCalls` method holds its arguments' keys; a return is followed through a call of another method - every return of one with several, its branches only as `arg:` links (#48), never a key such a return spells itself - and either side of `??`/`||` or a ternary's arms, no condition carried; a translate call WRAPPED in an object's property values, a list's non-object elements, a `+` chain or `Promise.resolve(..)` is held too, and so is what a `<list>.filter(..)`/`.slice(..)` receiver holds (#47; never the argument of another call), and so is one in a template literal's hole (`$hole_values`, a `const` local holding the call too) or under a string method chained on it (`replace`, `trim`, `toUpperCase`.., `$receiver_value`), and an argument that is a `const` object or ARRAY of strings read by an unknown index (`KEYS[this.kind]`, `LIST[i]`, `$indexed`) is every value of it; an `await <call>` is the call, and a field a `.then((x) => ..)` callback writes the bare parameter to holds what the call it follows resolves to (`key/fields/key_then.rs`) - an rxjs `.subscribe((x) => ..)` or `{ next: (x) => .. }` what it emits (through a `.pipe(..)` of operators that pass values on - `take`, `filter`, `tap`..; a `map` is a value of its own), `Promise.resolve(x).then` the `x`, and an rxjs `of(x)` is read through as `Promise.resolve(x)` is; a return of a bare local reads every later `=` and `+=` of it and every `.push(x)` on it too (`key/fields/key_pushes.rs`), its own or its enclosing method's, no branch carried; a call of the method's own parameter runs the function the call handed in, a call of a `const` local the arrow it holds, a call of a FREE function (a `function` or `const` arrow of no class, found by the call's `$target` when no member of that name is) is entered like a method - and a hole naming a parameter handed a string is that string (`t('a')` spells `x.a` alone), a string enum member's value, or a ternary of those (`t(c ? 'a' : 'b')` spells both, no condition carried, every combination however many)). **A KEY IN A LIST OF OBJECTS** (`tips = [{ tooltip: 'k' }]`, a call of a method or free function returning one from every return (`<list>.filter(..)`, `.slice(..)` or a `.map` handing its parameter back, bare or as `{ ...b }`, over one counts; `key/key_lists.rs`) - a property's value may be a translate call, its arguments' keys, or a call read as the callee reads its own (a local arrow, a hole handed a string), no condition carried - a `...` spread of such a call, or the union of every plain `=` the field's own class writes when it is initialised with nothing or `[]`, and every `this.<field>.push(x)` of it (under the push's branches) - each element under its write's branches, a callee's branches not carried) is a site only where an `*ngFor` (or an `@for` block, #41) over the field has its loop variable's property read (`t.tooltip`, `key/key_loops.rs`), at THAT node with ITS `gate_chain`; the whole field's read would drop every condition inside the loop from `always_gates`, and a list no `*ngFor` reads stays a `literal`. A list EVERY parent binds, as a bare field read, to a child's `@Input` the child writes nowhere is the child's own (`key/fields/key_inputs.rs`), and the parent that built it is no site of the key through it (#50, `key_routes::Routes::claim`): its `ts_call`/`literal` site goes, so the key renders on the paths through that render edge alone, under the binding node's gates. A parent that displays the key itself keeps its own site. A translation call (`ts_call`) takes its own row's chain; a literal the chain
  of the innermost assignment, `return` or argument holding it; a PIPE's `transform` key renders at every node
  applying the pipe (route `pipe`). `key_reach` adds `always_branches`/`maybe_branches` and `branch_senses`
  (`then`/`else`/`case`/`loop`/`bind`/`either`; `always_branches`/`maybe_branches` can carry `loop:`, `bind:` and `or:` ids); `always_gates` still names `gates` rows only.
- **A CALLEE'S OWN `if` CARRIED THROUGH THE CALL A FIELD OR GETTER HOLDS** (#26, `key/fields/key_args.rs`): `get t() { return this.svc.label(this.item.kind); }` over `label(kind) { if (warmHues.indexOf(kind) !== -1) { return t('a'); } .. }` holds `arg:<branch>@<call>` - the branch's tree with each parameter read replaced by the argument written at the call, resolved in the caller (the member the call is a return or an assignment of, its `const` locals in scope - a plain one is its own dimension) - so `a` is `item.kind in [..]`. Not a case. A DEEPER CALLEE'S `if` rides too (#48): `arg:<branch>@<E0>~^<E1>~..`, one place per call (`^E` a call inside expression `E`), each hop's arguments put in place of the parameters below it, no bound on the hops; each (member, arguments) read once and replayed per call. Refused at a hop (no condition of it or below): recursion (a member reached while still read), a member a subclass declares again, a call not found once in its expression, a spread, a written parameter, a local no value stands for. A `const` local a `private` one-return accessor of a parameter chain initializes is that chain. A METHOD A TEMPLATE CALLS (`text(row)`) carries them too, as `arg:<branch>@<site>@<template expression>`, its own parameters replaced by the template's arguments at that call (`row.kind`, not `r.kind`). A local arrow or a handed-in function is entered before its call is read as a translate call, even when its name is in `i18nCalls`.
- **A KEY A CLASS INSTANCE CARRIES** (#24, `key/fields/key_classes.rs`): a call (`$owner`) or property read (`$read`) through a receiver whose declared type is a class, a BASE class or an INTERFACE is the member of that class and of every `extends`/`implements` subtype (transitively) that a `new` builds somewhere in the tree - a subclass nothing builds contributes nothing - or the nearest base's when it has none. A method's returns are followed as a call's; a property holds its own writes, its class's (or a base's) constructor `this.<member> = ..` and a translate call written straight to it, and a subclass's constructor write to the base's field is kept per subclass (the field is still no template site). No branch of the callee is carried. **A LIST OF INSTANCES** (`key/fields/key_instances.rs`): a list a parent builds with `.push(new X(..))` - on a bare local handed to the field (`const bs = []; bs.push(..); this.list = bs`, also in a `.then` callback) or on `this.list` itself, a push in a callback counting for its enclosing member - and binds to a child's `@Input` is read per element: `new X` is X's class (never a subtype), its properties by dotted path - the member's initializer and the constructors' (and bases') `this.<member> = ..` writes, an object literal written to `tooltip` as `tooltip.text`, a function-valued property (`this.tipText`, an arrow in place, a method) its returns, a translate call written straight to a member its keys. A loop's `b.tooltip.text(row)` / `b.text` is a site at THAT node. A template CALLING a method of a base or an interface (`{{ s.tip() }}`) reads every built subtype's (`Classes::answers_of`), and one READING such a property (`{{ b.text }}`) every built subtype's writes to it (`Classes::answering_row`) - each on a path through a component that builds that subtype (`via:`, #55; a builder that is no component restricts nothing). A constructor write includes one in a callback inside it (`.then(() => { this.tooltip = .. })`), and a function an instance holds hands back every return's own key (`if (c) { return 'a'; } return 'b';`, `Returned::enter_member`). **WHERE THE LIST COMES FROM** (#25): a callee's returned local is its pushes too, in a callback of the callee as well (`key_lists::ListSource`), a pushed `const` holding a `new` is that `new`, and `push(...this.make())` adds what `make` returns (`$spread`). **HOW THE CHILD GETS IT**: an `@Input` setter copying its parameter into a field (the getter of its name is what the `*ngFor` reads), an input bound to a call or a getter whose every return is a list, and EACH RENDER EDGE'S OWN LIST - its elements carry `edge:<render>` and a way holds them only through that edge (`keyreach::ways_of`), so a parent binding nothing no longer voids the others; and a property of a field (`[items]="cfg.list"`: its initializer's array and every `this.cfg.list = [..]` of its class, `holder`; any other write leaves it unread). **HOW THE CHILD READS IT**: a template call handing the loop item to a method (`tipOf(b, row)`) reads the paths that method reads off the parameter (`$reads`, `key/fields/key_handed.rs`).
- **A BARE LITERAL EQUAL TO A KEY INSIDE A COMPONENT'S CLASS IS A `literal` SITE OF THAT COMPONENT ONLY** (#31, `key/routes/key_routes.rs`, step 5): the enclosing class is found by the literal's `line`/`col` against `classes`; one outside any class or in a non-component class is a site of every component of its file. **The initializer of a FIELD NOTHING READS is no site at all** (#44, `key/routes/key_unread.rs`): no template read resolves to the member (`target.row`), no unresolved read, property access (`files.prop_reads`) or string literal spells its name, it has no decorator, and its file never hands `this` away (`files.this_indexed`). A plain `=` write to the name THROUGH `this`, in any file, is no read (#68; a write through any other receiver, `notice.f = 'k'` on a made instance, stays a read and its literal a site): the literal such a write to an unread field holds is no site, nor is the literal a `return` of an unread GETTER is, nor an arm of a ternary of strings that is either's whole value; a literal in a call or condition of those stays one.
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
  `return`, in its own class's constructor or `ngOnInit` (not an `@Input`) is read both ways; a METHOD OF NO PARAMETER of that guarded shape is `C && E` where a gate CALLS it (#46; a bare read of a method is a function, truthy, and never replaced), and a `const` local tested as `C` is its initializer (an `E` that is a bare read chain, `this.d?.f`, is spelled back: no tree is kept for one, #66); written once
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
- **AN OR IS THE UNION OF ITS SIDES** (#10, `gate/gate_collapse.rs::either`); `!(a && b)` is `!a || !b`. A TERNARY in truth position is `(c && a) || (!c && b)` (#46, `gate_props::walk`), so its arms union only on a dimension `c` restricts nothing of. A
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
  nothing, and the way through it holds `bind:<render>#<gate>` (sense `bind`). An edge on which the condition of a gate that HIDES its
  element (`*ngIf`, an `@if` branch, `[hidden]`, a class a stylesheet proves hiding) becomes the literal `false` is no way; any other gate there (a `gateInputs` property such as `[disabled]`, a style, `*ngFor`, `*ngSwitchCase`) hides nothing and is no condition on that edge (#37). The child writing the flag itself is still unread.
- **A KEY CHOSEN IN A CONDITIONAL ARM takes the arm** (#11): the extractor stamps `choices` on each statement
  row inside an arm; the i18n ref joins it through `i18n_refs.call` and `key_reach` folds it like a branch.
  A key a TEMPLATE expression chooses (`[text]="c ? 'a' : 'b'"`, `x && 'k'`, `x || 'k'`, inside a pipe or a
  call too) holds its arm as `arm:<expression>#<n>` (#16, `key/key_arms.rs`): the condition's
  subtree is read as a `gates` tree is, so `c` restricts exactly as `*ngIf="c"` does.
- **A KEY IN AN `<ng-template #ref>` ONLY OUTLETS RENDER TAKES EACH OUTLET'S CHAIN AS A WAY** (#36, `key/routes/key_outlets.rs`): when every read of `ref` in its template is a bare `[ngTemplateOutlet]`/`*ngTemplateOutlet`/`*ngIf="..; then ref"` binding, a site inside it (template, template string, pipe, field binding) is one way per outlet: the outlet node's `gate_chain` (an outlet inside another such template: each of ITS ways) without the outlet's hand-over gate rows (`ngTemplateOutlet*`, `ngIfThen`, `ngIfElse`), then the site's own gates below the template. A ref an `else` reads (it renders under the negation, which a gate list cannot hold), that any other expression reads, that is declared twice in the template, or that a `@ViewChild`/`@ContentChild` queries (by name, or by type: every template of the class) keeps the site's own chain. An outlet inside the template it renders is no way. Not read: a COMPONENT rendered inside such a template (`render_path` keeps the edge's own chain).
- **A COMPONENT A FACTORY RETURNS UNDER A `case` OR `if` RENDERS UNDER IT** (#13, `key/key_returned.rs`): a
  dynamic render's `renders.return_ways` lists the `returns` rows each way the class came through; the edge
  holds the branches and cases EVERY way holds (one case and no case hold nothing; two or more cases, each alone
  on its way, hold `or:<case>|<case>`, the union of their values - #15). They ride `render_path.branches` - never `gates` - and `key_reach` folds them with the key's own,
  so `always_branches`/`always_values` gain the case. `path_always_gates` stays gates alone.
- **A BRANCH AROUND THE CALL OF A PRIVATE METHOD HOLDS INSIDE IT** (#45, `key/routes/key_called.rs::CallLift`): a dynamic render's edge holds its creating call's own chain (`renders.call`), and a statement of a `private` method (not an `ng*` hook, no decorator) holds what EVERY call of it holds - the intersection of each caller's chain joined to what the caller's own method lifts - when every read of its name is a `calls` row resolved to it: no template, string or other file reads it, its file's `prop_reads` count equals its callers and `this_indexed` is 0. Read for a render edge (`render_path.branches`), a `ts_call` site and every literal way (`key_branches::literal_ways`: literal, pipe). A method called from itself lifts nothing.

**AN `@switch` CASE IS AN `*ngSwitchCase`** (#40, `gate/gate_switch.rs`): a `SwitchBlockCase` gate restricts the nearest enclosing `SwitchBlock`'s discriminant to its member, `@default` (no expression, no gate) nothing.

A `const` local holding a DECISION (a comparison, `&&`/`||`, `!`, `includes`/`some`) is its initializer wherever a gate
reads it (`gate/ts/gate_tsrows.rs`), a bare `this.f = local` included.

**Not read yet**: a set held in a local (`active?.some(...)`), a property written more than once.

## Divergences

Every signed-off divergence from the predecessor, and the rule behind them: [DIVERGENCES.md](DIVERGENCES.md).
A new restriction is a new divergence: add it there.
