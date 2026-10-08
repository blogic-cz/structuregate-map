<#
    --map-plugin end to end: the rows a project states itself, folded into the map. Split from `Map` at its
    size limit; the map-reading helpers are `Map.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot 'Map.Helpers.ps1')

# ---------------------------------------------------------------------------------------------------
# --map-plugin - the rows no parser here can derive, because they do not exist until the project runs
# ---------------------------------------------------------------------------------------------------

# A stand-in for a project's own emitter. It prints the protocol and nothing else, which is the whole
# contract - what it read to get there is the project's business, not this tool's.
function New-MapPlugin([string]$Tree, [string]$Body) {
    $path = Join-Path $Tree 'emit.ps1'
    [System.IO.File]::WriteAllText($path, $Body)
    return $path
}

Test-Case 'map: a plugin contributes artifacts, steps, producers and readers' {
    $tree = Use-Tree @{ 'a.py' = "X = 1`n" }
    $plugin = New-MapPlugin $tree @'
Write-Output "MAP-ARTIFACT|catalog|path|build/catalog.json"
Write-Output "MAP-ARTIFACT|catalog|rebuild|python cli.py --catalog --write"
Write-Output "MAP-ARTIFACT|catalog|exists|true"
Write-Output "MAP-STEP|load|module|steps.load"
Write-Output "MAP-STEP|merge|module|steps.merge"
Write-Output "MAP-PRODUCES|catalog|cli.py"
Write-Output "MAP-READS|catalog|app.py"
Write-Output "MAP-DONE|1"
'@
    $found = Get-Map --root $tree --ext .py --map-check --map-plugin "$($script:PowerShell) -NoProfile -File `"$plugin`""
    Assert-Exit $found 0
    Assert-Equal $found.Map.artifacts.catalog.path 'build/catalog.json' 'the declared path'
    Assert-Equal $found.Map.artifacts.catalog.rebuild 'python cli.py --catalog --write' 'a value with spaces'
    Assert-Equal $found.Map.produced_by.catalog 'cli.py' 'who writes it'
    Assert-Equal $found.Map.read_by.catalog 'app.py' 'who reads it'
    # THE ORDER IS THE DEPENDENCY STATEMENT, so the steps keep the order they arrived in.
    Assert-Equal $found.Map.steps[0].step 'load' 'first step'
    Assert-Equal $found.Map.steps[1].step 'merge' 'second step'
}

Test-Case 'map: a plugin field this tool has never heard of lands in the JSON unchanged' {
    # A field name is not an enum. A wide fixed record would have to be widened here every time the project
    # learned a new column; one field per line means it never does.
    $tree = Use-Tree @{ 'a.py' = "X = 1`n" }
    $plugin = New-MapPlugin $tree @'
Write-Output "MAP-ARTIFACT|thing|degrades_to|every row defaults to tier=all"
Write-Output "MAP-ARTIFACT|thing|located_by|config.DATA_PATH"
Write-Output "MAP-DONE|1"
'@
    $found = Get-Map --root $tree --ext .py --map-plugin "$($script:PowerShell) -NoProfile -File `"$plugin`""
    Assert-Equal $found.Map.artifacts.thing.located_by 'config.DATA_PATH' 'an unknown field survives'
    Assert-Equal $found.Map.artifacts.thing.degrades_to 'every row defaults to tier=all' 'and another'
}

Test-Case 'map: a plugin states its OWN severity, and an error fails --map-check' {
    $tree = Use-Tree @{ 'a.py' = "X = 1`n" }
    $plugin = New-MapPlugin $tree @'
Write-Output "MAP-FINDING|note|UNREAD items: nothing asks for it by name"
Write-Output "MAP-FINDING|error|NO PRODUCER catalog: nothing declares a producer for it"
Write-Output "MAP-DONE|1"
'@
    $found = Get-Map --root $tree --ext .py --map-check --map-plugin "$($script:PowerShell) -NoProfile -File `"$plugin`""
    Assert-Exit $found 1
    Assert-Line $found 'error: PLUGIN    NO PRODUCER catalog'
    Assert-Line $found 'note : plugin    UNREAD items'
}

Test-Case 'map: a plugin that cannot be launched FAILS, it is not a skip' {
    # Unlike a language half - a tool that may simply be absent on a machine - a --map-plugin was NAMED on
    # the command line, so a map silently missing half its rows is worse than one that never ran.
    $tree = Use-Tree @{ 'a.py' = "X = 1`n" }
    $found = Get-Map --root $tree --ext .py --map-check --map-plugin "no-such-emitter.exe"
    Assert-Exit $found 1
    Assert-Line $found 'PLUGIN'
    Assert-Line $found 'no-such-emitter.exe'
}

Test-Case 'map: a plugin that stops half way is a finding, not a short map' {
    $tree = Use-Tree @{ 'a.py' = "X = 1`n" }
    $plugin = New-MapPlugin $tree @'
Write-Output "MAP-ARTIFACT|half|path|build/half.json"
exit 3
'@
    $found = Get-Map --root $tree --ext .py --map-check --map-plugin "$($script:PowerShell) -NoProfile -File `"$plugin`""
    Assert-Exit $found 1
    Assert-Line $found 'did not finish'
}

Test-Case 'map: a plugin is handed the map''s own file list' {
    # So a project script that scans the tree agrees with this one about what is in it, rather than
    # re-deriving a second, differing file set.
    $tree = Use-Tree @{ 'a.py' = "X = 1`n"; 'b.py' = "Y = 2`n" }
    $plugin = New-MapPlugin $tree @'
$rows = @(Get-Content $env:STRUCTUREGATE_MAP_LIST)
Write-Output "MAP-ARTIFACT|listed|files|$($rows.Count)"
Write-Output "MAP-ARTIFACT|listed|root|$(Split-Path $env:STRUCTUREGATE_MAP_ROOT -Leaf)"
Write-Output "MAP-DONE|1"
'@
    $found = Get-Map --root $tree --ext .py --map-plugin "$($script:PowerShell) -NoProfile -File `"$plugin`""
    Assert-Equal $found.Map.artifacts.listed.files '2' 'both files were listed'
    Assert-Equal $found.Map.artifacts.listed.root (Split-Path $tree -Leaf) 'and the root was named'
}

Test-Case 'map: --map-plugin without --map is refused' {
    $tree = Use-Tree @{ 'a.py' = "X = 1`n" }
    $result = Invoke-Gate --root $tree --ext .py --map-plugin "echo hi"
    Assert-Exit $result 2
    Assert-Line $result 'need --map'
}
