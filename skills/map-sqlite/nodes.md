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
`template` as written with the class route and tokens applied, one row per class `[Route]`, `route` normalized);
the two tables below need the TypeScript half in the same database, and are empty without it: `http_calls` every `HttpClient` request (not `HttpHeaders`/`HttpParams`), a wrapper's callers standing for the wrapper's own call A url folds through a helper that RETURNS ITS FIRST ARGUMENT (`createQueryUrl(url, params)`: the callee's own `returns` are `${url}${query}`) to that argument, query cut, at a call site as in a wrapper; a call whose callee turns its argument into a url (an id handed to a url builder) is what that builder RETURNS, folded in the builder with its parameters the caller's arguments (rows 88: `this.ordersUrl('list')` over `${root}orders/${relative}` is `*/orders/list`); a builder whose returns fold apart is an unknown piece, so `${this.pick(f)}/get` is `*/get`.
(`url` is the folded pattern - `{}` a parameter, `*` an unknown part, `via` the wrapper); `http_links` which call
reaches which endpoint (`how` `exact` or `ambiguous`; a call with no row there matched nothing). An unknown base URL
is a leading `*` and matches any leading segments; of the endpoints a url matches, only those with the most
segments it names literally are linked, and one it names none of (only parameters in common) is not. Only `@angular/common/http` calls and only attribute-routed
actions are rows: minimal APIs, python routes and `fetch` are not endpoints or calls yet.
