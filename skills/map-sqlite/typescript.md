# The two TypeScript halves

Everything in [SKILL.md](SKILL.md) holds. The Angular half maps the workspace where
`angular.json`/`nx.json` and `@angular/compiler` are found; the plain half maps every script file OUTSIDE it
(the whole tree when there is no workspace), its paths root-relative where the Angular half's are workspace-relative.
Their `files.lang` differ (`typescript` against `ts`) and so does the shape of their rows.

## Plain TypeScript / JavaScript (`files.lang = 'ts'`)

The tree's own `typescript` 5 parser, no program, no type checker, so the rows are SYNTACTIC and per file
like python's, in the same tables where the meaning is the same (`functions`, `calls`, `arguments`,
`imports`, `branches`, `assignments`, `consts`, `exports`, `returns`, `raises`, `string_literals`,
`expressions`, `parameters`, `classes`, `handlers`, `comments`, `regexes`) plus **`types`** (interfaces and
type aliases) and **`jsx`** (one row per JSX element).

* (rows 80) A class EXPRESSION (`static Inner = class { .. }`) is a `classes` row with `expression` 1, as the Angular half writes it, and `duplicate_types` skips it; `new RegExp(/x/g)` is a `regexes` call row whose pattern is the literal's and whose flags are
  the second argument's, else the literal's own - beside the literal's own row.
* A call binds (`target_path`, `target_name`) through the file's own imports and top-level declarations,
  and never when an enclosing function declares the same name.
* **`functions.body_shape`** and **`expressions.shape`** are the digests the JSON map's
  `duplicate_bodies`/`duplicate_expressions` group by, computed by the same function over the same nodes;
  group by the site set in SQL and the two agree. The `dry-guard` agent is the reader built on them.
* A file whose bytes did not move is not read again; an importer is read again when the file it imports
  appears, moves or goes. A tree with no `typescript` 5 to borrow is a NOTE, not an error.

## Angular (`files.lang = 'typescript'`, every row `half = 'typescript'`)

The workspace is parsed with ITS OWN `typescript` and `@angular/compiler` (borrowed, never pinned: a pinned
compiler error-recovers a newer template grammar into a wrong map), so the rows are SEMANTIC and
**replaced whole each run** - a rename changes rows of files that did not change. An unchanged workspace
does not start node at all.

Beside the shared tables it adds the FRAMEWORK tables:

| area | tables |
|---|---|
| declarations | `components` `directives` `pipes` `injectables` `ng_modules` `class_decorators` `members` `locals` `enums` `interfaces` `type_aliases` `type_members` `switch_cases` `template_literals` `template_strings` `projects` |
| injection and I/O | `di` `io` `input_usage` |
| templates | `templates` `template_nodes` `bindings` `renders` `renders_out_of_scope` `render_graph` `component_reach` `selector_index` |
| routing and gates | `routes` `route_keys` `gates` `gate_values` `gate_index` |
| NgRx | `state_actions` `state_handlers` `state_selectors` `state_effects` |
| the closure, derived LAST from every other table | `render_path` `key_reach` `gate_features` (+ `gate_values`): where a translation key can be travelled to from a routed root, what gates every way in, which capability each gate requires |

* **`components.class`** is the class row (`c:...`) that `routes.component_id`, `render_graph.from_class`
  and `renders` point at. `renders` holds the edges Angular instantiates, each with its `scope` (`declared`,
  `imported`, `unknown`, `direct_reference`) - the same edges `render_graph`, `component_reach` and `render_path`
  walk. A selector candidate the host's NgModule or standalone `imports` cannot see is a `renders_out_of_scope`
  row instead (same columns, `scope = 'out_of_scope'`), so every candidate a tag matched is `renders UNION ALL
  renders_out_of_scope` (a table no row was written to does not exist). A standalone component's own selector in its
  own template is `declared` (rows 82): Angular puts the component in its own scope. A host no module declares (`unknown`) keeps an
  edge to every candidate. `components.unused = 1` when no `renders` edge, route or `bootstrap` reaches the
  class - a `renders_out_of_scope` candidate is no render. A render created in code names the method that returned the class
  (`returned_by`) and each way it was returned (`return_ways`, `returns` ids); a `case` or `if` every way sits
  in lands in `render_path.branches`. `templates`/`template_nodes`/`bindings` are the template side; `gates`/
  `gate_values` what hides a node (`gates.gate_kind = 'hidden'` is `[hidden]`/`[attr.hidden]`, whose `expression` is a row holding the NEGATED condition; a `style` gate carries `unit`, `px` of `[style.height.px]` whose `name` is `height`) (`op` in/not_in/unknown; `listed_json` (child table `gate_values__listed_json`) what a config object lists, even an
  exclusion the map cannot polarise); `state_*` NgRx; `di`/`injectables` injection.
