<#
    The TypeScript half's rows about what Angular code IS: classes and their members, calls, branches,
    callbacks, components, templates, routes and the NgRx store. Split from `TsRows` at its size limit;
    the compiler, the fixture workspace and the query lens are `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '../TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

# WHAT A CLASS IS, AND WHAT IT INHERITS. `extends`/`implements` carry the resolved declaration beside the
# text, because the text alone leaves a consumer matching names against candidate files by hand.
Test-Case 'tsrows: a class records its heritage, resolved, and the decorators on it' {
    $tree = New-TsRowsWorkspace
    $made = New-TsRowsDb $tree
    $r = Invoke-TsRowsQ $made.Db ("SELECT name, abstract, exported, json_extract(extends,'`$.text') AS base, " +
        "json_extract([implements],'`$[0].text') AS iface FROM classes WHERE name = 'CartComponent'")
    Assert-Line $r 'Base'
    Assert-Line $r 'Item'
    # THE RESOLVED DECLARATION, ON ITS OWN. Asked beside the text, `Item` answers the assertion and a build
    # that resolves no heritage at all passes.
    $where = Invoke-TsRowsQ $made.Db ("SELECT json_extract([implements],'`$[0].file') AS iface_file " +
        "FROM classes WHERE name = 'CartComponent'")
    Assert-Line $where 'apps/shop/src/cart.ts'
}

# WHETHER A DECORATOR IS ANGULAR'S IS RESOLVED THROUGH THE CHECKER, never matched against a list of five
# names - a locally-defined `Component` would otherwise be counted as the framework's.
Test-Case 'tsrows: an Angular decorator is told apart from a local one by where it is declared' {
    $tree = New-TsRowsWorkspace
    $made = New-TsRowsDb $tree
    $r = Invoke-TsRowsQ $made.Db ("SELECT json_extract(angular,'`$[0]') AS ng FROM classes " +
        "WHERE name = 'CartComponent'")
    Assert-Line $r 'Component'
}

# A DECORATOR'S ARGUMENTS ARE EVALUATED, not kept as text: the selector a component answers to is the
# join key half the map is built on.
Test-Case 'tsrows: a decorator publishes its arguments as values' {
    $tree = New-TsRowsWorkspace
    $made = New-TsRowsDb $tree
    $r = Invoke-TsRowsQ $made.Db ("SELECT name, json_extract(args,'`$[0].selector') AS selector " +
        "FROM class_decorators WHERE name = 'Component'")
    Assert-Line $r 'app-cart'
}

Test-Case 'tsrows: a member records its kind, visibility, modifiers and evaluated value' {
    $tree = New-TsRowsWorkspace
    $made = New-TsRowsDb $tree
    $r = Invoke-TsRowsQ $made.Db ("SELECT kind, visibility, static, optional, value FROM members " +
        "WHERE name = 'VERSION'")
    Assert-Line $r 'property'
    Assert-Line $r '1.0'
    $private = Invoke-TsRowsQ $made.Db "SELECT visibility, optional FROM members WHERE name = 'count'"
    Assert-Line $private 'private'
}

# A METHOD'S PARAMETERS ARE ROWS OF THEIR OWN INSIDE THE MEMBER, with the type each one declares.
# `--find` WALKS A LIST OF NAME COLUMNS, and the list was the python half's: on an Angular map a method's
# name answered with every CALL to it and never its declaration, because `members` was not asked. The
# path is asserted too - these rows anchor by `owner_file`, and the lens joined `file`, which is NULL here.
Test-Case 'tsrows: --find reaches a member and names the file that owns it' {
    $tree = New-TsRowsWorkspace
    $made = New-TsRowsDb $tree
    $r = Invoke-Gate --map-query $made.Db --find 'greet'
    Assert-Exit $r 0
    $row = @($r.Lines | Where-Object { $_ -match '^members\s' })
    if ($row.Count -lt 1) { throw "no members row for greet. Output:`n$($r.Text)" }
    if ($row[0] -notmatch 'cart.component.ts') { throw "the members row names no file: $($row[0])" }
    # A table with NO line column (routes) used to fail its query silently and return nothing.
    $routes = Invoke-Gate --map-query $made.Db --find 'basket'
    Assert-Line $routes 'routes'
}

