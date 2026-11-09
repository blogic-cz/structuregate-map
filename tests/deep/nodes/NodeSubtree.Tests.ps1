<#
    --map-query --subtree: one node's whole subtree, read as the range n..last of its owner.
    --ancestors, --children, --siblings and --descendants-of-kind read the same key the other ways.

    The fixture's call `f(a.b, [1, 2])` has a python `expressions__shape` tree whose second child (`a.b`, an
    Attribute holding a Name) ends before a third sibling (the List) starts - so a range one node too long,
    or a depth not taken from the start, prints a line these cases refuse.
#>

$script:SubtreePython = [bool]$script:Python
if (-not $script:SubtreePython) { Write-Host '    (no python on this machine - the --subtree cases are not run)' }

# The deep map of one call, and the owner of that call's expression row.
function New-SubtreeMap {
    $tree = Use-Tree @{ 'a.py' = "def go(a):" + [char]10 + "    return f(a.b, [1, 2])" + [char]10 }
    $db = Join-Path $tree 'map.sqlite'
    Assert-Exit (Invoke-Gate --root $tree --ext .py --map --map-out (Join-Path $tree 'm.json') --map-sqlite $db) 0
    $ids = Invoke-Gate --map-query $db --sql "SELECT 'owner=' || id FROM expressions WHERE source = 'f(a.b, [1, 2])'"
    $line = @($ids.Lines | Where-Object { $_.StartsWith('owner=') })
    if ($line.Count -ne 1) { throw "no single expression row for the call. Output:`n$($ids.Text)" }
    return @{ Db = $db; Owner = $line[0].Substring(6).Trim() }
}

Test-Case 'subtree: a node prints its own subtree, indented from where it starts, and stops before its sibling' {
    if (-not $script:SubtreePython) { return }
    $m = New-SubtreeMap
    $r = Invoke-Gate --map-query $m.Db --subtree $m.Owner --node 5 --table expressions__shape
    Assert-Exit $r 0
    Assert-Line $r "expressions__shape  $($m.Owner)  n 5..9  (5 node(s))"
    Assert-Line $r '5  c[1] object'
    Assert-Line $r '6    k string Attribute'
    Assert-Line $r '9        k string Name'
    Assert-NoLine $r 'c[2]'
    Assert-NoLine $r 'List'
    Assert-NoLine $r 'Constant'
}

Test-Case 'subtree: the root by default, every node table of a row table, and --limit counts nodes' {
    if (-not $script:SubtreePython) { return }
    $m = New-SubtreeMap
    $r = Invoke-Gate --map-query $m.Db --subtree $m.Owner --table expressions --limit 2
    Assert-Exit $r 0
    Assert-Line $r "expressions__shape  $($m.Owner)  n 0..16  (17 node(s))"
    Assert-Line $r '... 15 more node(s); raise --limit'
    Assert-Line $r "expressions__reads  $($m.Owner)  n 0.."
}

Test-Case 'subtree: a node no table holds, or a --node that is not a number, is exit 2' {
    if (-not $script:SubtreePython) { return }
    $m = New-SubtreeMap
    $none = Invoke-Gate --map-query $m.Db --subtree $m.Owner --node 99 --table expressions__shape
    Assert-Exit $none 2
    Assert-Line $none "no node 99 of $($m.Owner) in any node table of expressions__shape"
    $bad = Invoke-Gate --map-query $m.Db --subtree $m.Owner --node five
    Assert-Exit $bad 2
    Assert-Line $bad '--node takes a node'
}

Test-Case 'ancestors: the path from the root down to a node, indented from the root, and no sibling on the way' {
    if (-not $script:SubtreePython) { return }
    $m = New-SubtreeMap
    $r = Invoke-Gate --map-query $m.Db --ancestors $m.Owner --node 9 --table expressions__shape
    Assert-Exit $r 0
    Assert-Line $r "expressions__shape  $($m.Owner)  n 9  depth 5  (6 node(s) from the root)"
    Assert-Line $r '0  object'
    Assert-Line $r '5      c[1] object'
    Assert-Line $r '9            k string Name'
    # Node 1 (the root's `k`) and node 6 (the Attribute's `k`) are siblings on the way, never ancestors.
    Assert-NoLine $r 'Call'
    Assert-NoLine $r 'Attribute'
}

Test-Case 'ancestors: --node is required, and --limit counts from the root' {
    if (-not $script:SubtreePython) { return }
    $m = New-SubtreeMap
    $none = Invoke-Gate --map-query $m.Db --ancestors $m.Owner --table expressions__shape
    Assert-Exit $none 2
    Assert-Line $none '--ancestors needs the owner'
    $cut = Invoke-Gate --map-query $m.Db --ancestors $m.Owner --node 9 --table expressions__shape --limit 2
    Assert-Exit $cut 0
    Assert-Line $cut '... 4 more node(s); raise --limit'
    Assert-NoLine $cut 'Name'
}

