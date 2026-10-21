# TypeScript rows: what a reference cell and a few other cells mean

Detail for [typescript.md](typescript.md) (the Angular half unless said).

## `type_ref`: the declaration a type names

On `members`, `type_members`, params (`members__params`, `functions__params`), `locals` and `di`. A node tree in `<table>__type_ref`
(root `n = 0`, an OBJECT node whose members are child rows with `parent` = the root's `n` and a `role` each; an object member such as
`element` is itself a node with `name`/`file`/`line` children).

* The root is the declaration the CHECKER names: `name`, `file`, `line`, `also` (the other declarations of a merged one); `param: true` is a
  type parameter (`T`) and carries `constraint` (`T extends Row`) and `default` (`T = Dto`), each a `type_ref` node when it names a declaration (rows 72).
  An `element` or `unwrapped` that is a type parameter (`T[]`) is that whole `param` node; one that is an unnamed intersection or union (`(Row & { .. })[]`,
  `(Row | null)[]`) is the ONE declaration its parts name. `args` of a generic ALIAS are its `aliasTypeArguments` (`Frame<{ .. }>`; rows 72).
* `args`: one node per type argument, in order - a `type_ref` or null. `element`: an array's or one-argument generic's declared element
  (`Item[]`, `Array<Item>`, `readonly Item[]` -> `Item`). `unwrapped`: the innermost declared type through one-argument wrappers
  (`Observable<Item[]>` -> `Item`). A primitive has none of them (`number[]` has no `element`): read the `type` text.
* `index` (rows 71): what a string (else number) INDEX SIGNATURE of the type holds, a ref as `element` is - `Bag` with `[k: string]: Item`, `Record<string, Item>`,
  `{ [k: string]: Item }`; an array's number index is its `element`, so a type with one has no `index`.
* An ANONYMOUS object type - a type literal or an object literal, whose declaration has no name - is `{shape: true, file, line, col}` and no `name`
  (rows 71): the place of its `{`, and `element`, `unwrapped`, `type_refs` and `args` hold such a node too. It names a `shapes` row (below). In a
  union or intersection it is dropped when another part names a declaration (`X & { .. }` is `X`; `A & B & { .. }` lists it) and listed in `type_refs` when no part does. An
  INTERSECTION's dropped literals are kept as `shapes` on the `X` node (rows 72): shape refs, the members of `X & { Extra?: E }` the literal alone declares.
* `type_refs` exists only when a union or intersection names SEVERAL declarations (then `type_ref` is null); it never repeats `type_ref`. `type_refs_of` (rows 72) is
  `and` for an intersection (listed in written order, its literals among them) and `or` for a union (sorted by place); `return_refs_of` beside `return_refs`. A property
  both parts of an intersection declare is the FIRST part's. A mapped type (`Pick<Dto, 'a'>`) has `props`, `{name, file, line}` of the declaration of each member it holds. An
  `element`, `unwrapped` or type parameter `constraint` naming several is `{type_refs, type_refs_of}`.
* A method row also has `return_ref` (the same shape, its return type; `return_refs` when it returns several, as `type_refs`); a static `ngTemplateContextGuard` has `guard_ref` (rows 74), the `type_ref` of its predicate's type (`ctx is Ctx<T>`). `type_ref` follows the TYPE the checker reaches, not the alias
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

## What a template read follows (rows 72)

A template read's `target` (`expressions__ast`, a `Read` node) is resolved from the declarations above; a hop whose type names nothing ends the chain. It
follows: a read chain; a CALL (`svc.list().x`, the method's `return_ref`, inferred when un-annotated); a keyed read (`element`, else `index`); a pipe (an
in-tree `transform`'s return, else the piped value's `unwrapped`); `a ?? b`, `a || b` and `c ? a : b` when both name one declaration. A loop variable
(`*ngFor`, `@for`) is the ELEMENT of that value and an `as` alias is the value itself - not the type of the first name met in the expression, so an
argument of a call never types the loop. A member typed by a type parameter reads as what the instantiation passed (`Frame<{ .. }>`, `Base<Payload>`),
else the parameter's `default` where nothing was passed (`Slot` is `Slot<Dto>`), else its `constraint`; an argument that names nothing stays nothing.
A literal answers the names of what it spreads (`shape_members` kind `spread`, last first) and a `type_ref` whose declaration is a library type reads through its `alias`. A class reads the members of its bases, an INTERFACE too (`interfaces.extends_refs`, one `type_ref` per base, null for a library type). Several declarations
at once (`A | B`, `A & B`, `X` with `shapes`) answer a name only when ONE of them declares it.

