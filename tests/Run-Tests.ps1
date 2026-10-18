<#
    Run-Tests.ps1 - every test in this repo, one command, one exit code.

      pwsh -NoProfile -File tests/Run-Tests.ps1              # all suites
      pwsh -NoProfile -File tests/Run-Tests.ps1 -Only PsGate # suites whose file name starts so
      pwsh -NoProfile -File tests/Run-Tests.ps1 -Changed     # what changed since the last green run reaches
      pwsh -NoProfile -File tests/Run-Tests.ps1 -Jobs 4      # the suites spread over 4 processes
      pwsh -NoProfile -File tests/Run-Tests.ps1 -WindowsOnly # the Test-WindowsCase cases alone (CI)
      dotnet build src/StructureGate.csproj                  # the build runs -Changed -Jobs <cores>; -p:FullTests=true all

    WHY BLACK BOX. The deliverable is a native exe that consumers call from MSBuild, from a Claude Code Stop
    hook and from node launchers. What has to keep working is therefore the CLI: which files are measured,
    what they count as, what is printed, and what is exited with. A unit test around an internal method
    would pass while all of that regressed.

    What is under test is `out\structuregate.dll` from the last build, or the published exe if no dll exists.
    Set PSGATE_TEST_GATE to point at a specific binary (e.g. a deployed copy in a consumer).

    A GREEN RUN THAT CHOSE ITS OWN SUITES (all of them, or `-Changed`) leaves `out\gate-tests.stamp`, dated when the
    run STARTED - a file edited while it ran is newer, and the next `-Changed` run takes it. Which suites a change
    reaches, and how `-Jobs` deals them out, is `harness\Suites.ps1`. `-List` is how a parallel run starts its shares.
#>
[CmdletBinding()]
param([string]$Only = '', [switch]$WindowsOnly, [switch]$Changed, [int]$Jobs = 1, [string]$List = '', [string]$TimesOut = '')

$ErrorActionPreference = 'Stop'
$started = [DateTime]::UtcNow
$repo = Split-Path $PSScriptRoot -Parent
. (Join-Path $PSScriptRoot 'harness/Suites.ps1')
. (Join-Path $PSScriptRoot 'Assert.ps1')

# -Recurse, so a suite may live in a FOLDER of its own: tests\ is at the file limit this gate holds every folder
# to, and a suite dropped in beside these would have been a suite that runs nowhere and says nothing. Sorted by
# NAME and not by path, because the order suites are dot-sourced in is the order their helpers overwrite each
# other, and that order must not change when a file moves into a folder.
$suites = @(Get-ChildItem $PSScriptRoot -Filter '*.Tests.ps1' -Recurse | Sort-Object Name)
if ($List) {
    $wanted = [System.IO.File]::ReadAllLines($List)
    $suites = @($suites | Where-Object { $wanted -contains $_.FullName })
}
if ($Only) { $suites = @($suites | Where-Object { $_.Name.StartsWith($Only, [StringComparison]::OrdinalIgnoreCase) }) }
# Only the suites that HOLD a Windows case are even loaded: loading one does its setup (a compiler install, a
# fixture build) whether or not any of its cases runs.
if ($WindowsOnly) {
    $script:WindowsOnlyRun = $true
    $suites = @($suites | Where-Object { [System.IO.File]::ReadAllText($_.FullName).Contains('Test-WindowsCase ') })
}
if ($suites.Count -eq 0 -and -not $Changed) { Write-Host "no suite matches '$Only'"; exit 1 }

# THE STAMP IS WRITTEN ONLY BY A RUN THAT CHOSE ITS OWN SUITES; a `-Only` run proves nothing about the others.
$certifies = -not ($Only -or $WindowsOnly -or $List)
function Write-Stamp {
    if (-not $certifies) { return }
    [void](New-Item -ItemType Directory -Force -Path (Split-Path $script:SuiteStamp -Parent))
    [System.IO.File]::WriteAllText($script:SuiteStamp, "")
    [System.IO.File]::SetLastWriteTimeUtc($script:SuiteStamp, $started)
}

if ($Changed -and $certifies) {
    $paths = Get-ChangedPaths $repo
    # A BINARY NEWER THAN THE STAMP WITH NO SOURCE NEWER (another cargo profile, a restored file) is a gate no
    # suite has run against: every suite.
    $binaries = @((Join-Path $script:Out 'structuregate.dll'), (Join-Path $script:Out 'libfbtcore.so'), (Join-Path $script:Out 'fbtcore.dll'))
    $rebuilt = $null -ne $paths -and @($binaries | Where-Object {
        (Test-Path $_) -and (Get-Item $_).LastWriteTimeUtc -gt (Get-Item $script:SuiteStamp).LastWriteTimeUtc }).Count -gt 0
    if ($null -eq $paths) { Write-Host "structuregate tests: no green run recorded - every suite" }
    elseif ($rebuilt -and $paths.Count -eq 0) { Write-Host "structuregate tests: the gate was rebuilt with no source changed - every suite" }
    else {
        $picked = Select-ChangedSuites $suites $paths
        if ($null -ne $picked) {
            if ($picked.Count -eq 0) {
                Write-Host "structuregate tests: nothing a suite covers changed since the last green run"
                Write-Stamp
                exit 0
            }
            Write-Host "structuregate tests: $($picked.Count) of $($suites.Count) suite(s), for $($paths.Count) changed file(s)"
            $suites = $picked
        }
    }
}

