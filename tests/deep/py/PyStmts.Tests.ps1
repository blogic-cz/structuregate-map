<#
    The python deep map's STATEMENT rows - every `except` clause, `with`, `global`, `match`, `del`, `assert`,
    and each comment - and the lens fixes that came with them: `--tables` cutting `_meta`, `--file` and
    `--cat` under a root folder's own name, `files.lines`, the interned `deps:python`.

    Its helpers are its own - `-Only PyStmts` runs this suite alone.
#>

$script:PyStmtsPython = [bool]$script:Python
if (-not $script:PyStmtsPython) { Write-Host '    (no python on this machine - the statement cases are not run)' }

# The deep map of a throwaway tree, as the tree and its database.
function New-PyStmtsDb([hashtable]$Files) {
    $tree = Use-Tree $Files
    $db = Join-Path $tree 'stmts.sqlite'
    Assert-Exit (Invoke-Gate --root $tree --ext .py --map-sqlite $db) 0
    return [pscustomobject]@{ Tree = $tree; Db = $db }
}

# One query through the exe, every cell whole.
function Invoke-PyStmtsQ([string]$Path, [Parameter(ValueFromRemainingArguments)][object[]]$LensArgs) {
    $result = Invoke-Gate --map-query $Path --width 0 --limit 0 @LensArgs
    Assert-Exit $result 0
    return $result
}

# `|`-joined cells, so a case compares one exact line and a containment check cannot pass on a neighbour.
function Assert-PyStmtsRow($Result, [string]$Expected) {
    $hits = @($Result.Lines | Where-Object { $_.Trim() -ceq $Expected })
    if ($hits.Count -eq 0) { throw "no line is exactly '$Expected'. Output:`n$($Result.Text)" }
}

