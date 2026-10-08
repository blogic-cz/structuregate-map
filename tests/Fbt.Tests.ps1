<#
    THE CHANGE DETECTOR, over the CLI that every half will ask.

    `--fbt-scan` walks a tree, hashes what moved and says which files have to be parsed again. Everything
    downstream trusts that answer, so the cases here are about the two ways it could lie: calling a tree
    unchanged when a file moved, and calling a file changed when only its timestamp did.

    The scan is rust linked into this exe. A case that cannot reach it at all fails loudly rather than
    skipping — a change detector that is silently absent is the worst of the three states.
#>

# A database of the case's own, removed with its trees when it passes: 760 of them were left in TEMP.
function New-FbtDb {
    $db = Join-Path ([System.IO.Path]::GetTempPath()) "fbt-$([guid]::NewGuid().ToString('N')).db"
    Register-Tree $db
    return $db
}

function Get-Scan {
    param(
        [Parameter(Mandatory)][string]$Tree,
        [Parameter(ValueFromRemainingArguments)][object[]]$Extra
    )
    $db = New-FbtDb
    $script:LastScanDb = $db
    return Invoke-ScanWithDb $Tree $db @(Get-RealArg $Extra)
}

# SPLATTING NOTHING IS NOT NOTHING. With no remaining arguments `$Extra` is `$null`, and
# `Invoke-Gate`'s flatten turns that into ONE EMPTY ARGUMENT - which the gate rejects as an
# unknown argument, so every case with no extra flag failed on the argument parser instead of
# on the scan. The helper drops them, and the gate keeps rejecting an empty argument, which is
# the right answer for one a caller really passed.
function Get-RealArg($Extra) {
    $out = @()
    foreach ($e in @($Extra)) { if ($null -ne $e -and "$e" -ne '') { $out += $e } }
    return $out
}

function Invoke-ScanWithDb {
    param(
        [Parameter(Mandatory)][string]$Tree,
        [Parameter(Mandatory)][string]$MapDb,
        [Parameter(ValueFromRemainingArguments)][object[]]$Extra
    )
    $result = Invoke-Gate --fbt-scan $MapDb --root $Tree @(Get-RealArg $Extra)
    Assert-Exit $result 0
    $json = ($result.Lines | Where-Object { -not $_.StartsWith('structuregate:') }) -join "`n"
    return $json | ConvertFrom-Json
}

Test-Case 'fbt: the first scan of a tree has no previous answer, so every file is stale' {
    $tree = Use-Tree @{ 'a.cs' = "class A { }`n"; 'b.cs' = "class B { }`n" }

    $scan = Get-Scan $tree
    Assert-Equal $scan.first_scan 'True' 'the first scan says so'
    Assert-Equal $scan.unchanged 'False' 'a tree never seen before is not unchanged'
    Assert-Equal $scan.stale 2 'both files are stale'
    if (-not $scan.root_hash) { throw 'the first scan produced no root hash' }
}

Test-Case 'fbt: a tree nobody touched reads as unchanged and opens no file' {
    $tree = Use-Tree @{ 'a.cs' = "class A { }`n" }
    $db = New-FbtDb

    $first = Invoke-ScanWithDb $tree $db
    $second = Invoke-ScanWithDb $tree $db

    Assert-Equal $second.unchanged 'True' 'the second scan of an untouched tree'
    Assert-Equal $second.stale 0 'nothing is stale'
    Assert-Equal $second.gone 0 'nothing has gone'
    Assert-Equal $second.read_from_disk 0 'no file was opened the second time'
    Assert-Equal $second.root_hash $first.root_hash 'the root hash did not move'
}

Test-Case 'fbt: an edited file is stale and every directory above it changes hash' {
    $tree = Use-Tree @{ 'lib/deep/a.cs' = "class A { }`n"; 'other/b.cs' = "class B { }`n" }
    $db = New-FbtDb

    $first = Invoke-ScanWithDb $tree $db --fbt-dir-hashes
    Set-Content -LiteralPath (Join-Path $tree 'lib/deep/a.cs') -Value "class A { int x; }`n" -NoNewline
    $second = Invoke-ScanWithDb $tree $db --fbt-dir-hashes

    Assert-Equal $second.stale 1 'one file is stale'
    Assert-Equal $second.stale_paths[0] 'lib/deep/a.cs' 'the edited file is the stale one'

    # THE MERKLE IS THE POINT: a change reaches the root through every directory on its path, and through
    # none of the others. A sibling that moved would mean the rollup folds in something it must not.
    foreach ($dir in @('lib', 'lib/deep')) {
        if ($first.dir_hashes.$dir -ceq $second.dir_hashes.$dir) {
            throw "$dir did not change hash although a file under it did"
        }
    }
    Assert-Equal $second.dir_hashes.other $first.dir_hashes.other 'an untouched sibling keeps its hash'
}

