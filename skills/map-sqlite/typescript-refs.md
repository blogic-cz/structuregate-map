# TypeScript rows: what a reference cell and a few other cells mean

Detail for [typescript.md](typescript.md) (the Angular half unless said).

## `type_ref`: the declaration a type names

On `members`, `type_members`, params (`members__params`, `functions__params`), `locals` and `di`. A node tree in `<table>__type_ref`
(root `n = 0`, an OBJECT node whose members are child rows with `parent` = the root's `n` and a `role` each; an object member such as
`element` is itself a node with `name`/`file`/`line` children).

* The root is the declaration the CHECKER names: `name`, `file`, `line`, `also` (the other declarations of a merged one); `param: true` is a
  type parameter (`T`).
* `args`: one node per type argument, in order - a `type_ref` or null. `element`: an array's or one-argument generic's declared element
  (`Item[]`, `Array<Item>`, `readonly Item[]` -> `Item`). `unwrapped`: the innermost declared type through one-argument wrappers
  (`Observable<Item[]>` -> `Item`). A primitive has none of them (`number[]` has no `element`): read the `type` text.
* `index` (rows 71): what a string (else number) INDEX SIGNATURE of the type holds, a ref as `element` is - `Bag` with `[k: string]: Item`, `Record<string, Item>`,
  `{ [k: string]: Item }`; an array's number index is its `element`, so a type with one has no `index`.
* An ANONYMOUS object type - a type literal or an object literal, whose declaration has no name - is `{shape: true, file, line, col}` and no `name`
  (rows 71): the place of its `{`, and `element`, `unwrapped`, `type_refs` and `args` hold such a node too. It names a `shapes` row (below). In a
  union or intersection it is dropped when another part names a declaration (`X & { .. }` is `X`) and listed in `type_refs` when no part does.
* `type_refs` exists only when a union or intersection names SEVERAL declarations (then `type_ref` is null); it never repeats `type_ref`.
* A method row also has `return_ref` (the same shape, its return type; `return_refs` when it returns several, as `type_refs`). `type_ref` follows the TYPE the checker reaches, not the alias
  written: `type Desc = Readonly<{..}>` gives `Readonly`. `alias` (rows 71; one node `{name, file, line}`, as `type_ref`'s own) is the type alias WRITTEN, an import followed:
  `Desc` here. Every `file` inside a `type_ref` is the ABSOLUTE path the compiler read, never a `files` id: join `type_aliases a`
  `JOIN files f ON f.id = a.file` on `a.name = alias.name AND a.line = alias.line`, and match the path by its suffix `f.path`. Absent when the written name is a class, interface or enum, and for a union's `type_refs`.

```sql
-- members whose type is an array of a declared type, and that element's name
SELECT m.name, n.value AS element FROM members m
JOIN members__type_ref r ON r.owner = m.id AND r.n = 0
JOIN members__type_ref e ON e.owner = r.owner AND e.parent = r.n AND e.role = 'element'
JOIN members__type_ref n ON n.owner = e.owner AND n.parent = e.n AND n.role = 'name';
```

## Cells that are not the source text

* `classes.extends.text` and each `implements[].text` are the name WITHOUT type arguments; the arguments are `type_args`.
* `calls.args` `$expr` is a NORMALISED print (casts and parentheses stripped, strings re-quoted), not source text. The `calls` row keeps
  no raw text of its call: read the file at `line`/`col`.
* A conditional in CODE (`?:`, `&&`, `||`, `??`) has an `expressions` row (`ternary`: its condition, `logic`: the left operand of `&&`/`||`/`??`), whether or not a row sits
  under one of its arms (rows 71); `choices` on an arm's row names it.
* A template write `(event)="a = b"` is a `Write`/`KeyedWrite` node in `expressions__ast` with its `target` (`name`, `file`, `line`, `row`).
  `$writes` of a value cell is a CODE field; it is absent for template expressions.
* An inline template's `line` and `col` (`template_nodes`, `bindings`, `attributes`) are the `.ts` file's: on the template's first line `col` includes
  the column the text starts at (after the opening quote; rows 71, `components.inline_template_col`). A template held in a non-literal has no such column, so `col` stays relative on its first line.
* A constructor parameter property (`constructor(private dep: Dep)`) is a `members` row with `parameter = 1` as well as its `di` row.

## `structuregate.ts.json` option `i18nCarriers`

A list of the pipes or element selectors that carry a translation key in a template (`"translate"`, or `"translate:text"` with the
input that holds the key). Which one carries a key is application knowledge, so it is an input: WITHOUT it a template's `translate` pipe
or `<translate>` element produces no `i18n_refs` row with `source = 'html'`. A carrier no pipe or selector declares is a diagnostics note
`i18n_carrier_not_declared`, never silently inert. The other options (`i18nCalls`, `i18nDictionaryCalls`) are in [typescript.md](typescript.md).

## `shapes` and `shape_members`: every type literal and object literal (rows 71)

`shapes` is one row per `{ .. }` in a type (`kind = 'type'`: `result: { error?: { message: string } }`) and in code (`kind = 'object'`: `labels = { name: 'x' }`,
an argument, a return, a decorator's object, a nested one), in a `.ts` file only - a `.json` source and a declaration file have none: `file`, `line`, `col`
(the `{`), `end_line`, and the walk's context where it stood in a body - `member`, `class`, `branch`, `case`, `loop`, `choices` as a `calls` row has them.
`shape_members` is one row per member, `shape` the owner: `name` (as `members.name`: a computed key is its value), `kind` (`property`, `shorthand`, `method`,
`getter`, `setter`, `spread`, `signature` for a type literal's property, `index`, `call`, `construct`), `optional`, `type` (the annotation for a signature, else what the
checker infers, as `members.type`), `type_ref` (a nested literal is a shape ref), `return_ref` of a method or function-valued property, `key_type` of an `index`, `line`, `col`.

```sql
-- the members of the literal a shape ref names (:file, :line and :col are the ref's own nodes; `files.abs` is the absolute path)
SELECT sm.name, sm.kind, sm.type FROM shapes s JOIN files f ON f.id = s.file
JOIN shape_members sm ON sm.shape = s.id WHERE f.abs = :file AND s.line = :line AND s.col = :col;
```

A template read chained through one resolves each hop's `target` to its `shape_members` row (`row` is its `sm:` id, `line` its line):
`{{ labels.nested.deep }}`, `{{ result?.error?.message }}`, and a keyed read `{{ items[key].label }}` through an index.
