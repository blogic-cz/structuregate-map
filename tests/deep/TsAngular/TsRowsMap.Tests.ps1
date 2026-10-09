<#
    The TypeScript half's rows about the MAP itself: the manifest it publishes, the closure tables derived
    from every other row, what an unchanged or partly changed tree costs, and the outputs asked for.
    Split from `TsRows` at its size limit; the helpers are `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '../TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

# ---- the manifest: what SQLite cannot record and every consumer needs -------------------------------

# SQLITE RECORDS NO FOREIGN KEY, so the map states its own. The tool this half reproduces publishes an
# `index.json` beside its shards and every consumer reads three things out of it - which columns hold ids,
# which of those reaches a row's own file, and what an id prefix names.
Test-Case 'tsrows: the map publishes its own join spec and id scheme' {
    $tree = New-TsRowsWorkspace
    $made = New-TsRowsDb $tree
    $r = Invoke-TsRowsQ $made.Db ("SELECT j.key || ' -> ' || j.value AS spec FROM _meta, " +
        "json_each(json_extract(_meta.value, '`$.joins')) j " +
        "WHERE _meta.key = 'spec:typescript' AND j.key = 'members.class'")
    Assert-Line $r 'members.class -> classes'
    $i = Invoke-TsRowsQ $made.Db ("SELECT json_extract(value, '`$.id_scheme.c') AS scheme " +
        "FROM _meta WHERE key = 'spec:typescript'")
    Assert-Line $i 'classes'
}

# A PREFIX CAN SERVE SEVERAL TABLES, and a scheme that names one of them silently resolves nothing for the
# rest: `k` is both a const and an i18n reference. Recorded as a SET where the row is written, never as a
# list kept somewhere else.
Test-Case 'tsrows: an id prefix that serves two tables names both' {
    $tree = New-TsRowsWorkspace
    $made = New-TsRowsDb $tree
    $r = Invoke-TsRowsQ $made.Db ("SELECT json_extract(value, '`$.id_scheme.k') AS scheme " +
        "FROM _meta WHERE key = 'spec:typescript'")
    Assert-Line $r 'consts | i18n_refs'
}

# THE OBVIOUS GRAPH WALK IS WRONG, which is the whole reason each step is PROVEN. A `renders` row carries
# `from_class`, so a walk files the edge under the component's `.ts` when the row was read from its
# `.html` - and the containment test catches exactly that, because the render's line is not inside the
# class's.
Test-Case 'tsrows: a join that reaches the wrong file is refused, however well it resolves' {
    $tree = New-TsRowsWorkspace
    $made = New-TsRowsDb $tree
    $r = Invoke-TsRowsQ $made.Db ("SELECT group_concat(json_extract(j.value, '`$.field')) AS route " +
        "FROM _meta, json_each(json_extract(_meta.value, '`$.anchors.renders')) j " +
        "WHERE _meta.key = 'spec:typescript'")
    Assert-Line $r 'template,node'
    Assert-NoLine $r 'from_class'
    # ...and the refused column is still a published JOIN: it is a real reference, just not a route home.
    $j = Invoke-TsRowsQ $made.Db ("SELECT j.key || ' -> ' || j.value AS j FROM _meta, " +
        "json_each(json_extract(_meta.value, '`$.joins')) j " +
        "WHERE _meta.key = 'spec:typescript' AND j.key = 'renders.from_class'")
    Assert-Line $j 'renders.from_class -> classes'
}

# A TABLE WITH NO ROUTE SAYS SO. `render_graph` is a SUMMARY row - one edge between two classes in two
# different files - so every join it has lands on a file the row was not read from, and the agreement test
# refutes both. Publishing "no route" is the honest answer; picking one of the two is a wrong file on every
# cross-file edge.
Test-Case 'tsrows: a table whose joins all disagree about the file is published as unanchored' {
    $tree = New-TsRowsWorkspace
    $made = New-TsRowsDb $tree
    # BY NAME, not by position: the list grows as the map does, and a case pinned to `[0]` would fail the
    # day another table joins it rather than the day this rule breaks.
    $r = Invoke-TsRowsQ $made.Db ("SELECT j.value AS t FROM _meta, " +
        "json_each(json_extract(_meta.value, '`$.unanchored')) j " +
        "WHERE _meta.key = 'spec:typescript' AND j.value = 'render_graph'")
    Assert-Line $r 'render_graph'
}