Test-Case 'fbt: rewriting a file with the SAME bytes is not a change' {
    $tree = Use-Tree @{ 'a.cs' = "class A { }`n" }
    $db = New-FbtDb

    $first = Invoke-ScanWithDb $tree $db
    Start-Sleep -Milliseconds 20
    # Same content, new timestamp. A detector that trusted mtime would call this a change and every half
    # downstream would re-parse a file that did not move.
    Set-Content -LiteralPath (Join-Path $tree 'a.cs') -Value "class A { }`n" -NoNewline
    $second = Invoke-ScanWithDb $tree $db

    Assert-Equal $second.stale 0 'identical bytes are not stale'
    Assert-Equal $second.unchanged 'True' 'the tree is unchanged'
    Assert-Equal $second.root_hash $first.root_hash 'the root hash did not move'
    Assert-Equal $second.read_from_disk 1 'the file WAS read again, which is how we know it is the same'
}

Test-Case 'fbt: a deleted file is reported as gone, not merely absent' {
    $tree = Use-Tree @{ 'a.cs' = "class A { }`n"; 'b.cs' = "class B { }`n" }
    $db = New-FbtDb

    Invoke-ScanWithDb $tree $db | Out-Null
    Remove-Item -LiteralPath (Join-Path $tree 'b.cs')
    $second = Invoke-ScanWithDb $tree $db

    Assert-Equal $second.gone 1 'one file has gone'
    Assert-Equal $second.gone_paths[0] 'b.cs' 'the deleted file is named'
    Assert-Equal $second.stale 0 'nothing became stale'
}

Test-Case 'fbt: a moved file is one rename, not an add and a delete' {
    $tree = Use-Tree @{ 'src/a.cs' = "class A { }`n" }
    $db = New-FbtDb

    Invoke-ScanWithDb $tree $db | Out-Null
    New-Item -ItemType Directory -Path (Join-Path $tree 'moved') | Out-Null
    Move-Item -LiteralPath (Join-Path $tree 'src/a.cs') -Destination (Join-Path $tree 'moved/a.cs')
    $second = Invoke-ScanWithDb $tree $db

    Assert-Equal @($second.renamed).Count 1 'exactly one rename'
    Assert-Equal $second.renamed[0].from 'src/a.cs' 'where it came from'
    Assert-Equal $second.renamed[0].to 'moved/a.cs' 'where it went'
    # Both ends are still reported: rows are keyed by path, so the old ones go and the new ones are built.
    Assert-Equal $second.stale 1 'the landing path is stale'
    Assert-Equal $second.gone 1 'the leaving path has gone'
}

Test-Case 'fbt: the same content always hashes the same, so a reverted tree reads as it did' {
    $tree = Use-Tree @{ 'a.cs' = "class A { }`n" }
    $db = New-FbtDb

    $first = Invoke-ScanWithDb $tree $db
    Set-Content -LiteralPath (Join-Path $tree 'a.cs') -Value "class A { int x; }`n" -NoNewline
    $edited = Invoke-ScanWithDb $tree $db
    if ($edited.root_hash -ceq $first.root_hash) { throw 'the root hash did not follow the content' }

    Set-Content -LiteralPath (Join-Path $tree 'a.cs') -Value "class A { }`n" -NoNewline
    $back = Invoke-ScanWithDb $tree $db
    Assert-Equal $back.root_hash $first.root_hash 'the root hash came back with the content'
}

Test-Case 'fbt: --fbt-no-store answers without recording, so the answer repeats' {
    $tree = Use-Tree @{ 'a.cs' = "class A { }`n" }
    $db = New-FbtDb

    Invoke-ScanWithDb $tree $db | Out-Null
    Set-Content -LiteralPath (Join-Path $tree 'a.cs') -Value "class A { int x; }`n" -NoNewline

    $planned = Invoke-ScanWithDb $tree $db --fbt-no-store
    $again = Invoke-ScanWithDb $tree $db --fbt-no-store
    Assert-Equal $planned.stale 1 'the change is reported'
    Assert-Equal $again.stale 1 'and reported again, because nothing was written'
}

<#
    THE ROWS A PARTIAL RUN HANDS BACK, over the CLI the TypeScript half will call.

    The rules `carry_over` obeys are unit-tested in rust and held against the python end over a real
    database while the python end existed. What is not covered by those is the WIRING: that the flags
    reach the store at all, and that what comes back is the `MAP-CARRY` protocol line the probe prints. A store that
    works and a flag that never reaches it look identical from inside rust.
