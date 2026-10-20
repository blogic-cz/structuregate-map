# Anonymous object types are rows (rows 71)

The gap: a `type_ref` to a type literal (`result: { error?: { message: string } }`) or an inferred object literal (`labels = { name: 'x' }`)
named nothing - the compiler calls the symbol `__type` / `__object` - so a template read ended at the first hop, and no row held an object
literal's properties. The code is `TsDecls/TsExpr/TsShapes.mjs`, `TsDecls/TsTypeRef.mjs::siteOf` and `TsTpl/TsTplReads.mjs`; the case is
`tests/deep/TsAngular/TsValues/TsRowsShapeRows.Tests.ps1`; what a consumer reads is `skills/map-sqlite/typescript-refs.md`.

- **`shapes` (`shp:`) and `shape_members` (`sm:`)** are written by ONE SCAN PER `.ts` FILE over its whole tree (`TsShapes.mjs::scan`, called from
  `TsWalk.mjs` beside the dynamic-import scan), so a literal in a type, a parameter, a top-level const or a decorator has a row as one in a body
  does. A `.json` source and a declaration file are skipped. The body walk only `remember`s the context of each object literal it passes
  (`member`, `class`, `branch`, `case`, `loop`, `choices`, as a `calls` row takes them); a type literal, and an object literal the body walk never
  reaches, has none. Both tables are in `TsStore.mjs::SOURCE_FILE_TABLES`; the unique key is whichever candidate holds (`line` + `col` always does),
  and the id prefixes are in the published `id_scheme` as every other. `sh` is `state_handlers`'s, hence `shp`.
- **A shape ref is `{shape: true, file, line, col}`** (`siteOf`): `col` is the `{`'s, the key a `shapes` row has, `file` the absolute canonical path as every
  ref's. Only a symbol whose FIRST declaration is a `TypeLiteral` or an `ObjectLiteralExpression` is one - any other `__` name (a function type, a
  mapped type, a class expression) still names nothing. `element`, `unwrapped` and `args` use the same head. A union or an intersection DROPS its
  shape parts when another part names a declaration (`refsOfParts`): `X & { .. }` stays `X`, which the intersection case pins.
- **`index`** (`TsTypeRef.mjs::indexOf`) is the ref of the type's string, else number, index signature - one level, no `index` inside it - and only
  when the type has no `element` (an array's number index IS its element). A keyed template read takes `element`, else `index`, else the `index`
  member of the shape the receiver reached (`TsTplReads.mjs`, `indexByShape`) - the last is what a loop variable over `{ [k: string]: Item }[]` has,
  since a variable has an owner and no ref.
- **A template read through a shape** finds its owner by (file, line, col) (`shapeByPlace`); `propByOwner` holds `shape:<id>#<name>` for every
  member that has a name (a spread, call, construct and index signature have none a read can spell); a getter wins over its setter, a write takes the
  setter, as a class's members do. The hop's `target` is `{name, file, line, row}` with `row` the `sm:` id.
- **Readers of a ref that assumed a `name`** (audited, none changed): `TsRegistry.mjs` and `TsClassRows.mjs` test `ref.name === 'TemplateRef'`;
  `rollupRefIds` reads `extends` only; the rust gate readers (`rows/ts/gate/**`) take `name` as an optional string and skip a ref without one.
