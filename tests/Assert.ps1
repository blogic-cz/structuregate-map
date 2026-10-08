<#
    Assert.ps1 - the whole test harness. Dot-sourced by Run-Tests.ps1 before every suite.

    BLACK BOX, ON PURPOSE. The deliverable is a native exe with no runtime beside it, so the thing worth
    testing is what the exe DOES: which files it measures, what it counts them as, what it prints, and what
    it exits with. A unit test around an internal method would pass while the CLI regressed - and every
    consumer calls the CLI, from MSBuild, from a Stop hook, or from a node launcher.

    Each case builds a throwaway tree under TEMP, runs the gate against it, and asserts on the exit code
    and the output lines. Trees are deleted after the run unless a case fails, and then the path is printed
    so the tree can be looked at.
#>

$script:Passed = 0
$script:Failed = New-Object System.Collections.ArrayList
$script:KeptTrees = New-Object System.Collections.ArrayList
# Every tree the running case made - `New-Tree` adds each one, so a case that builds two loses neither.
$script:CaseTrees = New-Object System.Collections.ArrayList

# A TREE OLDER THAN A DAY IS NOBODY'S: a failing case keeps its tree to be looked at, and an interrupted run keeps
# them all, so they piled up. Only `sgtest-` + 12 hex digits (and its
# `_env` sibling) is a case's tree; the npm caches beside them are named by package. A day, never "every one":
# a suite running in another worktree shares this TEMP.
foreach ($stale in Get-ChildItem ([System.IO.Path]::GetTempPath()) -Directory -Filter 'sgtest-*' -ErrorAction SilentlyContinue) {
    $id = $stale.Name.Substring(7)
    if ($id.EndsWith('_env')) { $id = $id.Substring(0, $id.Length - 4) }
    $hex = $id.Length -eq 12 -and -not ($id.ToCharArray() | Where-Object { '0123456789abcdef'.IndexOf($_) -lt 0 })
    if ($hex -and $stale.LastWriteTime -lt (Get-Date).AddDays(-1)) {
        Remove-Item -LiteralPath $stale.FullName -Recurse -Force -ErrorAction SilentlyContinue
    }
}
# And the Fbt suite's databases, `fbt-` + 32 hex digits - a failing case keeps its own.
foreach ($stale in Get-ChildItem ([System.IO.Path]::GetTempPath()) -File -Filter 'fbt-*.db' -ErrorAction SilentlyContinue) {
    $id = $stale.BaseName.Substring(4)
    $hex = $id.Length -eq 32 -and -not ($id.ToCharArray() | Where-Object { '0123456789abcdef'.IndexOf($_) -lt 0 })
    if ($hex -and $stale.LastWriteTime -lt (Get-Date).AddDays(-1)) { Remove-Item -LiteralPath $stale.FullName -Force -ErrorAction SilentlyContinue }
}
$script:Root = Split-Path $PSScriptRoot -Parent
# Every build output lands here (`OutputPath` in src/StructureGate.csproj), never in the repo root.
$script:Out = Join-Path $script:Root 'out'

# THE PYTHON THE SUITES RUN, and the one the gate defaults to: `python` on Windows, and `python3` where a
# distribution ships only that name. Empty when there is neither, and every python suite then says so.
$script:Python = @('python', 'python3') | Where-Object { Get-Command $_ -ErrorAction SilentlyContinue } |
    Select-Object -First 1

# THE POWERSHELL A SUITE STARTS: Windows PowerShell 5.1 where it exists, because that is what every consumer
# launches; `pwsh` everywhere else, where 5.1 does not.
$script:OnWindows = $env:OS -eq 'Windows_NT'
$script:PowerShell = if ($script:OnWindows) { 'powershell.exe' } else { 'pwsh' }

