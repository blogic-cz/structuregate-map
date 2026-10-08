<#
    Which FILE a python name reaches: a bare module name declared in several folders, a file named in a
    STRING (an entry point, a script path), and a CALL bound to the def it runs.

    Kept out of Map.Tests.ps1, which the baseline holds at its size. Its helpers are not leaned on - `-Only`
    runs this suite alone - so the two it needs are written here under names of their own.
#>

$script:PyBindPython = [bool]$script:Python
if (-not $script:PyBindPython) { Write-Host '    (no python on this machine - the binding cases are not run)' }

# The file-level map of a tree, read back as the object a consumer reads.
function Get-PyBindMap([string]$Tree) {
    $path = Join-Path $Tree 'bind-map.json'
    $result = Invoke-Gate --root $Tree --ext .py --map --map-out $path --map-check
    if (-not (Test-Path $path)) { throw "no map was written. Output:`n$($result.Text)" }
    $map = Get-Content $path -Raw | ConvertFrom-Json
    return [pscustomobject]@{ Exit = $result.Exit; Lines = $result.Lines; Text = $result.Text; Map = $map }
}

# One section of the map for one file, as a plain array - an absent entry is none.
function Get-PyBindEdges($Section, [string]$Rel) {
    if (-not $Section) { return ,@() }
    $property = $Section.PSObject.Properties | Where-Object { $_.Name -eq $Rel }
    if (-not $property) { return ,@() }
    return ,@($property.Value)
}

# The deep map of a tree, and one query over it, as rows of text.
function Get-PyBindRows([string]$Tree, [string]$Sql) {
    $db = Join-Path $Tree 'bind.sqlite'
    Assert-Exit (Invoke-Gate --root $Tree --ext .py --map-sqlite $db) 0
    $r = Invoke-Gate --map-query $db --sql $Sql --width 400
    Assert-Exit $r 0
    return $r
}

