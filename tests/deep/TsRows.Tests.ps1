<#
    src\TsRows - the TYPESCRIPT half of the deep map: node parses with the WORKSPACE's own compiler, python
    stores.

    WHAT IS ASSERTED IS A ROW. The half exists so a question about an Angular workspace can be asked of the
    same database that already holds python and C#, so the cases open the database and check the cells,
    exactly as the C# and python deep suites do.

    These need node, npm (once, to cache a compiler) and python. Without any of them the cases are skipped
    with a printed line rather than silently: a suite that quietly runs nothing looks like a suite that
    passed.
#>

# The compiler, the fixture workspace and the query lens are shared with `TsRowsGates` - see the helpers.
. (Join-Path $PSScriptRoot 'TsRows.Helpers.ps1')


if ($script:TsRowsModules -and $script:TsRowsPython) {

Test-Case 'tsrows: a workspace project is a row, with the name, dir and source root it declares' {
    $tree = New-TsRowsWorkspace
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    $r = Invoke-TsRowsQ $made.Db "SELECT name, dir, project_type, source_root FROM projects"
    Assert-Line $r 'shop'
    Assert-Line $r 'apps/shop'
    Assert-Line $r 'application'
    Assert-Line $r 'apps/shop/src'
}

# THE TSCONFIG DECIDES THE FILE SET. A references-only root config yields an EMPTY program, which reads as a
# project with no code in it - so the config the BUILD TARGET names has to win over the one in the folder.
Test-Case 'tsrows: the tsconfig a build target declares is the one the project is read through' {
    $tree = New-TsRowsWorkspace
    $made = New-TsRowsDb $tree
    $r = Invoke-TsRowsQ $made.Db "SELECT tsconfig FROM projects"
    Assert-Line $r 'apps/shop/tsconfig.app.json'
}

# THE SAME RULE WHERE THE ROOT CONFIG WOULD ALSO WORK. Above, the root config names no sources at all, so any
# order of candidates picks the target's - and a run with the order reversed passed. Here the root config
# declares its own `include`, so it is a real alternative and only the ORDER decides.
Test-Case 'tsrows: a usable root tsconfig still loses to the one the build target names' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/tsconfig.json' = '{"include":["src/**/*.ts","src/**/*.d.ts"]}'
    }
    $made = New-TsRowsDb $tree
    $r = Invoke-TsRowsQ $made.Db "SELECT tsconfig FROM projects"
    Assert-Line $r 'apps/shop/tsconfig.app.json'
    Assert-NoLine $r 'apps/shop/tsconfig.json'
}

# THE CONFIGS NOT CHOSEN ARE PART OF THE ANSWER: which one a project is read through decides its whole file
# set, so a wrong pick has to be reviewable rather than invisible. The references-only root config is NOT
# among them - it names no sources, so reading the project through it would yield an empty program.
Test-Case 'tsrows: the tsconfigs that were not chosen are published, and an empty one is not' {
    $tree = New-TsRowsWorkspace
    $made = New-TsRowsDb $tree
    # ONE ROW PER CANDIDATE, not the JSON cell: the query lens truncates a column to its width, so a
    # third candidate appearing at the end of the array was cut off and the assertion below could not see
    # it - the case passed against a build that published the empty config.
    $r = Invoke-TsRowsQ $made.Db "SELECT value FROM projects, json_each(projects.tsconfig_candidates)"
    Assert-Line $r 'apps/shop/tsconfig.app.json'
    Assert-Line $r 'apps/shop/tsconfig.spec.json'
    Assert-NoLine $r 'apps/shop/tsconfig.json'
}

Test-Case 'tsrows: every target the project declares is recorded' {
    $tree = New-TsRowsWorkspace
    $made = New-TsRowsDb $tree
    $r = Invoke-TsRowsQ $made.Db "SELECT targets FROM projects"
    Assert-Line $r 'build'
    Assert-Line $r 'test'
    Assert-Line $r 'lint'
}