# WHERE .NET AND THE NUGET CACHE ARE, found the way the gate finds them (rust/fbtcore/src/csproj/disk.rs):
# DOTNET_ROOT, else `%ProgramFiles%\dotnet` on Windows and the folder of the `dotnet` on PATH elsewhere.
function Get-DotnetRoot {
    if ($env:DOTNET_ROOT) { return $env:DOTNET_ROOT }
    if ($script:OnWindows) { return (Join-Path $env:ProgramFiles 'dotnet') }
    $dotnet = Get-Command dotnet -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
    if (-not $dotnet) { return '/usr/share/dotnet' }
    $real = (Get-Item $dotnet.Source).ResolveLinkTarget($true)
    if ($real) { return (Split-Path $real.FullName -Parent) }
    return (Split-Path $dotnet.Source -Parent)
}
$script:DotnetRoot = Get-DotnetRoot
$script:NuGetPackages = if ($env:NUGET_PACKAGES) { $env:NUGET_PACKAGES }
    elseif ($script:OnWindows) { Join-Path $env:USERPROFILE '.nuget\packages' }
    else { Join-Path $HOME '.nuget/packages' }

# WHAT IS UNDER TEST. `structuregate.dll` is what a build just produced, so it is preferred: a published
# exe can be older than the source being tested, and a test that silently checks last week's binary is
# worse than no test. PSGATE_TEST_GATE overrides for a one-off (e.g. testing a deployed copy).
function Get-GateInvocation {
    if ($env:PSGATE_TEST_GATE) { return @{ File = $env:PSGATE_TEST_GATE; Lead = @() } }
    $dll = Join-Path $script:Out 'structuregate.dll'
    if (Test-Path $dll) { return @{ File = 'dotnet'; Lead = @($dll) } }
    $exe = Join-Path $script:Out 'win-x64\publish\structuregate.exe'
    if (Test-Path $exe) { return @{ File = $exe; Lead = @() } }
    throw "no gate to test: build the project first (dotnet build src\StructureGate.csproj)"
}

function Invoke-Gate {
    # [object[]] and a manual flatten, NOT [string[]]. PowerShell parses a bare `.cs,.py,.js` as an ARRAY,
    # and coercing that to [string[]] turns it into the single string ".cs .py .js" — so `--ext` silently
    # received one bogus extension, the gate measured nothing, and the case failed on an empty dump instead
    # of on the thing it was testing.
    param([Parameter(ValueFromRemainingArguments)][object[]]$GateArgs)
    $gate = Get-GateInvocation
    $all = @($gate.Lead)
    foreach ($argument in $GateArgs) { foreach ($part in @($argument)) { if ($null -ne $part) { $all += "$part" } } }
    # A $null is DROPPED, never sent as "": pwsh 7 splats an `if` that produced nothing as one $null, where
    # 5.1 splatted no argument at all, and the gate then refused an empty argument it was never meant to get.
    # `2>&1` turns a native command's stderr into an ErrorRecord, and with $ErrorActionPreference = 'Stop'
    # that is a TERMINATING error — so the gate writing one honest note to stderr (an unreadable path, the
    # --dump NOTE) failed the case instead of being asserted on. stderr is part of what is under test.
    $previous = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try { $out = & $gate.File @all 2>&1 | ForEach-Object { "$_".TrimEnd() } }
    finally { $ErrorActionPreference = $previous }
    return [pscustomobject]@{ Exit = $LASTEXITCODE; Lines = @($out); Text = ($out -join "`n") }
}

# The gate with an EXACT command line, when what is under test is how the gate parses an argument. Windows
# PowerShell re-quotes native arguments on its own and there is no way to stop it, so a case about quoting
# (`--plugin "<path with a space>"`) cannot go through Invoke-Gate at all: the shell would have already
# rewritten the thing being asserted on.
function Invoke-GateRaw([string]$CommandLine) {
    $gate = Get-GateInvocation
    $lead = if ($gate.Lead.Count -gt 0) { '"' + ($gate.Lead -join '" "') + '" ' } else { '' }
    $psi = New-Object System.Diagnostics.ProcessStartInfo
    $psi.FileName = $gate.File
    $psi.Arguments = $lead + $CommandLine
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.UseShellExecute = $false
    $process = [System.Diagnostics.Process]::Start($psi)
    $out = $process.StandardOutput.ReadToEnd() + $process.StandardError.ReadToEnd()
    $process.WaitForExit()
    $lines = @($out -split "`n" | ForEach-Object { $_.TrimEnd() })
    return [pscustomobject]@{ Exit = $process.ExitCode; Lines = $lines; Text = $out }
}

