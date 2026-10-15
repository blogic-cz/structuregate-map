<#
    The template nodes and expressions LATER compilers build: `@let` (v18.1), `@for` variables as an array (v18),
    `@defer` hydrate triggers (v19), assignment as a `Binary` and the new expression kinds (v20), `@switch` groups
    (v21) and fall-through cases (v22). The shared helpers pin `@angular/compiler@17`, which parses none of them,
    so this suite borrows a compiler of the current major - the half parses with the WORKSPACE's compiler, and a
    consumer on a later one gets these nodes. Read in `src/TsRows/TsTpl/TsTplExtract.mjs` and `TsTplExpr.mjs`.
    Its helpers are `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '../../TsRows.Helpers.ps1')

# THE LATER COMPILER, in a prefix of its own beside the pinned one.
function Get-TsBlocksModules {
    if ($null -ne $script:TsBlocksCache) { return $script:TsBlocksCache.Path }
    $script:TsBlocksCache = @{ Path = $null }
    if (-not $script:TsRowsModules) { return $null }
    $cache = Join-Path ([System.IO.Path]::GetTempPath()) 'sgtest-tsrows-next'
    $modules = Join-Path $cache 'node_modules'
    $ready = { (Test-Path (Join-Path $modules 'typescript\package.json')) -and
        (Test-Path (Join-Path $modules '@angular\compiler\package.json')) -and
        (Test-Path (Join-Path $modules '@angular\core\package.json')) }
    if (-not (& $ready)) {
        if (-not (Get-Command npm -ErrorAction SilentlyContinue)) { return $null }
        [void](New-Item -ItemType Directory -Path $cache -Force)
        & npm install --no-save --silent --prefix $cache 'typescript@5' '@angular/compiler@22' '@angular/core@22' 2>&1 | Out-Null
    }
    if (& $ready) { $script:TsBlocksCache.Path = $modules }
    return $script:TsBlocksCache.Path
}

$script:TsBlocksModules = Get-TsBlocksModules
if ($script:TsRowsModules -and -not $script:TsBlocksModules) {
    Write-Host '    (no @angular/compiler@22 could be installed - the later-template cases are not run)'
}

if ($script:TsBlocksModules -and $script:TsRowsPython) {

# ONE TEMPLATE, every later shape in it, mapped once per case through the later compiler.
function New-TsBlocksDb {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/blocks.component.html' = "@let total = price + tax;`n" +
            "<b>{{ total }}</b>`n" +
            "@switch (mode) { @case ('a') { <i>{{ onlyA }}</i> } @case ('b') @case ('c') { <u>{{ bOrC }}</u> } " +
            "@default { <s>d</s> } }`n" +
            "@for (it of items; track it.id; let i = `$index) { <em>{{ i }}</em><em>{{ it.done }}</em> }`n" +
            "@if (user; as u) { <dfn>{{ u.name }}</dfn> }`n" +
            "<div #anchor></div>`n" +
            "@defer (when ready; prefetch when soon; on viewport(anchor)) { <p>late</p> } @placeholder { <q>wait</q> }`n" +
            "<span>{n, plural, =0 {none} other {{{ icuInner }} many}}</span>`n" +
            "<a>{{ -neg }}</a><a>{{ (paren) }}</a><a>{{ items.filter((x) => x.done) }}</a>" +
            "<a>{{ ``t`${tpl}`` }}</a><a>{{ typeof tp }}</a>`n" +
            "<button (click)=`"clicked = 1`"></button>`n"
        'apps/shop/src/blocks.component.ts' = "import { Component } from '@angular/core';`n" +
            "export interface Line { id: number; done: boolean; }`n" +
            "@Component({ selector: 'demo-blocks', templateUrl: './blocks.component.html', standalone: true })`n" +
            "export class BlocksComponent {`n" +
            "  price = 1; tax = 2; mode = 'a'; onlyA = 1; bOrC = 2; items: Line[] = [];`n" +
            "  user = { name: 'n' }; it: Line = { id: 0, done: false }; u = { name: 'm' }; total = 0;`n" +
            "  ready = false; soon = false; n = 0; icuInner = 3; neg = 4; paren = 5; tpl = 6; tp = 7; clicked = 0; x = 8;`n" +
            "}`n"
    }
    $db = Join-Path $tree 'map.sqlite'
    $result = Invoke-Gate --root $tree --map-sqlite $db --ts-node-modules $script:TsBlocksModules `
        --ts-config (Join-Path $tree 'structuregate.ts.json')
    Assert-Exit $result 0
    return $db
}

# The sources of the gates every node with `tag` sits under, outermost first.
$script:TsBlocksChainSql = "SELECT n.tag || ' <- ' || coalesce(group_concat(g.name || ':' || g.source, ' / ' ORDER BY j.ord), '(none)') " +
    "AS chain FROM template_nodes n LEFT JOIN template_nodes__gate_chain j ON j.owner = n.id AND j.parent = n.id || '/0' " +
    "LEFT JOIN gates g ON g.id = j.value " +
    "WHERE n.tag = '{0}' GROUP BY n.id"

# FROM v21 A CASE HOLDS NO CHILDREN: its gate sat on a leaf beside the content and gated nothing. A one-case
# group's content sits under its case again; a FALL-THROUGH group's content is under neither alternative, because
# a chain of gates means all of them hold.
Test-Case 'tsrows: an @switch case gates its content on a compiler that groups cases, and a fall-through group does not' {
    $db = New-TsBlocksDb
    $one = Invoke-TsRowsQ $db ($script:TsBlocksChainSql -f 'i')
    Assert-Line $one "i <- SwitchBlock:mode / SwitchBlockCase:'a'"
    $both = Invoke-TsRowsQ $db ($script:TsBlocksChainSql -f 'u')
    Assert-Line $both 'u <- SwitchBlock:mode'
    Assert-NoLine $both "SwitchBlockCase:'b'"
    Assert-NoLine $both "SwitchBlockCase:'c'"
}

# `@let` HOLDS ITS EXPRESSION IN `value`, where the block branch read `expression`: no row, no reads. It names a
# value and decides nothing, so it is no gate.
Test-Case 'tsrows: an @let declaration publishes its expression and its reads, and is no gate' {
    $db = New-TsBlocksDb
    $r = Invoke-TsRowsQ $db ("SELECT x.name || ' = ' || x.source || ' reads ' || $(Get-NodeRootListSql 'expressions__reads' 'x.id') AS let FROM expressions x " +
        "JOIN template_nodes n ON n.id = x.node WHERE n.kind = 'LetDeclaration' AND n.name = 'total'")
    Assert-Line $r 'total = price + tax reads ["price","tax"]'
    $g = Invoke-TsRowsQ $db "SELECT count(*) AS let_gates FROM gates WHERE name IN ('LetDeclaration', 'total')"
    Assert-Line $g '0'
}

# FROM v18 `contextVariables` IS AN ARRAY: `Object.keys` published its indices. The alias is a row field too.
Test-Case 'tsrows: an @for block names its context variables and the aliases the author gave them' {
    $db = New-TsBlocksDb
    $r = Invoke-TsRowsQ $db "SELECT $(Get-NodeRootListSql 'template_nodes__context_vars' 'template_nodes.id') AS vars FROM template_nodes WHERE kind = 'ForLoopBlock'"
    Assert-Line $r '["$index","$first","$last","$even","$odd","$count"]'
    $a = Invoke-TsRowsQ $db "SELECT $(Get-NodeObjectListSql 'template_nodes__context_aliases' 'template_nodes.id' @('name', 'value')) AS aliases FROM template_nodes WHERE kind = 'ForLoopBlock'"
    Assert-Line $a '[{"name":"i","value":"$index"}]'
}

# A `when` CONDITION IS AN EXPRESSION, per phase, and NOT a gate: the placeholder beside the main content shows
# exactly while it is false. The element a trigger watches is named.
Test-Case 'tsrows: an @defer block publishes its when conditions and trigger references, and gates no placeholder' {
    $db = New-TsBlocksDb
    $r = Invoke-TsRowsQ $db "SELECT kind || ' ' || source || ' ' || $(Get-NodeRootListSql 'expressions__reads' 'expressions.id') AS cond FROM expressions WHERE kind LIKE 'defer_%'"
    Assert-Line $r 'defer_when ready ["ready"]'
    Assert-Line $r 'defer_prefetch_when soon ["soon"]'
    $t = Invoke-TsRowsQ $db "SELECT $(Get-NodeObjectListSql 'template_nodes__trigger_refs' 'template_nodes.id' @('phase', 'trigger', 'reference')) || ' ' || $(Get-NodeRootListSql 'template_nodes__hydrate_triggers' 'template_nodes.id') AS t FROM template_nodes WHERE kind = 'DeferredBlock'"
    Assert-Line $t '[{"phase":"main","trigger":"viewport","reference":"anchor"}] []'
    Assert-Line (Invoke-TsRowsQ $db ($script:TsBlocksChainSql -f 'q')) 'q <- (none)'
}

# AN ICU KEEPS ITS SWITCH VALUE AND ITS INTERPOLATIONS IN RECORDS, `vars` and `placeholders`, which the
# shape walk did not look into: nothing inside a plural produced a row.
Test-Case 'tsrows: an ICU publishes its switch value and the interpolations inside its cases' {
    $db = New-TsBlocksDb
    $r = Invoke-TsRowsQ $db ("SELECT $(Get-NodeRootListSql 'expressions__reads' 'expressions.id') AS reads FROM expressions WHERE kind = 'interpolation' " +
        "AND ($(Get-NodeRootListSql 'expressions__reads' 'expressions.id') LIKE '%icuInner%' OR $(Get-NodeRootListSql 'expressions__reads' 'expressions.id') = '[`"n`"]')")
    Assert-Line $r '["icuInner"]'
    Assert-Line $r '["n"]'
}