#>

Test-Case 'fbt: a partial run carries back the rows of every file it is not re-extracting' {
    $work = Join-Path ([System.IO.Path]::GetTempPath()) "fbtcarry-$([guid]::NewGuid().ToString('N'))"
    New-Item -ItemType Directory -Path $work | Out-Null
    try {
        $db = Join-Path $work 'map.sqlite'
        $payload = Join-Path $work 'rows.json'
        $plan = Join-Path $work 'plan.json'
        $carried = Join-Path $work 'carry.json'

        # Two files, rows hanging off each by `owner_file`, and one derived table hanging off nothing.
        $rows = [ordered]@{
            all = $true; first = $true; final = $true; lang = 'typescript'; half = 'typescript'
            shas = [ordered]@{ 'a.ts' = 'sha-a'; 'b.ts' = 'sha-b' }
            tables = [ordered]@{
                files = @(
                    [ordered]@{ id = 'f:1'; path = 'a.ts'; sha = 'sha-a' },
                    [ordered]@{ id = 'f:2'; path = 'b.ts'; sha = 'sha-b' })
                bindings = @(
                    [ordered]@{ id = 'b:1'; owner_file = 'f:1'; name = 'one' },
                    [ordered]@{ id = 'b:2'; owner_file = 'f:2'; name = 'two' })
                key_reach = @([ordered]@{ key = 'menu.home'; route = 'template' })
            }
        }
        # WRITTEN WITHOUT A BYTE ORDER MARK, the way the two writers that exist in production do:
        # node and this exe. Windows PowerShell's `Set-Content -Encoding utf8` puts one in, and the
        # store reads the payload as JSON bytes and rejects it - "expected value at line 1 column 1".
        [System.IO.File]::WriteAllText($payload, ($rows | ConvertTo-Json -Depth 8))
        Assert-Exit (Invoke-Gate --fbt-rows $db --fbt-rows-file $payload --root $work) 0

        [System.IO.File]::WriteAllText($plan, '{"affected": ["a.ts"]}')
        $result = Invoke-Gate --fbt-rows $db --fbt-rows-lang typescript `
            --fbt-rows-affected $plan --fbt-rows-carry $carried
        Assert-Exit $result 0
        # Both `files` rows and ONE `bindings` row: the re-extracted file's binding is about to be
        # replaced, but the row that IS the file is not - a file whose contents changed is the same
        # file, and withholding it would make the reader mint a second id for it.
        Assert-Line $result 'MAP-CARRY|3|2'

        # ONE ROW A LINE, which is how the file is written and how `TsMap.mjs` reads it back - see
        # `rows::carry::carry_over`. Reading it as one document would be asserting the shape of the
        # wire; what matters is which rows came back, so they are collected the way the reader
        # collects them.
        $tables = @{}
        foreach ($line in (Get-Content -LiteralPath $carried)) {
            if (-not $line.Trim()) { continue }
            $one = $line | ConvertFrom-Json
            if ($one.PSObject.Properties.Name -contains 'claims') { continue }
            if (-not $tables.ContainsKey($one.t)) { $tables[$one.t] = @() }
            $tables[$one.t] += $one.r
        }
        Assert-Equal $tables['files'].Count 2 'a re-extracted file keeps its identity'
        Assert-Equal $tables['bindings'].Count 1 'but not the rows hanging off it'
        Assert-Equal $tables['bindings'][0].id 'b:2' 'which are the other file''s'
        if ($tables.ContainsKey('key_reach')) {
            throw 'a derived table was carried; node rebuilds those whole and would throw them away'
        }
    }
    finally {
        Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
    }
}

Test-Case 'fbt: carrying from a database that is not there says so rather than answering with nothing' {
    # An empty carry file reads as "this map has nothing else in it", which is the one answer a partial
    # run must never be given.
    $work = Join-Path ([System.IO.Path]::GetTempPath()) "fbtcarry-$([guid]::NewGuid().ToString('N'))"
    New-Item -ItemType Directory -Path $work | Out-Null
    try {
        $plan = Join-Path $work 'plan.json'
        [System.IO.File]::WriteAllText($plan, '{"affected": []}')
        $carried = Join-Path $work 'carry.json'
        $result = Invoke-Gate --fbt-rows (Join-Path $work 'gone.sqlite') --fbt-rows-lang typescript `
            --fbt-rows-affected $plan --fbt-rows-carry $carried
        Assert-Exit $result 1
        Assert-Line $result 'no database'
        if (Test-Path $carried) { throw 'it wrote a carry file anyway' }
    }
    finally {
        Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
    }
}