# The two streams kept APART, when what is under test is WHICH stream a line went to. Invoke-Gate merges
# them with `2>&1`, so it cannot tell a verdict on stdout from a verdict on stderr - and that difference is
# what a Claude Code hook shows the reader.
function Invoke-GateStream {
    param([Parameter(ValueFromRemainingArguments)][object[]]$GateArgs)
    $gate = Get-GateInvocation
    $all = @($gate.Lead)
    foreach ($argument in $GateArgs) { foreach ($part in @($argument)) { if ($null -ne $part) { $all += "$part" } } }
    $psi = New-Object System.Diagnostics.ProcessStartInfo
    $psi.FileName = $gate.File
    # A quoted Arguments STRING, not ArgumentList: Windows PowerShell 5.1 runs these suites and its
    # ProcessStartInfo has no ArgumentList. Test paths hold spaces, so every argument is quoted.
    $psi.Arguments = ($all | ForEach-Object { '"' + ($_ -replace '"', '\\"') + '"' }) -join ' '
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.UseShellExecute = $false
    $process = [System.Diagnostics.Process]::Start($psi)
    # BOTH read before WaitForExit: a full pipe buffer blocks the child, and the gate can fill one.
    $outTask = $process.StandardOutput.ReadToEndAsync()
    $errTask = $process.StandardError.ReadToEndAsync()
    $process.WaitForExit()
    # Text as well as the two streams, so the shared Assert-* helpers can report on this result too.
    return [pscustomobject]@{ Exit = $process.ExitCode; Out = $outTask.Result; Err = $errTask.Result
                              Text = $outTask.Result + $errTask.Result }
}

