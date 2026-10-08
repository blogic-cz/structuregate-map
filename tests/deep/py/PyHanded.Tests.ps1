<#
    The python half is HANDED the tree map's hashes (`hashes.json`) and reads a file's text only when it
    parses it. A file nothing changed is therefore never opened - which is what these cases hold open to prove.
#>

Test-Case 'pyhanded: an unchanged file is not even read when another one changed' {
    $tree = Use-Tree @{ 'a.py' = "def first():`n    return 1`n"; 'b.py' = "def second():`n    return 2`n" }
    $db = Join-Path $tree 'map.sqlite'
    Assert-Exit (Invoke-Gate --root $tree --ext .py --map-sqlite $db) 0
    [System.IO.File]::WriteAllText((Join-Path $tree 'b.py'), "def second():`n    return 3`n")
    # HELD OPEN EXCLUSIVELY: a host that still read every file to hash it would say it cannot be read.
    $held = [System.IO.File]::Open((Join-Path $tree 'a.py'), 'Open', 'ReadWrite', 'None')
    try { $again = Invoke-Gate --root $tree --ext .py --map-sqlite $db } finally { $held.Dispose() }
    Assert-Exit $again 0
    Assert-NoLine $again 'cannot be read'
    Assert-Line $again '1 file(s) re-read'
}
