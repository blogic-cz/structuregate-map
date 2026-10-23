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
* `calls.arg_refs` (rows 79, only when non-empty; `calls__arg_refs`) names the VARIABLE behind an argument, since `args` holds only its evaluated value: one `{position, name, $ref, file, line, col, kind}` per
  argument that is a bare name (through parentheses, `as`, `!`) of a `const`/`let`/`var` - local, module-level or imported, an import alias followed - or of a top-level destructured binding (`kind` `const`, `let`, `var`
  or `binding`). `position` is the index in `args` (a spread counts as one), `name` the name written, `$ref` the name declared, `file` the declaring file (`files.abs`), `line`/`col` the declaration's NAME. An enum, class or function argument
  has no item - its `$ref` is in `args`. `JOIN consts k ON k.file = <file id> AND k.line = <line>` for a module-level one, `locals` for a body's. A spread argument names nothing.
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

A name the template binds keeps its type (rows 73): a loop inside a loop over `X[][]` (any depth, `*ngFor` or `@for`) reads the inner variable as `X`. A loop over a framework pipe itself (rows 77, `*ngFor="let row of (grid$ | async)"`) takes the element of the list the observable EMITS, one level in: over `Observable<X[][]>` `row` is `X[]` and a loop over `row` takes `X`. A `#ref` on a component's element reads as that
component (`#c` then `c.prop` targets the member), and `#r="name"` as the directive - or component - whose `exportAs` lists `name`; a ref on a plain element or an `<ng-template>` has no row and its reads
have no `target` (the ref's own read never has one: it is no member).

Reads a template chain could not follow before (rows 80): a `#ref` on an element two components of one selector match is the one the host's SCOPE renders (the choice `renders` makes; an out-of-scope candidate only when nothing else matches); a loop over `x | keyvalue` (the framework's `KeyValuePipe`) over `Record<K, V>`, `Map<K, V>` or an index signature has a variable whose `value` is `V` and `key` is `K` (rows 86: also over an array - the element - a class or interface with no base - the union of its fields' types - and what `async` emits)
(those two hops carry no `target`: the declaration is the library's); `$event` in `(out)="f($event.row)"` is the first type argument of the bound component's or directive's output (`@Output() out = new EventEmitter<T>()`, `output<T>()`, bound by the element's generic instance; an output the class inherits from a base, the base's parameters bound by its `extends`, rows 85; the literal part of `T & { cell: X }` answers beside `T`, rows 85), a DOM event is not typed; `c ? [] : list` and `c ? a : b` over two lists of one element read as that list; `*ngIf="a && b() as v"` types `v` as `b()`'s value; and a chain past a UNION step whose arms all declare it as ONE type goes on (the step itself has no `target`, as it has several places).

What the directive on an element types (rows 74): the `let-x` / `*dir="let x"` variables of a template a STRUCTURAL directive in the tree receives are the properties of the context it declares - its
`ngTemplateContextGuard`'s predicate (`guard_ref`), else the `TemplateRef<Ctx>` it injects - `let-x="prop"` is `Ctx.prop`, a bare `let-x` (or `let x`) `$implicit`, `*dir="v as x"` the property `dir`; the directive's
type parameters are bound from the inputs on the element (`T` from an input typed `T`, `T[]` or `Box<T>`). A library directive (`ngFor`, `ngIf`) has no rows and keeps its own reading. A `#ref` to a GENERIC
component binds its parameters the same way (`<demo-table #t [rows]="items">` makes `t.selected` an `Item`), else by their `default`, else their `constraint`. A loop over an `as` alias of a framework
pipe (`*ngIf="items$ | async as list"` then `*ngFor="let i of list"`) takes the element of the list the observable emits. The parts of an intersection or union read with the bindings of the declaration they were reached in.
A directive taking its template through `inject(TemplateRef)` in a property initializer is structural as one with the constructor parameter is (rows 77): the `di` row of an `inject(X)` carries `type_ref`, the declaration its type names - the field's annotation, else the cast around the call (rows 78), else the call's own type. Where the `TemplateRef` declares no context (`inject(TemplateRef)`, `TemplateRef<any>`, `<unknown>`) the context is the object the directive's CODE hands the view (rows 75): the `di` row of that `TemplateRef` has
`context_ref` (`di__context_ref`, a `type_ref` node), the type of the second argument of `createEmbeddedView(this.tpl, { rule: this.rule })` - an object literal or a local typed by a literal is a shape ref - when every such call
of the class hands one type. A directive that declares no context takes its BASE class's, the base's parameters bound by the `extends` arguments.

A CODE expression's tree (`returns`/`locals`/`assignments`/`calls` through their `expressions` row, `branches.condition_expr`, `switch_cases`) is read the same way: EVERY link of a property chain carries its own `target`
(`{name, file, line}`, the declaration that name lands on, rows 73), not only the outermost - `this.holder.Inner.Id` targets `holder`, `Inner` and `Id` each, `SafeRead` links too. A link whose
type the checker cannot follow (an `any`) has none.

A bare NAME in a code tree is bound by what it is (rows 74), one of three - `target`, `local` or `param`, never two - and a name none of them applies to (`undefined`, a type parameter)
has none. A `target` is a top-level `const`, function, enum or CLASS (`JsonSerializer.parse(x)`, `new Foo()`, `Foo.Member`, an import alias followed); a class target also has `kind: 'class'` and its line is the
class NAME's - the `classes` row's own `line`, never the first decorator's. EVERY `target` of a read, a link of a chain included, is anchored at the declaration's NAME (rows 75): a decorated member
(`@Input()` above `label`) targets the line of its `members` row, so `JOIN members m ON m.file = <target file> AND m.line = <target line>` finds it. A name a top-level DESTRUCTURING `const` binds (`export const { DemoActions } = makeSlice()`, nested too; rows 76) has a `target` at the bound name's line, and `consts` holds one row per bound name there (`destructured`, no `value`). A bare name a LIBRARY declares as anything but a top-level
declaration - `import { cloneDeep } from 'lodash'` is the method of the interface the package types its export with, a namespace's member a module - has a `target` in the library's file under `node_modules` (rows 75),
as the link `_.cloneDeep` always had. A `local` is `{file, line, col}` of a `locals` row - a `const`/`let` with a value or without, a loop variable, a `catch` binding, and a name a destructuring binds (the place of its pattern, rows 75). A `param` is `{file, line, col}` of the PARAMETER'S NAME - of the binding element's
name when it is destructured (`({ a, b: c }) =>`, `([x]) =>`) - for every function-like with a body: a declaration, method, accessor, constructor, arrow, function expression and a nested function. Join it to
the parameter by place: each item of `members__params` and `functions__params` carries `line` and `col` of its name (of its pattern when destructured), and a destructured one `bindings`, one
`{name, line, col}` per name it binds (nested patterns included; rows 77: also `default`, the evaluated `= 1`, `type`, the checker's text for the name, and `index`, its position in an ARRAY pattern with holes counted - the same three on a `locals` row's `bindings` and on the per-name `consts` rows of a destructuring `const`; rows 78: a `locals` row's `bindings` is likewise one item per BOUND NAME at any depth, with `property` the path to it (`Input.Params.Tariff`) and `line`/`col`, never one item for an outer element; rows 80: an element at the TOP of an array pattern has `property` null and its `index`, as an array adds nothing to the path) - `JOIN` the item (or binding) whose `line`/`col` equal the read's `param` over the same file, through the owner row (`members.file` or
`functions.file`; `param.file` is the absolute path, `files.abs`). A shorthand property (`{ client }`) is a read of the variable it names and is bound as that name is (its property key is not a link).
An ALIAS OF AN INTERSECTION among the parts of a written intersection is one named part (rows 74): `Item & ViewDto` with `type ViewDto = Dto & { .. }` has `type_refs` `Item`, `ViewDto` - the checker
flattens it - and the alias row has `type_ref`/`type_refs` of its own parts (`type_aliases__type_ref`, `type_aliases__type_refs`), which a template read walks in its place.

**What the derived tables claim (second audit, rows 79).** `translations.referenced` is 1 when a key EQUALS a literal somewhere (a string, a template string, a static attribute's text, a route key, an `i18n_index` key) - an exact-string join - so 0 does NOT mean unused: a dynamic key, a dictionary prefix and a template literal's head are not marked (use `key_reach.n_refs`). `io.bound_count` counts the bindings that fill an input (`input_usage`) or name an output (an `(event)` binding; no `input_usage` row), 0 when none does. `input_usage` skips a directive no scope of the template reaches (`renders.scope = 'out_of_scope'`, set aside in `renders_out_of_scope`) even where no other candidate is in scope. `gate_index.component_count` does not count an orphan template's null component. `regexes` has a `kind = 'call'`, `api = 'RegExp'` row for `new RegExp(/x/g, 'i')` as well as the literal's own row; its `flags` are the argument's, else the literal's. `i18n_refs` has one ref per element for `[{ Name: 'a' }, { Name: 'b' }].map((t) => svc.get(t.Name))`. `class_hides` (`via = 'ngClass'`) names every class an array item, a string or a ternary arm spells. `style_rules.important` is 1 for a custom property marked `!important`. `routes.can_deactivate_ids`, `can_match_ids`, `can_load_ids` and `guards_ids` (canActivateChild) name each guard's `classes` row as `can_activate_ids` does. `duplicate_types` lists a TypeScript class, interface or alias (`kinds` `class`, `interface`, `alias`) declared in more than one file; a class EXPRESSION (`static Inner = class {..}`, a `classes` row with `expression = 1`) is no declaration and is skipped (rows 80). A `diagnostics` row `unresolved_import` has `case_insensitive_match = 1` when a specifier names a file only when case is ignored - a relative one, or (rows 80) a `paths` alias the compiler resolves through a case-insensitive host: the import resolves on Windows and macOS and breaks on Linux. `calls.target_id` is the row AT the declaration the checker named (rows 80, 81), found by its place - the call's `target` carries `line` - and never by its name: a function, a method, a class, a local (`locals`), a module `const`, a member of an interface or type literal (`type_members`), a member of an OBJECT LITERAL (`shape_members`); an overloaded method or function is its implementation. A bare `getX(..)` over a local `const getX` is that local, not a same-named method. A property that only names a value (`{ getX }`, `getX: getX`, `{ y: getX }`) is itself the declaration, so its `shape_members` row is `target_id` and `target.follow` is where the value is declared (rows 81; rows 80 followed it to the function by name). `calls.callee_id` is the callable that RUNS: the `functions`, `members` or `classes` row (`target_id` itself when it is one; through `follow` for such a property; for a local or const holding a function, the function its value names, rows 84; none for a method of an object literal). A function declared in a body (`function inner() {..}`) is a `functions` row named by its declaration, so a bare `inner(..)` binds it (rows 84) - join `callee_id` to reach the callee's `params` and `returns`.

**A literal in an object literal names the member it fills (rows 81, #104).** `string_literals` and `number_literals` rows whose value is a property of an object literal (`{ Field: 'alpha' }`) carry `target` `{name, file, line}` - the member the checker's CONTEXTUAL type declares (an element of a typed array, a typed variable, member, parameter or return, a call argument), anchored at the member's NAME line - and `target_id`, the `type_members`, `members` or `shape_members` row there (`WHERE string_literals.target_id = ?` finds every value a member is given). Where a literal has no `target`, `target` is the empty text of the other `use`s. An object with no contextual type, a property the type does not declare and a union whose arms declare it in SEVERAL places have none; a library's member keeps its `node_modules` file and has no `target_id` (`target_id`, here and on a call, is only ever the declaration's OWN row - a `type_members` row a computed type or alias made in another file for a member declared elsewhere, `via = 'computed'`, is not it). Parentheses, `as` and `satisfies` around the literal are stepped through (rows 83). An object in an un-annotated `const` (`const list = [{ .. }]; return list;` in a method typed `Rule[]`, or a const handed to a typed parameter) takes the type the const takes at its uses (rows 87, #106); an object pushed into an un-annotated array (`out.push(rule)`, `out.push({ .. })`) takes what that array takes later (rows 88); a const whose uses type it only by another literal's inferred type names none. A FUNCTION there - an arrow or function expression that is the property's value, or a method written in the object (`{ Check(v) { .. } }`) - is an inline `functions` row with the same `target` and `target_id` (rows 83, #105), so what it returns joins the member: `returns r JOIN functions fn ON fn.id = r.member WHERE fn.target_id = ?`. The Angular half only (the plain half has no checker).