# A tree from a map of relative path -> content. A path ending in `/` is an empty directory.
function New-Tree([hashtable]$Files) {
    $root = Join-Path ([System.IO.Path]::GetTempPath()) "sgtest-$([System.Guid]::NewGuid().ToString('N').Substring(0,12))"
    [void](New-Item -ItemType Directory -Path $root -Force)
    [void]$script:CaseTrees.Add($root)
    foreach ($key in $Files.Keys) {
        # A key may be written `app\App.csproj`. pwsh's cmdlets take `\` for a separator off Windows, but
        # File.WriteAllText does not, and would make one file with a backslash in its NAME.
        $rel = if ($script:OnWindows) { $key } else { $key.Replace('\', '/') }
        $full = Join-Path $root $rel
        if ($rel.EndsWith('/')) { [void](New-Item -ItemType Directory -Path $full -Force); continue }
        [void](New-Item -ItemType Directory -Path (Split-Path $full -Parent) -Force)
        [System.IO.File]::WriteAllText($full, [string]$Files[$key])
    }
    return $root
}

# N lines of real code, for pushing a file over a limit without writing 500 lines by hand.
function New-Code([int]$Lines, [string]$Comment = '') {
    $sb = New-Object System.Text.StringBuilder
    if ($Comment) { [void]$sb.AppendLine($Comment) }
    for ($i = 1; $i -le $Lines; $i++) { [void]$sb.AppendLine("var line$i = $i;") }
    return $sb.ToString()
}

# -WindowsOnly (Run-Tests.ps1) runs the Test-WindowsCase cases and nothing else - what CI on Windows asks,
# since every other case runs on any machine and is run where the gate is developed.
$script:WindowsOnlyRun = $false
$script:InWindowsCase = $false

function Test-Case([string]$Name, [scriptblock]$Body) {
    if ($script:WindowsOnlyRun -and -not $script:InWindowsCase) { return }
    $script:CurrentTree = $null
    $script:CaseTrees.Clear()
    try {
        & $Body
        $script:Passed++
        foreach ($tree in $script:CaseTrees) {
            if (Test-Path $tree) { Remove-Item $tree -Recurse -Force -ErrorAction SilentlyContinue }
        }
    } catch {
        [void]$script:Failed.Add("$Name`n      $($_.Exception.Message)")
        Write-Host "  FAIL  $Name"
        Write-Host "        $($_.Exception.Message)"
        foreach ($tree in $script:CaseTrees) {
            [void]$script:KeptTrees.Add($tree)
            Write-Host "        tree kept: $tree"
        }
    }
}

# A case whose subject only EXISTS on Windows - Windows PowerShell 5.1's grammar, `cmd`, the .NET Framework.
# Elsewhere it is named as not run, so a skipped case is never mistaken for a passing one.
function Test-WindowsCase([string]$Name, [scriptblock]$Body) {
    if ($script:OnWindows) {
        $script:InWindowsCase = $true
        try { Test-Case $Name $Body } finally { $script:InWindowsCase = $false }
        return
    }
    Write-Host "    (Windows only, not run here: $Name)"
}

# A folder a case made BESIDE its tree (Connect's `_env`, a config outside the tree): removed with the case's trees
# when it passes, kept with them when it fails.
function Register-Tree([string]$Path) {
    if (-not $script:CaseTrees.Contains($Path)) { [void]$script:CaseTrees.Add($Path) }
}

# The case's tree, by name: `New-Tree` already registers every tree, so a failing case leaves it on disk and a
# passing one does not.
function Use-Tree([hashtable]$Files) {
    $script:CurrentTree = New-Tree $Files
    return $script:CurrentTree
}

function Assert-Exit($Result, [int]$Expected) {
    if ($Result.Exit -ne $Expected) {
        throw "expected exit $Expected, got $($Result.Exit). Output:`n$($Result.Text)"
    }
}

function Assert-Line($Result, [string]$Needle) {
    foreach ($line in $Result.Lines) { if ($line.Contains($Needle)) { return } }
    throw "no line contains '$Needle'. Output:`n$($Result.Text)"
}

function Assert-NoLine($Result, [string]$Needle) {
    foreach ($line in $Result.Lines) {
        if ($line.Contains($Needle)) { throw "unexpected line contains '$Needle': $line" }
    }
}

function Assert-Equal($Actual, $Expected, [string]$What) {
    if ("$Actual" -cne "$Expected") { throw "$What`: expected '$Expected', got '$Actual'" }
}

# --dump, as an object. Every count the gate measured, which is the only assertion that pins a COUNT
# rather than the sentence printed about it.
function Get-Dump {
    param([Parameter(ValueFromRemainingArguments)][object[]]$GateArgs)
    $result = Invoke-Gate @GateArgs --dump
    Assert-Exit $result 0
    # stderr can carry the unreadable-path note, so only the JSON body is parsed.
    $json = ($result.Lines | Where-Object { -not $_.StartsWith('NOTE:') }) -join "`n"
    return $json | ConvertFrom-Json
}

# The measured paths in a --dump section. An EMPTY section yields no properties, and `@($null).Count` is 1,
# which once made an assertion pass on a dump that measured nothing at all.
# `,` because a scriptblock UNROLLS what it returns: a one-file section came back as the bare string
# 'real.cs', and `$measured[0]` indexed a character out of it. This is rule 8 of the PowerShell gate,
# caught in the gate's own test harness.
function Get-Measured($Dump, [string]$Kind) {
    return ,@($Dump.$Kind.PSObject.Properties.Name | Where-Object { $_ })
}

function Get-Count($Dump, [string]$Kind, [string]$Key) {
    $section = $Dump.$Kind
    $property = $section.PSObject.Properties | Where-Object { $_.Name -eq $Key }
    if (-not $property) { throw "$Kind has no entry '$Key'. Entries: $(($section.PSObject.Properties.Name) -join ', ')" }
    return [int]$property.Value
}

# THE REPO'S OWN TYPESCRIPT COMPILER, for the two suites that need one - the rules and the map. It lives here
# rather than in either suite because both ask for it and a second copy of the install-and-cache dance would
# be a second thing to keep right. One compiler is installed into a cache under TEMP on first ask and reached
# through NODE_PATH, which is the same escape hatch a pnpm or a hoisted layout uses.
function Get-TypeScriptPath([string]$Spec = 'typescript') {
    if (-not (Get-Command node -ErrorAction SilentlyContinue)) { return $null }
    $cache = Join-Path ([System.IO.Path]::GetTempPath()) "sgtest-$($Spec.Replace('@', '-'))"
    $modules = Join-Path $cache 'node_modules'
    if (Test-TypeScriptComplete $modules) { return $modules }
    # A CACHE THAT HAS LOST A FILE IS REBUILT, not trusted for having a package.json: one that had lost only
    # the native compiler's lib.d.ts failed every TypeScript case in the suite with "node did not finish".
    if (Test-Path $modules) { Remove-Item $modules -Recurse -Force -ErrorAction SilentlyContinue }
    if (-not (Get-Command npm -ErrorAction SilentlyContinue)) { return $null }
    [void](New-Item -ItemType Directory -Path $cache -Force)
    & npm install --no-save --silent --prefix $cache $Spec 2>&1 | Out-Null
    if (Test-TypeScriptComplete $modules) { return $modules }
    return $null
}

# Whether an installed typescript can start: its manifest, and the compiler's own lib.d.ts - in the package
# itself for 5.x, in the per-platform package beside it for the 7.x native compiler.
function Test-TypeScriptComplete([string]$Modules) {
    if (-not (Test-Path (Join-Path $Modules 'typescript\package.json'))) { return $false }
    if (Test-Path (Join-Path $Modules 'typescript\lib\lib.d.ts')) { return $true }
    foreach ($platform in @(Get-ChildItem (Join-Path $Modules '@typescript') -Directory -Filter 'typescript-*' -ErrorAction SilentlyContinue)) {
        if (Test-Path (Join-Path $platform.FullName 'lib\lib.d.ts')) { return $true }
    }
    return $false
}

function Get-TypeScriptMajor([string]$Modules) {
    if (-not $Modules) { return 0 }
    $manifest = Get-Content (Join-Path $Modules 'typescript\package.json') -Raw | ConvertFrom-Json
    return [int]($manifest.version.Split('.')[0])
}

# Asked once per RUN, not once per suite: the answer can legitimately be $null (no node, no npm), and a bare
# $null cache would reinstall on every ask. The hashtable is the "already asked" flag.
function Get-TsModules {
    if ($null -eq $script:TsCache) { $script:TsCache = @{ Path = (Get-TypeScriptPath) } }
    return $script:TsCache.Path
}

# The gate with a resolvable compiler. NODE_PATH is read by the node the gate spawns, so the fixture tree
# needs no node_modules of its own.
function Invoke-TsGate {
    param([Parameter(ValueFromRemainingArguments)][object[]]$GateArgs)
    $previous = $env:NODE_PATH
    $env:NODE_PATH = Get-TsModules
    try { return Invoke-Gate @GateArgs }
    finally { $env:NODE_PATH = $previous }
}

function Get-TsDump {
    param([Parameter(ValueFromRemainingArguments)][object[]]$GateArgs)
    $previous = $env:NODE_PATH
    $env:NODE_PATH = Get-TsModules
    try { return Get-Dump @GateArgs }
    finally { $env:NODE_PATH = $previous }
}