# An Nx `project.json` is authoritative where it exists, and the layers are NOT merged: one project
# described twice would become two programs over the same files.
Test-Case 'tsrows: an Nx project.json answers before angular.json, and the layers are not merged' {
    # `stall` exists ONLY in angular.json. Merged, it would be a second project; first-layer-wins, the
    # workspace file is never read at all. Without it, de-duplication by directory hides the merge and a
    # merging build passes.
    $tree = New-TsRowsWorkspace @{
        'apps/shop/project.json' = '{"name":"shop-nx","projectType":"application","sourceRoot":"apps/shop/src",' +
            '"targets":{"build":{"options":{"tsConfig":"apps/shop/tsconfig.app.json"}}}}'
        'angular.json' = '{"projects":{"shop":{"projectType":"application","root":"apps/shop",' +
            '"architect":{"build":{"options":{"tsConfig":"apps/shop/tsconfig.app.json"}}}},' +
            '"stall":{"projectType":"library","root":"libs/stall"}}}'
        'libs/stall/tsconfig.json' = '{"include":["src/**/*.ts"]}'
        'libs/stall/src/index.ts' = "export const stall = 1;`n"
    }
    $made = New-TsRowsDb $tree
    $r = Invoke-TsRowsQ $made.Db "SELECT count(*) AS n, group_concat(name) AS names FROM projects"
    Assert-Line $r 'shop-nx'
    Assert-Line $r '1'
    Assert-NoLine $r 'stall'
}

# THE HALF REPLACES ITS OWN ROWS, WHOLE. A half that only ever added them would answer with two rows for one
# project after the second run - and rows that are merely duplicated look real.
Test-Case 'tsrows: a second run replaces this half rows rather than adding to them' {
    $tree = New-TsRowsWorkspace
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    $again = Invoke-Gate --root $tree --map-sqlite $made.Db --ts-node-modules $script:TsRowsModules
    Assert-Exit $again 0
    $r = Invoke-TsRowsQ $made.Db "SELECT count(*) FROM projects"
    Assert-Line $r '1'
}

# A FILE THE PROGRAM CONTAINS is `core`, and its size comes from the COMPILER's own view of it: `chars` is
# the length of the text TypeScript decoded, which is not the byte count on disk.
Test-Case 'tsrows: a program source file is a core row with its lines and chars' {
    $tree = New-TsRowsWorkspace
    $made = New-TsRowsDb $tree
    $r = Invoke-TsRowsQ $made.Db "SELECT tier, parsed, reachable, lines, chars FROM files WHERE path = 'apps/shop/src/main.ts'"
    Assert-Line $r 'core'
    Assert-Line $r '1'
}

# EVERY FILE IN SCOPE GETS A ROW, parsed or not. Without the inventory walk a consumer cannot tell a file
# that is absent from one that was never opened - a .scss is in neither program nor template.
Test-Case 'tsrows: a file no program contains is still a row, as unparsed inventory' {
    $tree = New-TsRowsWorkspace
    $made = New-TsRowsDb $tree
    $r = Invoke-TsRowsQ $made.Db "SELECT tier, parsed, reachable, content_hash FROM files WHERE path = 'apps/shop/src/theme.scss'"
    Assert-Line $r 'inventory'
    # `reachable` is null for a file that is not a program concept at all, and the digest is still taken:
    # "its lines cannot be counted" is no reason to leave a file without content identity.
    Assert-NoLine $r 'inventory   1'
}

# REACHABILITY COMES FROM THE PROGRAM. A .ts in the folder that no tsconfig includes is dead code the build
# never ships, and that is a finding rather than a missing row.
Test-Case 'tsrows: a .ts in the tree that no program contains is recorded as unreachable' {
    $tree = New-TsRowsWorkspace
    $made = New-TsRowsDb $tree
    $r = Invoke-TsRowsQ $made.Db "SELECT path, reachable FROM files WHERE reachable = 0"
    Assert-Line $r 'apps/shop/extra/dead.ts'
    Assert-NoLine $r 'apps/shop/src/main.ts'
}

# THE KEYS A LOCALE FILE DEFINES, flattened to the DOTTED PATH the code uses - which is what an i18n
# reference holds, and the whole point is that the two join.
Test-Case 'tsrows: a translation file becomes rows, keyed by the dotted path' {
    $tree = New-TsRowsWorkspace
    $made = New-TsRowsDb $tree
    $r = Invoke-TsRowsQ $made.Db "SELECT locale, key, text FROM translations ORDER BY key, locale"
    Assert-Line $r 'shop.cart.empty'
    Assert-Line $r 'Empty'
    Assert-Line $r 'de'
    # ONLY the named directories. `angular.json` and the tsconfigs are .json files too, and a half that
    # claimed every one of them would publish a workspace file's keys as translations - which is exactly
    # what "nothing here sniffs a file to decide whether it looks like translations" is about.
    $all = Invoke-TsRowsQ $made.Db "SELECT DISTINCT locale FROM translations ORDER BY locale"
    Assert-Line $all 'en'
    Assert-NoLine $all 'angular'
    Assert-NoLine $all 'tsconfig'
}