# AN INLINE TEMPLATE'S LINES ARE THE CLASS FILE'S LINES. Without `line_offset` the template claims lines 1
# to n of a file it starts part way down, its own nodes fall outside it, and the containment test refutes
# the one join that is actually right - pushing every template row onto a longer route for no reason.
Test-Case 'tsrows: an inline template contains its own nodes, because the offset is applied' {
    $tree = New-TsRowsWorkspace
    $made = New-TsRowsDb $tree
    $r = Invoke-TsRowsQ $made.Db ("SELECT json_extract(value, '`$.anchors.template_nodes[0].field') " +
        "|| ' by ' || json_extract(value, '`$.anchors.template_nodes[0].proof') AS route " +
        "FROM _meta WHERE key = 'spec:typescript'")
    Assert-Line $r 'template by containment'
}


# THE ROWS POINT AT FILES; THIS CARRIES THEM. The closure over this map answers one question the rows
# cannot - whether a translation key nothing references is really dead, or merely written in a way the
# extractor does not parse - and it answers it by reading the SOURCE. A binary file is not carried: the
# set of extensions is a fixed list of text ones.
Test-Case 'tsrows: the source of every text file is carried beside the rows' {
    # The tree holds a file whose extension names no text, so "not carried" is something this case can
    # SEE. Without it the count below is 0 whether the rule holds or not.
    $tree = New-TsRowsWorkspace @{ 'apps/shop/src/assets/logo.png' = "PNG-not-really`n" }
    $made = New-TsRowsDb $tree
    $r = Invoke-TsRowsQ $made.Db ("SELECT path FROM file_text WHERE content LIKE '%CartComponent%' " +
        "ORDER BY path")
    Assert-Line $r 'cart.component.ts'
    $b = Invoke-TsRowsQ $made.Db ("SELECT count(*) AS carried FROM file_text WHERE path LIKE '%.png'")
    Assert-Line $b '0'
    # ...and the file is still a `files` row: it is inventory of the tree, only not source to read.
    $f = Invoke-TsRowsQ $made.Db "SELECT count(*) AS listed FROM files WHERE path LIKE '%.png'"
    Assert-Line $f '1'
}


# ---- the closure: the four tables derived from all the others --------------------------------------

# WHICH CAPABILITY A GATE REQUIRES. The CapabilityKey is in neither the gate nor the property it reads: the
# gate reads a property, the property is assigned from a call to a DECLARED feature check, and the code is
# an argument of that call. Three rows have to be joined before the gate means anything.
Test-Case 'tsrows: a gate reading a feature-assigned property resolves to the feature code' {
    $tree = New-TsRowsWorkspace
    $made = New-TsRowsDb $tree
    $r = Invoke-TsRowsQ $made.Db ("SELECT f.feature || ' via ' || g.name AS need FROM gate_features f " +
        "JOIN gates g ON g.id = f.gate")
    Assert-Line $r 'IsPromoEnabled via ngIf'
}

# ...AND WHICH VALUES IT STILL PERMITS, which is the other half of what a gate means and a different table:
# a comparison against an enum constant does not require anything to be granted, it says which members of a
# FINITE domain still reach the element.
Test-Case 'tsrows: a gate comparing an enum constant restricts that dimension to it' {
    $tree = New-TsRowsWorkspace
    $made = New-TsRowsDb $tree
    $r = Invoke-TsRowsQ $made.Db ("SELECT enum_name || ' ' || dimension || ' ' || op || ' ' || " +
        "values_json || ' of ' || domain AS restriction FROM gate_values")
    Assert-Line $r 'Tier tier in ["Gold"] of 2'
}


