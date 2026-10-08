<#
    --map-sqlite and --map-query: the DEEP python map, and the lenses over it.

    WHAT IS ASSERTED IS A ROW, not a count in a sentence. This half exists so a question about an
    EXPRESSION can be asked at all, and every claim it makes - what a call is called, what an expression
    reads, which function a row sits in - is acted on directly. So the cases open the database and check
    the cells.

    The database is written by python's own `sqlite3`, so these need a `python` on PATH and are skipped
    with a printed line rather than silently when there is none.
#>

$script:PyRowsPython = [bool]$script:Python
if (-not $script:PyRowsPython) { Write-Host '    (no python on this machine - the deep map cases are not run)' }

# Build the deep map over a throwaway tree and hand back the database path.
function New-Db([hashtable]$Files) {
    $tree = Use-Tree $Files
    $db = Join-Path $tree 'map.sqlite'
    $result = Invoke-Gate --root $tree --ext .py --map --map-out (Join-Path $tree 'm.json') --map-sqlite $db
    Assert-Exit $result 0
    if (-not (Test-Path $db)) { throw "no database was written. Output:`n$($result.Text)" }
    return $db
}

# One SQL query through the exe's own lens, as rows of text.
# `Path` and not `Db`: PowerShell reserves `-db` as the alias of the common `-Debug` parameter, so a
# parameter named that cannot be bound at all.
function Invoke-Q {
    param([string]$Path, [Parameter(ValueFromRemainingArguments)][object[]]$LensArgs)
    $result = Invoke-Gate --map-query $Path @LensArgs
    Assert-Exit $result 0
    return $result
}

