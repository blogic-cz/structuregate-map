<#
    `scripts/Update-Gate.ps1 -Trace`: OPT-IN machine-wide tracing. On Windows it sets the user's STRUCTUREGATE_TRACE to
    `trace.jsonl` beside the release, keeps a value the user set themselves, and `-NoTrace` removes only its
    own; without the switch nothing changes. Off Windows it prints the line for a shell profile.

    Run over a LOCAL archive (`-Zip`) into a scratch `-Root`, so no download and no real release is touched.
    Its helpers are its own - `-Only UpdateGate` runs this suite alone.
#>

# A release archive as the updater expects one - the two gate files, content irrelevant - and a root to unpack it to.
function New-UpdateGateRelease {
    $tree = Use-Tree @{ 'pack/structuregate.exe' = 'x'; 'pack/structuregate' = 'x'; 'pack/StructureGate.targets' = '<Project />' }
    $pack = Join-Path $tree 'pack'
    if ($script:OnWindows) {
        $archive = Join-Path $tree 'structuregate-win-x64.zip'
        Compress-Archive -Path (Join-Path $pack '*') -DestinationPath $archive
    } else {
        $archive = Join-Path $tree 'structuregate-linux-x64.tar.gz'
        & tar -czf $archive -C $pack .
    }
    return [pscustomobject]@{ Archive = $archive; Root = (Join-Path $tree 'appdata') }
}

function Invoke-UpdateGate($Release, [object[]]$Extra) {
    $script = Join-Path $script:Root 'scripts/Update-Gate.ps1'
    $previous = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try { $out = & $script:PowerShell -NoProfile -NonInteractive -ExecutionPolicy Bypass -File $script -Zip $Release.Archive -Root $Release.Root @Extra 2>&1 }
    finally { $ErrorActionPreference = $previous }
    return [pscustomobject]@{ Exit = $LASTEXITCODE; Text = (@($out) -join "`n"); Lines = @($out | ForEach-Object { "$_" }) }
}

Test-Case 'update-gate: without -Trace nothing about tracing is said or changed' {
    $release = New-UpdateGateRelease
    $run = Invoke-UpdateGate $release @()
    Assert-Exit $run 0
    Assert-NoLine $run 'trace -'
}

if (-not $script:OnWindows) {
    Test-Case 'update-gate: -Trace off Windows prints the line for a shell profile' {
        $release = New-UpdateGateRelease
        $run = Invoke-UpdateGate $release @('-Trace')
        Assert-Exit $run 0
        Assert-Line $run ('export STRUCTUREGATE_TRACE=' + (Join-Path $release.Root 'trace.jsonl'))
    }

    Test-Case 'update-gate: -NoTrace off Windows says to remove the line, and with -Trace it is refused' {
        $release = New-UpdateGateRelease
        $run = Invoke-UpdateGate $release @('-NoTrace')
        Assert-Exit $run 0
        Assert-Line $run 'remove STRUCTUREGATE_TRACE from your shell profile'
        Assert-Exit (Invoke-UpdateGate $release @('-Trace', '-NoTrace')) 2
    }
}

# -NoTrace, NOT -Trace:$false: `powershell -File` hands `$false` over as a STRING, which 5.1 will not bind to a switch -
# the command this script's own help printed failed there, and this case was red on every Windows CI run.
Test-WindowsCase 'update-gate: -Trace sets the user variable, keeps the user''s own, and -NoTrace removes only its own' {
    $saved = [Environment]::GetEnvironmentVariable('STRUCTUREGATE_TRACE', 'User')
    try {
        [Environment]::SetEnvironmentVariable('STRUCTUREGATE_TRACE', $null, 'User')
        $release = New-UpdateGateRelease
        $want = Join-Path $release.Root 'trace.jsonl'
        Assert-Exit (Invoke-UpdateGate $release @('-Trace')) 0
        Assert-Equal ([Environment]::GetEnvironmentVariable('STRUCTUREGATE_TRACE', 'User')) $want 'set'
        Assert-Exit (Invoke-UpdateGate $release @('-NoTrace')) 0
        Assert-Equal ([Environment]::GetEnvironmentVariable('STRUCTUREGATE_TRACE', 'User')) $null 'removed'
        [Environment]::SetEnvironmentVariable('STRUCTUREGATE_TRACE', 'C:\mine.jsonl', 'User')
        Assert-Line (Invoke-UpdateGate $release @('-Trace')) 'kept your own'
        Assert-Equal ([Environment]::GetEnvironmentVariable('STRUCTUREGATE_TRACE', 'User')) 'C:\mine.jsonl' 'the user''s own kept'
    } finally {
        [Environment]::SetEnvironmentVariable('STRUCTUREGATE_TRACE', $saved, 'User')
    }
}