Test-Case 'tsrows: a method member carries its parameters' {
    $tree = New-TsRowsWorkspace
    $made = New-TsRowsDb $tree
    $r = Invoke-TsRowsQ $made.Db ("SELECT kind, json_extract(params,'`$[0].name') AS first_param, " +
        "json_extract(params,'`$[0].type') AS first_type FROM members WHERE name = 'greet'")
    Assert-Line $r 'method'
    Assert-Line $r 'name'
}

# WHAT A CONSTRUCTOR IS HANDED is the dependency graph of an Angular application, and each parameter's type
# is resolved to where it is declared rather than left as a name.
Test-Case 'tsrows: a constructor parameter is a di row, with the type it resolves to' {
    $tree = New-TsRowsWorkspace
    $made = New-TsRowsDb $tree
    $r = Invoke-TsRowsQ $made.Db ("SELECT param, type, json_extract(resolved,'`$.file') AS declared_in " +
        "FROM di ORDER BY param")
    Assert-Line $r 'cart'
    Assert-Line $r 'apps/shop/src/cart.ts'
    Assert-Line $r 'other'
}

# EVERY ARGUMENT, not the first four: a call's fifth argument is as much a fact as its first, and the cut
# was invisible - a consumer saw a complete-looking row that silently dropped the rest.
Test-Case 'tsrows: a call records its callee, its target and every argument' {
    $tree = New-TsRowsWorkspace
    $made = New-TsRowsDb $tree
    $r = Invoke-TsRowsQ $made.Db ("SELECT callee, method, json_array_length(args) AS n FROM calls " +
        "WHERE method = 'compute'")
    Assert-Line $r 'this.compute'
    Assert-Line $r '4'
}

# EACH ARGUMENT GAINS THE PARAMETER IT FILLS, and a REST parameter absorbs every argument from its position
# onward - which is what the language does, stated rather than approximated.
Test-Case 'tsrows: a call argument is labelled with the parameter it fills, rest included' {
    $tree = New-TsRowsWorkspace
    $made = New-TsRowsDb $tree
    # THE FOURTH argument: at position 2 the rest parameter is simply the third entry of the list, so a
    # build with no absorb rule labels it correctly anyway and the case proves nothing.
    $r = Invoke-TsRowsQ $made.Db ("SELECT json_extract(arg_params,'`$[0]') AS first, " +
        "json_extract(arg_params,'`$[3]') AS fourth FROM calls WHERE method = 'compute'")
    Assert-Line $r 'a'
    Assert-Line $r 'rest'
}

# WHAT A CLASS PROPERTY IS ASSIGNED FROM - the edge that turns a template gate into a reason. `scope` says
# it is a member of THIS component and not a local.
Test-Case 'tsrows: an assignment to a class property records the target, its scope and the row it sets' {
    $tree = New-TsRowsWorkspace
    $made = New-TsRowsDb $tree
    $r = Invoke-TsRowsQ $made.Db ("SELECT a.target, a.scope, a.operator, m.name AS sets " +
        "FROM assignments a LEFT JOIN members m ON m.id = a.target_id WHERE a.target = 'total'")
    Assert-Line $r 'this'
    Assert-Line $r 'total'
}

# A DESTRUCTURING DECLARATION KEEPS ITS PATTERN, so a consumer can pair binding `id` with property `id` of
# whatever the call returns. Nothing here infers that pairing - it records the two halves.
Test-Case 'tsrows: a destructuring local records each binding and the shared initializer' {
    $tree = New-TsRowsWorkspace
    $made = New-TsRowsDb $tree
    $r = Invoke-TsRowsQ $made.Db ("SELECT destructured, json_extract(bindings,'`$[0].local') AS first, " +
        "source FROM locals WHERE destructured = 1")
    Assert-Line $r 'id'
    Assert-Line $r 'this.state()'
}

