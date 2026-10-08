<#
    A file that did not change is re-read when what its rows were BOUND THROUGH did: a module that moved,
    appeared, or gained a second spelling. And `--map-if-stale` sees a move, which keeps every mtime.

    Its helpers are its own - `-Only` runs this suite alone.
#>

$script:PyDepsPython = [bool]$script:Python
if (-not $script:PyDepsPython) { Write-Host '    (no python on this machine - the re-binding cases are not run)' }

# One run of the deep map over a tree, and where each call to `name` is bound, as `callee=path` lines.
function Get-PyDepsTargets([string]$Tree, [string]$Name) {
    $db = Join-Path $Tree 'deps.sqlite'
    $run = Invoke-Gate --root $Tree --ext .py --map-sqlite $db
    Assert-Exit $run 0
    $r = Invoke-Gate --map-query $db --width 400 --sql ("SELECT 'bound=' || callee || '->' || target_path AS b " +
        "FROM calls WHERE callee LIKE '%$Name' ORDER BY callee")
    Assert-Exit $r 0
    return [pscustomobject]@{ Run = $run; Bound = $r }
}

# `bound=` is a prefix of every bound line, so a containment check would take a bound call for an unbound one.
function Assert-PyDepsBound($Result, [string]$Expected) {
    $hits = @($Result.Lines | Where-Object { $_.Trim() -ceq $Expected })
    if ($hits.Count -eq 0) { throw "no line is exactly '$Expected'. Output:`n$($Result.Text)" }
}

function Set-PyDepsFile([string]$Tree, [string]$Rel, [string]$Text) {
    $path = Join-Path $Tree $Rel
    [void](New-Item -ItemType Directory -Path (Split-Path $path) -Force)
    [System.IO.File]::WriteAllText($path, $Text)
}

if ($script:PyDepsPython) {

$nl = [string][char]10

Test-Case 'pydeps: an unchanged importer is re-linked when the module behind its re-export moves' {
    # The report: `caller.py` kept pointing at `hub/impl.py` after it moved to `moved/`.
    $tree = Use-Tree @{
        'pkg/__init__.py'          = ''
        'pkg/moved/__init__.py'    = ''
        'pkg/hub/__init__.py'      = "from pkg.hub.impl import helper  # noqa$nl"
        'pkg/hub/impl.py'          = "def helper():$nl    return 1$nl"
        'pkg/caller.py'            = "from pkg import hub$nl$nl$nl" + "def run():$nl    return hub.helper()$nl"
    }
    Assert-PyDepsBound (Get-PyDepsTargets $tree 'helper').Bound 'bound=hub.helper->pkg/hub/impl.py'
    Move-Item (Join-Path $tree 'pkg/hub/impl.py') (Join-Path $tree 'pkg/moved/impl.py')
    Set-PyDepsFile $tree 'pkg/hub/__init__.py' "from pkg.moved.impl import helper  # noqa$nl"
    Assert-PyDepsBound (Get-PyDepsTargets $tree 'helper').Bound 'bound=hub.helper->pkg/moved/impl.py'
}

Test-Case 'pydeps: a module that did not exist binds its importers once written, through a scope read earlier' {
    # `a_first.py` sorts first, so the shim's scope is parsed for it and `caller.py` gets it from the cache:
    # what the shim probed has to reach `caller.py` without being probed again.
    # RELATIVE, because a relative import is only ever a probe: nothing joins it by name as a fallback.
    $tree = Use-Tree @{
        'pkg/__init__.py'          = ''
        'pkg/a_first.py'           = "from pkg import hub$nl$nl$nl" + "def warm():$nl    return hub.helper()$nl"
        'pkg/hub/__init__.py'      = "from ..late import helper  # noqa$nl"
        'pkg/caller.py'            = "from pkg import hub$nl$nl$nl" + "def run():$nl    return hub.helper()$nl"
    }
    Assert-PyDepsBound (Get-PyDepsTargets $tree 'helper').Bound 'bound=hub.helper->'
    Set-PyDepsFile $tree 'pkg/late.py' "def helper():$nl    return 1$nl"
    $after = (Get-PyDepsTargets $tree 'helper').Bound
    $bound = @($after.Lines | Where-Object { $_.Trim() -eq 'bound=hub.helper->pkg/late.py' })
    Assert-Equal $bound.Count 2 'both importers are bound to the module that now exists'
}

Test-Case 'pydeps: a module found by its name alone is unbound when a second file spells it' {
    # A bootstrap puts `tools/` on sys.path, so `import helper` is found by its name - while it is the only one.
    $tree = Use-Tree @{
        'app/main.py'      = "import helper$nl$nl$nl" + "def run():$nl    return helper.assist()$nl"
        'tools/helper.py'  = "def assist():$nl    return 1$nl"
    }
    Assert-PyDepsBound (Get-PyDepsTargets $tree 'assist').Bound 'bound=helper.assist->tools/helper.py'
    Set-PyDepsFile $tree 'other/helper.py' "def assist():$nl    return 2$nl"
    $after = (Get-PyDepsTargets $tree 'assist').Bound
    Assert-PyDepsBound $after 'bound=helper.assist->'
    Assert-NoLine $after 'bound=helper.assist->tools/helper.py'
}

Test-Case 'pydeps: a run where nothing moved re-reads nothing' {
    $tree = Use-Tree @{
        'pkg/__init__.py'          = ''
        'pkg/hub/__init__.py'      = "from pkg.hub.impl import helper  # noqa$nl"
        'pkg/hub/impl.py'          = "import os$nl$nl$nl" + "def helper():$nl    return os.sep$nl"
        'pkg/caller.py'            = "from pkg import hub$nl$nl$nl" + "def run():$nl    return hub.helper()$nl"
    }
    Assert-Line (Get-PyDepsTargets $tree 'helper').Run ', 4 file(s) re-read'
    Assert-Line (Get-PyDepsTargets $tree 'helper').Run ', 0 file(s) re-read'
    # One unrelated file: itself only, not every file that imports something.
    Set-PyDepsFile $tree 'pkg/unrelated.py' "X = 1$nl"
    Assert-Line (Get-PyDepsTargets $tree 'helper').Run ', 1 file(s) re-read'
}

}

Test-Case 'map: --map-if-stale re-maps after a move, which keeps every mtime' {
    $tree = Use-Tree @{ 'a/A.cs' = "namespace N;`npublic class A { }`n"; 'B.cs' = "namespace N;`npublic class B { }`n" }
    $path = Join-Path $tree 'map.json'
    Assert-Exit (Invoke-Gate --root $tree --map --map-out $path) 0
    [void](New-Item -ItemType Directory -Path (Join-Path $tree 'b'))
    Move-Item (Join-Path $tree 'a/A.cs') (Join-Path $tree 'b/A.cs')
    # Every source older than the map, as a move on one volume leaves it.
    $old = (Get-Item $path).LastWriteTimeUtc.AddMinutes(-5)
    Get-ChildItem $tree -Recurse -File -Filter *.cs | ForEach-Object { $_.LastWriteTimeUtc = $old }
    $again = Invoke-Gate --root $tree --map --map-out $path --map-if-stale
    Assert-Exit $again 0
    Assert-NoLine $again 'nothing re-parsed'
    Assert-Line $again 'wrote'
    $map = Get-Content $path -Raw | ConvertFrom-Json
    Assert-Equal ([bool]$map.files.'b/A.cs') $true 'the map names the file where it now is'
    $current = Invoke-Gate --root $tree --map --map-out $path --map-if-stale
    Assert-Line $current 'nothing re-parsed'
}
