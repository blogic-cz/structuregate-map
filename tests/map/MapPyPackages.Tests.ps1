<#
    WHAT A PYTHON IMPORT RUNS (#139, `src/Map/Py/PyMap.py::imports`): `from . import alpha` binds the submodule, as the absolute
    `from demo import alpha` does, and `from demo.sub.beta import f` runs `demo/__init__.py` and `demo/sub/__init__.py` first -
    each an edge, so none is reported as read by nobody - found from the root or, under another one, by its suffix,
    where the package beside the importing script is the one it runs (`rust/fbtcore/src/graph/join.rs`). A package holding the importer is no edge: Python ran it before the
    importer, and the edge back would make every package that re-exports its submodules a cycle. Helpers: `Map.Helpers.ps1`.
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