* **`route_keys`** (`rk:`) is one row per `{{key}}` in a route's own `path` (`'{{routes.order.detail}}/:id'`, as a
  translating router resolves it): `route` (`routes.id`), `file` (the route's), `ord` (the `/`-separated segment the
  key sits in, from 0), `key`, and `translation` - a `translations.id` defining the key, NULL when no locale does. Join
  a route to the code testing the same key by `key` (`i18n_refs.key`), never by parsing the path. A key a route path
  names counts as `translations.referenced`.
* **A Module Federation route** (`loadChildren: () => loadRemoteModule('demo-orders', './Module').then((m) => m.X)`, or
  `loadRemoteModule({ remoteName, exposedModule })`) carries `routes.load_children_remote_name` (`demo-orders/Module`, the
  request webpack resolves) and `load_children_remote`, the class `X` declared in the file the remote's own
  `module-federation.config.{js,ts}` (`name` = the remote) lists under `exposes['./Module']` - read syntactically, never
  run; NULL when no single config, exposed file or class proves it, or for a remote named only by its `remoteEntry` URL.
  A `.catch(() => import(..))` fallback stays `load_children_module` with `load_children_recovery = 1`.
  A plain `loadChildren: () => import('demo-orders/Module').then((m) => m.X)` is the same route when a host's `module-federation.config`
  lists `demo-orders` in `remotes` (an array of names, of `[name, url]` pairs, or an object keyed by name); a specifier no host lists is a package path.
