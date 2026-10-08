# The TypeScript half's extractor - the row contract node writes

What runs where, partial runs and the skip key are [../../.claude/rules/typescript-half.md](../../.claude/rules/typescript-half.md);
what the rust closure derives from these rows is
[../../rust/fbtcore/src/rows/ts/CLAUDE.md](../../rust/fbtcore/src/rows/ts/CLAUDE.md). Consumer-facing columns are the
`map-sqlite` skill's `typescript.md` (`skills/map-sqlite/typescript.md`).

## What the half writes

`--map-sqlite` over an Angular workspace writes FRAMEWORK facts no compiler produces alone: bindings, template
nodes, render edges, input uses, routes and what each can reach - 53 tables with the four the closure derives,
plus `diagnostics` (what the extraction could not answer: a count with no names cannot be checked). Every row
carries `half = "typescript"`.

**BUMP `rows` IN THE SETUP HASH (`TsMap.mjs`) WITH EVERY CHANGE TO WHAT A ROW CARRIES** - and with every change to
a table rust DERIVES from these rows (`gate_values`, `key_reach`, the stylesheet tables): an unchanged tree skips node and
keeps its rows, so a deployed exe that adds a column reads a consumer's tree as unchanged for ever. The comment
above it lists what each number added; add a line.

**THE MAP STATES WHAT SQLITE CANNOT RECORD** - no foreign keys, no arrays, no note of which column reaches a
row's own file. The half derives it from the finished rows and publishes it in `_meta` (`spec:typescript`,
`rust/fbtcore/src/rows/tsapply.rs`): every id prefix, every join, every anchored table and which columns hold a
structure (`TsDerive/TsSpecJoins.mjs`, `TsDerive/TsSpecAnchors.mjs`). The anchor is PROVEN per field, never read
off the join graph: `renders` carries `from_class`, so a graph walk files a render edge under the component's
`.ts` when the row was read from its `.html`.

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
- **`import_names`** (`ib:`, `TsImports.mjs`) is one row per name an import binds, in source order: `local`,
  `imported` (`default`, `*`), `kind`, `type_only`, and where it is DECLARED - `declared`, `declared_file`,
  `declared_id` (a top-level class, function, interface, enum, type alias or const; none when two share the name),
  `declared_name` - plus `via`, every barrel on the way. All three barrel shapes are followed: `export * from`,
  `export { X } from`, and `import { X } ...; export { X }`. `exports.resolved`/`resolved_file` is the file a
  re-export's `from` resolves to. An importer DEPENDS on its declaring file and every barrel in `via`
  (`rust/fbtcore/src/rows/partial/deps.rs`), so a partial run re-reads it when either moves.

## Expression trees the gate readers need (`TsDecls/TsBody.mjs`)

`value` keeps only the last link of a call chain, so the rust readers need the TREE: `returns.expression`,
`locals.expression`, `assignments.expression`, `branches.condition_expr`, `switch_cases.discriminant_expr` and
`label_exprs`, always written, plus a `target` on a bare name that is a module-level const, function or enum.

- **An arrow-function PROPERTY returns like a METHOD** (divergence #8): `isMain = (id: P) => [P.A].some(...)`
  keeps its `members.value` (`{"$fn": ...}`), gains `members.params` and a `returns` row keyed on the member,
  `implicit = 1` for a concise body. A function-valued `const` has the same under its `functions` row.
- **A statement inside a conditional's arm carries `choices`** (divergence #11): `c ? this.t('a') : this.t('b')`
  and `c && this.add(...)` stamp each `calls`, `assignments`, `returns` and `locals` row in an arm with the
  condition's `expressions` id, `!`-prefixed on the false side; a condition in statement position gets its own
  `logic`/`ternary` `expressions` row.
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
  `$variable` is not), `live` (0 inside a `@mixin` and for a `%placeholder`).
- **`class_hides`** (`cssh:`): a `[class.x]` gate (`via = 'class'`) or a literal `[ngClass]` key (`via =
  'ngClass'`) joined to every live, on-element, hiding rule whose subject requires the class - `bare` (the whole
  selector is `.x`) and `scope`: `own` (the component's `styleUrls`), `other` (another component's: reaches only
  through `::ng-deep` or `ViewEncapsulation.None`), `shared` (no component names the sheet).

Not followed, so not claimed: `@extend`, a mixin's body at its `@include`, `@use`/`@import` between sheets, and a
component's inline `styles: [...]`.

## How it is checked

Black-box over the CLI: `tests/deep/TsRows.Tests.ps1`, `tests/deep/TsAngular/` (imports, styles, reads,
gates), `tests/deep/TsKeys/` and `tests/deep/TsPlain/`, helpers in `tests/deep/TsRows.Helpers.ps1`. A case that changes rows must fail with the extractor line
reverted. **A port that improves rows cannot be told from one that breaks them**: a row that changes what a
consumer reads is a numbered, signed-off divergence (the list is in the rust file above), never a quiet fix.