if ($script:PyBindPython) {

$nl = [string][char]10

Test-Case 'pybind: a bare import binds to the ONE candidate beside the importer' {
    # `import util` in a flat tree reads the util.py in the importer's own folder - that folder is on
    # sys.path, or the import would not work at all. The other util.py is still read by nobody.
    $tree = Use-Tree @{
        'tools/a.py'      = "import util" + $nl + "if __name__ == '__main__':" + $nl + "    util.go()" + $nl
        'tools/util.py'   = "def go():" + $nl + "    return 1" + $nl
        'web/util.py'     = "def go():" + $nl + "    return 2" + $nl
    }
    $found = Get-PyBindMap $tree
    Assert-Exit $found 0
    Assert-Equal ((Get-PyBindEdges $found.Map.imports 'tools/a.py') -join ',') 'tools/util.py' 'the sibling'
    Assert-NoLine $found 'AMBIGUOUS util'
    Assert-Line $found 'NO READER web/util.py'
}

Test-Case 'pybind: a file an AMBIGUOUS import may bind to is not read by nobody' {
    # No sibling decides it, so no edge is drawn - but something plainly imports `util`, and calling
    # either candidate dead said the opposite of the AMBIGUOUS line printed beside it.
    $tree = Use-Tree @{
        'b.py'            = "import util" + $nl + "if __name__ == '__main__':" + $nl + "    pass" + $nl
        'tools/util.py'   = "X = 1" + $nl
        'web/util.py'     = "X = 2" + $nl
    }
    $found = Get-PyBindMap $tree
    Assert-Exit $found 0
    Assert-Line $found 'AMBIGUOUS util'
    Assert-NoLine $found 'NO READER tools/util.py'
    Assert-NoLine $found 'NO READER web/util.py'
}

Test-Case 'pybind: an ENTRY-POINT string is an optional edge to its module' {
    # `console_scripts` names `pkg.cli:main_x` and nothing imports cli.py - a launcher resolves the string.
    # `localhost:8080` has the colon and not the shape, so it names nothing.
    $tree = Use-Tree @{
        'setup.py'        = "from setuptools import setup" + $nl +
                            "setup(entry_points={'console_scripts': ['x = pkg.cli:main_x']}, url='localhost:8080')" + $nl
        'pkg/__init__.py' = ""
        'pkg/cli.py'      = "def main_x():" + $nl + "    return 0" + $nl
        'localhost.py'    = "X = 1" + $nl
    }
    $found = Get-PyBindMap $tree
    Assert-Equal ((Get-PyBindEdges $found.Map.soft_imports 'setup.py') -join ',') 'pkg/cli.py' 'the entry module only'
    Assert-Equal (Get-PyBindEdges $found.Map.imports 'setup.py').Count 0 'a string is not a hard import'
    Assert-NoLine $found 'NO READER pkg/cli.py'
    Assert-Line $found 'NO READER localhost.py'
}

Test-Case 'pybind: a SCRIPT PATH in a string is an optional edge, and a docstring naming one is not' {
    # PyInstaller's `Analysis(['app.py'])`, a subprocess launching `tools/x.py`: the path is the only trace.
    # A docstring mentions files in prose, and an edge drawn from one is an edge drawn from a comment.
    $tree = Use-Tree @{
        'build.py'      = "PACK = ['web/launch.py']" + $nl
        'notes.py'      = "'''See web/other.py for the details.'''" + $nl + "def f():" + $nl + "    '''web/other.py'''" + $nl
        'web/launch.py' = "X = 1" + $nl
        'web/other.py'  = "X = 2" + $nl
    }
    $found = Get-PyBindMap $tree
    Assert-Equal ((Get-PyBindEdges $found.Map.soft_imports 'build.py') -join ',') 'web/launch.py' 'the named script'
    Assert-Equal (Get-PyBindEdges $found.Map.soft_imports 'notes.py').Count 0 'a docstring draws nothing'
    Assert-NoLine $found 'NO READER web/launch.py'
    Assert-Line $found 'NO READER web/other.py'
}

Test-Case 'pybind: a bare main() at module scope is an entry point' {
    $tree = Use-Tree @{ 'runme.py' = "def main():" + $nl + "    return 0" + $nl + "main()" + $nl }
    $found = Get-PyBindMap $tree
    Assert-NoLine $found 'NO READER runme.py'
}

Test-Case 'pybind: a CALL is bound to the def it runs, and a same-named def elsewhere is not called' {
    # Joined by bare name, the dead `legacy.py::run` looked live because `main.py` calls a `run`. Bound
    # through main.py's own import, the call reaches jobs.py and nothing reaches legacy.py.
    $tree = Use-Tree @{
        'jobs.py'     = "def run():" + $nl + "    return 1" + $nl
        'legacy.py'   = "def run():" + $nl + "    return 2" + $nl
        'pkg/__init__.py' = ""
        'pkg/sub.py'  = "class Thing:" + $nl + "    def go(self):" + $nl + "        return self.help()" + $nl +
                        "    def help(self):" + $nl + "        return 0" + $nl
        'main.py'     = "from jobs import run" + $nl + "import pkg.sub as s" + $nl + "def outer(cb):" + $nl +
                        "    def inner():" + $nl + "        return 1" + $nl + "    run()" + $nl + "    inner()" + $nl +
                        "    s.Thing()" + $nl + "    cb()" + $nl + "def shadow():" + $nl + "    run = None" + $nl +
                        "    run()" + $nl
    }
    $db = Join-Path $tree 'bind.sqlite'
    Assert-Exit (Invoke-Gate --root $tree --ext .py --map-sqlite $db) 0
    $sql = "SELECT c.func || '>' || c.callee || '>' || c.target_path || '>' || c.target_name FROM calls c " +
           "JOIN files f ON f.id = c.file ORDER BY f.path, c.line"
    $r = Invoke-Gate --map-query $db --sql $sql
    Assert-Exit $r 0
    Assert-Line $r 'outer>run>jobs.py>run'
    Assert-Line $r 'outer>inner>main.py>outer.inner'
    Assert-Line $r 'outer>s.Thing>pkg/sub.py>Thing'
    Assert-Line $r 'go>self.help>pkg/sub.py>Thing.help'
    # UNBOUND, never guessed: a parameter, and a local that shadows the import.
    Assert-Line $r 'outer>cb>>'
    Assert-Line $r 'shadow>run>>'
    $dead = Invoke-Gate --map-query $db --sql "SELECT 'legacy calls=' || COUNT(*) FROM calls WHERE target_path = 'legacy.py'"
    Assert-Line $dead 'legacy calls=0'
}

Test-Case 'pybind: a file a LAUNCHER names is an entry point, and so is the def it calls' {
    # `[project.scripts]`, a PyInstaller `.spec` and `setup(entry_points=...)` name files no import names.
    $tree = Use-Tree @{
        'pyproject.toml' = '[project.scripts]' + $nl + 'mytool = "cli:main"' + $nl
        'build.spec'     = "a = Analysis(['app.py'])" + $nl
        'tools/setup.py' = "from setuptools import setup" + $nl +
                           "setup(entry_points={'console_scripts': ['x = job:run']})" + $nl
        'cli.py'         = "def main():" + $nl + "    return 0" + $nl + "def spare():" + $nl + "    return 1" + $nl
        'app.py'         = "X = 1" + $nl
        'tools/job.py'   = "def run():" + $nl + "    return 0" + $nl
    }
    $found = Get-PyBindMap $tree
    foreach ($rel in 'cli.py', 'app.py', 'tools/job.py') { Assert-NoLine $found "NO READER $rel" }
    $r = Get-PyBindRows $tree ("SELECT f.path || '>' || fn.qualname || '>' || fn.launched FROM functions fn " +
                               "JOIN files f ON f.id = fn.file")
    Assert-Line $r 'cli.py>main>1'
    Assert-Line $r 'cli.py>spare>0'
    Assert-Line $r 'tools/job.py>run>1'
}

Test-Case 'pybind: an entry-point string whose module does not bind the attribute names nothing' {
    # `session:token` is a cache key with the entry-point shape. session.py has no `token`, so no edge.
    $tree = Use-Tree @{
        'keys.py'    = "CACHE = 'session:token'" + $nl + "LIVE = 'worker:run'" + $nl
        'session.py' = "X = 1" + $nl
        'worker.py'  = "def run():" + $nl + "    return 0" + $nl
    }
    $found = Get-PyBindMap $tree
    Assert-Equal ((Get-PyBindEdges $found.Map.soft_imports 'keys.py') -join ',') 'worker.py' 'only the real entry'
    Assert-Line $found 'NO READER session.py'
}

Test-Case 'pybind: a call through a package RE-EXPORT binds to the file that defines the def' {
    $tree = Use-Tree @{
        'rx/__init__.py' = "from .impl import run" + $nl
        'rx/impl.py'     = "def run():" + $nl + "    return 1" + $nl
        'use.py'         = "from rx import run" + $nl + "def go():" + $nl + "    run()" + $nl
    }
    $r = Get-PyBindRows $tree "SELECT callee || '>' || target_path || '>' || target_name FROM calls"
    Assert-Line $r 'run>rx/impl.py>run'
}

Test-Case 'pybind: getattr with a COMPUTED name reaches every def of the module, a literal one reaches one' {
    $tree = Use-Tree @{
        'plugins.py' = "def a():" + $nl + "    return 1" + $nl
        'loader.py'  = "import plugins" + $nl + "def pick(name):" + $nl + "    return getattr(plugins, name)" + $nl +
                       "def one():" + $nl + "    return getattr(plugins, 'a')" + $nl
    }
    $r = Get-PyBindRows $tree ("SELECT func || '>' || callee || '>' || target_path || '>' || target_name FROM calls " +
                               "WHERE callee = 'getattr'")
    Assert-Line $r 'pick>getattr>plugins.py>*'
    Assert-Line $r 'one>getattr>plugins.py>a'
}

Test-Case 'pybind: with SEVERAL roots a call binds across them and is named with its root prefix' {
    # Each root is on sys.path in such a tree. Bound against the first root alone, `lib_b.go` bound to
    # nothing, and a sibling bound to `helper.py` - a path no files row carries, since rows say `app/...`.
    $tree = Use-Tree @{
        'app/main.py'   = "import helper" + $nl + "from lib_b import go" + $nl + "def run():" + $nl +
                          "    go()" + $nl + "    helper.aid()" + $nl
        'app/helper.py' = "def aid():" + $nl + "    return 1" + $nl
        'lib/lib_b.py'  = "def go():" + $nl + "    return 2" + $nl
    }
    $db = Join-Path $tree 'bind.sqlite'
    Assert-Exit (Invoke-Gate --root (Join-Path $tree 'app') --root (Join-Path $tree 'lib') --ext .py --map-sqlite $db) 0
    $r = Invoke-Gate --map-query $db --sql "SELECT callee || '>' || target_path || '>' || target_name FROM calls"
    Assert-Exit $r 0
    Assert-Line $r 'go>lib/lib_b.py>go'
    Assert-Line $r 'helper.aid>app/helper.py>aid'
    $orphans = Invoke-Gate --map-query $db --sql ("SELECT 'orphans=' || count(*) FROM calls c WHERE c.target_path <> '' " +
                                                  "AND NOT EXISTS (SELECT 1 FROM files f WHERE f.path = c.target_path)")
    Assert-Line $orphans 'orphans=0'
}

Test-Case 'pybind: a database the OLDER extractor wrote is re-read, not left without the bound columns' {
    # The deep map is incremental: an unchanged file keeps its rows. Rows written before calls were bound
    # have no target_path, and a query naming it failed with `no such column` until each file was edited.
    $tree = Use-Tree @{
        'jobs.py' = "def run():" + $nl + "    return 1" + $nl
        'main.py' = "from jobs import run" + $nl + "def go():" + $nl + "    run()" + $nl
    }
    $db = Join-Path $tree 'bind.sqlite'
    Assert-Exit (Invoke-Gate --root $tree --ext .py --map-sqlite $db) 0
    # AS THE OLD EXTRACTOR LEFT IT: every sha the plain digest of the text, and no bound columns.
    $aged = Join-Path $tree 'age.pyhelper'
    [System.IO.File]::WriteAllText($aged, (@(
        'import hashlib, os, sqlite3, sys',
        'db = sqlite3.connect(sys.argv[1])',
        'for rid, path in db.execute("SELECT id, path FROM files").fetchall():',
        '    text = open(os.path.join(sys.argv[2], path), encoding="utf-8").read()',
        '    db.execute("UPDATE files SET sha = ? WHERE id = ?", (hashlib.sha256(text.encode()).hexdigest()[:16], rid))',
        'db.execute("ALTER TABLE calls DROP COLUMN target_path")',
        'db.execute("ALTER TABLE calls DROP COLUMN target_name")',
        'db.commit()') -join $nl))
    & $script:Python $aged $db $tree
    if ($LASTEXITCODE -ne 0) { throw 'the database could not be aged' }
    Assert-Exit (Invoke-Gate --root $tree --ext .py --map-sqlite $db) 0
    $r = Invoke-Gate --map-query $db --sql "SELECT callee || '>' || target_path FROM calls"
    Assert-Exit $r 0
    Assert-Line $r 'run>jobs.py'
}

Test-Case 'pybind: an INSTANCE binds through the class that made it, and a module no root holds binds when unique' {
    # `state.handler.run()` reached state.py, where `handler = Handler()`: the call is Handler.run.
    # `state.raw` came from a FUNCTION, so its class is unknown and the call stays unbound - binding it to
    # `raw.go` bound to no def at all. `from deep import go` works only because a bootstrap puts `tools/` on
    # sys.path; exactly one mapped file spells it, so that is the one.
    $tree = Use-Tree @{
        'state.py'        = "from model import Handler" + $nl + "handler = Handler()" + $nl +
                            "def make():" + $nl + "    return 1" + $nl + "raw = make()" + $nl +
                            "def tidy(name):" + $nl + "    name = name.strip()" + $nl + "    return name.lower()" + $nl
        'model.py'        = "class Handler:" + $nl + "    def run(self):" + $nl + "        return 1" + $nl +
                            "    def help(self):" + $nl + "        return 2" + $nl
        'use.py'          = "import state" + $nl + "from model import Handler" + $nl + "def run():" + $nl +
                            "    state.handler.run()" + $nl + "    Handler.help(None)" + $nl + "    Handler.inherited()" + $nl +
                            "    state.raw.go()" + $nl + "    local = Handler()" + $nl + "    local.help()" + $nl
        'app/sub/main.py' = "from deep import go" + $nl + "def run():" + $nl + "    go()" + $nl
        'tools/deep.py'   = "def go():" + $nl + "    return 3" + $nl
    }
    $r = Get-PyBindRows $tree "SELECT callee || '>' || target_path || '>' || target_name FROM calls"
    Assert-Line $r 'state.handler.run>model.py>Handler.run'
    Assert-Line $r 'Handler.help>model.py>Handler.help'
    Assert-Line $r 'Handler.inherited>>'
    Assert-Line $r 'state.raw.go>>'
    Assert-Line $r 'local.help>model.py>Handler.help'
    Assert-Line $r 'go>tools/deep.py>go'
}

Test-Case 'pybind: a call through an ALIAS runs what the alias names, and an alias loop ends' {
    # `_strip = fold` left its `_strip()` calls unbound on a real tree although the import of `_strip` bound.
    # An alias to an external function stays unbound; `x = y` / `y = x` must end, not recurse.
    $tree = Use-Tree @{
        'lib.py'  = "import os" + $nl + "def fold(x):" + $nl + "    return x" + $nl + "_strip = fold" + $nl + "_join = os.path.join" + $nl
        'use.py'  = "from lib import _strip, _join" + $nl + "import lib" + $nl + "def go():" + $nl + "    _strip(1)" + $nl +
                    "    lib._strip(2)" + $nl + "    f = lib.fold" + $nl + "    f(3)" + $nl + "    _join('a')" + $nl
        'loop.py' = "x = y" + $nl + "y = x" + $nl + "def go():" + $nl + "    return x()" + $nl
    }
    $r = Get-PyBindRows $tree "SELECT callee || '>' || target_path || '>' || target_name FROM calls"
    Assert-Line $r '_strip>lib.py>fold'
    Assert-Line $r 'lib._strip>lib.py>fold'
    Assert-Line $r 'f>lib.py>fold'
    Assert-Line $r '_join>>'
    Assert-Line $r 'x>>'
}

Test-Case 'pybind: of several same-named modules the import binds the one that declares every name it takes' {
    # Three util.py, none beside the importer and none under a root: only the bootstrap's sys.path finds
    # one. `from util import ranked, WRITE` would raise ImportError against two of them.
    $tree = Use-Tree @{
        'app/cmd.py'         = "from util import ranked, WRITE" + $nl + "def go():" + $nl + "    return ranked(WRITE)" + $nl
        'keys/util.py'       = "WRITE = 1" + $nl + "def ranked(x):" + $nl + "    return x" + $nl
        'builder/util.py'    = "def ranked(x):" + $nl + "    return 2" + $nl
        'other/util.py'      = "WRITE = 3" + $nl
    }
    $r = Get-PyBindRows $tree ("SELECT callee || '>' || target_path FROM calls UNION ALL " +
                               "SELECT 'read>' || reads || '>' || binds FROM returns")
    Assert-Line $r 'ranked>keys/util.py'
    # The READ is bound too, position for position: `ranked` and `WRITE` both come from keys/util.py.
    Assert-Line $r 'read>["WRITE", "ranked"]>["keys/util.py::WRITE", "keys/util.py::ranked"]'
}

Test-Case 'pybind: the FILE map picks, of several same-named modules, the one binding every imported name' {
    # The same rule in the graph: `from util import ranked, WRITE` and the entry string `cli:main_x` each
    # have several files to choose from, no sibling among them, and exactly one that binds the names.
    $tree = Use-Tree @{
        'app/cmd.py'         = "from util import ranked, WRITE" + $nl + "STEPS = ['cli:main_x']" + $nl
        'keys/util.py'       = "WRITE = 1" + $nl + "def ranked(x):" + $nl + "    return x" + $nl
        'builder/util.py'    = "def ranked(x):" + $nl + "    return 2" + $nl
        'other/util.py'      = "WRITE = 3" + $nl
        'one/cli.py'         = "def main_x():" + $nl + "    return 0" + $nl
        'two/cli.py'         = "def main_y():" + $nl + "    return 0" + $nl
    }
    $found = Get-PyBindMap $tree
    Assert-Equal ((Get-PyBindEdges $found.Map.imports 'app/cmd.py') -join ',') 'keys/util.py' 'the util that binds both'
    Assert-Equal ((Get-PyBindEdges $found.Map.soft_imports 'app/cmd.py') -join ',') 'one/cli.py' 'the cli that binds main_x'
    Assert-NoLine $found 'AMBIGUOUS util'
    Assert-Line $found 'NO READER builder/util.py'
}

Test-Case 'pybind: a read names the SYMBOL it binds, so two files defining ROOT are told apart' {
    $tree = Use-Tree @{
        'config.py' = "ROOT = 'a'" + $nl
        'other.py'  = "ROOT = 'b'" + $nl
        'use.py'    = "from config import ROOT" + $nl + "def where(path):" + $nl + "    return ROOT + path" + $nl
    }
    $r = Get-PyBindRows $tree "SELECT reads || '>' || binds FROM returns"
    Assert-Line $r '["ROOT", "path"]>["config.py::ROOT", ""]'
}

}