# EVERY WAY IN TO A COMPONENT, from a component nothing renders. A component no template renders is its own
# root, at depth 0 - which is a path, not the absence of one, and publishing nothing for it would make a
# root indistinguishable from a component the walk never reached.
Test-Case 'tsrows: a component nothing renders is its own root, at depth zero' {
    $tree = New-TsRowsWorkspace
    $made = New-TsRowsDb $tree
    $r = Invoke-TsRowsQ $made.Db ("SELECT depth || ' | ' || hops || ' | ' || truncated AS path " +
        "FROM render_path WHERE component = root")
    Assert-Line $r '0 | ["ng:1"] | 0'
}

# WHERE A KEY CAN BE TRAVELLED TO, and what gates every way there. The route names the STRONGEST evidence -
# a carrier on a template node - and `always_gates` is the only column a consumer may read as necessary.
Test-Case 'tsrows: a key on a template carrier is placed, with the gates of the way in' {
    $tree = New-TsRowsWorkspace
    $made = New-TsRowsDb $tree
    $r = Invoke-TsRowsQ $made.Db ("SELECT route || ' | ' || n_refs || ' | ' || n_paths || ' | ' || " +
        "always_gates AS reach FROM key_reach WHERE key = 'shop.title'")
    Assert-Line $r 'template | 1 | 1 | ["g:1"]'
}

# A KEY NOTHING REFERENCES IS TRACED, NEVER SILENTLY DROPPED. "Dead" and "assembled at runtime" look the
# same in the rows, so the raw source is read to tell them apart - and a key the source does not show
# either is `absent`, which is the only reading that lets a consumer delete it.
Test-Case 'tsrows: a key nothing references is traced against the raw source' {
    $tree = New-TsRowsWorkspace
    $made = New-TsRowsDb $tree
    $r = Invoke-TsRowsQ $made.Db ("SELECT route || ' | ' || trace AS verdict FROM key_reach " +
        "WHERE key = 'shop.cart.empty'")
    Assert-Line $r 'none | absent'
}


# ---- nothing changed, nothing to do ----------------------------------------------------------------

# THIS HALF CANNOT BE INCREMENTAL PER FILE and the reason is worth stating: its rows are SEMANTIC. Rename
# a component and the resolved rows of files that did not change become wrong, so a per-file cache would go
# stale in silence - the same failure `CLAUDE.md` records for the C# half, where most of a large solution's files
# silently produced syntax-only rows while every count still looked right. What it CAN do is refuse to work
# when the answer is already right: minutes become seconds on a workspace of thousands of files.
Test-Case 'tsrows: a second run over an unchanged tree does nothing' {
    $tree = New-TsRowsWorkspace
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    $again = New-TsRowsDb $tree
    Assert-Exit $again.Result 0
    Assert-Line $again.Result 'had nothing to do'
    # ...and the rows it already held are still there, which is the whole claim.
    $r = Invoke-TsRowsQ $made.Db "SELECT count(*) AS n FROM templates"
    Assert-Line $r '3'
}

# ONE CHANGED BYTE IS A CHANGED MAP. The file need not be one the rows mention: a `.ts` nothing imports can
# still declare a selector that changes what a template resolves to.
Test-Case 'tsrows: a file that changed makes the half run again' {
    $tree = New-TsRowsWorkspace
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    Set-Content -LiteralPath (Join-Path $tree 'apps/shop/src/helpers.ts') -Value 'export const help = 2;'
    $again = New-TsRowsDb $tree
    Assert-Exit $again.Result 0
    Assert-NoLine $again.Result 'had nothing to do'
}

# A LEAF EDIT GOES THE SHORT WAY, and the parse run decides it from the PLAN's hashes (`--hashes`), never a
# second walk: a hand-over that lost them would read every file as changed and rebuild everything.
Test-Case 'tsrows: a leaf edit re-reads only what it reaches' {
    $tree = New-TsRowsWorkspace
    Assert-Exit (New-TsRowsDb $tree).Result 0
    Set-Content -LiteralPath (Join-Path $tree 'apps/shop/src/helpers.ts') -Value 'export const help = 3;'
    $again = New-TsRowsDb $tree
    Assert-Exit $again.Result 0
    Assert-Line $again.Result 'put back'
    Assert-NoLine $again.Result 'reading everything'
}

