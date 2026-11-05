<#
    The python deep map's lenses followed through a BINDING rather than a spelling: `--find` through an import
    alias to the def it binds, `--reads` through an alias to the constant it names, `--key` through
    a settings section held by a local, and `--decorators` matching the file and showing a decorator's
    arguments.

    Its helpers are its own - `-Only PyAlias` runs this suite alone.
#>

$script:PyAliasPython = [bool]$script:Python
if (-not $script:PyAliasPython) { Write-Host '    (no python on this machine - the alias cases are not run)' }

# The deep map of a throwaway tree, as its database path.
function New-PyAliasDb([hashtable]$Files) {
    $tree = Use-Tree $Files
    $db = Join-Path $tree 'alias.sqlite'
    Assert-Exit (Invoke-Gate --root $tree --ext .py --map-sqlite $db) 0
    return $db
}

# One lens through the exe, every cell whole.
function Invoke-PyAliasQ([string]$Path, [Parameter(ValueFromRemainingArguments)][object[]]$LensArgs) {
    $result = Invoke-Gate --map-query $Path --width 0 --limit 0 @LensArgs
    Assert-Exit $result 0
    return $result
}

# The output's rows split into cells, so a case asserts on one row and never on a neighbour's text.
function Get-PyAliasRow($Result, [string[]]$Cells) {
    foreach ($line in $Result.Lines) {
        $found = $line.Split([string[]]@('  '), [StringSplitOptions]::RemoveEmptyEntries) | ForEach-Object { $_.Trim() }
        $all = $true
        foreach ($cell in $Cells) { if (@($found) -notcontains $cell) { $all = $false } }
        if ($all) { return $line }
    }
    throw "no row holds every one of '$($Cells -join "', '")'. Output:`n$($Result.Text)"
}

