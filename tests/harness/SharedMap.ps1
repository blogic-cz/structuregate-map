<#
    SharedMap.ps1 - one deep map per input, per suite process. Not a suite: `Assert.ps1` dot-sources it, so every suite
    has it and `-Only` still runs any of them alone.

    A deep map of a small fixture is seconds of Roslyn, python or node startup, and many read-only cases map the very same
    tree. `Get-SharedMap` keys a map on the WHOLE input: every file of the tree on disk (path and bytes), every gate
    argument with the tree's own path taken out, every `STRUCTUREGATE_*` variable of the environment, and the binary under
    test - anything different maps afresh. A hit RESTORES the first map's snapshot (the tree with the database and
    whatever else the run wrote into it - `.fbt`, a JSON map - every file time kept) at the SAME path that map was built
    at, so `_meta`'s roots and the freshness check see exactly what the first case saw; the case's own tree (identical
    bytes) is deleted, so a case that read it after all fails instead of reading a tree the map is not of. That path is
    free only once the earlier case passed and its tree was removed: otherwise this maps afresh. Every map handed out here
    is FROZEN (`$script:FrozenMaps`, Assert.ps1): another `--map-sqlite` run over it throws, and a tree that changed under
    it fails the case at its end. Kept in memory and in one TEMP folder per process, never across runs.

    OPT IN AT THE CALL SITE, and only from a case that never edits its tree, maps it again, writes its database or runs
    anything else in the tree (a `dotnet build`, a python that edits a file). `SGTEST_SHARED_MAP_LOG=<file>` (or the
    older `SGTEST_TSROWS_SHARED_LOG`) logs each hit and miss.
#>

$script:SharedMaps = $null

# `$GateArgs` must carry `--map-sqlite <db>` with the database INSIDE `$Tree`, or the snapshot would not carry it.
# Hands back the database, the tree the map describes (a hit's differs from `$Tree`) and the gate's result.
function Get-SharedMap([string]$Tree, [object[]]$GateArgs) {
    if ($null -eq $script:SharedMaps) {
        $dir = Join-Path ([System.IO.Path]::GetTempPath()) "sgtest-$([System.Guid]::NewGuid().ToString('N').Substring(0,12))"
        [void](New-Item -ItemType Directory -Path $dir -Force)
        $script:SharedMaps = @{ Dir = $dir; Entries = @{} }
        [void]$script:RunScratch.Add($dir)
    }
    $db = "$($GateArgs[[array]::IndexOf($GateArgs, '--map-sqlite') + 1])"
    if (-not $db.StartsWith($Tree.TrimEnd('\', '/') + [System.IO.Path]::DirectorySeparatorChar)) { throw "a shared map's database must be inside its tree: $db" }
    # A database already there would make this run an UPDATE, which is never shared.
    if (Test-Path -LiteralPath $db) { throw "a shared map is built from nothing, and $db already exists - map it without -Shared" }
    $gate = Get-GateInvocation
    $spelled = @($GateArgs | ForEach-Object { "$_".Replace($Tree, '<tree>') }) -join "`n"
    # What the gate reads from its ENVIRONMENT is input too (`STRUCTUREGATE_MAP_BATCH` changes how a map is stored).
    $environment = @(Get-ChildItem Env: | Where-Object { $_.Name.StartsWith('STRUCTUREGATE', [StringComparison]::OrdinalIgnoreCase) } |
        Sort-Object Name | ForEach-Object { "$($_.Name)=$($_.Value)" }) -join "`n"
    $key = Get-TreeDigest $Tree '' ($spelled + "`n" + $gate.File + "`n" + ($gate.Lead -join "`n") + "`n" + $environment)
    $entry = $script:SharedMaps.Entries[$key]
    if ($entry -and -not (Test-Path -LiteralPath $entry.Tree)) {
        Copy-SharedTree $entry.Snap $entry.Tree
        Register-Tree $entry.Tree
        Remove-Item -LiteralPath $Tree -Recurse -Force
        $made = [pscustomobject]@{ Db = $entry.Db; Tree = $entry.Tree
            Result = [pscustomobject]@{ Exit = $entry.Result.Exit; Lines = @($entry.Result.Lines); Text = $entry.Result.Text } }
        $digest = $entry.Digest
        $how = 'hit'
    } else {
        $made = [pscustomobject]@{ Db = $db; Tree = $Tree; Result = (Invoke-Gate $GateArgs) }
        $digest = Get-TreeDigest $Tree $made.Db
        $how = 'fresh'
        if (-not $entry) {
            $snap = Join-Path $script:SharedMaps.Dir $key
            Copy-SharedTree $Tree $snap
            $script:SharedMaps.Entries[$key] = @{ Tree = $Tree; Db = $db; Snap = $snap; Result = $made.Result; Digest = $digest }
            $how = 'miss'
        }
    }
    $script:FrozenMaps[[System.IO.Path]::GetFullPath($made.Db)] = @{ Tree = $made.Tree; Digest = $digest }
    $log = if ($env:SGTEST_SHARED_MAP_LOG) { $env:SGTEST_SHARED_MAP_LOG } else { $env:SGTEST_TSROWS_SHARED_LOG }
    if ($log) { [System.IO.File]::AppendAllText($log, "$how`t$key`t$($made.Tree)`t$($script:CaseName)`n") }
    return $made
}

# A tree copied with every file's and folder's time kept: the freshness check compares them with the database's.
function Copy-SharedTree([string]$From, [string]$To) {
    [void](New-Item -ItemType Directory -Path $To -Force)
    foreach ($item in @(Get-ChildItem -LiteralPath $From -Force)) {
        $target = Join-Path $To $item.Name
        if ($item.PSIsContainer) { Copy-SharedTree $item.FullName $target; continue }
        [System.IO.File]::Copy($item.FullName, $target, $true)
        [System.IO.File]::SetLastWriteTimeUtc($target, $item.LastWriteTimeUtc)
    }
    [System.IO.Directory]::SetLastWriteTimeUtc($To, (Get-Item -LiteralPath $From -Force).LastWriteTimeUtc)
}