# The expression rows of one interpolation, `source => reads`, and the kind of its first expression.
function Get-TsBlocksExpr([string]$Db, [string]$Source) {
    return Invoke-TsRowsQ $Db ("SELECT source || ' => ' || $(Get-NodeRootListSql 'expressions__reads' 'x.id') || ' ' || " +
        "(SELECT e0.k FROM expressions__ast a JOIN expressions__ast xs ON xs.parent = a.id AND xs.role = 'expressions' JOIN expressions__ast e0 ON e0.parent = xs.id AND e0.ord = 0 WHERE a.parent = x.id || '/0' AND a.role = 'ast') " +
        "|| coalesce(' ' || (SELECT e0.op FROM expressions__ast a JOIN expressions__ast xs ON xs.parent = a.id AND xs.role = 'expressions' JOIN expressions__ast e0 ON e0.parent = xs.id AND e0.ord = 0 WHERE a.parent = x.id || '/0' AND a.role = 'ast'), '') AS e FROM expressions x WHERE source = '$Source'")
}

# `Unary` EXTENDS `Binary`: matched as one, `-a` kept neither operand and its read was lost.
Test-Case 'tsrows: a unary minus is a Unary and reads its operand' {
    $db = New-TsBlocksDb
    Assert-Line (Get-TsBlocksExpr $db '{{ -neg }}') '{{ -neg }} => ["neg"] Unary -'
}