# ...AND THE FILE STOPS READING AS NEVER OPENED. A locale file is an inventory row until something parses
# it, and `keys` is what says how much it carries.
Test-Case 'tsrows: the locale file itself records its locale and how many keys it defines' {
    $tree = New-TsRowsWorkspace
    $made = New-TsRowsDb $tree
    # Two leaves: `shop.title` and `shop.cart.empty`. A nested object is not a key, its leaves are.
    $r = Invoke-TsRowsQ $made.Db ("SELECT locale || ' | ' || keys AS locale_keys, parsed FROM files " +
        "WHERE path LIKE '%locales/en.json'")
    Assert-Line $r 'en | 2'
}

# WHICH FILES ARE TRANSLATIONS IS THE CALLER'S CALL. With no config naming the directories, nothing is
# claimed - the same .json is an ordinary inventory row.
Test-Case 'tsrows: no locale directories named means no translation rows are claimed' {
    $tree = New-TsRowsWorkspace @{ 'structuregate.ts.json' = '{}' }
    $made = New-TsRowsDb $tree
    # The table is not there at all - a table with no rows is never created - and the .json is an ordinary
    # unparsed inventory row, carrying no locale.
    $r = Invoke-TsRowsQ $made.Db "SELECT count(*) AS translation_tables FROM sqlite_master WHERE name = 'translations'"
    Assert-Line $r '0'
    $f = Invoke-TsRowsQ $made.Db "SELECT parsed, tier FROM files WHERE path LIKE '%locales/en.json'"
    Assert-Line $f 'inventory'
}

# A SPECIFIER IS A STRING; WHICH FILE IT IS is the compiler's answer. Without `resolved`/`resolved_file` a
# consumer asking "who uses this class" would have to re-implement module resolution.
Test-Case 'tsrows: an import records the file the compiler resolved it to' {
    $tree = New-TsRowsWorkspace
    $made = New-TsRowsDb $tree
    $r = Invoke-TsRowsQ $made.Db ("SELECT i.module, i.external, i.resolved, f.path AS resolved_path " +
        "FROM imports i LEFT JOIN files f ON f.id = i.resolved_file WHERE i.module = './helpers'")
    Assert-Line $r 'apps/shop/src/helpers.ts'
    Assert-Line $r './helpers'
}

# THE NAMES, PARSED - not the statement's text. A rename, a default and a namespace are three facts, and a
# consumer should not have to re-parse the clause to tell them apart.
Test-Case 'tsrows: an import records its names, the kind of each, and the name a rename came through' {
    $tree = New-TsRowsWorkspace
    $made = New-TsRowsDb $tree
    $r = Invoke-TsRowsQ $made.Db ("SELECT json_extract(n.value,'$.name') AS name, " +
        "json_extract(n.value,'$.kind') AS kind, json_extract(n.value,'$.as') AS renamed_from " +
        "FROM imports i, json_each(i.names) n ORDER BY name")
    Assert-Line $r 'Basket'
    Assert-Line $r 'Cart'
    Assert-Line $r 'namespace'
    Assert-Line $r 'default'
    # THE DIRECTION, which the lines above cannot see - both names appear either way round. `name` is the
    # LOCAL binding and `as` the name the rename came through: the old tool's contract, read by consumers.
    $dir = Invoke-TsRowsQ $made.Db ("SELECT json_extract(n.value,'$.name') || '<-' || json_extract(n.value,'$.as') AS d " +
        "FROM imports i, json_each(i.names) n WHERE json_extract(n.value,'$.as') IS NOT NULL")
    Assert-Line $dir 'Basket<-Cart'
}

# A TYPE-ONLY IMPORT IS ERASED AT RUNTIME. It is still an edge in the map, and a consumer asking what the
# build actually loads needs to tell the two apart.
Test-Case 'tsrows: a type-only import is recorded as one' {
    $tree = New-TsRowsWorkspace
    $made = New-TsRowsDb $tree
    $r = Invoke-TsRowsQ $made.Db "SELECT module, type_only FROM imports WHERE type_only = 1"
    Assert-Line $r './cart'
}

