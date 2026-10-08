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
        'chain.py'  = "from ids_b import _both$nl"
        'plain.py'  = "from sets_a import both$nl"
    }
    $r = Invoke-PyAliasQ $db --find _both
    [void](Get-PyAliasRow $r @('functions', 'both (as _both)', 'sets_a.py', '1'))
    [void](Get-PyAliasRow $r @('imports', '_both', 'ids_b.py'))
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

}
