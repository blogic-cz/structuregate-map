# Body rows the 1:1 audit found missing (rows 70)

Loops, bare expression statements and `++`/`--` had no row of their own. The case for all three is
`tests/deep/TsAngular/TsValues/TsRowsLoopRows.Tests.ps1`; the code is `TsDecls/TsExpr/TsLoops.mjs` and `TsDecls/TsBody.mjs`
(`collectLoop`, `collectStatement`, `emitUpdate`).

- **A loop is a `loops` row** (`lp:`): `kind` (`for`, `for_of`, `for_in`, `while`, `do`), `await` (only when true, `for await`), `variables` (the names its
  initializer declares, destructuring flattened), `source` (the header expression's text - the iterable of a `for..of`/`for..in`, the condition of
  the others, `''` for `for (;;)`), `expr` (the `expressions` row, role `loop`, of that same expression - null when none), `incrementor_source` of a
  `for`, and `line`/`col`/`end_line`; `member`, `class`, `branch`, `case`, `choices` as a `calls` row has them. A loop nested in a loop carries
  the outer one as its own `loop`. The header's initializer, condition, incrementor and iterable are walked in the ENCLOSING context, so a call in
  them is a `calls` row outside the loop's body.
- **A row in a loop's BODY carries `loop`**, the innermost loop's id, on every table that takes `branch` (`calls`, `assignments`, `locals`, `returns`,
  `raises`, `handlers`, `loops`). `loop` is LEXICAL like `branch` (rows 71): a function or class expression inside the body keeps the enclosing loop's id,
  so `for (x of xs) { xs.forEach((y) => this.f(y)); }` puts the call under the loop. The id then names a loop of another member, as a `branch` does.
- **A LOOP IS NEVER A `branches` ROW HERE.** The plain half and C# record one with a `kind`; this half's `branches` rows are if-arms (`sense`) that the
  rust closure reads as CONDITIONS (`rust/fbtcore/src/rows/ts/key/key_branches.rs`, the gate readers). A key assigned inside a loop written as a
  branch would turn conditional and the release compare would report a lost restriction. Nothing in rust reads `loop` yet; `deriveJoins` declares it
  a join to `loops` as it does `branch`, so a partial run's `dangling` check covers it.
- **A bare expression statement whose walk wrote no row is an `expressions` row** of role `statement` (`this.a.b;`, `x;`, `a && b;`, `delete o.k;`):
  decided by what the walk wrote (a call, a write, a local, a function), never by the expression's kind.
- **`x++`, `--this.n` are `assignments` rows**: `operator` `++`/`--`, `value` and `expression` null, `source` the whole update, `target`/`scope`
  as for `=`. A counter written only by an update had no writer in the map.

# What a bare name in a code tree is bound to (rows 74)

`TsDecls/TsExpr/TsReadRefs.mjs` answers it, `TsExprNorm.mjs::bareRead` writes it; the cases are `tests/deep/TsAngular/TsResolved/TsRowsChainRows.Tests.ps1`.