# `external` IS ABOUT THE SPELLING, and `resolved` about the file: a tsconfig path alias is how this kind of
# monorepo imports its own libraries, so "not relative" does not mean "not ours".
Test-Case 'tsrows: a bare specifier is external even when it resolves inside the workspace' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/main.ts' = "import { Cart } from '@shop/cart';`nexport const started = true;`n"
        'apps/shop/tsconfig.app.json' = '{"compilerOptions":{"baseUrl":".","paths":{"@shop/*":["src/*"]}},' +
            '"include":["src/**/*.ts"]}'
    }
    $made = New-TsRowsDb $tree
    $r = Invoke-TsRowsQ $made.Db ("SELECT i.external, i.resolved FROM imports i WHERE i.module = '@shop/cart'")
    Assert-Line $r 'apps/shop/src/cart.ts'
    # This fixture's only import is the aliased one, so a build that decided `external` from whether the
    # specifier RESOLVED would report one internal import here and none is what the rule says.
    # Asked of THIS specifier: a fixture that later grew a relative import would otherwise answer about that
    # one instead, and the rule under test is about the aliased spelling.
    $counted = Invoke-TsRowsQ $made.Db ("SELECT count(*) AS internal FROM imports " +
        "WHERE module = '@shop/cart' AND external = 0")
    Assert-Line $counted '0'
}

# LEADING TRIVIA OF TOP-LEVEL STATEMENTS IS NOT ENOUGH. The comments that explain business rules sit
# inside method bodies, and a statement-level pass finds ZERO of them there.
Test-Case 'tsrows: a comment inside a method body is a row, in the declaration it sits in' {
    $tree = New-TsRowsWorkspace
    $made = New-TsRowsDb $tree
    $r = Invoke-TsRowsQ $made.Db "SELECT kind, context, text FROM comments ORDER BY line"
    Assert-Line $r 'Greeter.greet'
    Assert-Line $r 'a business rule lives here'
    Assert-Line $r 'block'
    Assert-Line $r 'line'
}

# THE NAMES, PARSED. `text` alone made this table unjoinable: learning that `export { Greeter as Welcomer }`
# is what makes that class public meant pattern-matching the source line.
Test-Case 'tsrows: an export records the local name, the exported name and the file it re-exports from' {
    $tree = New-TsRowsWorkspace
    $made = New-TsRowsDb $tree
    $r = Invoke-TsRowsQ $made.Db "SELECT names, exported_as, [from] FROM exports ORDER BY line"
    Assert-Line $r 'Greeter'
    Assert-Line $r 'Welcomer'
    Assert-Line $r './helpers'
}

# A BARE STRING SAYS NOTHING. `'laptop'` as the value of a property in an object literal is a different fact
# from `'laptop'` passed to a call, and `context`/`property` are what publish it.
Test-Case 'tsrows: a string literal records the kind of node it sits in' {
    $tree = New-TsRowsWorkspace
    $made = New-TsRowsDb $tree
    $r = Invoke-TsRowsQ $made.Db "SELECT value, context, property FROM string_literals WHERE value = 'laptop'"
    Assert-Line $r 'PropertyAssignment'
}

# A COMPUTED KEY IS ITS VALUE. `[Category.Hardware]: 'laptop'` builds the property `1`, and publishing the
# source text would make every consumer resolve the enum for itself - which it cannot do without a compiler.
Test-Case 'tsrows: a computed enum key is published as the value it folds to' {
    $tree = New-TsRowsWorkspace
    $made = New-TsRowsDb $tree
    $r = Invoke-TsRowsQ $made.Db "SELECT property FROM string_literals WHERE value = 'laptop'"
    Assert-Line $r '1'
    Assert-NoLine $r 'Category.Hardware'
}