# A FILE WHOSE HASH DID NOT MOVE IS NOT READ AGAIN FOR ITS LINE COUNT, even on a run that reads everything: the
# count the database recorded against that hash is the count. FORGED here, so only a count taken from the
# database can say 777.
Test-Case 'tsrows: a file whose hash did not move keeps its recorded line count on a full run' {
    $tree = New-TsRowsWorkspace
    $made = New-TsRowsDb $tree
    & $script:Python -c "import sqlite3,sys; c = sqlite3.connect(sys.argv[1]); c.execute(sys.argv[2]); c.commit()" $made.Db "UPDATE files SET lines = 777 WHERE path IN ('angular.json', 'apps/shop/src/helpers.ts')"
    # A SELECTOR CHANGE, which is what still reads everything: a comment in a component goes the short way now.
    $cart = Join-Path $tree 'apps/shop/src/cart.component.ts'
    [IO.File]::WriteAllText($cart, [IO.File]::ReadAllText($cart).Replace("selector: 'app-cart'", "selector: 'app-cart2'"))
    Set-Content -LiteralPath (Join-Path $tree 'apps/shop/src/helpers.ts') -Value "export const help = 4;`n// two"
    $again = New-TsRowsDb $tree
    Assert-Line $again.Result 'reading everything'
    Assert-Line (Invoke-TsRowsQ $made.Db "SELECT 'n=' || lines AS n FROM files WHERE path = 'angular.json'") 'n=777'
    Assert-NoLine (Invoke-TsRowsQ $made.Db "SELECT 'n=' || lines AS n FROM files WHERE path = 'apps/shop/src/helpers.ts'") 'n=777'
}

# A CHANGED CONFIG IS A DIFFERENT MAP OVER THE SAME BYTES, and the file hashes cannot see it: the config
# can live OUTSIDE the workspace (in a folder beside it), so nothing
# in the fingerprint moves when it changes. Without the setup
# check the run would skip and publish the old answer for a new question.
Test-Case 'tsrows: a changed config makes the half run again, though no file moved' {
    $tree = New-TsRowsWorkspace
    $outside = Join-Path ([IO.Path]::GetTempPath()) ("sgtest-cfg-" + [Guid]::NewGuid().ToString('N').Substring(0, 12))
    [void](New-Item -ItemType Directory -Path $outside -Force)
    Register-Tree $outside
    $config = Join-Path $outside 'structuregate.ts.json'
    $db = Join-Path $tree 'map.sqlite'
    Set-Content -LiteralPath $config -Value '{"gateInputs":["disabled"]}'
    $first = Invoke-Gate --root $tree --map-sqlite $db --ts-node-modules $script:TsRowsModules --ts-config $config
    Assert-Exit $first 0
    $same = Invoke-Gate --root $tree --map-sqlite $db --ts-node-modules $script:TsRowsModules --ts-config $config
    Assert-Line $same 'had nothing to do'
    Set-Content -LiteralPath $config -Value '{"gateInputs":["disabled","hidden"]}'
    $changed = Invoke-Gate --root $tree --map-sqlite $db --ts-node-modules $script:TsRowsModules --ts-config $config
    Assert-Exit $changed 0
    Assert-NoLine $changed 'had nothing to do'
}