# FROM v20 PARENTHESES ARE A NODE; the TypeScript side drops them, so the template side does too.
Test-Case 'tsrows: parentheses leave no node of their own' {
    $db = New-TsBlocksDb
    Assert-Line (Get-TsBlocksExpr $db '{{ (paren) }}') '{{ (paren) }} => ["paren"] Read'
}

# AN ARROW FUNCTION'S PARAMETER IS NO MEMBER READ: `x.done` reads the argument, and resolving it would answer
# with the component's own `x`.
Test-Case 'tsrows: an arrow function is an Fn and its parameter is no read' {
    $db = New-TsBlocksDb
    # the first expression's first argument is the arrow: its kind, and the names it takes
    $fn = "FROM expressions__ast a JOIN expressions__ast xs ON xs.parent = a.id AND xs.role = 'expressions' " +
        "JOIN expressions__ast e0 ON e0.parent = xs.id AND e0.ord = 0 JOIN expressions__ast al ON al.parent = e0.id AND al.role = 'args' " +
        "JOIN expressions__ast f ON f.parent = al.id AND f.ord = 0 WHERE a.parent = x.id || '/0' AND a.role = 'ast'"
    $params = Get-NodeTextSql 'expressions__ast' "zl.parent = (SELECT f.id $fn) AND zl.role = 'params'"
    $r = Invoke-TsRowsQ $db ("SELECT $(Get-NodeRootListSql 'expressions__reads' 'x.id') || ' ' || (SELECT f.k $fn) || ' ' || $params AS e FROM expressions x " +
        "WHERE source = '{{ items.filter((x) => x.done) }}'")
    Assert-Line $r '["items.filter","items"] Fn ["x"]'
}