* **A route is one row** however many `RouterModule.forRoot`/`forChild`/`provideRouter` calls reach its array: it sits in the file that declares the array (a module
  that only hands `appRoutes` to `forRoot` has none), so a lazy root has one loader and its `parent_route` and `absolute_path` are set. A root SEVERAL routes load (rows 80: the module an in-app `import()` loads is the one a federated remote's `loadRemoteModule` loads too) keeps the in-app loader as `parent_route` - the standalone path -
  and lists every OTHER loader's route id in `routes.also_loaded_by` (`routes__also_loaded_by`, items `role = 'also_loaded_by'`; remote loaders included, two in-app loaders both listed with no `parent_route`); a root only a remote loads has no `parent_route` and lists it. `routes` also carries
  `path_match`, `can_deactivate`, `can_match` (guards, as `can_activate`) and `title`.
* **`key_reach.paths`** lists a key's ways, one `{path, own}` per way (`n_paths` entries). `path` joins `render_path.id`, may repeat, and is renumbered every build, so only join within one DB. `own` holds the ids the way holds beyond that row's `gates`/`branches`. Fold a per-way fact the map does not compute over the items of `key_reach__paths` (each item's `path` is a row below it, `role = 'path'`; `own` a child list, `role = 'own'`) joined to `render_path` (AND along `gates ∪ branches ∪ own`, UNION over entries). Never filter `render_path` by `key_reach.components`.
* **A component field NOTHING reads is no site of the key its initializer holds**: `labelCode = 'k'` in a class whose
  template binds only another field adds no way to `key_reach`. `files.prop_reads` (node rows `files__prop_reads`: one row per property name the file reads - the name its `role`, the count its `value` -
  unresolved) and `files.this_indexed` (1 when the file hands `this` to anything but `this.x`/`.bind`) are what proves it.
* **A branch around the call of a PRIVATE method holds inside it**: a component the method creates in code
  (`render_path.branches`) and a key it translates (`key_reach.always_branches`) carry the `if` every call of it sits in,
  when every read of the method is such a call. A public method carries only the creating call's own `if`.
* **A gate through a guarded method or a ternary**: a field set from `this.isA()` over `if (C) { return E; } return false;`
  restricts as `C && E`, and `c ? isHome(..) : isFlat(..)` as the union of both arms (`gate_values`).
* **`template_literals.parts`** are a template's literal pieces, its `holes` what fills them; a string constant joined
  in front with `+` (`BASE + `.${name}.tip``, through `( )` and either arm of `?:`) leads the first piece.
* **Block nodes** (`template_nodes.kind`): an `@switch` case is the parent of its content; a fall-through group
  (`@case (1) @case (2)`) is a `SwitchBlockCaseGroup` row that no case gates. `@let` carries `name`, its expression
  `expressions.kind = 'block_LetDeclaration'` and no gate. `@for`'s collection is a `block` gate named
  `ForLoopBlock` (as `*ngFor`'s `ngForOf`; its `track` is no gate), and `@empty` is the loop's SIBLING, outside that
  gate. `@for` lists `context_vars` and `context_aliases` (`{name, value}`). `@defer` lists `triggers`, `prefetch_triggers`, `hydrate_triggers` and `trigger_refs`
  (`{phase, trigger, reference}`); its `when` conditions are `expressions` (`defer_when`, `defer_prefetch_when`,
  `defer_hydrate_when`), never gates - the placeholder shows while they are false. An ICU's value and inner
  `{{ }}` are rows under it. A read rooted at an arrow function's parameter is no read (`param` on its `Read`).
  `@for` (its item `$implicit`, its context variables) and `@if (x; as u)` list what they bind in `variables`, as
  `*ngFor` does: a read of such a name, or of a `@let`, never resolves to a component member of that name. An
  `as` alias (`@if`, `*ngIf`) of a read chain or a pipe over one resolves through its condition's type, as an `@for`
  item through its collection's. An `@case` restricts its `@switch` in `gate_values` as `*ngSwitchCase` does, and an
  `@for` over a list of objects carries per-element key sites and `loop:` links as `*ngFor` does.
* **A template's tables have no `file`** - a binding hangs off a template node - so join by the ids; every
  row carries `owner_file`. A body's `calls`, `locals`, `assignments`, `returns`, `raises`, `branches`, `loops`,
  `shapes`, `shape_members`, `handlers`, `switch_cases` and the literal tables carry `file` too, as every other half does. `_meta` publishes what SQLite cannot record: the id prefixes, the joins and which
  column anchors a row to its file (`--id` follows them). Every declared join column leads an index, and so do
  `key_reach.key`, `render_path.component` and `gate_values.gate`/`gate_features.gate`: `WHERE calls.target_id = ?` (or `callee_id`) is a probe.
* **A loop is a `loops` row** (Angular half; the plain half's loops are `branches` rows with a `kind`): `kind` (`for`, `for_of`, `for_in`,
  `while`, `do`), `await` (1 for `for await`), `variables` (list: the names its initializer declares), `source` (the iterable of a
  `for..of`/`for..in`, else the condition; `''` for `for (;;)`), `expr` (joins `expressions.id`, role `loop`), `incrementor_source`, `line`/`col`/`end_line`,
  `member`/`class`, and `branch`/`case`/`choices` as a `calls` row has them. A row INSIDE a loop's body (`calls`, `assignments`, `locals`, `returns`,
  `raises`, `handlers`, a nested `loops`) carries `loop` (joins `loops.id`, the innermost); a callback in the body is its own member and has none.
  `branches` holds only if-arms and `try`: a statement in a loop is not conditional. A bare expression statement that is no call or write
  (`this.a.b;`) is an `expressions` row, role `statement`; `x++`/`--x` is an `assignments` row, `operator` `++`/`--`, `value` NULL.
* **A `value` cell is the EVALUATED expression** (`returns`, `assignments`, `locals`, `members`, `consts`, `raises`, and
  each of `calls.args`): a literal is its value (`'x'`, `1`, `true`), an expression the map cannot evaluate is
  `{$expr, $kind, $reads, ..}`. A KEYWORD literal is `{$literal, $kind}`: `undefined` anywhere (`$kind` `UndefinedKeyword`,
  `VoidExpression` for `void 0`; a local or parameter NAMED `undefined` is a name), and `null` as a whole cell
  (`NullKeyword`) - inside a list or an object `null` is its own `null` node. A NULL cell is a statement with no value
  (`return;`). A name is never a literal: `$kind = 'Identifier'`, `$expr` the name, plus `$local` (`{file, line, col}` of
  its `locals` row) when a destructuring binds it; a name whose value the map can follow is that value (`return s` over
  `let s = null` holds the `null` literal, `source` says `s`). `return null` rows:
  `JOIN returns__value v ON v.owner = r.id AND v.parent = 0 AND v.role = '$literal' AND v.value = 'null'`.
* **`imports.names`** is `{name, as, kind}` with `name` the LOCAL binding and `as` the name a rename came
  through. **`imports.external`** is about the SPELLING (a tsconfig alias is external and still in the
  tree; "in the tree" is `JOIN files rf ON rf.id = i.resolved_file WHERE rf.sha IS NOT NULL`).
  **`imports.resolved`** is the module the specifier NAMES - a barrel a third of the time; `import_names`
  follows the barrel to where each name is DECLARED (`declared_file`, `declared_id`, `via`).
  A bare `import('./x')` or `require('./x')` is an `imports` row too, with `dynamic` = `import`/`require` and no names.
  **`imports.in_tree`** is whether the import lands in the tree (not `node_modules`, not outside the root) - what
  `external = 0` was often read as.
* **A template pipe usage names the pipe it runs**: `expressions.pipes`/`template_strings.pipes` are the names, and
  `pipe_refs` (node rows `<table>__pipe_refs`, one item per name, in order) resolves each by the component's SCOPE (its
  NgModule's declarations and its imports' exports, or a standalone component's `imports`) - never by the name alone: `id`
  joins `pipes.id`; `external` (with `class`) is a library pipe (`date` through `CommonModule`); `ambiguous` is two in scope
  (Angular warns and takes the last registered, so none is picked); none of them, no pipe of that name in scope - a
  `diagnostics` row `pipe_unresolved_in_scope` (`reason`) or `pipe_ambiguous_in_scope`. A translate pipe's `i18n_refs` row
  reaches it through `i18n_refs.expression`. A TAG several components match is `selector_ambiguous_in_scope` (`node`, `tag`,
  `candidates`) where a host's scope sees them all, `selector_ambiguous_without_scope` in a template no component owns (rows 93).
* **`classes.extends`** is `{text, name, file, external, line, id, type_args}`: `text` as written, `name`/`file`/`line` the
  declaration the checker reached (alias and barrel followed), `id` its `classes` row (none for a library class), and
  `type_args` each type argument the same way (`extends Base<Order>`: Order's `id`), nested arguments under their own.
* **`comments`** holds EVERY comment of a parsed `.ts` file, once (rows 78): `line`, `col`, `kind`, `text`, and `context`, the declaration it sits in (null at the top level) - one in a call chain, after a class's last member, after a regex or template, or inside an EMPTY block, object, array, `switch` or type literal (`onEdit(_) { // TODO }`, rows 80) included.
* **`di`** is a constructor parameter, or an `inject()` (`inject = 1`). A FIELD that IS the call (`x = inject(T)`, through `as`/`<T>`/parentheses/`!`) is `param` the field; a call NESTED in
  the initializer (`x = (inject(T) as I).x`, `inject(T).y`, `inject(T) ?? z`) makes `x` a copy of something the injected object holds, so the class has ONE row per token it injects that way, `param` NULL and
  `via` the first field (none where a field already is that call; rows 78); a call in a FUNCTION that is no class member (a functional interceptor, a provider's factory) is a row with `class` NULL and
  `function`, its `functions` row, `param` the local it initialises (a CONSTRUCTOR body is an injection context, rows 80: a row of the class with `member` the constructor's row and `param` the local it initialises, null for a bare call; a callback in it and a method body are none: no row). `type`/`type_ref` are the field's (or local's) annotation, else the cast
  around the call, else the call's own type (rows 77, 78) - `y: I = inject(T) as I` is an `I`, never the token's `unknown`. `token`,
  `token_ref` and `token_id` name the injection token - an `@Inject(TOKEN)` or `inject()`'s argument - where `resolved`
  stays the declared TYPE; a token that is a property of a const object (`Tokens.X`)
  joins that const in `token_id`. An `export { A as B }` alias resolves to `A` (`routes.component_id`, the `id` on each
  `ng_modules` ref). A ref INSIDE an `ng_modules` list element carries its `id` too: a lazy route's `$module` in
  `RouterModule.forChild([...])` (the class `routes.load_children_module` names) and each `$ref` of a provider object. `in_module` marks an interface or type alias declared inside `declare module`, `declare global` or a
  namespace, and a namespace's class, function, const or enum. `state_selectors.factory` is set on a function that RETURNS `createSelector`. A static attribute that
  matches an input counts in `input_usage` (its `expression` is null) and `io.bound_count`.
* **`state_handlers`** is one row per action a reducer entry names: `on(A, B, fn)` is two (the handler is the LAST argument), and `immerOn` of `ngrx-immer` is an entry as `on` of `@ngrx/store` is - its
  `writes` are the paths its handler assigns, increments, deletes or mutates through (`s.list.push(..)`, `s.a.b = 1`, a local taken from the draft: `list.active`), where an `on` handler's are the keys it returns.
* **`state_effects`** is one row per `createEffect` member, and per `createEffect` call in a constructor body (`member` the constructor's row, `name` `constructor@<line>:<col>`): `actions` the `ofType` refs it listens to (as `state_handlers.action_source` spells them), `dispatch` 0 for `{ dispatch: false }`, `emits` the calls its callbacks return, as written.
* `handlers`: TypeScript names no type a clause catches, so every row is `bare = 1`; the caught name is
  `handlers.name`. The half also writes `raises`, and a `try` row in `branches` (`sense = 'try'`).
* Its ids restart from 1 when it is the only half in the database, so the same tree numbers the same way;
  nine prefixes (`f`, `c`, `x`, `fn`, `br`, `p`, `k`, `e`, `i`) are shared with the C# half.

* **`structuregate.ts.json`** (found at the workspace or an ancestor) names what no compiler can: `i18nCalls` lists the
  methods that take ONE translation key (`translate.get('a.b')`), and `i18nDictionaryCalls` the methods that hand back
  EVERY key under a prefix (`getTranslationDictionary('demo.sort.options')`, read as `dict[name]` or passed to a
  converter). A dictionary call whose first argument is a string makes each locale key under `<prefix>.` reachable from
  the component the call sits in (route `ts_call`), and from every template reading the field or getter that holds the
  result (`field_binding`); a prefix built at run time is not read. `i18n_refs` carries it as a row with `prefix`
  and `kind = 'dictionary_call'`, and no `key`. A dictionary call handed on to another function is not followed. `i18nCarriers` (template pipes/elements carrying a key) and the
  other cell contracts: [typescript-refs.md](typescript-refs.md).

Three outputs are options, off by default: `--ts-html <dir>` (one JSON per template, the tree in document
order), `--map-row-fts` (`row_fts`: which row ANYWHERE mentions a word), `--map-atlas <dir>` (projects,
NgModule areas, every route with what it reaches). `src/TsRows/CLAUDE.md` and `rust/fbtcore/src/rows/ts/CLAUDE.md` in the origin repo have the rest.

## Worked queries

```sql
-- which route loads which component
SELECT r.absolute_path, k.name FROM routes r JOIN components k ON k.class = r.component_id;

-- what a component renders
SELECT DISTINCT g.to_name, g.kind FROM render_graph g JOIN components k ON k.class = g.from_class
WHERE k.name = 'AccountComponent';

-- a name imported through a barrel: where it is really declared
SELECT i.local, i.declared, i.via FROM import_names i WHERE i.local = 'OrderService';

-- duplicate bodies in a plain-TS tree, grouped the way the JSON map groups them
SELECT body_shape, count(*) n, group_concat(name) FROM functions fn JOIN files f ON f.id = fn.file
WHERE f.lang = 'ts' AND body_shape <> '' GROUP BY body_shape HAVING n > 1 ORDER BY n DESC;
```