# THE SAME TREE NUMBERS THE SAME WAY, however often it is rebuilt. This half replaces its rows WHOLE, so
# continuing the counters the database recorded buys it nothing and costs reproducibility: a workspace
# numbered `renders` from 1 and then from ever higher starts over three runs, and `key_reach` sorts its gate
# lists by id STRING, so they reordered and a row-by-row comparison reported thousands of changed rows with no
# fact different. The ids therefore RESTART when this half is the only one in the database, as a map built
# from scratch numbers them.
Test-Case 'tsrows: the same tree numbers the same way, however often it is rebuilt' {
    $tree = New-TsRowsWorkspace
    $helpers = Join-Path $tree 'apps/shop/src/helpers.ts'
    $original = Get-Content -LiteralPath $helpers -Raw

    $first = New-TsRowsDb $tree
    Assert-Exit $first.Result 0
    $before = (Invoke-TsRowsQ $first.Db "SELECT id || ' ' || path AS row FROM files ORDER BY path").Text

    # A REBUILD AND NOT THE EARLY EXIT: the tree has to move for the half to run at all, or this case
    # would pass against a database nothing rewrote and prove nothing about the counters.
    Set-Content -LiteralPath $helpers -Value 'export const help = 99;' -NoNewline
    $changed = New-TsRowsDb $tree
    Assert-Exit $changed.Result 0
    Assert-NoLine $changed.Result 'had nothing to do'

    # ...and back to exactly the bytes the first run saw. Same tree in, same ids out.
    Set-Content -LiteralPath $helpers -Value $original -NoNewline
    $third = New-TsRowsDb $tree
    Assert-Exit $third.Result 0
    Assert-NoLine $third.Result 'had nothing to do'
    $after = (Invoke-TsRowsQ $third.Db "SELECT id || ' ' || path AS row FROM files ORDER BY path").Text

    if ($before -ne $after) {
        throw "the ids moved across rebuilds.`nfirst run:`n$before`nthird run:`n$after"
    }
}


# EVERY ROW SAYS WHICH FILE IT CAME OUT OF, which is what lets a rebuild replace one file's rows instead
# of the whole half. 21 of the 53 tables carry no `file` of their own, and the parent chain that would
# stand in for one was MEASURED INCOMPLETE: over a thousand `calls` rows are module-scope calls carrying `line`,
# `col` and `callee` and no id link at all, so nothing could ever attribute them.
#
# `owner_file` AND NOT `file`: stamping `file` where it was empty changed a FACT rather than adding one -
# `expressions.file` is empty for a template expression by contract, and filling it would change every one
# of those rows. This is a field only this map has.
#
# THREE PASSES MINT ROWS and each needed saying separately: the per-file walk, the template pass (tens of
# thousands of template-side expressions had no owner until it did) and the orphan templates (hundreds more).
Test-Case 'tsrows: every row records the file it was extracted from' {
    $tree = New-TsRowsWorkspace
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0

    # `calls` comes from the per-file walk, `bindings` and `template_nodes` from the template pass - the
    # two passes that had to be stamped separately. A table with no rows would make this unobservable, so
    # the count is asserted too.
    foreach ($table in @('calls', 'bindings', 'template_nodes', 'expressions')) {
        $r = Invoke-TsRowsQ $made.Db ("SELECT 'rows=' || count(*) || ' unowned=' || " +
            "sum(CASE WHEN owner_file IS NULL THEN 1 ELSE 0 END) AS n " +
            "FROM $table WHERE half = 'typescript'")
        Assert-NoLine $r 'rows=0'
        Assert-Line $r 'unowned=0'
    }

    # ...and it is the file the row came OUT of, not whatever its own `file` column says. A template
    # expression's `file` is empty in the map being reproduced; its owner is the template it sits in.
    $tpl = Invoke-TsRowsQ $made.Db ("SELECT 'owned=' || count(*) AS n FROM expressions e " +
        "JOIN files f ON f.id = e.owner_file " +
        "WHERE e.half = 'typescript' AND e.file IS NULL AND f.ext = 'html'")
    Assert-NoLine $tpl 'owned=0'
}