if ($script:PyStmtsPython) {

$nl = [string][char]10

Test-Case 'pystmts: a handler of a DOTTED type is caught, on the try row and on its own' {
    # Many try rows in one tree had `test = ''`: only a bare name was kept.
    $made = New-PyStmtsDb @{
        'a.py' = "import subprocess$nl" + "def go():$nl    try:$nl        pass$nl" +
                 "    except subprocess.TimeoutExpired:$nl        pass$nl" +
                 "    except (OSError, subprocess.CalledProcessError):$nl        pass$nl"
    }
    $r = Invoke-PyStmtsQ $made.Db --sql "SELECT 'try=' || test FROM branches WHERE kind = 'try'"
    Assert-PyStmtsRow $r 'try=subprocess.TimeoutExpired, OSError, subprocess.CalledProcessError'
    $h = Invoke-PyStmtsQ $made.Db --sql "SELECT 'h=' || line || ':' || types FROM handlers ORDER BY line"
    Assert-PyStmtsRow $h 'h=5:["subprocess.TimeoutExpired"]'
    Assert-PyStmtsRow $h 'h=7:["OSError", "subprocess.CalledProcessError"]'
}

Test-Case 'pystmts: a handler row says what its body does, and the waiver on its line' {
    $made = New-PyStmtsDb @{
        'a.py' = "def go(log):$nl    try:$nl        pass$nl" +
                 "    except Exception as e:  # noqa: BLE001 - logged$nl        log.warning('x', exc_info=True)$nl        raise$nl" +
                 "    except ValueError as v:$nl        return v$nl" +
                 "    except:$nl        pass$nl"
    }
    $r = Invoke-PyStmtsQ $made.Db --sql ("SELECT 'h=' || line || ':' || name || ':' || bare || name_read || passes || raises || " +
        "reraises || exc_info || noqa || ':' || codes || ':' || calls FROM handlers ORDER BY line")
    Assert-PyStmtsRow $r 'h=4:e:0001111:["BLE001"]:["log.warning"]'
    Assert-PyStmtsRow $r 'h=7:v:0100000:[]:[]'
    Assert-PyStmtsRow $r 'h=9::1010000:[]:[]'
}

Test-Case 'pystmts: with, global, nonlocal, del, assert and match are rows, and a bare name in one is READ' {
    # `with LOCK:` reads LOCK in no expression row - a bare name is under the size of one.
    $made = New-PyStmtsDb @{
        'a.py' = "LOCK = object()$nl" + "CACHE = {}$nl" + "READY = True$nl" +
                 "def go(x):$nl    global CACHE$nl    with LOCK, open(x) as h:$nl        del CACHE[x]$nl" +
                 "    assert READY, 'no'$nl    match x:$nl        case Color.RED if READY:$nl            pass$nl" +
                 "    def inner():$nl        nonlocal x$nl"
    }
    $w = Invoke-PyStmtsQ $made.Db --sql "SELECT 'w=' || position || ':' || source || ':' || target || ':' || binds FROM withs"
    Assert-PyStmtsRow $w 'w=0:LOCK::["a.py::LOCK"]'
    Assert-PyStmtsRow $w 'w=1:open(x):h:["", ""]'
    $g = Invoke-PyStmtsQ $made.Db --sql "SELECT 'g=' || kind || ':' || name || ':' || func FROM globals ORDER BY line"
    Assert-PyStmtsRow $g 'g=global:CACHE:go'
    Assert-PyStmtsRow $g 'g=nonlocal:x:inner'
    $d = Invoke-PyStmtsQ $made.Db --sql "SELECT 'd=' || target || ':' || reads FROM deletes"
    Assert-PyStmtsRow $d 'd=CACHE[x]:["CACHE", "x"]'
    $b = Invoke-PyStmtsQ $made.Db --sql "SELECT 'b=' || kind || ':' || test FROM branches WHERE kind IN ('assert', 'match', 'case') ORDER BY line"
    Assert-PyStmtsRow $b 'b=assert:READY'
    Assert-PyStmtsRow $b 'b=match:x'
    Assert-PyStmtsRow $b 'b=case:Color.RED if READY'
    Assert-Line (Invoke-PyStmtsQ $made.Db --reads LOCK) 'withs'
}

Test-Case 'pystmts: a comment is a row with its scope, and a # inside a string is not one' {
    $made = New-PyStmtsDb @{
        'a.py' = "# top$nl" + "class Thing:$nl    def go(self):$nl        x = '# not a comment'  # type: ignore  # noqa: E501, F401$nl" +
                 "        return x$nl"
    }
    $r = Invoke-PyStmtsQ $made.Db --sql ("SELECT 'c=' || line || ':' || col || ':' || coalesce(context, '-') || ':' || inline || noqa || ':' || codes " +
        "FROM comments ORDER BY line")
    Assert-PyStmtsRow $r 'c=1:1:-:00:[]'
    Assert-PyStmtsRow $r 'c=4:32:Thing.go:11:["E501", "F401"]'
    $n = Invoke-PyStmtsQ $made.Db --sql "SELECT 'n=' || count(*) FROM comments"
    Assert-PyStmtsRow $n 'n=2'
}

Test-Case 'pystmts: files.lines is the lines a file HAS, a last newline not counted as one more' {
    # A form feed is a character, not a line break: `splitlines` counted it as one.
    $made = New-PyStmtsDb @{ 'ends.py' = "a = 1${nl}b = 2$nl"; 'open.py' = "a = 1${nl}b = 2"
                             'feed.py' = "a = 1$nl" + [char]12 + "b = 2$nl" }
    $r = Invoke-PyStmtsQ $made.Db --sql "SELECT 'l=' || path || ':' || lines FROM files ORDER BY path"
    Assert-PyStmtsRow $r 'l=ends.py:2'
    Assert-PyStmtsRow $r 'l=open.py:2'
    Assert-PyStmtsRow $r 'l=feed.py:2'
    # `--cat` counts the same lines - it said one line more than the file's row did.
    Assert-Line (Invoke-PyStmtsQ $made.Db --cat 'ends.py') 'ends.py  lines 1-2 of 2'
    Assert-Line (Invoke-PyStmtsQ $made.Db --cat 'open.py') 'open.py  lines 1-2 of 2'
}

Test-Case 'pystmts: --tables cuts a long _meta value like any cell, and --width 0 shows it whole' {
    # `deps:python` printed whole made `--tables` megabytes long over one tree.
    $files = @{ 'main.py' = (0..15 | ForEach-Object { "from pkg.mod$_ import thing$_" }) -join $nl }
    $files['pkg/__init__.py'] = ''
    $made = New-PyStmtsDb $files
    $cut = Invoke-Gate --map-query $made.Db --tables
    Assert-Exit $cut 0
    $deps = @($cut.Lines | Where-Object { $_.TrimStart().StartsWith('deps:python') })
    Assert-Equal $deps.Count 1 'one deps:python line'
    if (-not $deps[0].Contains('...[+')) { throw "deps:python was not cut: $($deps[0])" }
    $whole = Invoke-PyStmtsQ $made.Db --tables
    $deps = @($whole.Lines | Where-Object { $_.TrimStart().StartsWith('deps:python') })
    if ($deps[0].Contains('...[+')) { throw "--width 0 cut deps:python: $($deps[0])" }
}

Test-Case 'pystmts: --file and --cat take a path that names the root folder itself' {
    # `--file pkg/two/config.py` for a map rooted AT pkg/ once found nothing.
    $made = New-PyStmtsDb @{ 'two/config.py' = "LIMIT = 5$nl" }
    $above = (Split-Path $made.Tree -Leaf) + '/two/config.py'
    $file = Invoke-PyStmtsQ $made.Db --file $above
    Assert-Line $file 'two/config.py  (config'
    Assert-Line $file '(read as'
    Assert-Line (Invoke-PyStmtsQ $made.Db --cat $above --lines 1-1) 'LIMIT = 5'
    # A fragment that matches nothing is NOT shortened into one that matches something else.
    $miss = Invoke-PyStmtsQ $made.Db --file 'elsewhere/config.py'
    Assert-Line $miss 'no file matches'
    Assert-Line $miss 'relative to the root'
}

Test-Case 'pystmts: --file under SEVERAL roots takes the path from the folder above them' {
    # A tree mapped as pkg/one, pkg/two, ... - a path is `two/config.py`, and `_meta.root`
    # names only the first root, so the lens needs every root to read `pkg/two/config.py`.
    $tree = Use-Tree @{ 'pkg/one/run.py' = "X = 1$nl"; 'pkg/two/config.py' = "LIMIT = 5$nl" }
    $db = Join-Path $tree 'stmts.sqlite'
    Assert-Exit (Invoke-Gate --root (Join-Path $tree 'pkg/one') --root (Join-Path $tree 'pkg/two') --ext .py --map-sqlite $db) 0
    $file = Invoke-PyStmtsQ $db --file 'pkg/two/config.py'
    Assert-Line $file 'two/config.py  (config'
    Assert-Line $file "(read as 'two/config.py'"
}

Test-Case 'pystmts: deps:python is stored interned, and a record in the old plain form is still read' {
    $tree = Use-Tree @{
        'pkg/__init__.py'          = ''
        'pkg/moved/__init__.py'    = ''
        'pkg/hub/__init__.py'      = "from pkg.hub.impl import helper  # noqa$nl"
        'pkg/hub/impl.py'          = "def helper():$nl    return 1$nl"
        'pkg/caller.py'            = "from pkg import hub$nl$nl$nl" + "def run():$nl    return hub.helper()$nl"
    }
    $db = Join-Path $tree 'stmts.sqlite'
    Assert-Exit (Invoke-Gate --root $tree --ext .py --map-sqlite $db) 0
    $meta = Invoke-PyStmtsQ $db --sql "SELECT value FROM _meta WHERE key = 'deps:python'"
    Assert-Line $meta '"interned":1'
    # Rewritten in the form an earlier version stored, the record still re-reads caller.py when the module moves.
    $script = "import json, sqlite3, sys$nl" + "db = sqlite3.connect(sys.argv[1])$nl" +
              "v = json.loads(db.execute(`"SELECT value FROM _meta WHERE key = 'deps:python'`").fetchone()[0])$nl" +
              "plain = {rel: [v['keys'][i] for i in ids] for rel, ids in v['files'].items()}$nl" +
              "db.execute(`"UPDATE _meta SET value = ? WHERE key = 'deps:python'`", (json.dumps(plain),))$nl" +
              "db.commit()$nl"
    $file = Join-Path $tree 'plain.py.txt'
    [System.IO.File]::WriteAllText($file, $script)
    & $script:Python $file $db
    if ($LASTEXITCODE -ne 0) { throw 'the record could not be rewritten in the plain form' }
    Assert-NoLine (Invoke-PyStmtsQ $db --sql "SELECT value FROM _meta WHERE key = 'deps:python'") '"interned"'
    Move-Item (Join-Path $tree 'pkg/hub/impl.py') (Join-Path $tree 'pkg/moved/impl.py')
    [System.IO.File]::WriteAllText((Join-Path $tree 'pkg/hub/__init__.py'), "from pkg.moved.impl import helper  # noqa$nl")
    Assert-Exit (Invoke-Gate --root $tree --ext .py --map-sqlite $db) 0
    $bound = Invoke-PyStmtsQ $db --sql "SELECT 'b=' || target_path FROM calls WHERE callee = 'hub.helper'"
    Assert-PyStmtsRow $bound 'b=pkg/moved/impl.py'
}

}