- **`target`** for a top-level `const`, function, enum or CLASS (an import alias followed). A top-level DESTRUCTURING `const` counts (rows 76): `export const { DemoActions } = makeSlice()`, nested patterns too, targets the bound NAME's line, where `consts` has one value-less
  `destructured` row per bound name (the initializer is not the name's value, so no reader takes it for one). A class target has `kind: 'class'` and the class NAME's line (its `classes` row's `line`,
  never the decorator's). `rust/fbtcore/src/rows/ts/gate/ts/gate_tsrows.rs::names_class` makes every closure reader take it as the bare name it was: no row, never a constant or a member.
- **`local`** for a `const`/`let` with an initializer; **`param`** for a parameter of a function-like with a body, `{file, line, col}` of the parameter's name (a destructured
  parameter: the binding element's name). `members__params`/`functions__params` items carry `line`/`col`, and `bindings` when destructured, so the join is exact. A name a destructuring binds also has `default` (`evalCell` of `= 1`), `type` (the checker's text) and, in an array pattern, `index` (holes counted, never listed) on a `locals` row's `bindings`, a params item's `bindings` and its per-name `consts` row (rows 77, `TsExpr/TsBindings.mjs`). A `locals` row's `bindings` is one item per name the pattern binds at ANY depth (rows 78, `TsBody.mjs::emitLocal`): `property` the path (`A.B.C`, an array element adds none - at the top of an array pattern `property` is null and `index` says where, rows 80), `local`, `line`/`col` of the name. `param` on a CODE tree is that place,
  `param: true` on a template tree the mark `TsExprSummary.mjs` reads - which is why the summary tests `=== true`.
- **A shorthand property (`{ client }`) is a read of the variable**, bound by `getShorthandAssignmentValueSymbol`: `getSymbolAtLocation` on its name answers with the property's own symbol.
- **A library name is a `target` wherever it is declared** (rows 75, `resolveName`): `import { cloneDeep } from 'lodash'` binds a METHOD of an interface (`MethodSignature`), a namespace's member or a module, not a top-level
  function - most of the bare names that had no `target` in a workspace using lodash. When EVERY declaration is in a declaration file under `node_modules` it is the target, in that file, as the link `_.cloneDeep` always was; an in-tree
  declaration that is not top-level stays unbound.
- **`local` is every `locals` row** (rows 75, `resolveLocal`): a loop variable (`for (const key of keys)`, `for..in`), a `catch (e)` binding, `let t;` and a name a destructuring binds (`for (const [k, v] of m)`, `const { a } = f()`) -
  the place of its declaration's row, with a value or without; the largest share of the reads with no binding was these. Nothing in rust reads `local` on a tree: the closure's `const` locals are found by (member, name).
- **A `target` is anchored at the declaration's NAME** (rows 75, `at`): `@Input()` above `label` made a read target the decorator's line, where no `members` row sits - and rust's line-keyed `row_of` found no row for it.
- **Not bound**: `undefined`, a type parameter, a name of a library no `node_modules` declaration file backs.

## Routes, NgRx entries and federation (rows 79)

In `TsDerive/TsRoutes.mjs`, `TsDerive/TsNgrx.mjs`: the route array a module reaches through `forRoot(appRoutes)` is the const's array (its signature leaves out `$fn_id`, which only the const's copy holds), walked once; `routes` carries `path_match`, `can_deactivate`, `can_match`, `title`. `immerOn` of `ngrx-immer` (by its declaration's module) is an `on`, its `writes` the draft paths `TsExpr/TsExprDescribe.mjs::draftWrites` finds; the handler is the LAST argument of either and every one before it an action. A `createEffect` call in a constructor is a `state_effects` row (`constructor@line:col`). A bare `import('remote/Module')` is a federated route when a host config's `remotes` lists the first segment (`TsFederation.mjs`).

## A call argument that names a variable (rows 79, #103)

`calls.args` keeps the EVALUATED value of every argument - never changed, every reader takes it as evaluated. `calls.arg_refs` (`TsExpr/TsReadRefs.mjs::resolveArg`, written by `TsDecls/TsBody.mjs::emitCall`, only when non-empty) is one `{position, name, $ref, file, line, col, kind}` per argument that is a bare name (through parentheses, `as`, `!`, `satisfies`) resolving through import aliases to a `const`/`let`/`var` or a top-level destructured binding, so a passed table joins the `consts` or `locals` row that declares it. A declaration file has none; a class, enum or function argument keeps its `$ref` in the value.

## A literal in an object literal names the member it fills (rows 81, #104)

`string_literals.target` / `number_literals.target` (`TsText.mjs::collectStrings`, `TsExpr/TsReadRefs.mjs::fieldTarget`) is `{name, file, line}` of the member the object's contextual type declares - one declaration place, else none - and `target_id` the row there (`TsRefs.mjs::rollupRefIds`, the place key a call's `target_id` uses). The text `target` of `compare`/`index` uses never meets it: a literal inside a property is never an operand. Parentheses, `as` and `satisfies` around the value are stepped through (rows 83). An inline `functions` row that is such a property's value - an arrow, a function expression, an object-literal method - carries the same `target` and `target_id` (rows 83, #105, `TsWalk.mjs::declareInline`); a `prototype-method` row's `target` stays what a call of it resolves to, with no id.