if ($script:PyRowsPython) {

Test-Case 'pyrows: a call is a row, with its callee, its scope and its own source' {
    $db = New-Db @{ 'a.py' = "def outer():" + [char]10 + "    return helper.run(1, key=2)" + [char]10 }
    $r = Invoke-Q $db --sql "SELECT callee, func, args, kwargs, source FROM calls"
    Assert-Line $r 'helper.run'
    Assert-Line $r 'outer'
    # SCOPE IS RECORDED, NOT INFERRED: the row says which function it sits in.
    Assert-Line $r 'helper.run(1, key=2)'
}

Test-Case 'pyrows: an expression carries what it READS and CALLS, so a query never re-parses it' {
    $db = New-Db @{ 'a.py' = "value = (STORE.get(name) or {}).items()" + [char]10 }
    $r = Invoke-Q $db --sql "SELECT reads, calls FROM expressions WHERE source LIKE '%items%' LIMIT 1"
    Assert-Line $r 'STORE.get'
    Assert-Line $r 'name'
}

Test-Case 'pyrows: the tables a tree produces' {
    $db = New-Db @{
        'a.py' = "import os" + [char]10 + "TOTAL = 3" + [char]10 + "__all__ = ['go']" + [char]10 +
                 "class Thing:" + [char]10 + "    @property" + [char]10 + "    def go(self):" + [char]10 +
                 "        if TOTAL:" + [char]10 + "            raise ValueError('no')" + [char]10 +
                 "        return 'yes'" + [char]10
    }
    foreach ($pair in @(@('imports','os'), @('consts','TOTAL'), @('exports','go'), @('classes','Thing'),
                        @('functions','go'), @('decorators','property'), @('branches','if'),
                        @('raises','ValueError'), @('returns','yes'), @('string_literals','yes'))) {
        $r = Invoke-Q $db --sql "SELECT * FROM $($pair[0])"
        Assert-Line $r $pair[1]
    }
}

Test-Case 'pyrows: a guarded import is marked optional in the row' {
    # The same rule the file-level half applies: an import is optional because of what ENCLOSES it.
    $db = New-Db @{ 'a.py' = "try:" + [char]10 + "    import plugin" + [char]10 + "except ImportError:" +
                             [char]10 + "    plugin = None" + [char]10 }
    $r = Invoke-Q $db --sql "SELECT module, guarded FROM imports WHERE module = 'plugin'"
    Assert-Line $r 'plugin'
    Assert-Line $r '1'
}

Test-Case 'pyrows: the source is carried, FTS5-indexed, and --cat prints it' {
    # The file-level map POINTS at files and never carries them. This is the half that does.
    $db = New-Db @{ 'a.py' = "def go():" + [char]10 + "    return 'a distinctive phrase'" + [char]10 }
    Assert-Line (Invoke-Q $db --text 'distinctive') 'a.py'
    Assert-Line (Invoke-Q $db --cat 'a.py' --lines 2-2) 'distinctive phrase'
}

Test-Case 'pyrows: --cat takes the file a fragment NAMES, and refuses a guess or a range past the end' {
    # `LIKE %fragment%` took whichever row came first: `RecordService.cs` printed the shorter
    # `OldRecordService.cs` with a range past its end, beside a file of exactly that name.
    $two = "x = 1" + [char]10 + "y = 2" + [char]10
    $db = New-Db @{
        'a/cart_record.py'      = "z = 0" + [char]10
        'b/record.py'           = $two
        'c/twin.py'             = $two
        'd/twin.py'             = $two
    }
    Assert-Line (Invoke-Q $db --cat 'record.py' --lines 2-2) 'b/record.py  lines 2-2 of 2'
    $many = Invoke-Gate --map-query $db --cat 'twin.py'
    Assert-Exit $many 2
    Assert-Line $many "'twin.py' matches 2 files"
    Assert-Line $many 'd/twin.py'
    $past = Invoke-Gate --map-query $db --cat 'b/record.py' --lines 3-4
    Assert-Exit $past 2
    Assert-Line $past 'b/record.py has 2 lines'
}

Test-Case 'pyrows: a file that does not parse is REPORTED and the rest still lands' {
    $tree = Use-Tree @{
        'good.py' = "def go():" + [char]10 + "    return 1" + [char]10
        'bad.py'  = "def broken(" + [char]10
    }
    $db = Join-Path $tree 'map.sqlite'
    $result = Invoke-Gate --root $tree --ext .py --map --map-out (Join-Path $tree 'm.json') `
        --map-sqlite $db --map-check
    Assert-Exit $result 1
    Assert-Line $result 'UNPARSED'
    Assert-Line $result 'bad.py'
    # NOT all-or-nothing: the file that parsed is still in the database.
    Assert-Line (Invoke-Q $db --sql "SELECT path FROM files") 'good.py'
}

Test-Case 'pyrows: the database is REBUILT, so a deleted file LEAVES it' {
    # It is a cache of a tree as it stands. A database that accumulated across runs would answer with rows
    # describing files the tree no longer has - which is worse than no database, because the rows look real.
    $tree = Use-Tree @{
        'keep.py' = 'def go():' + [char]10 + '    return 1' + [char]10
        'drop.py' = 'def gone():' + [char]10 + '    return 2' + [char]10
    }
    $db = Join-Path $tree 'map.sqlite'
    $call = @('--root', $tree, '--ext', '.py', '--map', '--map-out', (Join-Path $tree 'm.json'),
              '--map-sqlite', $db)
    Assert-Exit (Invoke-Gate @call) 0
    Assert-Line (Invoke-Q $db --sql 'SELECT path FROM files') 'drop.py'

    Remove-Item (Join-Path $tree 'drop.py') -Force
    Assert-Exit (Invoke-Gate @call) 0
    $after = Invoke-Q $db --sql 'SELECT path FROM files'
    Assert-Line $after 'keep.py'
    Assert-NoLine $after 'drop.py'
}

Test-Case 'pyrows: the deep half accounts for every file it was given' {
    # It answers in TABLES, not per file, so the guarantee the mapping halves get from a record per file
    # is checked the only way it can be here: against the count on its DONE line. Without it the run
    # reported every file as unread by the half - one error per file - for a map that was fine.
    $tree = Use-Tree @{
        'a.py' = 'X = 1' + [char]10
        'b.py' = 'Y = 2' + [char]10
    }
    $result = Invoke-Gate --root $tree --ext .py --map --map-out (Join-Path $tree 'm.json') `
        --map-sqlite (Join-Path $tree 'map.sqlite') --map-check
    Assert-Exit $result 0
    Assert-NoLine $result 'returned nothing for this file'
    Assert-NoLine $result 'said nothing about the rest'
}

Test-Case 'pyrows: --map-query needs no --map, and reports a database that is not there' {
    $missing = Join-Path ([System.IO.Path]::GetTempPath()) 'no-such-map.sqlite'
    $result = Invoke-Gate --map-query $missing --tables
    Assert-Exit $result 2
    Assert-Line $result 'no database at'
}

Test-Case 'pyrows: --tables names the root it was built from' {
    # A number taken from a cache built against a tree that has since changed is a number nobody can check.
    $db = New-Db @{ 'a.py' = "X = 1" + [char]10 }
    $r = Invoke-Q $db --tables
    Assert-Line $r 'root'
    Assert-Line $r 'files'
}

Test-Case 'pyrows: --find reaches every column that carries a name' {
    $db = New-Db @{
        'a.py' = "def build_index():" + [char]10 + "    return 1" + [char]10
        'b.py' = "import a" + [char]10 + "v = a.build_index()" + [char]10
    }
    $r = Invoke-Q $db --find 'build_index'
    Assert-Line $r 'functions'
    Assert-Line $r 'calls'
}

Test-Case 'pyrows: --map-sqlite without --map builds the DEEP map and nothing else' {
    # It used to be refused. It is now the mode a per-turn hook runs: the graph re-reads the whole tree on
    # every run by design, and the rows do not need it. The flags that DO need --map still say so.
    $tree = Use-Tree @{ 'a.py' = "X = 1`n" }
    $db = Join-Path $tree 'x.sqlite'
    $result = Invoke-Gate --root $tree --ext .py --map-sqlite $db
    Assert-Exit $result 0
    if (-not (Test-Path $db)) { throw "no database was written. Output:`n$($result.Text)" }
    if (Test-Path (Join-Path $tree 'buildmap.json')) { throw 'a JSON map was written by a run that asked for none' }
    Assert-Line (Invoke-Q $db --sql 'SELECT name FROM consts') 'X'

    $refused = Invoke-Gate --root $tree --ext .py --map-sqlite $db --map-if-stale
    Assert-Exit $refused 2
    Assert-Line $refused 'need --map'
}

}