$gate = Get-GateInvocation
$clock = [Diagnostics.Stopwatch]::StartNew()

# A PARALLEL RUN: each share is this script again over a list, its output kept whole and printed in turn, so two
# suites' lines never interleave. The parent only deals, waits, prints and adds up.
if ($Jobs -gt 1 -and $suites.Count -gt 1 -and -not $List) {
    $shares = Split-Suites $suites $Jobs
    Write-Host "structuregate tests: $(($gate.Lead + $gate.File) -join ' '), $($suites.Count) suite(s) over $($shares.Count) process(es)"
    $work = Join-Path $script:Out "gate-tests-$PID"
    [void](New-Item -ItemType Directory -Force -Path $work)
    $host_ = (Get-Process -Id $PID).Path
    $runs = for ($i = 0; $i -lt $shares.Count; $i++) {
        $listFile = Join-Path $work "share$i.txt"
        [System.IO.File]::WriteAllLines($listFile, [string[]]@($shares[$i] | ForEach-Object { $_.FullName }))
        $arguments = @('-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-File', $PSCommandPath,
            '-List', $listFile, '-TimesOut', (Join-Path $work "times$i.tsv"))
        if ($WindowsOnly) { $arguments += '-WindowsOnly' }
        $process = Start-Process -FilePath $host_ -ArgumentList $arguments -NoNewWindow -PassThru `
            -RedirectStandardOutput (Join-Path $work "out$i.txt") -RedirectStandardError (Join-Path $work "err$i.txt")
        # The handle, taken now: Start-Process hands back a process whose ExitCode reads empty once it exits otherwise.
        [void]$process.Handle
        [pscustomobject]@{ Index = $i; Process = $process }
    }
    $failed = 0
    $measured = @{}
    foreach ($run in $runs) {
        $run.Process.WaitForExit()
        $i = $run.Index
        Write-Host ""
        Write-Host "  ---- share $($i + 1) of $($shares.Count): $($shares[$i].Count) suite(s) ----"
        foreach ($file in @("out$i.txt", "err$i.txt")) {
            $path = Join-Path $work $file
            if (Test-Path $path) { foreach ($line in [System.IO.File]::ReadAllLines($path)) { Write-Host $line } }
        }
        if ($run.Process.ExitCode -ne 0) { $failed++ }
        $times = Join-Path $work "times$i.tsv"
        if (Test-Path $times) {
            foreach ($line in [System.IO.File]::ReadAllLines($times)) {
                $cells = $line.Split("`t")
                $measured[$cells[0]] = [double]::Parse($cells[1], [Globalization.CultureInfo]::InvariantCulture)
            }
        }
    }
    Save-SuiteTimes $measured
    Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
    Write-Host ""
    if ($failed -gt 0) { Write-Host "FAILED: $failed of $($shares.Count) share(s), $([int]$clock.Elapsed.TotalSeconds)s - each share's failures are listed above"; exit 1 }
    Write-Host "PASSED: $($suites.Count) suite(s) over $($shares.Count) process(es), $([int]$clock.Elapsed.TotalSeconds)s"
    Write-Stamp
    exit 0
}

Write-Host "structuregate tests: $(($gate.Lead + $gate.File) -join ' ')"
$measured = @{}
foreach ($suite in $suites) {
    Write-Host ""
    Write-Host "  $($suite.BaseName)"
    $before = $script:Passed + $script:Failed.Count
    $suiteClock = [Diagnostics.Stopwatch]::StartNew()
    # A suite that THROWS outside a case is one failure, not the end of the run: every suite after it
    # would otherwise go unreported, and the exit code would say nothing about which ones they were.
    try { . $suite.FullName }
    catch {
        [void]$script:Failed.Add("$($suite.BaseName) stopped outside a case`n      $($_.Exception.Message)")
        Write-Host "  FAIL  $($suite.BaseName) stopped outside a case: $($_.Exception.Message)"
    }
    $measured[$suite.BaseName] = $suiteClock.Elapsed.TotalSeconds
    $ran = ($script:Passed + $script:Failed.Count) - $before
    Write-Host "    $ran case(s), $([int]$suiteClock.Elapsed.TotalSeconds)s"
}
foreach ($scratch in $script:RunScratch) { Remove-Item -LiteralPath $scratch -Recurse -Force -ErrorAction SilentlyContinue }
if ($TimesOut) {
    [System.IO.File]::WriteAllLines($TimesOut, [string[]]@($measured.Keys | ForEach-Object {
        "$_`t$($measured[$_].ToString('0.0', [Globalization.CultureInfo]::InvariantCulture))" }))
}
else { Save-SuiteTimes $measured }

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
Write-Stamp
exit 0
