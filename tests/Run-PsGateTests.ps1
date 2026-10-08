<#
    Run-PsGateTests.ps1 - the regression test for the PowerShell rules.

    Runs src\PsGate\PsGate.ps1 over the two fixtures and compares its WHOLE output against expected.txt, line for
    line: the findings AND the token line counts. Nothing is asserted loosely, because the failure this
    guards against is a rule going QUIET - which no loose assertion notices.

    The build runs it (PsGateTests in StructureGate.csproj), so a rule that stops firing fails the build.
    By hand:
      powershell -NoProfile -File tests\Run-PsGateTests.ps1            # 5.1, the host that ships
      powershell -NoProfile -File tests\Run-PsGateTests.ps1 -Update    # rewrite expected.txt, then DIFF it

    Exit 0 = the output matches. Exit 1 = it does not, and every differing line is printed.
#>
[CmdletBinding()]
param(
    # 5.1 on Windows, the host that ships; `pwsh` where 5.1 does not exist.
    [string]$PsHost = $(if ($env:OS -eq 'Windows_NT') { 'powershell.exe' } else { 'pwsh' }),
    [switch]$Update
)

$ErrorActionPreference = 'Stop'

$tests = $PSScriptRoot
$gate = Join-Path (Split-Path $tests -Parent) 'src\PsGate\PsGate.ps1'
$fixtures = Join-Path $tests 'fixtures'
$expectedFile = Join-Path $fixtures 'expected.txt'
$names = @('violations.ps1', 'clean.ps1')

if (-not (Test-Path $gate)) { Write-Host "psgate tests: $gate is missing"; exit 1 }

# The list carries a STABLE relative path, so expected.txt does not depend on where the repo is checked out.
$listFile = Join-Path ([System.IO.Path]::GetTempPath()) "psgate-tests-$PID.txt"
$rows = foreach ($name in $names) { "tests/fixtures/$name`t$(Join-Path $fixtures $name)" }
[System.IO.File]::WriteAllLines($listFile, $rows)

$actual = & $PsHost -NoProfile -NonInteractive -ExecutionPolicy Bypass -File $gate -ListFile $listFile 2>&1 |
    ForEach-Object { "$_".TrimEnd() }
Remove-Item $listFile -ErrorAction SilentlyContinue

if ($Update) {
    [System.IO.File]::WriteAllLines($expectedFile, $actual)
    Write-Host "psgate tests: expected.txt rewritten ($($actual.Count) lines) - review the diff"
    exit 0
}

if (-not (Test-Path $expectedFile)) {
    Write-Host "psgate tests: expected.txt is missing - run with -Update and review the diff"
    exit 1
}
$expected = [System.IO.File]::ReadAllLines($expectedFile) | ForEach-Object { $_.TrimEnd() }

# Compared by INDEX, not as sets: the order is the rule order, and a rule that moves is a change to report.
$failed = $false
$max = [Math]::Max($actual.Count, $expected.Count)
for ($i = 0; $i -lt $max; $i++) {
    $a = if ($i -lt $actual.Count) { $actual[$i] } else { '<missing>' }
    $e = if ($i -lt $expected.Count) { $expected[$i] } else { '<unexpected>' }
    if ($a -ceq $e) { continue }
    if (-not $failed) { Write-Host "psgate tests: FAILED" }
    $failed = $true
    Write-Host "  line $($i + 1)"
    Write-Host "    expected: $e"
    Write-Host "    actual  : $a"
}

if ($failed) {
    Write-Host "psgate tests: $($expected.Count) expected line(s), $($actual.Count) produced."
    Write-Host "  A rule that went quiet is the failure this test exists for - fix the rule, or accept the"
    Write-Host "  change with -Update and put the reason in the commit."
    exit 1
}

$findings = @($actual | Where-Object { $_.StartsWith('PSGATE|') }).Count
Write-Host "psgate tests OK: $($names.Count) fixtures, $findings finding(s), $($expected.Count) lines matched."
exit 0
