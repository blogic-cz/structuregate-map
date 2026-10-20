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
  `raises`, `handlers`, `loops`). A function or class expression inside the body starts outside any loop - it is its own member, and a `loop` naming
  another member's loop would join nothing. (A `branch` is still inherited by such a function; that is older behaviour, unchanged.)
- **A LOOP IS NEVER A `branches` ROW HERE.** The plain half and C# record one with a `kind`; this half's `branches` rows are if-arms (`sense`) that the
  rust closure reads as CONDITIONS (`rust/fbtcore/src/rows/ts/key/key_branches.rs`, the gate readers). A key assigned inside a loop written as a
  branch would turn conditional and the release compare would report a lost restriction. Nothing in rust reads `loop` yet; `deriveJoins` declares it
  a join to `loops` as it does `branch`, so a partial run's `dangling` check covers it.
- **A bare expression statement whose walk wrote no row is an `expressions` row** of role `statement` (`this.a.b;`, `x;`, `a && b;`, `delete o.k;`):
  decided by what the walk wrote (a call, a write, a local, a function), never by the expression's kind.
- **`x++`, `--this.n` are `assignments` rows**: `operator` `++`/`--`, `value` and `expression` null, `source` the whole update, `target`/`scope`
  as for `=`. A counter written only by an update had no writer in the map.