# A STATEMENT INSIDE AN `if` IS AS CONDITIONAL AS ONE INSIDE A `case`. `sense` is what a case row does not
# need: an `else` branch means the condition is FALSE, and without it a consumer would invert half the chain.
Test-Case 'tsrows: an if produces a then and an else branch, each carrying the condition' {
    $tree = New-TsRowsWorkspace
    $made = New-TsRowsDb $tree
    $r = Invoke-TsRowsQ $made.Db ("SELECT sense, condition_source FROM branches ORDER BY sense")
    Assert-Line $r 'then'
    Assert-Line $r 'else'
    Assert-Line $r 'kind > 1'
}

# A CASE GROUP, NOT A CLAUSE: `case 1: case 2: return x` puts the return in 2 while 1 is empty, so one row
# per clause would make a consumer walk backwards to discover 1 leads there too.
Test-Case 'tsrows: a fallthrough case group is one row carrying every label' {
    $tree = New-TsRowsWorkspace
    $made = New-TsRowsDb $tree
    $r = Invoke-TsRowsQ $made.Db ("SELECT json_array_length(labels) AS n, discriminant_source, is_default " +
        "FROM switch_cases WHERE is_default = 0")
    Assert-Line $r '2'
    Assert-Line $r 'kind'
}

# A RETURN INSIDE A `RANCH IS NOT WHAT THE METHOD RETURNS - it is what it returns THERE. The row carries the
# branch it sits in, which is the whole reason branches are rows.
Test-Case 'tsrows: a return records the branch it sits in' {
    $tree = New-TsRowsWorkspace
    $made = New-TsRowsDb $tree
    $r = Invoke-TsRowsQ $made.Db ("SELECT r.source, b.sense FROM returns r JOIN branches b ON b.id = r.branch " +
        "WHERE r.source = '''big'''")
    Assert-Line $r 'then'
}

# A CALL`ACK IS A FIRST-CLASS `ODY: its own `functions` row, its own returns, and `parent` linking it to the
# body it sits in. Attributed to the enclosing method, the method looks like it declares what it does not.
Test-Case 'tsrows: an inline callback is its own function row, parented to the body it sits in' {
    $tree = New-TsRowsWorkspace
    $made = New-TsRowsDb $tree
    $r = Invoke-TsRowsQ $made.Db ("SELECT f.form, f.inline, m.name AS sits_in FROM functions f " +
        "LEFT JOIN members m ON m.id = f.parent WHERE f.inline = 1")
    Assert-Line $r 'arrow'
    Assert-Line $r 'run'
    # ...and its concise body IS its return, recorded as an implicit one. Without that, every concise arrow
    # returned nothing in the map.
    $ret = Invoke-TsRowsQ $made.Db ("SELECT implicit, source FROM returns WHERE implicit = 1")
    Assert-Line $ret 'n + 1'
    # ...and its contents belong to IT. Walked by the enclosing body as well, the callback's own local is
    # recorded TWICE and the method looks like it declares what it does not.
    $owned = Invoke-TsRowsQ $made.Db ("SELECT count(*) AS rows_for_doubled FROM locals WHERE name = 'doubled'")
    Assert-Line $owned '1'
    $whose = Invoke-TsRowsQ $made.Db ("SELECT f.inline AS declared_by_a_callback FROM locals l " +
        "JOIN functions f ON f.id = l.member WHERE l.name = 'doubled'")
    Assert-Line $whose '1'
}

# A COMPONENT'S TEMPLATE IS A FILE ROW, interned from the `templateUrl` resolved against the .ts file's own
# directory - which is what lets a template and its class be joined at all.
Test-Case 'tsrows: a component resolves its templateUrl to the file row for that template' {
    $tree = New-TsRowsWorkspace
    $made = New-TsRowsDb $tree
    $r = Invoke-TsRowsQ $made.Db ("SELECT c.selector, c.standalone, f.path AS template FROM components c " +
        "JOIN files f ON f.id = c.template_file WHERE c.name = 'BasketComponent'")
    Assert-Line $r 'app-basket'
    Assert-Line $r 'apps/shop/src/basket.component.html'
}

# BOTH SPELLINGS of the stylesheet. Angular 17 added the singular `styleUrl` beside `styleUrls`, and reading
# only the plural dropped the stylesheet of every component written the new way.
Test-Case 'tsrows: a component records its stylesheet under either spelling' {
    $tree = New-TsRowsWorkspace
    $made = New-TsRowsDb $tree
    $r = Invoke-TsRowsQ $made.Db ("SELECT json_extract(style_urls,'`$[0]') AS style FROM components " +
        "WHERE name = 'BasketComponent'")
    Assert-Line $r './basket.scss'
}

