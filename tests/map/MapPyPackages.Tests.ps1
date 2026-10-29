<#
    WHAT A PYTHON IMPORT RUNS (#139, `src/Map/Py/PyMap.py::imports`): `from . import alpha` binds the submodule, as the absolute
    `from demo import alpha` does, and `from demo.sub.beta import f` runs `demo/__init__.py` and `demo/sub/__init__.py` first -
    each an edge, so none is reported as read by nobody. A package holding the importer is no edge: Python ran it before the
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

}
