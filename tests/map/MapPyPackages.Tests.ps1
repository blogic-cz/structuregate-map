<#
    WHAT A PYTHON IMPORT RUNS (#139, `src/Map/Py/PyMap.py::imports`): `from . import alpha` binds the submodule, as the absolute
    `from demo import alpha` does, and `from demo.sub.beta import f` runs `demo/__init__.py` and `demo/sub/__init__.py` first -
    each an edge, so none is reported as read by nobody - found from the root or, under another one, by its suffix,
    where the package beside the importing script is the one it runs (`rust/fbtcore/src/graph/join.rs`). A package holding the importer is no edge: Python ran it before the
    importer, and the edge back would make every package that re-exports its submodules a cycle - so `from . import alpha` written
    inside `demo/` is no edge to `demo/__init__.py`, while `from . import HELPER`, a name the package itself defines, is. Helpers: `Map.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot 'Map.Helpers.ps1')

if ($script:MapPython) {

Test-Case 'map: a relative from-import of a module binds the module, and a dotted import runs every package on the way' {
    $tree = Use-Tree @{
        'demo/__init__.py'        = "from . import alpha`n"
        'demo/alpha.py'           = "VALUE = 1`n"
        'demo/sub/__init__.py'    = "`"`"`"Sub-package.`"`"`"`n"
        'demo/sub/beta.py'        = "def f():`n    return 1`n"
        'demo/sub/gamma.py'       = "from ..sub.beta import f`nfrom demo.sub import beta`n"
        'main.py'                 = "import demo`nfrom demo.sub.beta import f`n"
    }
    $found = Get-Map --root $tree --ext .py
    Assert-Exit $found 0
    $init = Get-Imports $found.Map 'demo/__init__.py'
    if ($init -notcontains 'demo/alpha.py') { throw "from . import alpha has no edge to the module: $($init -join ', ')" }
    $main = Get-Imports $found.Map 'main.py'
    foreach ($ran in 'demo/__init__.py', 'demo/sub/__init__.py', 'demo/sub/beta.py') {
        if ($main -notcontains $ran) { throw "main.py has no edge to $ran`: $($main -join ', ')" }
    }
    # Inside `demo/sub`, the package above it is passed through by both spellings and is no edge.
    $gamma = Get-Imports $found.Map 'demo/sub/gamma.py'
    if ($gamma -contains 'demo/__init__.py') { throw "gamma.py has an edge to the package holding it: $($gamma -join ', ')" }
    if ($gamma -notcontains 'demo/sub/beta.py') { throw "gamma.py has no edge to beta.py: $($gamma -join ', ')" }
    Assert-NoLine $found 'NO READER demo/alpha.py'
    Assert-NoLine $found 'NO READER demo/sub/__init__.py'
}

Test-Case 'map: a submodule imported from inside its own package is no edge to that package, a name the package defines is' {
    $tree = Use-Tree @{
        'demo/__init__.py' = "from . import alpha`nHELPER = 1`n"
        'demo/alpha.py'    = "VALUE = 1`n"
        'demo/beta.py'     = "from . import alpha`n"
        'demo/gamma.py'    = "from demo import alpha`n"
        'demo/delta.py'    = "from . import HELPER`n"
        'main.py'          = "import demo`n"
    }
    $found = Get-Map --root $tree --ext .py
    Assert-Exit $found 0
    foreach ($file in 'demo/beta.py', 'demo/gamma.py') {
        $edges = Get-Imports $found.Map $file
        if ($edges -contains 'demo/__init__.py') { throw "$file has an edge to the package holding it: $($edges -join ', ')" }
        if ($edges -notcontains 'demo/alpha.py') { throw "$file has no edge to alpha.py: $($edges -join ', ')" }
    }
    $delta = Get-Imports $found.Map 'demo/delta.py'
    if ($delta -notcontains 'demo/__init__.py') { throw "delta.py reads HELPER from the package, yet has no edge: $($delta -join ', ')" }
}