# ---------------------------------------------------------------------------------------------------
# Incremental: a file whose sha has not moved is not re-read, and the cache still cannot lie
# ---------------------------------------------------------------------------------------------------

# A tree of three files and the command that maps it, so a case can run it again and again.
function New-IncTree {
    $tree = Use-Tree @{
        'a.py' = 'def one():' + [char]10 + '    return helper.run(1)' + [char]10
        'b.py' = 'def two():' + [char]10 + '    return helper.run(2)' + [char]10
        'c.py' = 'def three():' + [char]10 + '    return helper.run(3)' + [char]10
    }
    return $tree
}

function Invoke-Map([string]$Tree) {
    return Invoke-Gate --root $Tree --ext .py --map --map-out (Join-Path $Tree 'm.json') `
        --map-sqlite (Join-Path $Tree 'map.sqlite')
}

# `Get-Scalar` and NOT `Get-Count`: every suite is dot-sourced into ONE scope, in name order, so a helper
# named like the harness's own silently replaces it for every suite that runs later. This file sorts before
# Ratchet, Rules, TsGate and Walk, and a `Get-Count` here broke one case in each of them.
function Get-Scalar([string]$Tree, [string]$Sql) {
    $r = Invoke-Q (Join-Path $Tree 'map.sqlite') --sql $Sql
    return ($r.Lines | Where-Object { $_ -match '^\s*\d+\s*$' } | Select-Object -First 1).Trim()
}

if ($script:PyRowsPython) {

Test-Case 'incremental: a second run over an unchanged tree leaves the rows exactly as they were' {
    $tree = New-IncTree
    Assert-Exit (Invoke-Map $tree) 0
    $first = Get-Scalar $tree 'SELECT count(*) FROM expressions'
    Assert-Exit (Invoke-Map $tree) 0
    Assert-Equal (Get-Scalar $tree 'SELECT count(*) FROM expressions') $first 'no rows added or lost'
    # AND NO ID IS HANDED OUT TWICE: an incremental run continues the sequence, it does not restart it.
    Assert-Equal (Get-Scalar $tree 'SELECT count(*) FROM (SELECT id FROM expressions GROUP BY id HAVING count(*) > 1)') '0' 'no duplicate ids'
}

Test-Case 'incremental: only the CHANGED file is re-read' {
    $tree = New-IncTree
    Assert-Exit (Invoke-Map $tree) 0
    [System.IO.File]::WriteAllText((Join-Path $tree 'b.py'),
        'def two():' + [char]10 + '    return helper.run(2, extra=9)' + [char]10)
    $result = Invoke-Map $tree
    Assert-Exit $result 0
    Assert-Line $result '1 file(s) re-read'
}

Test-Case 'incremental: a changed file REPLACES its rows, it does not add to them' {
    # A database that only ever added rows would answer with rows describing files that no longer say that,
    # which is worse than no database because the rows look real.
    $tree = New-IncTree
    Assert-Exit (Invoke-Map $tree) 0
    $before = Get-Scalar $tree "SELECT count(*) FROM calls WHERE source LIKE '%helper.run(2)%'"
    Assert-Equal $before '1' 'the original call is there once'
    [System.IO.File]::WriteAllText((Join-Path $tree 'b.py'),
        'def two():' + [char]10 + '    return other.thing(2)' + [char]10)
    Assert-Exit (Invoke-Map $tree) 0
    Assert-Equal (Get-Scalar $tree "SELECT count(*) FROM calls WHERE source LIKE '%helper.run(2)%'") '0' 'the old row is gone'
    Assert-Equal (Get-Scalar $tree "SELECT count(*) FROM calls WHERE callee = 'other.thing'") '1' 'the new one is there'
}

# The deep map ALONE, as a per-turn hook runs it: it prints its notes.
function Invoke-Deep([string]$Tree) {
    return Invoke-Gate --root $Tree --ext .py --map-sqlite (Join-Path $Tree 'map.sqlite')
}

Test-Case 'incremental: a tree where nothing moved does not start python at all' {
    $tree = New-IncTree
    Assert-Exit (Invoke-Deep $tree) 0
    $second = Invoke-Deep $tree
    Assert-Exit $second 0
    Assert-Line $second 'the python half had nothing to do'
    Assert-Equal (Get-Scalar $tree "SELECT count(*) FROM calls WHERE callee = 'helper.run'") '3' 'the rows are still there'
}

Test-Case 'incremental: an edit after a skipped run is read' {
    $tree = New-IncTree
    Assert-Exit (Invoke-Deep $tree) 0
    Assert-Exit (Invoke-Deep $tree) 0
    [System.IO.File]::WriteAllText((Join-Path $tree 'b.py'),
        'def two():' + [char]10 + '    return other.thing(2)' + [char]10)
    $third = Invoke-Deep $tree
    Assert-Exit $third 0
    Assert-NoLine $third 'the python half had nothing to do'
    Assert-Equal (Get-Scalar $tree "SELECT count(*) FROM calls WHERE callee = 'other.thing'") '1' 'the edit landed'
}

Test-Case 'incremental: a file that does not parse is reported on EVERY run, never skipped into silence' {
    $tree = Use-Tree @{
        'a.py'   = 'def one():' + [char]10 + '    return 1' + [char]10
        'bad.py' = 'def broken(:' + [char]10
    }
    foreach ($run in 1, 2) {
        $result = Invoke-Deep $tree
        Assert-Line $result 'bad.py'
        Assert-NoLine $result 'the python half had nothing to do'
    }
}

Test-Case 'incremental: a file DELETED from the tree leaves the database' {
    $tree = New-IncTree
    Assert-Exit (Invoke-Map $tree) 0
    Remove-Item (Join-Path $tree 'c.py') -Force
    Assert-Exit (Invoke-Map $tree) 0
    Assert-NoLine (Invoke-Q (Join-Path $tree 'map.sqlite') --sql 'SELECT path FROM files') 'c.py'
    Assert-Equal (Get-Scalar $tree "SELECT count(*) FROM calls WHERE source LIKE '%helper.run(3)%'") '0' 'its rows went with it'
}

Test-Case 'incremental: touch and restore returns the tree to exactly the rows it started with' {
    # The round trip is the honest test of a cache: anything left behind shows up as a row count that does
    # not come back.
    $tree = New-IncTree
    Assert-Exit (Invoke-Map $tree) 0
    $start = Get-Scalar $tree 'SELECT count(*) FROM expressions'
    $original = [System.IO.File]::ReadAllText((Join-Path $tree 'a.py'))
    [System.IO.File]::WriteAllText((Join-Path $tree 'a.py'), $original + '# a touch' + [char]10)
    Assert-Exit (Invoke-Map $tree) 0
    [System.IO.File]::WriteAllText((Join-Path $tree 'a.py'), $original)
    Assert-Exit (Invoke-Map $tree) 0
    Assert-Equal (Get-Scalar $tree 'SELECT count(*) FROM expressions') $start 'back to where it started'
}

Test-Case 'incremental: a database of an older SCHEMA is rebuilt, never added to' {
    # If the extractor changes what a row means, the rows already in there are the old shape. Mixing two
    # vocabularies in one table is the failure the version exists to prevent.
    $tree = New-IncTree
    Assert-Exit (Invoke-Map $tree) 0
    $before = Get-Scalar $tree 'SELECT count(*) FROM expressions'
    $helper = Join-Path $tree 'age.pyhelper'
    # PARAMETERISED, so the helper needs no single quotes at all: a SQL literal written inside a
    # single-quoted PowerShell string inside a here-doc is three levels of quoting to get right, and it was
    # got wrong twice.
    [System.IO.File]::WriteAllText($helper, (@(
        'import sqlite3, sys',
        'd = sqlite3.connect(sys.argv[1])',
        'd.execute("UPDATE _meta SET value = ? WHERE key = ?", ("0", "schema"))',
        'd.commit()') -join [char]10))
    & $script:Python $helper (Join-Path $tree 'map.sqlite')
    $result = Invoke-Map $tree
    Assert-Exit $result 0
    Assert-Line $result '3 file(s) re-read'
    Assert-Equal (Get-Scalar $tree 'SELECT count(*) FROM expressions') $before 'rebuilt to the same rows, not doubled'
}

Test-Case 'pyrows: an expression row carries the FUNCTION it sits in, closure included' {
    # The rows used to be walked after the visit had finished, with an empty scope stack, so every one of
    # them carried cls='' and func='' - and "which functions read this constant" could only answer with a
    # file. A closure and a method are both here because a flat walk gets both wrong in the same way.
    $db = New-Db @{ 'a.py' = @(
        'TOTAL = 3',
        '',
        'def outer():',
        '    def inner(k):',
        '        return k + TOTAL',
        '    return inner',
        '',
        'class Thing:',
        '    def go(self):',
        '        return TOTAL + 1') -join [char]10 }
    $r = Invoke-Q $db --sql "SELECT func, cls, source FROM expressions WHERE source LIKE '%TOTAL%' ORDER BY line"
    Assert-Line $r 'inner'
    Assert-Line $r 'go'
    $flat = Invoke-Q $db --sql "SELECT count(*) FROM expressions WHERE func = '' AND source LIKE '%TOTAL%'"
    Assert-Line $flat '0'
}

Test-Case 'pyrows: a DEFAULT is read where the def is written, not inside the function' {
    # `def load(path=where())` evaluates `where()` in the enclosing scope. Recording it inside `load` says
    # a function reads a name its body never mentions, which is the wrong answer to the only question the
    # expressions table exists for.
    $db = New-Db @{ 'a.py' = @(
        'import os',
        '',
        'ROOT = os.getcwd()',
        '',
        'def load(path=os.path.join(ROOT, "x")):',
        '    return path') -join [char]10 }
    $r = Invoke-Q $db --sql "SELECT func FROM expressions WHERE source LIKE '%os.path.join%'"
    Assert-NoLine $r 'load'
}

Test-Case 'pyrows: a PARAMETER is a row, with its annotation, its default and what the default reads' {
    $db = New-Db @{ 'a.py' = @(
        'SETTINGS = "x"',
        '',
        'def load(path=SETTINGS, mode: str = "r", *rest, **extra):',
        '    return path') -join [char]10 }
    $r = Invoke-Q $db --sql "SELECT name, kind, annotation, default_expr, reads, qualname FROM parameters ORDER BY position"
    Assert-Line $r 'path'
    Assert-Line $r 'SETTINGS'
    Assert-Line $r 'str'
    Assert-Line $r 'vararg'
    Assert-Line $r 'kwarg'
    # The owning function, spelled the way `functions.qualname` spells it, so the two join.
    Assert-Line $r 'load'
}

Test-Case 'pyrows: an assignment and a constant carry what the RIGHT-HAND SIDE reads' {
    # `_SAFE = inspect.getsource` bound a name to another name and recorded no read of it at all, so a
    # constant assigned this way looked unused however often it was assigned.
    $db = New-Db @{ 'a.py' = @(
        'import inspect',
        '',
        'SAFE_GET = inspect.getsource',
        '',
        'def go():',
        '    local = inspect.signature',
        '    return local') -join [char]10 }
    $r = Invoke-Q $db --sql "SELECT name, reads FROM consts WHERE name = 'SAFE_GET'"
    Assert-Line $r 'inspect.getsource'
    $a = Invoke-Q $db --sql "SELECT target, reads FROM assignments WHERE target = 'local'"
    Assert-Line $a 'inspect.signature'
}

Test-Case 'pyq: --reads answers with the FUNCTION, through a parameter default as well' {
    $db = New-Db @{ 'a.py' = @(
        'DATA_PATH = "s"',
        '',
        'def load(path=DATA_PATH):',
        '    return path',
        '',
        'def use():',
        '    return DATA_PATH.upper()') -join [char]10 }
    $r = Invoke-Q $db --reads DATA_PATH
    Assert-Line $r 'use'
    # The default is a read too - it is where a settings constant is consumed most often, and where a
    # dead-code pass built on the expressions table alone finds nothing.
    Assert-Line $r 'parameters'
    Assert-Line $r 'load'
}

Test-Case 'pyq: --key traces a settings key to the constant built from it and on to its readers' {
    $db = New-Db @{ 'a.py' = @(
        'SETTINGS = {}',
        'DATA_PATH = SETTINGS["REGISTRY_PATH"]',
        '',
        'def go():',
        '    return DATA_PATH') -join [char]10 }
    $r = Invoke-Q $db --key REGISTRY_PATH
    Assert-Line $r 'DATA_PATH'
    Assert-Line $r 'go'
}

Test-Case 'pyq: --decorators names the TARGET, and marks one applied twice' {
    $db = New-Db @{ 'a.py' = @(
        'import functools',
        '',
        '@functools.lru_cache',
        '@functools.lru_cache',
        'def cached():',
        '    return 1') -join [char]10 }
    $r = Invoke-Q $db --decorators lru_cache
    Assert-Line $r 'cached'
    Assert-Line $r 'REPEATED'
}

Test-Case 'pyq: --tables prints the COLUMNS, so nobody reasons around a column that is there' {
    $db = New-Db @{ 'a.py' = "@property" + [char]10 + "def go():" + [char]10 + "    return 1" + [char]10 }
    $r = Invoke-Q $db --tables
    Assert-Line $r 'target_kind'
    Assert-Line $r 'qualname'
}

}