A name the template binds keeps its type (rows 73): a loop inside a loop over `X[][]` (any depth, `*ngFor` or `@for`) reads the inner variable as `X`. A `#ref` on a component's element reads as that
component (`#c` then `c.prop` targets the member), and `#r="name"` as the directive - or component - whose `exportAs` lists `name`; a ref on a plain element or an `<ng-template>` has no row and its reads
have no `target` (the ref's own read never has one: it is no member).

What the directive on an element types (rows 74): the `let-x` / `*dir="let x"` variables of a template a STRUCTURAL directive in the tree receives are the properties of the context it declares - its
`ngTemplateContextGuard`'s predicate (`guard_ref`), else the `TemplateRef<Ctx>` it injects - `let-x="prop"` is `Ctx.prop`, a bare `let-x` (or `let x`) `$implicit`, `*dir="v as x"` the property `dir`; the directive's
type parameters are bound from the inputs on the element (`T` from an input typed `T`, `T[]` or `Box<T>`). A library directive (`ngFor`, `ngIf`) has no rows and keeps its own reading. A `#ref` to a GENERIC
component binds its parameters the same way (`<demo-table #t [rows]="items">` makes `t.selected` an `Item`), else by their `default`, else their `constraint`. A loop over an `as` alias of a framework
pipe (`*ngIf="items$ | async as list"` then `*ngFor="let i of list"`) takes the element of the list the observable emits. The parts of an intersection or union read with the bindings of the declaration they were reached in.

A CODE expression's tree (`returns`/`locals`/`assignments`/`calls` through their `expressions` row, `branches.condition_expr`, `switch_cases`) is read the same way: EVERY link of a property chain carries its own `target`
(`{name, file, line}`, the declaration that name lands on, rows 73), not only the outermost - `this.holder.Inner.Id` targets `holder`, `Inner` and `Id` each, `SafeRead` links too. A link whose
type the checker cannot follow (an `any`) has none.

A bare NAME in a code tree is bound by what it is (rows 74), one of three - `target`, `local` or `param`, never two - and a name none of them applies to (a loop variable, a `catch` binding, a global such as `Math`)
has none. A `target` is a top-level `const`, function, enum or CLASS (`JsonSerializer.parse(x)`, `new Foo()`, `Foo.Member`, an import alias followed); a class target also has `kind: 'class'` and its line is the
class NAME's - the `classes` row's own `line`, never the first decorator's. A `local` is `{file, line, col}` of a `locals` row. A `param` is `{file, line, col}` of the PARAMETER'S NAME - of the binding element's
name when it is destructured (`({ a, b: c }) =>`, `([x]) =>`) - for every function-like with a body: a declaration, method, accessor, constructor, arrow, function expression and a nested function. Join it to
the parameter by place: each item of `members__params` and `functions__params` carries `line` and `col` of its name (of its pattern when destructured), and a destructured one `bindings`, one
`{name, line, col}` per name it binds, nested patterns included - `JOIN` the item (or binding) whose `line`/`col` equal the read's `param` over the same file, through the owner row (`members.file` or
`functions.file`; `param.file` is the absolute path, `files.abs`). A shorthand property (`{ client }`) is a read of the variable it names and is bound as that name is (its property key is not a link).
An ALIAS OF AN INTERSECTION among the parts of a written intersection is one named part (rows 74): `Item & ViewDto` with `type ViewDto = Dto & { .. }` has `type_refs` `Item`, `ViewDto` - the checker
flattens it - and the alias row has `type_ref`/`type_refs` of its own parts (`type_aliases__type_ref`, `type_aliases__type_refs`), which a template read walks in its place.