# AN ENUM MEMBER CARRIES THE VALUE THE COMPILER FOLDED, including the one it inherits by position. Publishing
# the initializer alone would leave `High` with nothing, and a consumer cannot fold it without a compiler.
Test-Case 'tsrows: an enum member carries the value the compiler folded, inherited numbering included' {
    $tree = New-TsRowsWorkspace
    $made = New-TsRowsDb $tree
    # ONE MEMBER, ONE VALUE. Asked over every member at once, a query's output holds the other members'
    # numbers too, and an assertion on `2` passed against a build that published nothing for `High`.
    $r = Invoke-TsRowsQ $made.Db ("SELECT json_extract(m.value,'$.value') AS high_value FROM enums e, " +
        "json_each(e.members) m WHERE json_extract(m.value,'$.name') = 'High'")
    Assert-Line $r '2'
    $s = Invoke-TsRowsQ $made.Db ("SELECT json_extract(m.value,'$.value') AS on_value FROM enums e, " +
        "json_each(e.members) m WHERE json_extract(m.value,'$.name') = 'On'")
    Assert-Line $s 'on'
}

Test-Case 'tsrows: a const enum is recorded as one' {
    $tree = New-TsRowsWorkspace
    $made = New-TsRowsDb $tree
    $r = Invoke-TsRowsQ $made.Db "SELECT name, [const] FROM enums ORDER BY name"
    Assert-Line $r 'Flag'
    Assert-Line $r 'Level'
}

# A NESTED MEMBER IS ADDRESSABLE. The summary on the declaration row stops at the first level, so a read that
# lands on `cell.label` pointed at a file:line with no row to join to.
Test-Case 'tsrows: a nested type member is its own row, with the dotted path and its parent' {
    $tree = New-TsRowsWorkspace
    $made = New-TsRowsDb $tree
    $r = Invoke-TsRowsQ $made.Db ("SELECT t.path, t.optional, p.path AS parent_path FROM type_members t " +
        "LEFT JOIN type_members p ON p.id = t.parent WHERE t.path LIKE 'cell%' ORDER BY t.path")
    Assert-Line $r 'cell.label'
    Assert-Line $r 'cell'
    # AN ANONYMOUS TYPE HAS NO IDENTITY TO CLAIM. The compiler calls its symbol `__type`, which is a
    # placeholder and not a name a consumer can do anything with, so the member resolves to nothing.
    $ref = Invoke-TsRowsQ $made.Db ("SELECT json_extract(type_ref,'$.name') AS ref_name FROM type_members " +
        "WHERE path = 'cell'")
    Assert-NoLine $ref '__'
}

# THE SECOND HOP. A member's type as TEXT is where a walk stops: `Row[]` has the name and no way to reach the
# declaration.
Test-Case 'tsrows: a type member resolves its own type to the declaration it names' {
    $tree = New-TsRowsWorkspace
    $made = New-TsRowsDb $tree
    # `type_ref` ALONE. Selected beside `type`, the text `Row[]` satisfies an assertion on `Row` and a build
    # resolving nothing passes.
    # `rows: Row[]` resolves to `Array`, and the declared type a consumer is after is its ELEMENT. Both are
    # published, because "an array of Row" and "Row" are different answers and the consumer picks.
    $r = Invoke-TsRowsQ $made.Db ("SELECT json_extract(type_ref,'$.name') AS ref_name, " +
        "json_extract(type_ref,'$.element.name') AS element_name FROM type_members WHERE path = 'rows'")
    Assert-Line $r 'Row'
}

# A COMPUTED TYPE HAS MEMBERS TOO. `Pick<Row,'id'>` declares no syntax to walk, so an alias built that way
# contributed nothing and a read landing on it stopped dead.
Test-Case 'tsrows: an alias built from a mapped type still publishes its members, marked as computed' {
    $tree = New-TsRowsWorkspace
    $made = New-TsRowsDb $tree
    $r = Invoke-TsRowsQ $made.Db ("SELECT a.name, t.name AS member, t.via FROM type_members t " +
        "JOIN type_aliases a ON a.id = t.owner WHERE a.name = 'Slim'")
    Assert-Line $r 'computed'
    Assert-Line $r 'id'
}

# AN ARRAY IS NOT AN OBJECT SURFACE. `type RowList = Row[]` asked for its properties answers with `length`,
# `push` and the rest of `Array` from lib.es5.d.ts - true of the VALUE, silent about the alias, and hundreds of rows
# of a real map pointed into node_modules because of it. Detected by the number index signature every array
# has, so no type is recognised by name.
Test-Case 'tsrows: an alias of an array publishes no members of its own' {
    $tree = New-TsRowsWorkspace
    $made = New-TsRowsDb $tree
    $r = Invoke-TsRowsQ $made.Db ("SELECT count(*) AS members FROM type_members t JOIN type_aliases a " +
        "ON a.id = t.owner WHERE a.name = 'RowList'")
    Assert-Line $r '0'
}