# A template literal and `typeof` in the TypeScript side's vocabulary: an `Interpolation` and a `Unary`.
Test-Case 'tsrows: a template literal is an Interpolation and typeof a Unary' {
    $db = New-TsBlocksDb
    Assert-Line (Get-TsBlocksExpr $db "{{ ``t`${tpl}`` }}") '=> ["tpl"] Interpolation'
    Assert-Line (Get-TsBlocksExpr $db '{{ typeof tp }}') '{{ typeof tp }} => ["tp"] Unary typeof'
}

# FROM v20 AN ASSIGNMENT IS A `Binary`: published as the `Write` it was before, so its target is no read.
Test-Case 'tsrows: an assignment in an event handler is a Write and reads nothing' {
    $db = New-TsBlocksDb
    $w = Invoke-TsRowsQ $db ("SELECT a.k || ' ' || a.name || ' ' || $(Get-NodeRootListSql 'expressions__reads' 'x.id') AS w FROM expressions x " +
        "JOIN expressions__ast a ON a.parent = x.id || '/0' AND a.role = 'ast' WHERE x.source = 'clicked = 1'")
    Assert-Line $w 'Write clicked []'
}


# The read targets an expression's `Read` nodes resolved to, `name@row`, by its source.
function Get-TsBlocksTargets([string]$Db, [string]$Source) {
    return Invoke-TsRowsQ $Db ("SELECT j.name || '@' || j.[row] AS t " +
        "FROM expressions x JOIN expressions__ast j ON j.owner = x.id AND j.role = 'target' WHERE x.source = '$Source'")
}

# A NAME A BLOCK BINDS IS THE TEMPLATE'S, not the component member it shadows. The component declares `it`, `u` and
# `total` too; a read of the block's name resolved to that member. The `@for` item is typed from its collection,
# as an `*ngFor` variable is, so `it.done` lands on `Line.done`.
Test-Case 'tsrows: a name an @for, @if or @let binds shadows the component member of that name' {
    $db = New-TsBlocksDb
    $it = Get-TsBlocksTargets $db '{{ it.done }}'
    Assert-Line $it 'done@'
    Assert-NoLine $it 'it@'
    $line = Invoke-TsRowsQ $db ("SELECT 'line_done=' || count(*) AS hit FROM expressions x JOIN expressions__ast j ON j.owner = x.id AND j.role = 'target' " +
        "JOIN type_members t ON t.id = j.[row] JOIN interfaces i ON i.id = t.owner " +
        "WHERE x.source = '{{ it.done }}' AND i.name = 'Line' AND t.name = 'done'")
    Assert-Line $line 'line_done=1'
    Assert-NoLine (Get-TsBlocksTargets $db '{{ u.name }}') 'u@'
    Assert-NoLine (Get-TsBlocksTargets $db '{{ total }}') 'total@'
}

}