# WHICH FILES A CHANGE REACHES, stored beside the rows so the next run can parse only those. The graph is
# derived MECHANICALLY from every id reference rather than from a list of relations - a hand-written list
# was wrong on the second sample, because an `.html` template's row names the component CLASS in a `.ts`
# file and neither imports the other. See `rust/fbtcore/src/rows/deps.rs`.
Test-Case 'tsrows: the map records which files a change reaches' {
    $tree = New-TsRowsWorkspace
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0

    # IT IS STORED, not derived on demand: deriving it re-read a database of hundreds of MB and cost about a minute.
    $held = Invoke-TsRowsQ $made.Db ("SELECT 'targets=' || count(*) AS n FROM " +
        "json_each((SELECT value FROM _meta WHERE key = 'deps:typescript'))")
    Assert-NoLine $held 'targets=0'

    # IT IS KEYED BY PATH, not by row id: node is the only consumer and node has no id for a file until it
    # has extracted one. An import is an edge - `main.ts` imports `./cart`, so a change to cart.ts must
    # re-extract main.ts.
    $edge = Invoke-TsRowsQ $made.Db ("SELECT 'edge=' || count(*) AS n FROM " +
        "json_each((SELECT value FROM _meta WHERE key = 'deps:typescript')) AS t, " +
        "json_each(t.value) AS d " +
        "WHERE t.key = 'apps/shop/src/cart.ts' AND d.value = 'apps/shop/src/main.ts'")
    Assert-Line $edge 'edge=1'

    # A ROW REFERENCE IS AN EDGE TOO, and this is the half a list of relations misses: the component's
    # template is a file of its own, and `templates.class` names a class the template file never imports.
    $tpl = Invoke-TsRowsQ $made.Db ("SELECT 'tpl=' || count(*) AS n FROM " +
        "json_each((SELECT value FROM _meta WHERE key = 'deps:typescript')) AS t, " +
        "json_each(t.value) AS d " +
        "WHERE t.key = 'apps/shop/src/basket.component.ts' " +
        "AND d.value = 'apps/shop/src/basket.component.html'")
    Assert-Line $tpl 'tpl=1'

    # AND WHICH FILES THE GRAPH CANNOT SPEAK FOR. A file that DECLARES a selector can make a template match
    # something it never matched, and no recorded edge can describe a fact that has not happened yet - so a
    # change to one of these rebuilds everything. The component is in the set; its template is not, which
    # is the whole point of keeping the two apart.
    #
    # `cart.component.ts` AND NOT `basket.component.ts`, which declares a Component, a Directive, a Pipe
    # and an NgModule in one file - it reaches the set through four tables at once, so dropping two of them
    # left it in and the case passed while proving nothing. The cart declares only a component.
    $scope = Invoke-TsRowsQ $made.Db ("SELECT 'declares=' || count(*) AS n FROM " +
        "json_each((SELECT value FROM _meta WHERE key = 'scope:typescript')) " +
        "WHERE value = 'apps/shop/src/cart.component.ts'")
    Assert-Line $scope 'declares=1'
    $notScope = Invoke-TsRowsQ $made.Db ("SELECT 'template=' || count(*) AS n FROM " +
        "json_each((SELECT value FROM _meta WHERE key = 'scope:typescript')) " +
        "WHERE value = 'apps/shop/src/basket.component.html'")
    Assert-Line $notScope 'template=0'
}


# A PARTIAL RUN'S LISTS AND OBJECTS ARE STORED AS A FULL RUN STORES THEM: the payload keeps them as the text node
# sent and spells them at the write (`rows/rawcells.rs`) - a write that missed one would store the wrapper instead.
Test-Case 'tsrows: an AST a partial run re-extracted is stored as the object it is' {
    $tree = New-TsRowsWorkspace
    Assert-Exit (New-TsRowsDb $tree).Result 0
    Set-Content -LiteralPath (Join-Path $tree 'apps/shop/src/helpers.ts') -Value 'export const help = [1, 2].map((x) => x + 1);'
    $again = New-TsRowsDb $tree
    Assert-Line $again.Result 'put back'
    $of = "FROM expressions WHERE ast IS NOT NULL AND file = (SELECT id FROM files WHERE path = 'apps/shop/src/helpers.ts')"
    Assert-NoLine (Invoke-TsRowsQ $again.Db "SELECT 'n=' || count(*) AS n $of") 'n=0'
    Assert-Line (Invoke-TsRowsQ $again.Db "SELECT 'bad=' || count(*) AS n $of AND json_extract(ast, '$.k') IS NULL") 'bad=0'
}

