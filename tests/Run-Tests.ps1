<#
    Run-Tests.ps1 - every test in this repo, one command, one exit code.

      powershell -NoProfile -File tests\Run-Tests.ps1              # all suites
      powershell -NoProfile -File tests\Run-Tests.ps1 -Only PsGate # one suite
      powershell -NoProfile -File tests\Run-Tests.ps1 -WindowsOnly # the Test-WindowsCase cases alone (CI)
      dotnet build src\StructureGate.csproj                        # the build runs them too

    WHY BLACK BOX. The deliverable is a native exe that consumers call from MSBuild, from a Claude Code Stop
    hook and from node launchers. What has to keep working is therefore the CLI: which files are measured,
    what they count as, what is printed, and what is exited with. A unit test around an internal method
    would pass while all of that regressed.

    What is under test is `out\structuregate.dll` from the last build, or the published exe if no dll exists.
    Set PSGATE_TEST_GATE to point at a specific binary (e.g. a deployed copy in a consumer).
#>
[CmdletBinding()]
param([string]$Only = '', [switch]$WindowsOnly)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'Assert.ps1')

# -Recurse, so a suite may live in a FOLDER of its own: tests\ is at the 15-file limit this gate holds
# every folder to, and a sixteenth suite dropped in beside these would have been a suite that runs nowhere
# and says nothing. Sorted by NAME and not by path, because the order suites are dot-sourced in is the order
# their helpers overwrite each other, and that order must not change when a file moves into a folder.
$suites = Get-ChildItem $PSScriptRoot -Filter '*.Tests.ps1' -Recurse | Sort-Object Name
if ($Only) { $suites = $suites | Where-Object { $_.Name.StartsWith($Only, [StringComparison]::OrdinalIgnoreCase) } }
# Only the suites that HOLD a Windows case are even loaded: loading one does its setup (a compiler install, a
# fixture build) whether or not any of its cases runs.
if ($WindowsOnly) {
    $script:WindowsOnlyRun = $true
    $suites = $suites | Where-Object { [System.IO.File]::ReadAllText($_.FullName).Contains('Test-WindowsCase ') }
}
if (-not $suites) { Write-Host "no suite matches '$Only'"; exit 1 }

$gate = Get-GateInvocation
Write-Host "structuregate tests: $(($gate.Lead + $gate.File) -join ' ')"

$clock = [Diagnostics.Stopwatch]::StartNew()
foreach ($suite in $suites) {
    Write-Host ""
    Write-Host "  $($suite.BaseName)"
    $before = $script:Passed + $script:Failed.Count
    # A suite that THROWS outside a case is one failure, not the end of the run: every suite after it
    # would otherwise go unreported, and the exit code would say nothing about which ones they were.
    try { . $suite.FullName }
    catch {
        [void]$script:Failed.Add("$($suite.BaseName) stopped outside a case`n      $($_.Exception.Message)")
        Write-Host "  FAIL  $($suite.BaseName) stopped outside a case: $($_.Exception.Message)"
    }
    $ran = ($script:Passed + $script:Failed.Count) - $before
    Write-Host "    $ran case(s)"
}

Write-Host ""
if ($script:Failed.Count -gt 0) {
    Write-Host "FAILED: $($script:Failed.Count) of $($script:Passed + $script:Failed.Count) case(s), $([int]$clock.Elapsed.TotalSeconds)s"
    foreach ($failure in $script:Failed) { Write-Host "  - $failure" }
    if ($script:KeptTrees.Count -gt 0) {
        Write-Host "  trees kept for inspection:"
        foreach ($tree in $script:KeptTrees) { Write-Host "    $tree" }
    }
    exit 1
}

Write-Host "PASSED: $($script:Passed) case(s) in $($suites.Count) suite(s), $([int]$clock.Elapsed.TotalSeconds)s"
exit 0
