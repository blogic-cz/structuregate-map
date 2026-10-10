<#
    THE DEEP PYTHON HALF STARTS BESIDE THE FILE MAP (`payload::Early`), not after it: on a `.py` edit the two
    python hosts waited for each other for nothing. Its answer is used only when the database state at its turn is
    still the one it was started from - a TypeScript half that stores first moves the id counters every half shares,
    so an answer computed before it would hand out ids it has just taken. The plain half stores first; the Angular
    half flies beside python and stores after it (`MapFlight.Tests.ps1`).

    Whether the early answer was used or thrown away is read off the run's trace (`structuregate.early.python` on `deep: python`).
    Its helpers are its own - `-Only PyEarly` runs this suite alone.
#>

# A TypeScript 5 the PLAIN half can read (7.x has no in-process parser) - the cache the TsRows suites fill, or a
# fresh install there.
function Get-PyEarlyModules {
    $modules = Join-Path ([System.IO.Path]::GetTempPath()) 'sgtest-tsrows/node_modules'
    if (-not (Get-Command node -ErrorAction SilentlyContinue)) { return $null }
    if (Test-Path (Join-Path $modules 'typescript/package.json')) { return $modules }
    if (-not (Get-Command npm -ErrorAction SilentlyContinue)) { return $null }
    $cache = Split-Path $modules
    [void](New-Item -ItemType Directory -Path $cache -Force)
    & npm install --no-save --silent --prefix $cache 'typescript@5' 2>&1 | Out-Null
    if (Test-Path (Join-Path $modules 'typescript/package.json')) { return $modules }
    return $null
}
$script:PyEarlyModules = Get-PyEarlyModules
if (-not $script:PyEarlyModules) { Write-Host '    (no node/npm-installed typescript 5 - the early python cases are not run)' }

# One `--map --map-sqlite` run over $Tree, traced; the result and what became of the early python answer.
function Invoke-PyEarly([string]$Tree) {
    $trace = Join-Path $Tree "trace-$([guid]::NewGuid().ToString('N').Substring(0, 6)).jsonl"
    $saved = $env:STRUCTUREGATE_TRACE
    try {
        $env:STRUCTUREGATE_TRACE = $trace
        $result = Invoke-Gate --root $Tree --ext '.ts,.py' --map --map-out (Join-Path $Tree 'm.json') --map-sqlite (Join-Path $Tree 'map.sqlite') `
            --ts-node-modules $script:PyEarlyModules
    } finally {
        if ($null -eq $saved) { Remove-Item Env:STRUCTUREGATE_TRACE -ErrorAction SilentlyContinue } else { $env:STRUCTUREGATE_TRACE = $saved }
    }
    $request = [System.IO.File]::ReadAllText($trace) | ConvertFrom-Json
    Remove-Item $trace
    $deep = $request.resourceSpans[0].scopeSpans[0].spans | Where-Object { $_.name -eq 'deep: python' }
    $early = ($deep.attributes | Where-Object { $_.key -eq 'structuregate.early.python' }).value.stringValue
    return [pscustomobject]@{ Result = $result; Early = $early }
}

# The first number a `--map-query --sql` prints.
function Get-PyEarlyNumber([string]$Tree, [string]$Sql) {
    $r = Invoke-Gate --map-query (Join-Path $Tree 'map.sqlite') --sql $Sql
    foreach ($line in $r.Lines) {
        $trimmed = $line.Trim()
        if ($trimmed.Length -gt 0 -and ($trimmed.ToCharArray() | Where-Object { -not [char]::IsDigit($_) }).Count -eq 0) { return $trimmed }
    }
    return ''
}

if ($script:PyEarlyModules -and (Get-Command python3, python -ErrorAction SilentlyContinue)) {

Test-Case 'pyearly: a .py edit is answered by the python host started beside the file map' {
    $tree = Use-Tree @{
        'web/a.ts' = "export const A = 1;`n"
        'x.py'     = "def one():`n    return 1`n"
        'y.py'     = "from x import one`n`ndef two():`n    return one()`n"
    }
    Assert-Exit (Invoke-PyEarly $tree).Result 0
    [System.IO.File]::AppendAllText((Join-Path $tree 'x.py'), "`ndef three():`n    return 3`n")
    $edited = Invoke-PyEarly $tree
    Assert-Exit $edited.Result 0
    Assert-Equal $edited.Early 'used' 'the early answer, with nothing stored before python'
    Assert-Equal (Get-PyEarlyNumber $tree "SELECT count(*) FROM functions WHERE name = 'three'") '1' 'the edit is in the rows'
}

Test-Case 'pyearly: an early answer is thrown away when the TypeScript half stored first, and no id is handed out twice' {
    $tree = Use-Tree @{
        'web/a.ts' = "export const A = 1;`n"
        'x.py'     = "def one():`n    return 1`n"
    }
    Assert-Exit (Invoke-PyEarly $tree).Result 0
    # A NEW FILE IN EACH HALF: the TypeScript one takes the next file ids before python's turn.
    [System.IO.File]::WriteAllText((Join-Path $tree 'web/b.ts'), "export const B = 2;`n")
    [System.IO.File]::WriteAllText((Join-Path $tree 'z.py'), "def four():`n    return 4`n")
    $both = Invoke-PyEarly $tree
    Assert-Exit $both.Result 0
    Assert-Equal $both.Early 'stale' 'the early answer, after the TypeScript half stored'
    Assert-NoLine $both.Result 'disagreed'
    Assert-Equal (Get-PyEarlyNumber $tree 'SELECT count(*) FROM (SELECT id FROM files GROUP BY id HAVING count(*) > 1)') '0' 'no file id twice'
    Assert-Equal (Get-PyEarlyNumber $tree "SELECT count(*) FROM files WHERE path IN ('web/b.ts', 'z.py')") '2' 'both new files recorded'
}

}
