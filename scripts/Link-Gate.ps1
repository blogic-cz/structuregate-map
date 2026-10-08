<#
    Link-Gate.ps1 - point every consumer's gate files at the ONE release on this machine.

      powershell -NoProfile -File scripts\Link-Gate.ps1 -Release <dir> -Consumer <dir>[;<dir>...] [-Source <file>[;<file>...]]

    `-Source` writes those files INTO the release first, IN PLACE (`File.Copy` truncates the same file), so
    every link a previous run made sees the new bytes. MSBuild's own `Copy` task deletes and recreates its
    destination, which leaves each consumer holding the OLD file under a broken link - measured, and the
    reason the deploy does not use it.

    Run by `DeployGate` after a publish has written the release, and safe to run by hand: a consumer whose
    two files already ARE the release is left alone. See `Set-GateLink` in GateWiring.ps1 for why a hard
    link, and what happens across drives.
#>
param(
    [Parameter(Mandatory = $true)][string]$Release,
    [Parameter(Mandatory = $true)][string[]]$Consumer,
    [string[]]$Source = @()
)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'GateWiring.ps1')

if (-not (Test-Path $Release)) { [void](New-Item -ItemType Directory -Path $Release -Force) }
foreach ($file in @($Source | ForEach-Object { $_.Split([char]';') } | Where-Object { $_.Trim() -ne '' })) {
    [System.IO.File]::Copy($file, (Join-Path $Release (Split-Path $file -Leaf)), $true)
    Write-Host "structuregate: released $(Split-Path $file -Leaf) to $Release"
}

# MSBuild hands the item list over as ONE string joined by `;`.
$dirs = @($Consumer | ForEach-Object { $_.Split([char]';') } | Where-Object { $_.Trim() -ne '' })
$failed = 0
foreach ($dir in $dirs) {
    try {
        Write-Host "structuregate: $(Set-GateLink $Release $dir) $dir"
    } catch {
        # ONE LOCKED TREE DOES NOT STOP THE REST: a gate running in a Stop hook holds its exe open. It is
        # reported and counted, and the run fails at the end so the publish does not read as complete.
        Write-Host "structuregate: FAILED $dir - $($_.Exception.Message)"
        $failed++
    }
}
if ($failed) { exit 1 }
