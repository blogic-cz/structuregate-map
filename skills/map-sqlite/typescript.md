# The two TypeScript halves

Everything in [SKILL.md](SKILL.md) holds. A tree gets ONE of these: the Angular half where
`angular.json`/`nx.json` and `@angular/compiler` are found, the plain half everywhere else. Their
`files.lang` differ (`typescript` against `ts`) and so does the shape of their rows.

## Plain TypeScript / JavaScript (`files.lang = 'ts'`)

The tree's own `typescript` 5 parser, no program, no type checker, so the rows are SYNTACTIC and per file
like python's, in the same tables where the meaning is the same (`functions`, `calls`, `arguments`,
`imports`, `branches`, `assignments`, `consts`, `exports`, `returns`, `raises`, `string_literals`,
`expressions`, `parameters`, `classes`, `handlers`, `comments`, `regexes`) plus **`types`** (interfaces and
type aliases) and **`jsx`** (one row per JSX element).

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
| templates | `templates` `template_nodes` `bindings` `renders` `render_graph` `component_reach` `selector_index` |
| routing and gates | `routes` `gates` `gate_values` `gate_index` |
| NgRx | `state_actions` `state_handlers` `state_selectors` |
| the closure, derived LAST from every other table | `render_path` `key_reach` `gate_features` (+ `gate_values`): where a translation key can be travelled to from a routed root, what gates every way in, which capability each gate requires |

* **`components.class`** is the class row (`c:...`) that `routes.component_id`, `render_graph.from_class`
  and `renders` point at. `templates`/`template_nodes`/`bindings` are the template side; `gates`/
  `gate_values` what hides a node (`op` in/not_in/unknown; `listed_json` what a config object lists, even an
  exclusion the map cannot polarise); `state_*` NgRx; `di`/`injectables` injection.
* **`template_literals.parts`** are a template's literal pieces, its `holes` what fills them; a string constant joined
  in front with `+` (`BASE + `.${name}.tip``, through `( )` and either arm of `?:`) leads the first piece.
* **A template's tables have no `file`** - a binding hangs off a template node - so join by the ids; every
  row carries `owner_file`. A body's `calls`, `locals`, `assignments`, `returns`, `raises`, `branches`,
  `handlers`, `switch_cases` and the literal tables carry `file` too, as every other half does. `_meta` publishes what SQLite cannot record: the 43 id prefixes, 129 joins and which
  column anchors a row to its file (`--id` follows them).
* **`imports.names`** is `{name, as, kind}` with `name` the LOCAL binding and `as` the name a rename came
  through. **`imports.external`** is about the SPELLING (a tsconfig alias is external and still in the
  tree; "in the tree" is `JOIN files rf ON rf.id = i.resolved_file WHERE rf.sha IS NOT NULL`).
  **`imports.resolved`** is the module the specifier NAMES - a barrel a third of the time; `import_names`
  follows the barrel to where each name is DECLARED (`declared_file`, `declared_id`, `via`).
* `handlers`: TypeScript names no type a clause catches, so every row is `bare = 1`; the caught name is
  `handlers.name`. The half also writes `raises`, and a `try` row in `branches` (`sense = 'try'`).
* Its ids restart from 1 when it is the only half in the database, so the same tree numbers the same way;
  nine prefixes (`f`, `c`, `x`, `fn`, `br`, `p`, `k`, `e`, `i`) are shared with the C# half.

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