# AN ENUM MEMBER KEEPS BOTH HALVES. Returning the bare number turned `[TierIDs.A, TierIDs.B]` into
# `[1, 2]` with no record of WHICH members those were.
Test-Case 'tsrows: a const referring to an enum member keeps the member AND its value' {
    $tree = New-TsRowsWorkspace
    $made = New-TsRowsDb $tree
    $r = Invoke-TsRowsQ $made.Db ("SELECT json_extract(value,'$.`$enum') AS member, " +
        "json_extract(value,'$.value') AS folded FROM consts WHERE name = 'LEVEL'")
    Assert-Line $r 'Category.Hardware'
    Assert-Line $r '1'
}

# A CLASS OF STATIC FIELDS IS A CONST MAP; AN INSTANCE FIELD IS NOT. An instance field's initializer is what
# the object STARTS with, and resolving it would assert runtime state the map cannot know.
Test-Case 'tsrows: a static class field resolves, and an instance field deliberately does not' {
    $tree = New-TsRowsWorkspace
    $made = New-TsRowsDb $tree
    $static = Invoke-TsRowsQ $made.Db ("SELECT json_extract(value,'$.value') AS folded FROM consts " +
        "WHERE name = 'FLAG'")
    Assert-Line $static 'IsXEnabled'
    $instance = Invoke-TsRowsQ $made.Db ("SELECT json_extract(value,'$.value') AS folded, " +
        "json_extract(value,'$.`$expr') AS text FROM consts WHERE name = 'ONLINE'")
    Assert-Line $instance 'new Runtime().isOnline'
    Assert-NoLine $instance 'false'
}

# A SPREAD MUST BE FLATTENED. `declarations: [...COMPONENTS]` is how a module lists what it declares, and
# leaving it opaque made hundreds of declared components invisible.
Test-Case 'tsrows: a spread of a known array is flattened into the value' {
    $tree = New-TsRowsWorkspace
    $made = New-TsRowsDb $tree
    # THREE elements, the last of them 3: an unflattened spread would be one opaque element plus 3.
    $r = Invoke-TsRowsQ $made.Db ("SELECT json_array_length(value) AS n, json_extract(value,'`$[2]') AS last " +
        "FROM consts WHERE name = 'ALL'")
    Assert-Line $r '3'
    $first = Invoke-TsRowsQ $made.Db "SELECT json_extract(value,'`$[0]') AS first FROM consts WHERE name = 'ALL'"
    Assert-Line $first '1'
}

# A VALUE BEHIND A CONDITION IS NEVER UNCONDITIONAL. Collapsing `flag && {...}` to the object would assert
# the object is always there - the same mistake as an empty guard list reading "unguarded".
Test-Case 'tsrows: a conditional value keeps the operator and both operands' {
    $tree = New-TsRowsWorkspace
    $made = New-TsRowsDb $tree
    $r = Invoke-TsRowsQ $made.Db ("SELECT json_extract(value,'$.`$logic') AS op, " +
        "json_extract(value,'$.`$operands[1].id') AS then_id FROM consts WHERE name = 'MAYBE'")
    Assert-Line $r '&&'
    Assert-Line $r '1'
}

# AN UNEVALUABLE VALUE IS STILL A STRUCTURED EXPRESSION. The text alone forces every consumer to
# pattern-match it; `reads`/`identifiers` make the same fact joinable.
Test-Case 'tsrows: a value that cannot be known still publishes what it reads' {
    $tree = New-TsRowsWorkspace
    $made = New-TsRowsDb $tree
    $text = Invoke-TsRowsQ $made.Db ("SELECT json_extract(value,'$.`$expr') AS text FROM consts " +
        "WHERE name = 'DYNAMIC'")
    Assert-Line $text 'window.location.href'
    # THE SUMMARY, ON ITS OWN. Asked beside the text, the text answers the assertion and a build that
    # publishes no summary at all passes - which is how this case first went green against one.
    $reads = Invoke-TsRowsQ $made.Db ("SELECT json_extract(value,'$.`$reads[0]') AS first_read FROM consts " +
        "WHERE name = 'DYNAMIC'")
    Assert-Line $reads 'window.location.href'
}


}
