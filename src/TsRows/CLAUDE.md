# The TypeScript half's extractor - the row contract node writes

What runs where, partial runs and the skip key are [../../.claude/rules/typescript-half.md](../../.claude/rules/typescript-half.md);
what the rust closure derives from these rows is
[../../rust/fbtcore/src/rows/ts/CLAUDE.md](../../rust/fbtcore/src/rows/ts/CLAUDE.md). Consumer-facing columns are the
`map-sqlite` skill's `typescript.md` (`skills/map-sqlite/typescript.md`).

## What the half writes

`--map-sqlite` over an Angular workspace writes FRAMEWORK facts no compiler produces alone: bindings, template
nodes, render edges, input uses, routes and what each can reach - 67 tables with rows on a large workspace (v2.8.34), the closure's included,
plus `diagnostics` (what the extraction could not answer: a count with no names cannot be checked). Every row
carries `half = "typescript"`.

**BUMP `rows` IN THE SETUP HASH (`TsMap.mjs`) WITH EVERY CHANGE TO WHAT A ROW CARRIES**: an unchanged tree skips node and
keeps its rows, so a deployed exe that adds a column reads a consumer's tree as unchanged for ever. A change only to a
table rust DERIVES from these rows (`gate_values`, `key_reach`, `render_path`, the stylesheet tables) needs no bump and
reads no tree: a new build derives them again from the stored rows (`rust/fbtcore/src/rows/derive/tsderive.rs`). A change to
the rust that STORES them is in the setup by itself (`--build` is the digest of the scripts and the files
`rust/fbtcore/build.rs` lists). The comment above it lists what each number added; add a line.