Test-Case 'map: under another root, a module of its own package is no edge to that package by name, suffix or string' {
    # `apps/` is no root: every import here is joined by name or by suffix in the graph (`join.rs::holding`).
    $tree = Use-Tree @{
        'apps/demo/__init__.py'  = "HELPER = 1`n"
        'apps/demo/state.py'     = "VALUE = 1`n"
        'apps/demo/sub/__init__.py' = "`n"
        'apps/demo/sub/leaf.py'  = "def work():`n    return 1`n"
        'apps/demo/sub/other.py' = "from demo.sub import leaf`n"
        'apps/demo/beta.py'      = "import sys`nfrom demo import state`nfrom demo.sub import leaf`nNAME = getattr(sys, `"demo`", None)`n"
        'apps/demo/delta.py'     = "from demo import HELPER`n"
        'apps/main.py'           = "import demo`n"
    }
    $found = Get-Map --root $tree --ext .py
    Assert-Exit $found 0
    # `demo.sub` does not hold beta.py, so python runs `sub/__init__.py` for it: that one is an edge.
    $soft = $found.Map.soft_imports.PSObject.Properties | Where-Object { $_.Name -eq 'apps/demo/beta.py' }
    $hard = Get-Imports $found.Map 'apps/demo/beta.py'
    $beta = @($hard) + @(if ($soft) { $soft.Value })
    if ($beta -contains 'apps/demo/__init__.py') { throw "beta.py has an edge to the package holding it: $($beta -join ', ')" }
    foreach ($ran in 'apps/demo/state.py', 'apps/demo/sub/__init__.py', 'apps/demo/sub/leaf.py') {
        if ($beta -notcontains $ran) { throw "beta.py has no edge to $ran`: $($beta -join ', ')" }
    }
    # Inside `demo/sub`, `demo.sub` is found by its suffix and holds the importer.
    $other = Get-Imports $found.Map 'apps/demo/sub/other.py'
    if ($other -contains 'apps/demo/sub/__init__.py') { throw "other.py has an edge to the package holding it: $($other -join ', ')" }
    if ($other -notcontains 'apps/demo/sub/leaf.py') { throw "other.py has no edge to leaf.py: $($other -join ', ')" }
    $delta = Get-Imports $found.Map 'apps/demo/delta.py'
    if ($delta -notcontains 'apps/demo/__init__.py') { throw "delta.py reads HELPER from the package, yet has no edge: $($delta -join ', ')" }
}

Test-Case 'map: a dotted import found by its suffix under another root runs every package on the way' {
    # `src/` is no root and no import names it: the module is found by the spelling `demo/sub/leaf.py` (`join.rs`).
    $tree = Use-Tree @{
        'src/demo/__init__.py'     = "`n"
        'src/demo/sub/__init__.py' = "`n"
        'src/demo/sub/leaf.py'     = "def work(x):`n    return x`n"
        'tests/run.py'             = "from demo.sub.leaf import work`n"
    }
    $found = Get-Map --root $tree --ext .py
    Assert-Exit $found 0
    $run = Get-Imports $found.Map 'tests/run.py'
    foreach ($ran in 'src/demo/__init__.py', 'src/demo/sub/__init__.py', 'src/demo/sub/leaf.py') {
        if ($run -notcontains $ran) { throw "tests/run.py has no edge to $ran`: $($run -join ', ')" }
    }
}

Test-Case 'map: of two packages a suffix names, the one beside the importing script is the one it runs' {
    $tree = Use-Tree @{
        'a/scripts/main.py'                   = "from office.helpers import run`n"
        'a/scripts/office/__init__.py'        = "`n"
        'a/scripts/office/helpers/__init__.py' = "def run():`n    return 1`n"
        'b/scripts/main.py'                   = "from office.helpers import run`n"
        'b/scripts/office/__init__.py'        = "`n"
        'b/scripts/office/helpers/__init__.py' = "def run():`n    return 2`n"
    }
    $found = Get-Map --root $tree --ext .py
    Assert-Exit $found 0
    $a = Get-Imports $found.Map 'a/scripts/main.py'
    if ($a -notcontains 'a/scripts/office/helpers/__init__.py') { throw "a/scripts/main.py misses its own helpers: $($a -join ', ')" }
    if ($a -contains 'b/scripts/office/helpers/__init__.py') { throw "a/scripts/main.py reaches b's helpers: $($a -join ', ')" }
    $b = Get-Imports $found.Map 'b/scripts/main.py'
    if ($b -notcontains 'b/scripts/office/helpers/__init__.py') { throw "b/scripts/main.py misses its own helpers: $($b -join ', ')" }
    Assert-NoLine $found 'AMBIGUOUS'
}

}
