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
* `type_refs` exists only when a union or intersection names SEVERAL declarations (then `type_ref` is null); it never repeats `type_ref`.
* A method row also has `return_ref` (the same shape, its return type; `return_refs` when it returns several, as `type_refs`). `type_ref` follows the TYPE the checker reaches, not the alias
  written: `type Desc = Readonly<{..}>` gives `Readonly` - join `type_aliases` by the `type` text for the alias.

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
* A conditional in CODE (`?:`, `&&`, `||`, `??`) gets an `expressions` row (`ternary`/`logic`) only when a row sits under one of its arms
  (`choices` names it). `expressions` does NOT count the conditionals of a body, and no table lists every one.
* A template write `(event)="a = b"` is a `Write`/`KeyedWrite` node in `expressions__ast` with its `target` (`name`, `file`, `line`, `row`).
  `$writes` of a value cell is a CODE field; it is absent for template expressions.
* An inline template's `bindings.line` is the `.ts` line; `col` is relative to the template text on its first line.
* A constructor parameter property (`constructor(private dep: Dep)`) is a `members` row with `parameter = 1` as well as its `di` row.

## `structuregate.ts.json` option `i18nCarriers`

A list of the pipes or element selectors that carry a translation key in a template (`"translate"`, or `"translate:text"` with the
input that holds the key). Which one carries a key is application knowledge, so it is an input: WITHOUT it a template's `translate` pipe
or `<translate>` element produces no `i18n_refs` row with `source = 'html'`. A carrier no pipe or selector declares is a diagnostics note
`i18n_carrier_not_declared`, never silently inert. The other options (`i18nCalls`, `i18nDictionaryCalls`) are in [typescript.md](typescript.md).