# A PARTIAL RUN REPAIRS THE GRAPH it read its own plan from: the next run decides what to re-read from it,
# so an import this run added has to be an edge afterwards, or its file is not re-read when its target moves.
Test-Case 'tsrows: a partial run records the import it just added in the graph' {
    $tree = New-TsRowsWorkspace
    Assert-Exit (New-TsRowsDb $tree).Result 0
    Set-Content -LiteralPath (Join-Path $tree 'apps/shop/src/helpers.ts') -Value "import { Cart } from './cart';`nexport const help = new Cart();"
    $again = New-TsRowsDb $tree
    Assert-Line $again.Result 'put back'
    $edge = Invoke-TsRowsQ $again.Db ("SELECT 'edge=' || count(*) AS n FROM " +
        "json_each((SELECT value FROM _meta WHERE key = 'deps:typescript')) AS t, json_each(t.value) AS d " +
        "WHERE t.key = 'apps/shop/src/cart.ts' AND d.value = 'apps/shop/src/helpers.ts'")
    Assert-Line $edge 'edge=1'
}

# THE CLOSURE TABLES ARE REPLACED ON A REBUILD, NEVER APPENDED. They are written by the PYTHON half now,
# and a row that is not stamped with `half` cannot be found by `drop_half` - so the second run leaves the
# first run's rows behind and adds a second copy of every path, key and gate. Before this case existed
# `render_path` came out at two rows for every path, exactly double. A row-by-row comparison cannot see
# it - it skips `half` as housekeeping. Only rebuilding twice over one database shows it.
Test-Case 'tsrows: the closure tables are replaced on a rebuild, never appended' {
    $tree = New-TsRowsWorkspace
    $helpers = Join-Path $tree 'apps/shop/src/helpers.ts'
    $original = Get-Content -LiteralPath $helpers -Raw

    $first = New-TsRowsDb $tree
    Assert-Exit $first.Result 0
    $counted = "SELECT 'render_path=' || (SELECT count(*) FROM render_path) || " +
        "' key_reach=' || (SELECT count(*) FROM key_reach) AS n"
    $before = (Invoke-TsRowsQ $first.Db $counted).Text
    # THE FIXTURE HAS TO MAKE THE RULE OBSERVABLE. Zero rows double to zero, and this case would pass
    # against a half that appends every time - which is the trap three earlier cases here fell into.
    if ($before -like '*render_path=0*' -or $before -like '*key_reach=0*') {
        throw "the fixture produced no closure rows, so appending would be invisible: $before"
    }

    # A REBUILD AND NOT THE EARLY EXIT: the tree has to move for the half to run again at all.
    Set-Content -LiteralPath $helpers -Value 'export const help = 3;' -NoNewline
    $again = New-TsRowsDb $tree
    Assert-Exit $again.Result 0
    Assert-NoLine $again.Result 'had nothing to do'
    Set-Content -LiteralPath $helpers -Value $original -NoNewline
    $third = New-TsRowsDb $tree
    Assert-Exit $third.Result 0
    $after = (Invoke-TsRowsQ $third.Db $counted).Text
    if ($before -ne $after) { throw "the closure tables grew across rebuilds.`nfirst: $before`nthird: $after" }

    # ...and what makes them droppable at all is the stamp, so the case says so rather than relying on it.
    $stamped = Invoke-TsRowsQ $third.Db ("SELECT 'stamped=' || count(*) AS n FROM pragma_table_info('render_path') " +
        "WHERE name = 'half'")
    Assert-Line $stamped 'stamped=1'
}


# WHAT COULD NOT BE ANSWERED, AS ROWS. A tally says how many; only the items say WHICH, and a map that
# reports thousands of diagnostics it cannot name is asking to be believed. `apps/shop/extra/dead.ts` is real
# TypeScript that no tsconfig includes - the build drops it on the floor, and that is a different fact
# from content deliberately not extracted.
Test-Case 'tsrows: what the half could not answer is a row, not just a count' {
    $tree = New-TsRowsWorkspace
    $made = New-TsRowsDb $tree
    $r = Invoke-TsRowsQ $made.Db "SELECT kind || ' ' || file AS problem FROM diagnostics"
    Assert-Line $r 'ts_not_in_any_program apps/shop/extra/dead.ts'
}

}