if ($script:PyAliasPython) {

$nl = [string][char]10

Test-Case 'pyalias: --find follows an import ALIAS, and a re-import of it, to the def it binds' {
    # ids_b.py re-exports `both as _both`, chain.py imports `_both` from it: both lines name the def in
    # sets_a.py, as calls.target_path already did.
    $db = New-PyAliasDb @{
        'sets_a.py' = "def both(a, b):$nl    return a | b$nl"
        'ids_b.py'  = "from sets_a import both as _both$nl"
        'chain.py'  = "from ids_b import _both$nl" + "def run(x):$nl    return _both(x, x)$nl"
        'plain.py'  = "from sets_a import both$nl"
    }
    $r = Invoke-PyAliasQ $db --find _both
    [void](Get-PyAliasRow $r @('functions', 'both (as _both)', 'sets_a.py', '1'))
    [void](Get-PyAliasRow $r @('imports', '_both', 'ids_b.py'))
    # ...FIRST, before every call and import spelling the alias: appended after them, it was the last of dozens of
    # rows on a real tree and read as not there.
    $first = @($r.Lines | Where-Object { $_ -and -not $_.StartsWith('table') -and -not $_.StartsWith('---') })[0]
    if (-not $first.Contains('both (as _both)')) { throw "the def is not the first row:`n$($r.Text)" }
    # An import under the def's OWN name adds no def row of its own: `--find both` finds the def by name.
    $plain = Invoke-PyAliasQ $db --find both
    if ($plain.Text.Contains('(as both)')) { throw "an import under the def's own name was followed:`n$($plain.Text)" }
}

Test-Case 'pyalias: --reads NAME finds a read through a local import ALIAS of it' {
    $db = New-PyAliasDb @{
        'settings.py' = "IMPORT_OUT_DIR = '/tmp/out'$nl"
        'jobs.py'     = "from settings import IMPORT_OUT_DIR as OUT_DIR$nl" + "def export():$nl    return OUT_DIR + '/x'$nl"
    }
    $r = Invoke-PyAliasQ $db --reads IMPORT_OUT_DIR
    [void](Get-PyAliasRow $r @('returns', 'jobs.py', '3', 'export'))
}

Test-Case 'pyalias: --key sec.key follows a section bound to a local, to the name built from it' {
    # `_svc = _S.get("service") or {}` and then `_svc.get("url")` on another line read service.url into
    # SERVICE_URL; a chained `_S["server"]["host"]` is server.host. A plain `url` elsewhere is not either.
    $db = New-PyAliasDb @{
        'config.py' = "_S = {}$nl" + "_svc = _S.get('service') or {}$nl" + "SERVICE_URL = ($nl    _svc.get('url')$nl    or 'x'$nl)$nl" +
                      "HOST = _S['server']['host']$nl" + "def where():$nl    return SERVICE_URL, HOST$nl"
        'other.py'  = "D = {}$nl" + "OTHER_URL = D.get('url')$nl"
    }
    $r = Invoke-PyAliasQ $db --key service.url
    [void](Get-PyAliasRow $r @('config.py', '4', 'url'))
    [void](Get-PyAliasRow $r @('config.py', '3', 'SERVICE_URL'))
    [void](Get-PyAliasRow $r @('returns', 'config.py', '9', 'where'))
    if ($r.Text.Contains('other.py')) { throw "a url key of another dict was read as service.url:`n$($r.Text)" }
    $h = Invoke-PyAliasQ $db --key server.host
    [void](Get-PyAliasRow $h @('config.py', '7', 'HOST'))
}

Test-Case 'pyalias: --decorators matches the FILE, and a row carries the decorator''s arguments' {
    $db = New-PyAliasDb @{
        'web.py' = "class App:$nl    def post(self, path, **kw):$nl        return lambda f: f$nl" + "app = App()$nl"
        'api.py' = "from web import app$nl" + "@app.post('/items', tags=['x'])$nl" + "def items():$nl    return 1$nl"
    }
    $r = Invoke-PyAliasQ $db --decorators api.py
    [void](Get-PyAliasRow $r @('api.py', '2', 'app.post', "'/items', tags=['x']", 'items'))
}


Test-Case 'pyalias: an import binds what it imports, a comprehension its own name and a `global` read the module''s' {
    # Three ways a read was bound to the wrong symbol or none: the import named the def beside it in the other branch
    # (and the imported def read as dead), the comprehension's `name` named the module's `__main__` loop variable,
    # and a function declaring `global _CACHE` read its own local where there is none.
    $db = New-PyAliasDb @{
        'beta.py'  = "def scope(row):$nl    return [row]$nl"
        'alpha.py' = "def build(kind):$nl    if kind:$nl        from beta import scope$nl    else:$nl        def scope(row):$nl            return []$nl    return scope(1)$nl" +
            "def pick(got):$nl    return {name for name, _s in got}$nl" +
            "_CACHE = None$nl" + "def load():$nl    global _CACHE$nl    if _CACHE is None:$nl        _CACHE = 1$nl    return _CACHE$nl" +
            "if __name__ == '__main__':$nl    for name in ['x']:$nl        print(name)$nl"
    }
    $i = Invoke-PyAliasQ $db --sql "SELECT 'import=' || name || '|' || bind AS x FROM imports WHERE name = 'scope'"
    Assert-Line $i 'import=scope|beta.py::scope'
    $r = Invoke-PyAliasQ $db --sql ("SELECT 'read=' || r.func || '|' || b.value AS x FROM returns r JOIN returns__binds b ON b.owner = r.id " +
        "WHERE b.node = 'string' AND r.func IN ('pick', 'load')")
    Assert-Line $r 'read=load|alpha.py::_CACHE'
    Assert-NoLine $r 'read=pick|alpha.py::name'
}


Test-Case 'pyalias: a module two folders spell is the one that binds the name taken from it' {
    # No root holds `owner.py`; a bootstrap puts a folder on sys.path. Of the two, only one binds what the line asks for.
    $db = New-PyAliasDb @{
        'a/owner.py' = "def f():$nl    return 1$nl"
        'b/owner.py' = "def g():$nl    return 2$nl"
        'main.py'    = "import owner$nl" + "from owner import g$nl" + "def run():$nl    return owner.f() + g()$nl"
    }
    $i = Invoke-PyAliasQ $db --sql "SELECT 'from=' || name || '|' || from_path AS x FROM imports WHERE name = 'g'"
    Assert-Line $i 'from=g|b/owner.py'
    $c = Invoke-PyAliasQ $db --sql "SELECT 'call=' || callee || '|' || target_path AS x FROM calls WHERE callee = 'owner.f'"
    Assert-Line $c 'call=owner.f|a/owner.py'
}


Test-Case 'pyalias: a test''s conftest import, a namespace folder''s submodule and a package''s flat import find the file python does' {
    # `from conftest import _bundle` is the nearest conftest above the test that defines it, not the root's. `from pkg.sub
    # import mod` with no `pkg/sub/__init__.py` is the file `pkg/sub/mod.py`. `import state` inside a package is not the
    # sibling `pkg/state.py` - a package's folder is on no `sys.path` - while a script's folder still is (`run.py`'s helper).
    $db = New-PyAliasDb @{
        'conftest.py'           = "def other():$nl    return 0$nl"
        't/conftest.py'         = "def _bundle():$nl    return 1$nl"
        't/sub/conftest.py'     = "def _bundle():$nl    return 2$nl"
        't/sub/deep/test_c.py'  = "from conftest import _bundle$nl" + "def test_c():$nl    assert _bundle()$nl"
        'pkg/__init__.py'       = ""
        'pkg/sub/mod.py'        = "def f():$nl    return 1$nl"
        'user.py'               = "from pkg.sub import mod$nl" + "def run():$nl    return mod.f()$nl"
        'svc/state.py'          = "def lock():$nl    return 1$nl"
        'svc/inner/__init__.py' = "import state$nl" + "def go():$nl    return state.lock()$nl"
        'svc/inner/state.py'    = "def other():$nl    return 2$nl"
        'tools/run.py'          = "import helper$nl" + "helper.f()$nl"
        'tools/helper.py'       = "def f():$nl    return 1$nl"
        'more/helper.py'        = "def f():$nl    return 2$nl"
    }
    $i = Invoke-PyAliasQ $db --sql "SELECT 'from=' || module || '|' || name || '|' || from_path AS x FROM imports"
    Assert-Line $i 'from=conftest|_bundle|t/sub/conftest.py'
    Assert-Line $i 'from=pkg.sub|mod|pkg/sub/mod.py'
    Assert-NoLine $i 'from=state||svc/inner/state.py'
    Assert-Line $i 'from=helper||tools/helper.py'
}

Test-Case 'pyalias: a constant aliasing a def is read, a comprehension name inside a nested row is its own, a loose string import is a row' {
    # `ALIAS = util.helper` read in a test is a read of ALIAS (it read as dead), its call still reaches the def. `k * 2` in
    # `[k * 2 for k in xs]` is its own row and reads the comprehension's `k`, not the module loop's. `return
    # importlib.import_module("util")` imports util though no name keeps it.
    $db = New-PyAliasDb @{
        'util.py'   = "def helper(x):$nl    return x$nl" + "class Box:$nl    pass$nl"
        'test_a.py' = "import util$nl" + "ALIAS = util.helper$nl" + "BOX = util.Box$nl" +
            "def test_a():$nl    assert ALIAS(1) == 1$nl    assert BOX.__name__$nl"
        'm.py'      = "PAIRS = {'a': 1}$nl" + "for k, v in PAIRS.items():$nl    print(k, v)$nl" +
            "def g(xs):$nl    return [k * 2 + len(xs) for k in xs]$nl"
        'dyn.py'    = "import importlib$nl" + "def get():$nl    return importlib.import_module('util')$nl"
    }
    $d = Invoke-PyAliasQ $db --dead
    Assert-NoLine $d 'ALIAS'
    Assert-NoLine $d 'BOX'
    $c = Invoke-PyAliasQ $db --sql "SELECT 'call=' || callee || '|' || target_path || '|' || target_name AS x FROM calls WHERE callee = 'ALIAS'"
    Assert-Line $c 'call=ALIAS|util.py|helper'
    $x = Invoke-PyAliasQ $db --sql ("SELECT 'x=' || x.source || '|' || b.value AS x FROM expressions x JOIN expressions__binds b ON b.owner = x.id " +
        "WHERE b.node = 'string' AND x.func = 'g'")
    Assert-Line $x 'x=k * 2 + len(xs)|'
    Assert-NoLine $x 'm.py::k'
    $i = Invoke-PyAliasQ $db --sql "SELECT 'dyn=' || module || '|' || from_path || '|' || used AS x FROM imports WHERE file = (SELECT id FROM files WHERE path = 'dyn.py')"
    Assert-Line $i 'dyn=util|util.py|1'
}

Test-Case 'pyalias: an argument of a dataclass or a NamedTuple fills the field it lands on' {
    # Neither writes an `__init__`; the one python gives it is the fields in order, a ClassVar and a field(init=False) not among them.
    $db = New-PyAliasDb @{
        'shapes.py' = "from dataclasses import dataclass, field, KW_ONLY$nl" + "from typing import NamedTuple, ClassVar$nl" +
            "@dataclass$nl" + "class Order:$nl    code: str$nl    total: ClassVar[int] = 0$nl    seen: list = field(init=False)$nl    size: int = 1$nl" +
            "    _: KW_ONLY$nl    note: str = ''$nl" +
            "class Pair(NamedTuple):$nl    left: int$nl    right: int$nl" +
            "def make():$nl    Order('a', 2, note='x')$nl    return Pair(1, right=2)$nl"
    }
    $r = Invoke-PyAliasQ $db --sql ("SELECT 'arg=' || c.callee || '|' || a.position || '|' || a.keyword || '|' || a.param AS x " +
        "FROM arguments a JOIN calls c ON c.id = a.call WHERE c.callee IN ('Order', 'Pair')")
    Assert-Line $r 'arg=Order|0||code'
    Assert-Line $r 'arg=Order|1||size'
    Assert-Line $r 'arg=Order|-1|note|note'
    Assert-Line $r 'arg=Pair|0||left'
    Assert-Line $r 'arg=Pair|-1|right|right'
}

}
