<#
    What `--map-query` says ABOUT the map it answers from, rather than a lens's rows: `--help` naming `--width`,
    `--find` printing the path it exists to answer whole, `_meta` naming every `--root` the map was built from,
    and one note on STDERR when the tree moved on since the map was written. The python half builds the map
    because it is the cheapest host; nothing here is about python. Its helpers are its own: `-Only PyQuery`.
#>

$script:PyQueryPython = [bool]$script:Python
if (-not $script:PyQueryPython) { Write-Host '    (no python on this machine - the query cases are not run)' }

if ($script:PyQueryPython) {

Test-Case 'pyquery: --help names --width, and --find prints a long file path whole' {
    $help = Invoke-Gate --help
    Assert-Line $help '--width N'
    # A path well past the 70 characters a cell is cut at, declaring a name only it holds.
    $rel = 'demo/application/orders/validators/archived/get_archived_orders_request_validator.py'
    $tree = Use-Tree @{ $rel = "class GetArchivedOrdersRequestValidator:`n    pass`n" }
    $db = Join-Path $tree 'map.sqlite'
    Assert-Exit (Invoke-Gate --root $tree --ext .py --map-sqlite $db) 0
    $r = Invoke-Gate --map-query $db --find GetArchivedOrdersRequestValidator
    Assert-Exit $r 0
    Assert-Line $r $rel
}

Test-Case 'pyquery: _meta records every --root the map was built from, the first still under root' {
    $tree = Use-Tree @{ 'src/demo/order.py' = "def place():`n    return 1`n"; 'test/demo_test.py' = "def check():`n    return 2`n" }
    $db = Join-Path $tree 'map.sqlite'
    $src = Join-Path $tree 'src'
    $test = Join-Path $tree 'test'
    Assert-Exit (Invoke-Gate --root $src --root $test --ext .py --map-sqlite $db) 0
    $r = Invoke-Gate --map-query $db --sql "SELECT value FROM _meta WHERE key = 'roots'" --width 0
    Assert-Exit $r 0
    # Header, rule, the one value: a JSON array of every root, in the order given.
    $roots = @($r.Lines[2].Trim() | ConvertFrom-Json)
    Assert-Equal $roots.Count 2 'roots recorded'
    Assert-Equal $roots[0] $src 'the first root'
    Assert-Equal $roots[1] $test 'the second root'
    $first = Invoke-Gate --map-query $db --sql "SELECT value FROM _meta WHERE key = 'root'" --width 0
    Assert-Equal $first.Lines[2].Trim() $src 'root, kept as it was'
}

Test-Case 'pyquery: a stale map is said once on stderr - changed, added and gone counted - and a rebuild clears it' {
    $tree = Use-Tree @{ 'demo/orders.py' = "def archive():`n    return 1`n"; 'demo/old.py' = "def gone():`n    return 2`n"
                        'demo/same.py' = "def kept():`n    return 3`n" }
    $db = Join-Path $tree 'map.sqlite'
    Assert-Exit (Invoke-Gate --root $tree --ext .py --map-sqlite $db) 0
    $fresh = Invoke-GateStream --map-query $db --find archive
    Assert-Exit $fresh 0
    Assert-Equal $fresh.Err.Trim() '' 'stderr over a current map'
    # Written AFTER the map, as an edit is: never dated ahead, since a file dated past the last run's look at the
    # tree is one that run cannot have seen.
    [System.IO.File]::WriteAllText((Join-Path $tree 'demo/orders.py'), "def archive_all():`n    return 1`n")
    [System.IO.File]::WriteAllText((Join-Path $tree 'demo/new_thing.py'), "def fresh():`n    return 4`n")
    Remove-Item (Join-Path $tree 'demo/old.py')
    $stale = Invoke-GateStream --map-query $db --find archive
    Assert-Exit $stale 0
    # STDOUT IS PARSED: the answer alone, the note never in it.
    if ($stale.Out.Contains('note:')) { throw "the note went to stdout:`n$($stale.Out)" }
    $notes = @($stale.Err.Split([char]10) | Where-Object { $_.Trim() })
    Assert-Equal $notes.Count 1 'stderr lines'
    foreach ($needle in @('1 changed', '1 added', '1 gone', 'demo/orders.py', 'demo/new_thing.py', 'demo/old.py', '--map-sqlite')) {
        if (-not $notes[0].Contains($needle)) { throw "the note does not say '$needle': $($notes[0])" }
    }
    if ($notes[0].Contains('demo/same.py')) { throw "an untouched file was named: $($notes[0])" }
    Assert-Exit (Invoke-Gate --root $tree --ext .py --map-sqlite $db) 0
    $rebuilt = Invoke-GateStream --map-query $db --find archive
    Assert-Equal $rebuilt.Err.Trim() '' 'stderr after a rebuild'
    # A FILE TOUCHED AND NOT CHANGED: the rebuild re-reads nothing and leaves the database as it was, older than the
    # file - which is still current, so nothing is said.
    $same = Join-Path $tree 'demo/same.py'
    (Get-Item $same).LastWriteTime = Get-Date
    Assert-Exit (Invoke-Gate --root $tree --ext .py --map-sqlite $db) 0
    $touched = Invoke-GateStream --map-query $db --find archive
    Assert-Equal $touched.Err.Trim() '' 'stderr after a touch and a rebuild'
}

Test-Case 'pyquery: a map whose root is gone answers without a note and without failing' {
    $tree = Use-Tree @{ 'demo/orders.py' = "def archive():`n    return 1`n" }
    $beside = Use-Tree @{ 'keep.txt' = 'x' }
    $db = Join-Path $beside 'map.sqlite'
    Assert-Exit (Invoke-Gate --root $tree --ext .py --map-sqlite $db) 0
    Remove-Item $tree -Recurse -Force
    $r = Invoke-GateStream --map-query $db --find archive
    Assert-Exit $r 0
    Assert-Equal $r.Err.Trim() '' 'stderr over a map whose root is gone'
    if (-not $r.Out.Contains('archive')) { throw "the answer is missing:`n$($r.Out)" }
}

}