Test-Case 'children: only the nodes one level down, each with how many it holds below' {
    if (-not $script:SubtreePython) { return }
    $m = New-SubtreeMap
    $r = Invoke-Gate --map-query $m.Db --children $m.Owner --node 2 --table expressions__shape
    Assert-Exit $r 0
    Assert-Line $r "expressions__shape  $($m.Owner)  n 2  depth 1  (3 child(ren))"
    Assert-Line $r '3  c[0] object  +1 below'
    Assert-Line $r '5  c[1] object  +4 below'
    Assert-Line $r '10  c[2] object  +6 below'
    # Every `k` is a grandchild of node 2, never a child.
    Assert-NoLine $r 'k string'
}

Test-Case 'children: the root by default, a scalar holds nothing below, and --limit counts children' {
    if (-not $script:SubtreePython) { return }
    $m = New-SubtreeMap
    $r = Invoke-Gate --map-query $m.Db --children $m.Owner --table expressions__shape
    Assert-Exit $r 0
    Assert-Line $r "expressions__shape  $($m.Owner)  n 0  depth 0  (2 child(ren))"
    Assert-Line $r '1  k string Call'
    Assert-NoLine $r 'Call  +'
    Assert-Line $r '2  c list  +14 below'
    $cut = Invoke-Gate --map-query $m.Db --children $m.Owner --node 2 --table expressions__shape --limit 1
    Assert-Line $cut '... 2 more node(s); raise --limit'
}

Test-Case 'siblings: the children of its parent, the asked node marked among them, and nothing one level down' {
    if (-not $script:SubtreePython) { return }
    $m = New-SubtreeMap
    $r = Invoke-Gate --map-query $m.Db --siblings $m.Owner --node 5 --table expressions__shape
    Assert-Exit $r 0
    Assert-Line $r "expressions__shape  $($m.Owner)  n 5  parent 2  depth 2  (3 node(s) under 2, it among them)"
    Assert-Line $r '3  c[0] object  +1 below'
    Assert-NoLine $r '+1 below  <- asked'
    Assert-Line $r '5  c[1] object  +4 below  <- asked'
    Assert-Line $r '10  c[2] object  +6 below'
    Assert-NoLine $r 'k string'
}

Test-Case 'siblings: the root stands alone, and --node is required' {
    if (-not $script:SubtreePython) { return }
    $m = New-SubtreeMap
    $root = Invoke-Gate --map-query $m.Db --siblings $m.Owner --node 0 --table expressions__shape
    Assert-Exit $root 0
    Assert-Line $root "expressions__shape  $($m.Owner)  n 0  depth 0  (the root: no siblings)"
    $none = Invoke-Gate --map-query $m.Db --siblings $m.Owner --table expressions__shape
    Assert-Exit $none 2
    Assert-Line $none '--siblings needs the owner'
}

Test-Case 'descendants-of-kind: every object below the node whose kind is K, each at its depth with its size' {
    if (-not $script:SubtreePython) { return }
    $m = New-SubtreeMap
    $r = Invoke-Gate --map-query $m.Db --descendants-of-kind $m.Owner --kind Name --table expressions__shape
    Assert-Exit $r 0
    Assert-Line $r "expressions__shape  $($m.Owner)  n 0..16  (2 node(s) of kind Name)"
    Assert-Line $r '3    c[0] object Name  +1 below'
    Assert-Line $r '8        c[0] object Name  +1 below'
    Assert-NoLine $r 'Attribute'
    $in = Invoke-Gate --map-query $m.Db --descendants-of-kind $m.Owner --kind Name --node 5 --table expressions__shape
    Assert-Line $in "expressions__shape  $($m.Owner)  n 5..9  (1 node(s) of kind Name)"
    Assert-NoLine $in '3    c[0]'
    # The Constants sit after node 5's subtree ends, at 13 and 15: a range read past `last` would print them.
    $after = Invoke-Gate --map-query $m.Db --descendants-of-kind $m.Owner --kind Constant --node 5 --table expressions__shape
    Assert-Line $after "expressions__shape  $($m.Owner)  n 5..9  (0 node(s) of kind Constant)"
    Assert-NoLine $after 'object Constant'
}

Test-Case 'descendants-of-kind: the node itself is not its own descendant, --limit counts, --kind is required' {
    if (-not $script:SubtreePython) { return }
    $m = New-SubtreeMap
    $self = Invoke-Gate --map-query $m.Db --descendants-of-kind $m.Owner --kind Call --table expressions__shape
    Assert-Exit $self 0
    Assert-Line $self '(0 node(s) of kind Call)'
    Assert-NoLine $self '0  object'
    $cut = Invoke-Gate --map-query $m.Db --descendants-of-kind $m.Owner --kind Constant --table expressions__shape --limit 1
    Assert-Line $cut '... 1 more node(s); raise --limit'
    $none = Invoke-Gate --map-query $m.Db --descendants-of-kind $m.Owner --table expressions__shape
    Assert-Exit $none 2
    Assert-Line $none '--descendants-of-kind needs the owner and the kind'
}
