# A list or an object is NODE ROWS, never text

**NO CELL HOLDS A LIST OR AN OBJECT.** Each one is rows of a child table `<table>__<column>` (`expressions__reads`,
`assignments__reads`, `enums__members`, `expressions__ast`), so there is no `json_each`, `json_extract` or
`json_array_length` anywhere: every question is a join. `_cells(tbl, col, child)` lists which columns went where.

| column | meaning |
|---|---|
| `owner` | the id of the row the cell belongs to (`#<rowid>` for a table without one) |
| `n` | the node's preorder number in that cell; the ROOT is `n = 0`. A node IS `(owner, n)` - a unique index, so a node table scans in no promised order: `ORDER BY owner, n` (or `ord`) where order matters |
| `parent` | the `n` of the node it hangs off, NULL for the root |
| `role` | its KEY in the parent object (`k`, `name`, `args`); an item of a member list carries the list's key too |
| `ord` | its index in a list |
| `node` | `object`, `list`, `string`, `number`, `bool` (0/1) or `null` |
| `value` | a scalar's own value |
| `last` | the `n` of the LAST node of its subtree (itself for a scalar). `n` is a gapless preorder, so a node's whole subtree is `e.owner = s.owner AND e.n BETWEEN s.n AND s.last` |
| `depth` | its distance from the root, the root 0 |

**THE COLUMNS ARE FIXED: EVERY MEMBER OF AN OBJECT IS A ROW OF ITS OWN**, its name in `role` - never a column of
its parent, because a name is data as often as it is a field (an object literal's keys, a property map, a
document's headers). One field of node `x` is one join: `f.owner = x.owner AND f.parent = x.n AND f.role = '<name>'`.
The items of a top-level list are `j.owner = t.id AND j.parent = 0`, in `ord` order. A column that is a scalar in
some rows keeps those in the parent table.

```sql
-- the names an expression reads, in order
SELECT j.value FROM expressions x JOIN expressions__reads j ON j.owner = x.id AND j.parent = 0
WHERE x.id = 'x:41' ORDER BY j.ord;

-- how many members an enum has, and one enum's members (each member an object: its `name` is a row below it)
SELECT e.name, (SELECT count(*) FROM enums__members j WHERE j.owner = e.id AND j.parent = 0) n FROM enums e;
SELECT nm.value AS name, v.value FROM enums e
JOIN enums__members m ON m.owner = e.id AND m.parent = 0
JOIN enums__members nm ON nm.owner = m.owner AND nm.parent = m.n AND nm.role = 'name'
LEFT JOIN enums__members v ON v.owner = m.owner AND v.parent = m.n AND v.role = 'value'
WHERE e.name = 'Kinds' ORDER BY m.ord;

-- every expression holding a Read node of a given name: any node of the tree is a row
SELECT DISTINCT x.id, x.source FROM expressions x
JOIN expressions__ast n ON n.owner = x.id
JOIN expressions__ast k ON k.owner = n.owner AND k.parent = n.n AND k.role = 'k' AND k.value = 'Read'
JOIN expressions__ast m ON m.owner = n.owner AND m.parent = n.n AND m.role = 'name' AND m.value = 'DATA_DIR';
```

**A SUBTREE OR THE ANCESTORS OF A NODE ARE ONE RANGE, NEVER `WITH RECURSIVE`.** Below a node is `n BETWEEN s.n AND s.last`
of its owner; above it is `x.n BETWEEN a.n AND a.last`. Both read the `(owner, n)` key: measured on a large workspace, a
query over every subtree took a fraction of a second against tens of seconds recursive over `parent`, and one asking
what sits anywhere below a node seconds against minutes.

`--map-query <db> --subtree OWNER [--node N] [--table T]` prints it without SQL: one line per node, its `n`, then
`role` (`[ord]` for a list item) and `node` indented by depth below the start, and a scalar's value. `--node`
defaults to the root 0; `--table` takes one node table (`expressions__ast`) or a row table (`expressions`: every
node table of it), and without it every node table holding that node answers. `--limit` counts nodes per tree.
`--ancestors OWNER --node N [--table T]` prints the path from the root down to node N the same way, indented
by depth from the root, the node itself last. `--children OWNER [--node N] [--table T]` prints only the nodes one
level below N, each that holds nodes of its own with `+K below` - the size of its subtree, so a large one can be
asked of `--subtree` next. `--siblings OWNER --node N [--table T]` prints its parent's children the same way, N
marked `<- asked` among them (the root has none). `--descendants-of-kind OWNER --kind K [--node N] [--table T]` prints
every object strictly below N whose kind - its `k` member, or `kind` where a half names it so - is K, each with its kind
and `+K below`, indented by depth (every `Call` inside one expression, every `Map` inside one literal).
`--path-to OWNER --node N [--from F] [--table T]` prints the way down from F (the root 0 by default) to N, and in its
header the accessor path the roles spell (`items[0].values[5].k`, a list item `[ord]` after its list); an N that F does
not hold is exit 2. `--leaves OWNER [--node N] [--table T]` prints only the nodes of N's subtree that hold nothing
(`last = n`: every scalar, and an empty list or object), indented by depth from N. `--depth-range OWNER --depth A[-B]
[--node N] [--table T]` prints the nodes of N's subtree A to B levels below N (`--depth 1` its children, `--depth 0-2` it
and two levels down), each holding more with `+K below` - the shape of a large tree without its whole range. `--lca OWNER --node A --with B
[--table T]` prints the deepest node whose range holds both (`n <= min AND last >= max`, the last in preorder) and the
accessor path from it to each (`to 9: [1].c[0].k`); a `--with` no table holds is exit 2. `--nth-child OWNER --index I [--node N] [--table T]` prints N's child I in source
order (0 the first, -1 the last, -2 the one before it) as its whole subtree; a place past either end is exit 2. A node no table holds is exit 2.

