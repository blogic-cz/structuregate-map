<#
    The LENSES over the python deep map - `--dead`, `--defaults`, `--unused-imports`, `--limit` - each an
    ANALYSIS over the rows PyBind.Tests.ps1 proves are bound right. Split from that suite when it reached the
    gate's ceiling. Its helpers are its own: `-Only PyLens` runs this file alone.
#>

$script:PyLensPython = [bool]$script:Python
if (-not $script:PyLensPython) { Write-Host '    (no python on this machine - the lens cases are not run)' }

if ($script:PyLensPython) {

$nl = [string][char]10

Test-Case 'pylens: --defaults says whether any caller passes a defaulted parameter, and when it cannot tell' {
    # One def per verdict and per mapping rule: `self` skipped for an instance call and passed by hand for a
    # class-qualified one, a static method taking no `self`, `Thing(...)` filling `__init__`, a splat, an
    # unbound call of the same name, a route handler, and a def nothing calls.
    $tree = Use-Tree @{
        'lib.py'    = "class Thing:" + $nl + "    def __init__(self, a, b=2):" + $nl + "        self.a = a" + $nl +
                      "    def go(self, x, y=None, *, z=5):" + $nl + "        return x" + $nl +
                      "    @staticmethod" + $nl + "    def st(p, q=1):" + $nl + "        return p" + $nl +
                      "def helper(a, b=3):" + $nl + "    return a" + $nl + "def splatted(a, b=4):" + $nl + "    return a" + $nl +
                      "def byname(a, b=5):" + $nl + "    return a" + $nl + "def lonely(a, b=6):" + $nl + "    return a" + $nl +
                      "class Pair:" + $nl + "    def __init__(self, a, b=8):" + $nl + "        self.a = a" + $nl
        'web.py'    = "app = object()" + $nl
        'routes.py' = "from web import app" + $nl + "@app.get('/')" + $nl + "def handler(q=7):" + $nl + "    return q" + $nl
        'use.py'    = "from lib import Thing, helper, splatted, byname, Pair" + $nl + "t = Thing(1)" + $nl + "t.go(1, 2)" + $nl +
                      "Thing.go(t, 3, z=9)" + $nl + "t.st(1)" + $nl + "helper(1)" + $nl + "splatted(*[1, 2])" + $nl +
                      "byname(1)" + $nl + "def run(obj):" + $nl + "    return obj.byname(1, 2)" + $nl + "Pair(1, 2)" + $nl
    }
    $db = Join-Path $tree 'bind.sqlite'
    Assert-Exit (Invoke-Gate --root $tree --ext .py --map-sqlite $db) 0
    $r = Invoke-Gate --map-query $db --defaults --limit 0 --width 200
    Assert-Exit $r 0
    function Get-Verdict([string]$Def, [string]$Param) {
        foreach ($line in $r.Lines) {
            $cells = $line.Split([char[]]' ', [StringSplitOptions]::RemoveEmptyEntries)
            if ($cells.Count -ge 6 -and $cells[2] -ceq $Def -and $cells[3] -ceq $Param) { return $line }
        }
        throw "no --defaults row for $Def.$Param. Output:`n$($r.Text)"
    }
    foreach ($case in @(@('Thing.__init__', 'b', 'never passed'), @('Thing.st', 'q', 'never passed'),
                        @('helper', 'b', 'never passed'), @('Thing.go', 'z', 'passed'), @('Thing.go', 'y', 'passed'),
                        @('splatted', 'b', 'cannot tell'), @('byname', 'b', 'cannot tell'),
                        @('handler', 'q', 'cannot tell'), @('lonely', 'b', 'no caller'), @('Pair.__init__', 'b', 'passed'))) {
        $line = Get-Verdict $case[0] $case[1]
        # `never passed` CONTAINS `passed`: without the second test a broken mapping still read as right.
        if (-not $line.Contains($case[2]) -or ($case[2] -eq 'passed' -and $line.Contains('never passed'))) {
            throw "$($case[0]).$($case[1]) is not '$($case[2])': $line"
        }
    }
    # `Thing.go(t, 3, ...)` passes self by hand: its 3 is x, never y - so y is passed ONCE, at line 3.
    $y = Get-Verdict 'Thing.go' 'y'
    if (-not $y.Contains('1x: use.py:3=2')) { throw "y was credited with the wrong sites: $y" }
    Assert-Line $r 'splats'
    Assert-Line $r '1 unbound call(s) named byname'
    # `bound` answers over the bound calls alone: `byname(1)` is bound and leaves b alone, whatever the
    # unbound `obj.byname(1, 2)` does.
    $b = Get-Verdict 'byname' 'b'
    if (-not ($b.Contains('cannot tell') -and $b.Contains('never passed'))) { throw "byname.b has no bound verdict: $b" }
    Assert-Line $r 'handed to @app.get'
}

Test-Case 'pylens: --unused-imports lists import lines nothing uses, re-exports followed through' {
    # `hidden` is re-exported and taken by an import nobody uses: both lines are dead. `shown` is taken and
    # used, `typed` is read as `pkg.typed`, `listed` is in `__all__`, `Path` only annotates, `os` is unused.
    $tree = Use-Tree @{
        'pkg/__init__.py' = "from .impl import shown, hidden, typed" + $nl + "from .impl import listed" + $nl + "__all__ = ['listed']" + $nl +
                            "from . import impl" + $nl
        'pkg/impl.py'     = "def shown():" + $nl + "    return 1" + $nl + "def hidden():" + $nl + "    return 2" + $nl +
                            "class typed:" + $nl + "    pass" + $nl + "def listed():" + $nl + "    return 3" + $nl
        'client.py'       = "from __future__ import annotations" + $nl + "from pkg import shown" + $nl + "import pkg" + $nl +
                            "import os" + $nl + "from pathlib import Path" + $nl + "def go(p: 'Path') -> None:" + $nl +
                            "    return shown() + pkg.typed" + $nl
        'other.py'        = "from pkg import hidden" + $nl
        'probe.py'        = "try:" + $nl + "    import optional_thing" + $nl + "except ImportError:" + $nl + "    optional_thing = None" + $nl
    }
    $db = Join-Path $tree 'bind.sqlite'
    Assert-Exit (Invoke-Gate --root $tree --ext .py --map-sqlite $db) 0
    $r = Invoke-Gate --map-query $db --unused-imports --limit 0 --width 200
    Assert-Exit $r 0
    Assert-Line $r 'import os'
    Assert-Line $r 'from .impl import hidden'
    Assert-Line $r 're-exported, but no importer uses it: other.py:1'
    Assert-Line $r 'from pkg import hidden'
    # A package's own `from . import impl` takes impl from ITSELF; it is never its own taker.
    $own = @($r.Lines | Where-Object { $_.Contains('from . import impl') })
    if ($own.Count -ne 1 -or -not $own[0].Contains('never used in its file')) {
        throw "a package's own submodule import was not reported as simply unused: $own"
    }
    foreach ($live in 'import shown', 'import typed', 'import listed', 'import Path', '__future__', 'optional_thing', 'import pkg') {
        Assert-NoLine $r $live
    }
}

Test-Case 'pylens: --limit 0 prints every row, and no --limit still stops at 50' {
    # An audit counting rows asked for `--limit 0` and got only the first 50 with nothing to say it was cut.
    $body = (1..60 | ForEach-Object { "def f$($_)():" + $nl + "    return $($_)" + $nl }) -join ''
    $tree = Use-Tree @{ 'many.py' = $body }
    $db = Join-Path $tree 'bind.sqlite'
    Assert-Exit (Invoke-Gate --root $tree --ext .py --map-sqlite $db) 0
    $all = Invoke-Gate --map-query $db --sql "SELECT name FROM functions" --limit 0
    Assert-Line $all 'f60'
    Assert-NoLine $all 'more row(s)'
    Assert-Line (Invoke-Gate --map-query $db --sql "SELECT name FROM functions") '10 more row(s)'
}

Test-Case 'pylens: --dead lists the defs nothing reaches, and none of the ways a def is reached' {
    # The recipe this replaced listed a route handler, a dunder and a def named in a dispatch table as dead,
    # and kept `readable` alive because `_readable` ends the same way. Each of those is one def here.
    $tree = Use-Tree @{
        'lib.py'  = "def used():" + $nl + "    return 1" + $nl + "def gone():" + $nl + "    return 2" + $nl +
                    "def readable():" + $nl + "    return 3" + $nl + "def _readable():" + $nl + "    return 4" + $nl +
                    "def by_name():" + $nl + "    return 5" + $nl + "def callback():" + $nl + "    return 6" + $nl +
                    "def by_entry():" + $nl + "    return 7" + $nl + "def public():" + $nl + "    return 8" + $nl +
                    "__all__ = ['public']" + $nl +
                    "class Box:" + $nl + "    def __len__(self):" + $nl + "        return 0" + $nl
        'app.py'  = "import lib" + $nl + "from web import app" + $nl + "STEPS = ['by_name', 'lib:by_entry']" + $nl +
                    "@app.get('/')" + $nl + "def route():" + $nl + "    return lib.used()" + $nl +
                    "def go():" + $nl + "    lib._readable()" + $nl + "    return sorted([], key=lib.callback)" + $nl
        'web.py'  = "app = object()" + $nl
        'jobs.py'   = "def work():" + $nl + "    return 1" + $nl
        'legacy.py' = "def work():" + $nl + "    return 2" + $nl
        'main.py'   = "from jobs import work" + $nl + "if __name__ == '__main__':" + $nl + "    work()" + $nl
    }
    $db = Join-Path $tree 'bind.sqlite'
    Assert-Exit (Invoke-Gate --root $tree --ext .py --map-sqlite $db) 0
    $r = Invoke-Gate --map-query $db --dead --limit 100
    Assert-Exit $r 0
    # The DEF column is the last one, read as a whole word so `readable` is never found inside `_readable`.
    $defs = @($r.Lines | ForEach-Object { ($_.Split([char[]]' ', [StringSplitOptions]::RemoveEmptyEntries))[-1] })
    foreach ($dead in 'gone', 'readable', 'go') {
        if ($defs -cnotcontains $dead) { throw "$dead is not listed. Output:`n$($r.Text)" }
    }
    foreach ($live in 'used', '_readable', 'by_name', 'by_entry', 'public', 'callback', 'Box.__len__', 'route') {
        if ($defs -ccontains $live) { throw "$live is live but listed. Output:`n$($r.Text)" }
    }
    # BOUND, NOT NAMED: main.py calls jobs.work, so the other `work` is dead however the name reads.
    Assert-Line (Invoke-Gate --map-query $db --dead legacy.py) 'work'
    Assert-Line (Invoke-Gate --map-query $db --dead jobs.py) '(no rows)'
    Assert-Line (Invoke-Gate --map-query $db --dead lib.py) 'readable'
    Assert-Line (Invoke-Gate --map-query $db --dead web.py) '(no rows)'
}

Test-Case 'pylens: --dead skips an OVERRIDE and a class with a base outside the map, and lists dead CONSTANTS' {
    # `self.step()` in Base binds to Base.step, and runs Derived.step for a Derived: the override is live.
    # Constants follow the def rules, and four more ways a constant is reached: one name of a tuple read,
    # a caught exception tuple, an import by name, and a computed getattr over the whole module.
    $tree = Use-Tree @{
        'shapes.py' = "class Base:" + $nl + "    def run(self):" + $nl + "        return self.step()" + $nl +
                      "    def step(self):" + $nl + "        return 1" + $nl +
                      "class Derived(Base):" + $nl + "    def step(self):" + $nl + "        return 2" + $nl +
                      "    def spare(self):" + $nl + "        return 3" + $nl +
                      "class Oops(Exception):" + $nl + "    def describe(self):" + $nl + "        return 4" + $nl
        'consts.py' = "UNUSED = 1" + $nl + "USED = 2" + $nl + "LEFT, RIGHT = 1, 2" + $nl +
                      "ERRORS = (ValueError, KeyError)" + $nl + "KEPT = 3" + $nl + "def helper():" + $nl + "    return 0" + $nl +
                      "def guarded():" + $nl + "    try:" + $nl + "        return helper()" + $nl +
                      "    except ERRORS:" + $nl + "        return 0" + $nl
        'use.py'    = "from consts import USED, RIGHT, KEPT, guarded" + $nl + "from shapes import Derived" + $nl +
                      "def go():" + $nl + "    return USED + RIGHT + Derived().run() + guarded()" + $nl
        'cfg.py'    = "SETTING = 1" + $nl + "SPELLED = 2" + $nl
        'look.py'   = "import cfg" + $nl + "WANT = 'SPELLED'" + $nl + "def pick(name):" + $nl + "    return getattr(cfg, name)" + $nl
        # A RE-EXPORT counts only when an import of it is used downstream: `shown` is, `hidden` is not.
        'pkg/__init__.py' = "from .impl import shown, hidden" + $nl
        'pkg/impl.py'     = "def shown():" + $nl + "    return 1" + $nl + "def hidden():" + $nl + "    return 2" + $nl
        'client.py'       = "from pkg import shown" + $nl + "def go():" + $nl + "    return shown()" + $nl
        # A USED import of an EXTERNAL package names nothing here: the local `wrapped` of the same name is dead.
        'wrap.py'         = "from extpkg import wrapped as _ext" + $nl + "def wrapped():" + $nl + "    return _ext()" + $nl
    }
    $db = Join-Path $tree 'bind.sqlite'
    Assert-Exit (Invoke-Gate --root $tree --ext .py --map-sqlite $db) 0
    $r = Invoke-Gate --map-query $db --dead --limit 100
    Assert-Exit $r 0
    $names = @($r.Lines | ForEach-Object { ($_.Split([char[]]' ', [StringSplitOptions]::RemoveEmptyEntries))[-1] })
    # KEPT is imported and never used: dead, and listed WITH the import that has to go too. SETTING is
    # reachable only through a computed getattr, which spares no constant; SPELLED is spelled by a literal.
    foreach ($dead in 'Derived.spare', 'UNUSED', 'LEFT', 'KEPT', 'SETTING', 'hidden', 'wrapped') {
        if ($names -cnotcontains $dead) { throw "$dead is not listed. Output:`n$($r.Text)" }
    }
    foreach ($live in 'Derived.step', 'Base.step', 'Oops.describe', 'USED', 'RIGHT', 'ERRORS', 'helper', 'SPELLED', 'shown') {
        if ($names -ccontains $live) { throw "$live is live but listed. Output:`n$($r.Text)" }
    }
    Assert-Line (Invoke-Gate --map-query $db --dead consts.py) 'const'
    Assert-Line (Invoke-Gate --map-query $db --dead consts.py) 'use.py:1'
    Assert-Line (Invoke-Gate --map-query $db --dead pkg/impl.py) 'pkg/__init__.py:1'
}

}