**THE MAP STATES WHAT SQLITE CANNOT RECORD** - no foreign keys, no arrays, no note of which column reaches a
row's own file. The half derives it from the finished rows and publishes it in `_meta` (`spec:typescript`,
`rust/fbtcore/src/rows/tsapply.rs`): every id prefix, every join, every anchored table and which columns hold a
structure (`TsDerive/TsSpecJoins.mjs`, `TsDerive/TsSpecAnchors.mjs`) - and which key claims a row's id (`identity`). A
PARTIAL run derives it from every row it holds, carried ones included (`Store.load` notes their prefixes), so it
publishes what a full run of the same tree would; only `rebuilt` is republished, since only a full run sees where a
table's rows are minted. The anchor is PROVEN per field, never read
off the join graph: `renders` carries `from_class`, so a graph walk files a render edge under the component's
`.ts` when the row was read from its `.html`. Every join it declares is INDEXED (`schema::index_declared`, after the
closure is written, #70): the map is opened read-only, so a consumer cannot add one.

**IDS ARE HANDLES, NEVER FACTS.** A row's identity is what it says about the source - a call: file + line + callee
+ resolved symbol; a binding: file + line + target + source; a translation: key + file + line. Two maps (or two
runs) are compared by that, every carried field equal, never by `c:412`.

## Imports - three columns that were each read the wrong way once

- **`imports.names`** is `{name, as, kind}`, `name` the LOCAL binding and `as` the name a rename came THROUGH:
  `import { A as B }` is `{name: B, as: A}`. Kept because consumers read it. A dependency NgModule is matched by
  its LOCAL name (divergence #5, `TsDerive/TsModuleScope.mjs`): reading `as ?? name` let an alias through.
- **`imports.external`** is the SPELLING (a specifier not starting with `.`), not the location: a tsconfig
  alias (`@utils`) is external and resolves in the tree. "In the tree" is
  `JOIN files rf ON rf.id = i.resolved_file WHERE rf.sha IS NOT NULL`.
- **`imports.resolved`** is the module the specifier NAMES - often a barrel.
- **`imports.in_tree`** is whether the resolved file is in the tree (not `node_modules`, not above the root): the LOCATION `external` is not.
- **`imports.dynamic`** (`import` / `require`) marks a row for a bare `import('./x')` or `require('./x')`: no names, no `import_names`.
- **`import_names`** (`ib:`, `TsImports.mjs`) is one row per name an import binds, in source order: `local`,
  `imported` (`default`, `*`), `kind`, `type_only`, and where it is DECLARED - `declared`, `declared_file`,
  `declared_id` (a top-level class, function, interface, enum, type alias or const; none when two share the name),
  `declared_name` - plus `via`, every barrel on the way. All three barrel shapes are followed: `export * from`,
  `export { X } from`, and `import { X } ...; export { X }` - through a chain of any length; only a (file, name) the
  walk already stood on (a cycle) stops it. `exports.resolved`/`resolved_file` is the file a
  re-export's `from` resolves to. An importer DEPENDS on its declaring file and every barrel in `via`
  (`rust/fbtcore/src/rows/partial/deps.rs`), so a partial run re-reads it when either moves.

## What a name resolves to - by the declaration, never by the name written

- **A template pipe usage names the pipe Angular runs** (rows 59, #86, `TsTpl/TsPipeRefs.mjs`): `expressions.pipes` and
  `template_strings.pipes` stay the names; `pipe_refs` beside them is one `{name, ...}` per name, resolved by the SCOPE of
  the template's component - its NgModule's declarations and what each import exports, transitively, or a standalone
  component's own `imports` (`TsModuleScope.mjs::makeScopes`, the render rollup's machinery) - `id` the `pipes` row,
  `external: true` + `class` for a library pipe, `ambiguous: true` for two of one name (Angular warns, NG0313, and takes the
  last registered - an order no row holds). A library entry's pipes are read off its typings at extraction
  (`ɵpipe`/`ɵmod`, `TsDecls/TsLibPipes.mjs`) into `external_pipes` on `components`/`ng_modules` (`from`: `imports` or
  `exports`). An unsettled name is a `diagnostics` row (`pipe_unresolved_in_scope` with `reason`, `pipe_ambiguous_in_scope`).
  Resolved over the WHOLE store on every run, carried rows too - a template expression's `pipes` is carried for it
  (`TsSetup/TsCarried.mjs`).
- **A base class carries its row** (rows 58, #87): `classes.extends` and each of its `type_args` (every level) hold `line`,
  where the declaration the checker reached is (`TsClassRows.mjs::heritageRef`, alias and barrel followed), and `id`, the
  `classes` row at that file and line (`TsRefs.mjs::rollupRefIds`); a library class, or a place two rows share, has none.
- **A union type names what its members name** (rows 61, #88, `TsDecls/TsTypeRef.mjs::refsOfUnion`): `type_ref` on `members`,
  `members.params` and `type_members` is the ONE declaration a union's members name - `Codes | string`, `Codes | null`, and
  `Kind | undefined` as the enum `Kind` (the checker flattens it to members) - and when they name SEVERAL (`A | B`) it is
  null and `type_refs` lists each `{name, file, line, ...}` as a `type_ref` would be, sorted by place (the checker's order
  is not a full and a partial run's). A parameter's `resolved` is likewise the one name its written union's members
  resolve to. A union an alias names (`type Id = A | B`) is the alias, as before. `locals` and `returns` resolve no type.
- **A getter or setter is typed as a property is** (rows 63, #95, `TsDecls/TsClassRows.mjs::memberRow`): without an annotation
  its `type` and `type_ref` are what the checker infers - an accessor's symbol is the property's, never a function type - so a
  template read chained off `get vm() { return this.load<Outer>(); }` resolves past it. A method still takes only its annotation.
- **A member an alias borrows from a generic is typed as the alias instantiates it** (rows 64, #96, `TsDecls/TsTypeRows.mjs::typeMemberRowsFromType`):
  `type PayloadBox = Box<Payload>` gives its `computed` member `Payload: TPayload` the type `Payload` and its `type_ref`, the checker's type for the
  alias, wherever it differs from the declared one - a member nothing instantiates keeps its annotation's text.
- **An intersection names what its parts name** (rows 65, #97, `TsDecls/TsTypeRef.mjs::refsOfParts`): `X & { .. }` is `type_ref` X, `A & B`
  lists both in `type_refs` as a union naming several does; a part that is itself an unaliased union or intersection is walked into.
  A TEMPLATE READ follows `type_refs` as `type_refs_of` says they combine (rows 72, `TsTpl/TsTplOwners.mjs::propWithEnv`, [TsTemplateRows.md](TsTemplateRows.md)): a union (`or`) answers when ONE
  declaration does, an intersection (`and`) with the FIRST part written that declares the name. A hop whose type names nothing ENDS the chain (never the previous owner:
  `{{ x.y }}` over an untyped `x` is not the component's `y`) - but an anonymous type or object literal is a `shapes` row and a hop (rows 71): [TsShapeRows.md](TsShapeRows.md).
- **An enum a field holds is read through** (rows 66, `TsTpl/TsTplReads.mjs`): `Kinds = Kind` is typed `typeof Kind`, and `{{ Kinds.Alpha }}`
  targets the member - the enum's row at the member's `line`, which each `enums.members` item now carries. A name the enum lacks ends the chain.
- **A generic is read as it is instantiated** (rows 67, #98, `TsTpl/TsTplReads.mjs::resolveRef`): classes and interfaces carry `type_params`, a
  `type_ref` naming a type parameter carries `param: true` and a generic one its `args` in order (`TsDecls/TsTypeRef.mjs`). A read binds them as it
  walks: `Concrete extends Base<Payload>` binds `Base`'s `T` from `extends.type_args`, `Wrapped: Wrapper<T>` binds `Wrapper`'s parameter to
  what `T` is bound to, and `Inner: T` then reads on as `Payload`. A parameter nothing passed reads as its `default`, else its `constraint` (rows 72).
- **A template write names what it writes** (rows 68, #99): `(valueChange)="f.On = $event"` - a `Write` node heads its chain like a read and carries
  the `target` of the member written, so which element SETS a member is a join, not only which reads it.
- **The rows are 1:1 with the source** (rows 69, `TsDecls/TsBody.mjs::collectExpr`): code OUTSIDE a function body - a property initializer,
  a class or member decorator's arguments, a parameter default, a top-level statement - is walked as a body: its calls, assignments and
  locals are rows under its member (class, or none at the top level), and every callback in it - an ngrx `createEffect`/`select`
  callback too - is a `functions` row with its own body (`state_effects` reads an effect's calls through that chain, never the callback's
  own return). A concise body that is a function (`(a) => (b) => ..`) is its own row; a class expression is a `classes` row named by
  its holder; `(x as T) = v` and `[a, b] = [b, a]` are assignments; a declaration without an initializer (`let t;`, a loop variable,
  `catch (e)`) is a local. Measured against a fresh parse of a large workspace by file and line, the only differences left are the
  contract: arrow PROPERTIES are members, implicit concise returns, top-level `const`s are `consts`.
- **Four rows a source declares and the map missed** (rows 70): a CONSTRUCTOR PARAMETER PROPERTY (`constructor(private dep: Dep)`) is a
  `members` row, `kind: 'property'`, `parameter: true` (`TsDecls/TsClassRows.mjs::extractClass`; no input, output or summary), so `this.dep` and a
  template's `{{ cfg.id }}` resolve; an ES `#name` member is `visibility: 'private'` (`TsModifiers.mjs`); a single-member enum's `type_ref` names
  the ENUM when the annotation is written as it (`Kind.Alpha` as written still names the member, `TsTypeRef.mjs::enumOfMember`); and a type literal
  nested in a union, intersection, type argument, tuple, parentheses or a signature's parameters and return type has its `type_members`, as one
  directly under a property does (`TsTypeRows.mjs::nestedLiterals`; a literal member another operand also declares has its own row too, rows 75). Also: a concise arrow's implicit `returns` is anchored at its body START (`TsNodes.mjs::locationOf`: an expression is never anchored at a name
  in it); the flat `reads`/`calls` hold no chain tail of a pipe, call, keyed read or binary as a root (`TsExprSummary.mjs::hangsFromContext`; `''`
  stays in `strings`); a call of an OVERLOADED method has `target_id`, the implementation, the class's last row of that name (`TsRefs.mjs`); and a
  side-effect `import './flags'` of a script resolves through the compiler's module resolution (`TsImports.mjs::importRow`). Four TEMPLATE rows follow Angular too (wrapper matches, accessor pairs, chains past a call, a condition's outer scope): [TsTemplateRows.md](TsTemplateRows.md). A LOOP is a `loops` row (never a `branches` row - those are conditions), the rows in its body carry `loop`, a bare expression statement is an `expressions` row and `x++` an assignment: [TsBodyRows.md](TsBodyRows.md).
## Expression trees the gate readers need (`TsDecls/TsBody.mjs`)

`value` keeps only the last link of a call chain, so the rust readers need the TREE: `returns.expression`,
`locals.expression`, `assignments.expression`, `branches.condition_expr`, `switch_cases.discriminant_expr` and
`label_exprs`, always written, plus on a bare name a `target` (module-level const, function, enum or CLASS), a `local`
(`{file, line, col}`, its `locals` row's place) or a `param` (the place of a parameter's name): [TsBodyRows.md](TsBodyRows.md).
An `Fn` tree names its `params` and its `rest` parameter.

- **A keyword literal is `{$literal, $kind}`** (rows 58, #85, `TsExpr/TsValue.mjs::cell`, `TsValueRefs.mjs::undefinedLiteral`):
  the global `undefined` (the checker's symbol has no declaration - a shadowing local is followed) and `void 0` are
  `{$literal: 'undefined', $kind: 'UndefinedKeyword' | 'VoidExpression'}` everywhere; `null` stays JSON null inside a
  list or an object and is `{$literal: 'null', $kind: 'NullKeyword'}` as a WHOLE cell (`evalCell`: every statement and
  declaration `value`, `condition`, `discriminant`), since a NULL cell is `return;`. The rust store reads that root back
  as `Value::Null` (`store/row.rs::null_literal`, the http pass too), so no gate reads it differently. A name a
  destructuring binds keeps its `Identifier` value plus `$local`, the `locals` row's place.

- **A namespace's statements are a file's** (`TsTypeRows.mjs::moduleRows`): a class, function or const in `namespace X { .. }` is a row like a top-level one, and every row a namespace declares carries `in_module`; an AMBIENT block (`declare module`, `declare global`, `declare namespace`) yields only its interfaces and aliases.

- **A top-level `X.prototype.m = function` is a `functions` row** with `form: 'prototype-method'`, `name` the assigned text and `target` the declaration a call of `m` resolves to (`TsDeclRows.mjs::prototypeRow`). Also under a top-level `if (!X.prototype.m)` and from `Object.defineProperty(X.prototype, 'm', { value: function })`, targeted through the prototype's type. Its body's rows carry the enclosing `if` arm's `branch` (rows 80), which no reader takes for a condition on what the method returns (`gate_curried.rs::definition_guards`).

- **A template literal keeps each hole evaluated** (`$hole_values`, beside `$holes`' source text, rows 41); a string method
  chained on a call keeps the receiver's value (`$receiver_value`, only a string-typed one); a const object of strings read by
  an unknown index keeps `$indexed`, its values - a `const` array's too, rows 42 (`TsValueRefs.mjs`), which `i18n_refs` claims like a ternary's arms.

- **A call or property read through a receiver keeps its declaring class** (rows 43, `TsValueRefs.mjs::ownerTarget`): `$owner` (a name, the file is the call's `$target`) on a `$call`, `$read` `{name, file, owner}` on a `$expr` property access - not on `this.x`. The rust side joins it to `classes` by (file, name) to reach every subtype a `new` builds (`key/fields/key_classes.rs`).

- **A write to a property of a field names the field** (`assignments.holder`, rows 44: `this.cfg.list = ..` holds `cfg`), and a
  `this.<x>` write a file+name key refuses - two classes of one file declare `x` - takes the writing class's own member, or its
  nearest base's (`TsRefs.mjs::ownMember`).

- **A spread call argument keeps what it spreads** (rows 44, `TsBody.mjs::emitCall`): `list.push(...this.make())` carries `$spread`, the
  evaluated expression, beside the argument's own text - `key/lists/key_lists.rs` reads the callee's list through it.

- **The rows join by id, not by position** (rows 33): a `$fn` call argument, return value, local value, or one an assignment or a member's initializer holds (in place or inside an object, rows 44) names its `functions` row (`$fn_id`,
  `TsWalk.mjs::linkFunctions` - a `const` arrow's row sits at the NAME, its tree at the arrow), and a chained call
  names the call it is made on (`calls.receiver_call`).

- **A shorthand property's value is its const's** (rows 46, `TsValueRefs.mjs::evalShorthand`): `{ template }` is `{ template: template }`, in every object literal. A const-held inline template's spans are placed at the const's line (`TsClassRows.mjs::argPlacesOf`), same file only - an imported const keeps the decorator's line.
- **A `createEffect` callback's body is walked** (rows 46, `TsDecls/TsClassRows.mjs`, `fromNgrx`): its `ofType`/`pipe` calls are `calls` rows and its nested arrows `functions` rows under the effect member, which `state_effects` (`TsDerive/TsNgrx.mjs`) reads. Routes, reducer entries and effects in a constructor (rows 79): [TsBodyRows.md](TsBodyRows.md).
- **An arrow-function PROPERTY returns like a METHOD** (divergence #8): `isMain = (id: P) => [P.A].some(...)`
  keeps its `members.value` (`{"$fn": ...}`), gains `members.params` and a `returns` row keyed on the member,
  `implicit = 1` for a concise body. A function-valued `const` has the same under its `functions` row.
- **A statement inside a conditional's arm carries `choices`** (divergence #11): `c ? this.t('a') : this.t('b')`
  and `c && this.add(...)` stamp each `calls`, `assignments`, `returns` and `locals` row in an arm with the
  condition's `expressions` id, `!`-prefixed on the false side; a condition in statement position gets its own
  `logic`/`ternary` `expressions` row.
- **What property names a file reads** (rows 51, `TsDecls/TsPropReads.mjs`): `files.prop_reads` is `{name: count}` of every name a property
  access, a destructuring or a shorthand property spells - by NAME, never resolved, so it only over-counts - and
  `files.this_indexed` is 1 when `this` is anything but the receiver of `this.x`/`this['x']` or a `.bind`/`.call`/`.apply`
  argument. The closure reads them to prove a component field unread (`key/routes/key_unread.rs`, divergence #44) and every read
  of a private method a call (`key/routes/key_called.rs`, #45).
- **`template_shared_by_components`** is a fact about the TREE, read off the `templates` rows whether or not the
  `--ts-html` mirror is asked for. A `@for`'s TRACK expression (`block_track`, `TsTpl/TsTplExtract.mjs`) sits on
  the block's own node and must not overwrite the block's expression.

## Stylesheets - does a class binding hide its element (`rust/fbtcore/src/cssmap/`)

A `[class.not-visible]` gate names a class; only the stylesheet says it hides anything. Every `.css`, `.scss`,
`.sass`, `.less` row of the half is parsed by `raffia` in the exe (the workspace has no parser to borrow) AFTER
node's rows are stored, on every run. The three tables are WHOLE - rewritten by every run, partial included - and
carry no `owner_file`.

- **`stylesheets`** (`cssf:`): `file`, `syntax`, `declarations`, `recovered` (errors read past), `error`/
  `error_line` when unreadable - then no rules and a `diagnostics` row `stylesheet_unparsed`.
- **`style_rules`** (`css:`): one per declaration under one selector - `selector` as written, `resolved` as Sass
  emits it (`&` the parent, a parent list multiplies), `subject`, `classes` (required of the element, never one in
  `:not()`), `on_element` (0 for a pseudo-element), `media`, `context` (every other enclosing at-rule, ` > `-joined),
  `property`, `value`, `important`, `hides` (`display: none`, `visibility: hidden|collapse` as written - a
  `$variable` is not - or the ZERO of a rule that hides as a whole, #89, `cssmap/collapse.rs`: `height`/`max-height: 0`
  with `overflow(-y): hidden|clip` and no vertical padding, non-zero `min-height`, `display: inline|contents` in the rule;
  `opacity: 0` with `pointer-events: none`; a `&.x`-nested rule inherits its outer rule's partners), `live` (0 inside a
  `@mixin` and for a `%placeholder`).
- **`class_hides`** (`cssh:`): a `[class.x]` gate (`via = 'class'`) or a literal `[ngClass]` class (an object key, a string, an array item or a ternary arm; `via =
  'ngClass'`) joined to every live, on-element, hiding rule whose subject requires the class - `bare` (the whole
  selector is `.x`) and `scope`: `own` (the component's `styleUrls`), `other` (another component's: reaches only
  through `::ng-deep` or `ViewEncapsulation.None`), `shared` (no component names the sheet). `class_hides` is written
  BEFORE the closure and read by it (`class_polarity`): a class gate restricts only through a row of scope `own` or `shared`.
- **`style_hides`** (`cssy:`): one row per `[style.x]` gate (`via = 'style'`) - `property`, `unit` (the gate row's `unit`, rows 62:
  Angular's `px` of `[style.height.px]`, which the gate's `name` drops), `condition`, `alone` (`display`/`visibility`), `hides`, and `when`
  from its literal value or two-literal ternary: `always`/`never`/`then`/`else`, NULL when not a literal (#53) - then one per LITERAL key of an
  `[ngStyle]` object (`via = 'ngStyle'`, `gate` NULL, `'height.px'` split into property and unit, #54). A gate of `when = always` is dead
  as `*ngIf="false"` is (`key_dead`); an ngStyle row restricts nothing.

Not followed, so not claimed: `@extend`, a mixin's body at its `@include`, `@use`/`@import` between sheets, and a
component's inline `styles: [...]`.

## How it is checked

Black-box over the CLI: `tests/deep/TsRows.Tests.ps1`, `tests/deep/TsAngular/` (imports, styles, reads,
gates), `tests/deep/TsKeys/` and `tests/deep/TsPlain/`, helpers in `tests/deep/TsRows.Helpers.ps1`. A case that changes rows must fail with the extractor line
reverted. **A port that improves rows cannot be told from one that breaks them**: a row that changes what a
consumer reads is a numbered, signed-off divergence (listed in [DIVERGENCES.md](../../rust/fbtcore/src/rows/ts/DIVERGENCES.md)), never a quiet fix.