# A DIRECTIVE IS NOT A COMPONENT, a pipe is neither, and an injectable says where it is provided.
Test-Case 'tsrows: directives, pipes and injectables are their own rows' {
    $tree = New-TsRowsWorkspace
    $made = New-TsRowsDb $tree
    $d = Invoke-TsRowsQ $made.Db "SELECT name, selector FROM directives"
    Assert-Line $d 'appHighlight'
    $p = Invoke-TsRowsQ $made.Db "SELECT pipe_name, pure FROM pipes"
    Assert-Line $p 'money'
    $i = Invoke-TsRowsQ $made.Db "SELECT name, provided_in FROM injectables"
    Assert-Line $i 'root'
}

# AN ALIAS IS THE NAME A TEMPLATE BINDS TO. Publishing the member name would make every aliased input
# unmatchable from the template side.
Test-Case 'tsrows: an aliased Input binds under its alias, and a required one says so' {
    $tree = New-TsRowsWorkspace
    $made = New-TsRowsDb $tree
    # `binding_name` ALONE: asked beside `alias`, the alias column answers the assertion and a build that
    # binds under the member name passes.
    $r = Invoke-TsRowsQ $made.Db "SELECT binding_name FROM io WHERE member = 'label'"
    Assert-Line $r 'labelAlias'
    $required = Invoke-TsRowsQ $made.Db "SELECT member, required FROM io WHERE required = 1"
    Assert-Line $required 'count'
}

# THE SIGNAL API IS THE SAME FACT IN A DIFFERENT SPELLING: `size = input<number>()` declares an input, and a
# map that only knew the decorator would report the component as having none.
Test-Case 'tsrows: a signal input is an io row, marked as a signal' {
    $tree = New-TsRowsWorkspace
    $made = New-TsRowsDb $tree
    $r = Invoke-TsRowsQ $made.Db "SELECT member, kind, signal FROM io WHERE member = 'size'"
    Assert-Line $r 'input'
    Assert-Line $r 'size'
}

# NGMODULE ARRAYS ARE FLATTENED BY ANGULAR AT ANY DEPTH. `exports: [GROUP]` is one element that IS an array,
# and read without flattening the module reads as exporting NOTHING.
Test-Case 'tsrows: a nested NgModule array is flattened, so the module exports what it really exports' {
    $tree = New-TsRowsWorkspace
    $made = New-TsRowsDb $tree
    $r = Invoke-TsRowsQ $made.Db ("SELECT json_array_length(exports) AS n, " +
        "json_extract(exports,'`$[0].name') AS first FROM ng_modules WHERE name = 'BasketModule'")
    Assert-Line $r '2'
    Assert-Line $r 'HighlightDirective'
}

# THE TEMPLATE IS PARSED BY ANGULAR'S OWN PARSER, so what the map holds is what the compiler compiles -
# including the structural directive desugared into a Template wrapper.
Test-Case 'tsrows: a template becomes nodes, with the tag, the order and the parent of each' {
    $tree = New-TsRowsWorkspace
    $made = New-TsRowsDb $tree
    $r = Invoke-TsRowsQ $made.Db ("SELECT n.kind, n.tag FROM template_nodes n JOIN templates t ON t.id = n.template " +
        "WHERE t.name = 'BasketComponent' ORDER BY n.[order]")
    Assert-Line $r 'Element'
    Assert-Line $r 'app-cart'
    Assert-Line $r 'Template'
}

