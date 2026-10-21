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

- **`target`** for a top-level `const`, function, enum or CLASS (an import alias followed). A class target has `kind: 'class'` and the class NAME's line (its `classes` row's `line`,
  never the decorator's). `rust/fbtcore/src/rows/ts/gate/ts/gate_tsrows.rs::names_class` makes every closure reader take it as the bare name it was: no row, never a constant or a member.
- **`local`** for a `const`/`let` with an initializer; **`param`** for a parameter of a function-like with a body, `{file, line, col}` of the parameter's name (a destructured
  parameter: the binding element's name). `members__params`/`functions__params` items carry `line`/`col`, and `bindings` when destructured, so the join is exact. `param` on a CODE tree is that place,
  `param: true` on a template tree the mark `TsExprSummary.mjs` reads - which is why the summary tests `=== true`.
- **A shorthand property (`{ client }`) is a read of the variable**, bound by `getShorthandAssignmentValueSymbol`: `getSymbolAtLocation` on its name answers with the property's own symbol.
- **Not bound**: a loop variable and a `catch` binding (a `locals` row without an initializer, `resolveLocal`), a global, a type parameter.