```sql
-- one node's whole subtree, indented, in source order
SELECT printf('%.*c', e.depth * 2, ' ') || coalesce(e.role, '') || ' ' || e.node || ' ' || coalesce(e.value, '')
FROM expressions__ast s JOIN expressions__ast e ON e.owner = s.owner AND e.n BETWEEN s.n AND s.last
WHERE s.owner = 'x:41' AND s.n = 5 ORDER BY e.n;

-- every Call whose arguments read DATA_DIR at ANY depth
SELECT DISTINCT c.owner, c.parent FROM expressions__ast c
JOIN expressions__ast p ON p.owner = c.owner AND p.n = c.parent
JOIN expressions__ast m ON m.owner = c.owner AND m.n > p.n AND m.n <= p.last AND m.role = 'name' AND m.value = 'DATA_DIR'
WHERE c.role = 'k' AND c.value = 'Call';
```

Absent is `NOT EXISTS (SELECT 1 FROM t__c j WHERE j.owner = t.id)`; an EMPTY list is a root with no child
(`EXISTS` on `j.owner = t.id AND j.n = 0`, `NOT EXISTS` on `j.owner = t.id AND j.parent = 0`).

**Derived across halves, rewritten each run:** `seed_<schema>_<table>` is a VIEW per seeded table, one column
per seed column, so join on `ProductID` instead of parsing `sql_seeds.row_values`. `duplicate_types` lists a type
name declared in more than one place, and for an enum whether the copies' members differ. With a
`structuregate.facts.json`, `doc_facts` holds the tables of a document outside the tree, and `doc_links` holds
each fact against the code: `bound`, `missing in code` or `missing in doc` - the config, `--facts-pull` and
the token are in [facts.md](facts.md).

**Angular HTTP calls against C# actions** (rewritten each run): `http_endpoints` is every attribute-routed C#
action, written for a C#-only tree too (`cls`, `method`, `line`, `verb` - `ANY` for a `[Route]` with no verb -,
`template` as written with the class route and tokens applied, one row per class `[Route]`, `route` normalized, `func` the action's `functions` id - its `parameters` by `qualname = functions.symbol`, its own calls by `file` and `func` name, a mediator `Send` on through `dispatch`);
`http_calls` every `HttpClient` request of either half - in C# (#143) a call bound to `System.Net.Http.HttpClient` or `HttpClientJsonExtensions`, its verb from its name (`GetAsync`, `PostAsJsonAsync`) or from the `HttpRequestMessage` a `SendAsync` is handed - its arguments or its initializer's `Method`/`RequestUri` (`ANY` when the method is not one of `HttpMethod`'s), its url the `requestUri` argument folded (an interpolation's holes `{}`, a `new Uri(url, ..)` its string, `string.Empty` the base address `""`, a local only ever `+=`-appended to its path once the query is cut, a ternary to each of its paths - a row per path, a call to a method of the tree what it returns with its parameters the caller's arguments - not a virtual one - and `QueryHelpers.AddQueryString` its url), a method whose url is its own parameter - bare, or joined to a base the run time knows (`_cfg.Url + url`, `string.Concat(_base, url)`) or formatted from it (`string.Format(url, args)`: each caller's `{n}` a hole `{}`), text written before it a prefix of each caller's url (one nothing in the tree calls is its own row, the parameter `{}`) - a wrapper whose callers are the rows (`via` its `functions` id, `ANY` when its requests ask with different verbs, an extension method over `HttpClient` included, an overloaded name never); in TypeScript every `HttpClient` request (not `HttpHeaders`/`HttpParams`), a wrapper's callers standing for the wrapper's own call A url folds through a helper that RETURNS ITS FIRST ARGUMENT (`createQueryUrl(url, params)`: the callee's own `returns` are `${url}${query}`) to that argument, query cut, at a call site as in a wrapper; a call whose callee turns its argument into a url (an id handed to a url builder) is what that builder RETURNS, folded in the builder with its parameters the caller's arguments (rows 88: `this.ordersUrl('list')` over `${root}orders/${relative}` is `*/orders/list`); a builder whose returns fold apart is an unknown piece, so `${this.pick(f)}/get` is `*/get` - unless it switches on a parameter the caller hands a literal one case names (`this.baseOf(Kind.Orders)` is that case's return). An array joined into a url (`parts.join('')`) is its elements and what the callable pushes onto it, in order. A `+` chain folds operand by operand, a ternary to its arms when they agree once the query is cut, and a member the constructor sets once (and nothing writes again) to that value (rows 88).
(`url` is the folded pattern - `{}` a parameter, `*` an unknown part, `via` the wrapper); `http_links` which call
reaches which endpoint (`how` `exact` or `ambiguous`; a call with no row there matched nothing). An unknown base URL
is a leading `*` and matches any leading segments; of the endpoints a url matches, only those with the most
segments it names literally are linked, and one it names none of (only parameters in common) is not; on a tie, those taking the url's parameters as parameters (`orders/{}` reaches `orders/{id}`, not `orders/list`). Only `@angular/common/http` calls and only attribute-routed
actions are rows: minimal APIs, python routes and `fetch` are not endpoints or calls yet.