# AN ELEMENT MATCHING A COMPONENT'S SELECTOR IS A RENDER EDGE - the graph the whole map is built to answer
# questions over.
Test-Case 'tsrows: an element that matches a component selector is a render edge to it' {
    $tree = New-TsRowsWorkspace
    $made = New-TsRowsDb $tree
    $r = Invoke-TsRowsQ $made.Db ("SELECT r.to_name, r.kind, r.tag, r.via FROM renders r WHERE r.tag = 'app-cart'")
    Assert-Line $r 'CartComponent'
    Assert-Line $r 'element'
    # ONCE. `*ngIf` desugars into a synthetic wrapper that KEEPS the pre-desugar tag, so the component
    # selector matches the wrapper as well as the real element - a sizeable share of one map's render rows sat on a
    # wrapper, and an assertion on the name alone passes against every one of them.
    $counted = Invoke-TsRowsQ $made.Db "SELECT count(*) AS edges FROM renders WHERE tag = 'app-cart'"
    Assert-Line $counted '1'
}

# WHY A BINDING IS A GATE, not just whether: `*ngIf` is structural, `[class.x]` is proved by the compiler's
# own binding type, and a declared input is the caller's judgement.
Test-Case 'tsrows: a structural directive, a class binding and an @if block are each gates, labelled' {
    $tree = New-TsRowsWorkspace
    $made = New-TsRowsDb $tree
    $r = Invoke-TsRowsQ $made.Db "SELECT DISTINCT gate_kind FROM gates ORDER BY gate_kind"
    Assert-Line $r 'structural'
    Assert-Line $r 'class'
    Assert-Line $r 'block'
}

# A KEY IS CLAIMED ONLY WHERE THE AST PROVES IT: a literal passed through the carrier pipe. The `class`
# attribute beside it is not a key, and neither is the interpolation.
Test-Case 'tsrows: a literal through the carrier pipe is a translation reference, and nothing else is' {
    $tree = New-TsRowsWorkspace
    $made = New-TsRowsDb $tree
    $r = Invoke-TsRowsQ $made.Db "SELECT key, kind, pipe FROM i18n_refs"
    Assert-Line $r 'shop.title'
    Assert-Line $r 'translate_pipe'
    Assert-NoLine $r 'intro'
}

# A STATIC ATTRIBUTE IS A BINDING ROW TOO, with its value - no pattern decides which values are interesting.
Test-Case 'tsrows: a static attribute and a bound input are both binding rows' {
    $tree = New-TsRowsWorkspace
    $made = New-TsRowsDb $tree
    $r = Invoke-TsRowsQ $made.Db "SELECT kind, name, value, source FROM bindings WHERE name IN ('class','label')"
    Assert-Line $r 'attribute'
    Assert-Line $r 'intro'
    Assert-Line $r 'input'
}

# A NESTED ROUTE'S PARENT IS A ROW, NOT A PREFIX, and its absolute path is composed down that chain -
# `full_path` carries the composition within ONE declaration and would repeat every shared segment.
Test-Case 'tsrows: a nested route carries its parent row and its absolute path' {
    $tree = New-TsRowsWorkspace
    $made = New-TsRowsDb $tree
    $r = Invoke-TsRowsQ $made.Db ("SELECT c.path AS child, p.path AS parent, c.absolute_path FROM routes c " +
        "JOIN routes p ON p.id = c.parent_route")
    Assert-Line $r 'basket'
    Assert-Line $r 'shop/basket'
}

# A ROUTE'S COMPONENT IS A ROW, not a name: `component_id` is what makes the route an ENTRY POINT the reach
# pass can walk from.
Test-Case 'tsrows: a route resolves its component to the class that declares it' {
    $tree = New-TsRowsWorkspace
    $made = New-TsRowsDb $tree
    $r = Invoke-TsRowsQ $made.Db ("SELECT r.path, c.name FROM routes r JOIN classes c ON c.id = r.component_id " +
        "WHERE r.path = 'shop'")
    Assert-Line $r 'CartComponent'
}

# WHICH ENTRY POINTS REACH A COMPONENT - the walk from every routed or bootstrapped class down the render
# edges, which is what says a component is live at all.
Test-Case 'tsrows: a routed component is reachable from an entry point' {
    $tree = New-TsRowsWorkspace
    $made = New-TsRowsDb $tree
    $r = Invoke-TsRowsQ $made.Db ("SELECT name, is_root, root_count FROM component_reach WHERE name = 'CartComponent'")
    Assert-Line $r 'CartComponent'
    # ...and a component nothing renders or routes says so, which is only a claim because the whole
    # workspace was mapped.
    $unused = Invoke-TsRowsQ $made.Db "SELECT name, unused FROM components WHERE name = 'BasketComponent'"
    Assert-Line $unused 'BasketComponent'
}

# A BINDING FILLS A DECLARED INPUT, and the count is what says an input is live. The alias is the name the
# template binds, so the join is on THAT and not on the member.
Test-Case 'tsrows: a template binding is attributed to the input it fills' {
    $tree = New-TsRowsWorkspace
    $made = New-TsRowsDb $tree
    $r = Invoke-TsRowsQ $made.Db ("SELECT u.input, i.member, i.bound_count FROM input_usage u " +
        "JOIN io i ON i.id = u.io WHERE u.input = 'label'")
    Assert-Line $r 'label'
    Assert-Line $r '1'
    # THE ALIAS IS THE NAME A TEMPLATE BINDS. Matched on the member instead, an aliased input is bound by
    # nothing and reads as dead.
    $aliased = Invoke-TsRowsQ $made.Db ("SELECT u.input, i.member FROM input_usage u JOIN io i ON i.id = u.io " +
        "WHERE u.input = 'labelAlias'")
    Assert-Line $aliased 'labelAlias'
}

# THE DISPATCHED NAME IS NOT THE DECLARED KEY: an action group lowercases the event key to build the
# property the application calls, so publishing the raw key made `name` wrong for about a third of the rows.
Test-Case 'tsrows: an action group publishes one row per event, under the dispatched name' {
    $tree = New-TsRowsWorkspace
    $made = New-TsRowsDb $tree
    $r = Invoke-TsRowsQ $made.Db "SELECT name, event, type, [group] FROM state_actions WHERE [group] IS NOT NULL"
    Assert-Line $r 'events.setstep'
    Assert-Line $r 'setStep'
}

# A STRING BUILT FROM A SAME-FILE CONST IS STILL KNOWABLE: the evaluator keeps it as parts and holes, and
# the hole names a const whose value the map already holds - so the fold is a JOIN, not a guess.
Test-Case 'tsrows: an action type built from a template literal is folded to its string' {
    $tree = New-TsRowsWorkspace
    $made = New-TsRowsDb $tree
    $r = Invoke-TsRowsQ $made.Db "SELECT name, type FROM state_actions WHERE name = 'reset'"
    Assert-Line $r '[Shop] reset'
}

# A REDUCER HANDLER JOINS AN ACTION TO THE PATHS IT WRITES - the whole point of the state graph.
Test-Case 'tsrows: a reducer handler records the action it answers and what it writes' {
    $tree = New-TsRowsWorkspace
    $made = New-TsRowsDb $tree
    $r = Invoke-TsRowsQ $made.Db ("SELECT reducer_name, action_source, writes FROM state_handlers")
    Assert-Line $r 'reducer'
    Assert-Line $r 'shopActions.reset'
    Assert-Line $r 'step'
}

# A TREE THAT IS NOT AN ANGULAR WORKSPACE IS NOT A FAILURE: every other repository this exe is deployed into
# is one, and a half that errored there would make --map-sqlite unusable everywhere it is not needed.
Test-Case 'tsrows: a tree with no Angular workspace is a note, never an error' {
    $tree = Use-Tree @{ 'src/app.ts' = "export const x = 1;`n" }
    $db = Join-Path $tree 'map.sqlite'
    $result = Invoke-Gate --root $tree --map-sqlite $db --ts-node-modules $script:TsRowsModules
    Assert-Exit $result 0
    Assert-NoLine $result 'HALF      typescript rows'
}

# ...AND A WORKSPACE THIS HALF CANNOT PARSE IS. A map silently missing every TypeScript row looks exactly
# like a repository with no TypeScript in it.
Test-Case 'tsrows: an Angular workspace with no compiler to borrow is an error, not silence' {
    $tree = New-TsRowsWorkspace
    $db = Join-Path $tree 'map.sqlite'
    $empty = Join-Path $tree 'no_modules'
    [void](New-Item -ItemType Directory -Path $empty -Force)
    $result = Invoke-Gate --root $tree --map-sqlite $db --ts-node-modules $empty
    Assert-Line $result 'typescript'
    Assert-Line $result 'node_modules'
}

}
