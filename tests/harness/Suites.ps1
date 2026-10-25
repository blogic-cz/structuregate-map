<#
    Suites.ps1 - WHICH suites a run takes and HOW they are split across processes. Dot-sourced by Run-Tests.ps1.

    A PARTIAL RUN (`-Changed`) takes the suites a change since the last green run can reach: every file git knows
    (tracked or untracked, never ignored) that is NEWER than the stamp a passing run leaves in `out\`. A path is
    matched against `$script:SuiteRules` by prefix; a path NO rule names - the store, the walk, the CLI, a harness
    helper, the csproj - runs EVERY suite, so a rule that is missing costs minutes, never a regression. Only paths
    that reach no binary and no suite (`$script:SuiteInert`, docs) are passed over.

    A PARALLEL RUN (`-Jobs N`) deals the suites to N processes by the time each took last run (`out\gate-tests.times`),
    longest first onto the shortest process; each process keeps NAME order among its own suites, so the order helpers
    overwrite each other in is the full run's. Every suite already has to pass alone (`-Only`), which is what makes
    a suite safe to run beside any other.
#>

# Suites are matched by NAME PREFIX: 'Ts' is TsGate, TsRows*, TsSkip and TsPlain*; 'Map' every Map* suite.
$script:SuiteRules = @(
    @{ Paths = @('src/TsRows/', 'src/TsGate/', 'rust/fbtcore/src/rows/ts/', 'rust/fbtcore/src/rows/tsapply',
                 'rust/fbtcore/src/rows/derive/tsderive', 'rust/fbtcore/src/mapper/deep/ts.rs', 'rust/fbtcore/src/cssmap/')
       Suites = @('Ts', 'HttpLinks', 'Map', 'Magic', 'FullText', 'PyEarly') }
    @{ Paths = @('rust/fbtcore/src/rows/http/')
       Suites = @('HttpLinks', 'Ts', 'Cs') }
    @{ Paths = @('src/Map/Sql/', 'rust/fbtcore/src/sqlpg/', 'rust/fbtcore/src/rows/sqlshape/', 'rust/fbtcore/src/rows/sqlrun.rs',
                 'rust/fbtcore/src/rows/seeds/', 'rust/fbtcore/src/cli/sqlparse.rs', 'rust/fbtcore/src/mapper/deep/sqlconfig.rs',
                 'rust/fbtcore/vendor/', 'rust/fbtcore/build.rs')
       Suites = @('Sql', 'DocFacts', 'Magic', 'FullText', 'CsRows') }
    @{ Paths = @('rust/fbtcore/src/facts/')
       Suites = @('DocFacts') }
    @{ Paths = @('src/Map/Py/')
       Suites = @('Py', 'Map', 'Magic', 'FullText') }
    @{ Paths = @('src/Map/CsRows/', 'src/Map/CSharpMap.cs', 'src/Map/MapRows.cs', 'rust/fbtcore/src/csproj/')
       Suites = @('Cs', 'HttpLinks', 'Sql', 'Map', 'Magic', 'FullText') }
    @{ Paths = @('src/AsyncRules.cs')
       Suites = @('Discipline', 'Cs') }
    @{ Paths = @('src/PsGate/')
       Suites = @('PsGate', 'Discipline', 'Rules') }
    @{ Paths = @('src/Map/PsMap.ps1')
       Suites = @('MapPowerShell', 'Map') }
    @{ Paths = @('rust/fbtcore/src/rsmap/', 'rust/fbtcore/src/mapper/deep/rust.rs')
       Suites = @('Rust', 'Map', 'Magic', 'Lines') }
    @{ Paths = @('rust/fbtcore/src/mdmap/')
       Suites = @('Markdown', 'Map', 'Docs') }
    @{ Paths = @('rust/fbtcore/src/gomap/')
       Suites = @('MapGo', 'Map') }
    @{ Paths = @('src/MapView/', 'rust/fbtcore/src/view/')
       Suites = @('MapView') }
    @{ Paths = @('rust/fbtcore/src/trace/')
       Suites = @('Trace') }
    @{ Paths = @('rust/fbtcore/src/slice/')
       Suites = @('TsRowsSlice', 'CsEnumTypes') }
    @{ Paths = @('rust/fbtcore/src/rows/keys/')
       Suites = @('EntityKeys', 'TsEntityKeys', 'Map', 'Cs', 'Sql', 'Py', 'TsRows') }
    @{ Paths = @('rust/fbtcore/src/rows/dispatch/')
       Suites = @('CsDispatch', 'CsEnumTypes') }
    @{ Paths = @('rust/fbtcore/src/cli/hook.rs')
       Suites = @('ClaudeHook') }
    @{ Paths = @('scripts/Update-Gate.ps1')
       Suites = @('UpdateGate') }
    @{ Paths = @('scripts/')
       Suites = @('Connect', 'UpdateGate') }
    @{ Paths = @('skills/', 'agents/', 'hooks/', '.claude-plugin/', 'StructureGate.targets')
       Suites = @('Connect', 'ClaudeHook', 'MapPlugins', 'UpdateGate') }
    @{ Paths = @('tests/fixtures/', 'tests/Run-PsGateTests.ps1')
       Suites = @('PsGate') }
)
# What reaches neither the binary nor a suite.
$script:SuiteInert = @('.claude/', 'docs/', 'README.md', 'CLAUDE.md', 'LICENSE', '.gitignore', '.github/',
    'map-baseline.json', 'structure-baseline.json', 'leak-deny.txt')
# THE SELF MAP maps these folders of THIS repo, so a change in any of them can move its verdict.
$script:SelfMapped = @('src/', 'rust/fbtcore/src/', 'rust/fbt/src/', 'scripts/')

$script:SuiteStamp = Join-Path (Join-Path (Split-Path $PSScriptRoot -Parent | Split-Path -Parent) 'out') 'gate-tests.stamp'
$script:SuiteTimes = Join-Path (Split-Path $script:SuiteStamp -Parent) 'gate-tests.times'

# The repo paths, '/'-separated, changed since the last green run; $null when there is no stamp (everything).
function Get-ChangedPaths([string]$Repo) {
    if (-not (Test-Path $script:SuiteStamp)) { return $null }
    $since = (Get-Item $script:SuiteStamp).LastWriteTimeUtc
    $listed = & git -C $Repo ls-files -c -o --exclude-standard 2>$null
    if ($LASTEXITCODE -ne 0) { return $null }
    $changed = New-Object System.Collections.Generic.List[string]
    foreach ($rel in $listed) {
        $file = [System.IO.FileInfo]::new((Join-Path $Repo $rel))
        if ($file.Exists -and $file.LastWriteTimeUtc -gt $since) { $changed.Add($rel.Replace('\', '/')) }
    }
    # A DELETED file is newer than nothing: git names it, and it moved the binary all the same.
    foreach ($rel in @(& git -C $Repo ls-files -d 2>$null)) { $changed.Add($rel.Replace('\', '/')) }
    return , $changed.ToArray()
}

# The suites a set of changed paths reaches, or $null for every suite.
function Select-ChangedSuites($Suites, [string[]]$Changed) {
    $prefixes = New-Object System.Collections.Generic.HashSet[string]
    $exact = New-Object System.Collections.Generic.HashSet[string]
    foreach ($path in $Changed) {
        $name = [System.IO.Path]::GetFileName($path)
        if ($path.StartsWith('tests/') -and $name.EndsWith('.Tests.ps1')) { [void]$exact.Add($name.Substring(0, $name.Length - 10)); continue }
        if ($name -eq 'CLAUDE.md' -or $name -eq 'AGENTS.md') { continue }
        if (@($script:SuiteInert | Where-Object { $path.StartsWith($_) }).Count -gt 0) { continue }
        $rules = @($script:SuiteRules | Where-Object { @($_.Paths | Where-Object { $path.StartsWith($_) }).Count -gt 0 })
        if ($rules.Count -eq 0) {
            Write-Host "  $path is in no suite rule: every suite"
            return $null
        }
        foreach ($rule in $rules) { foreach ($prefix in $rule.Suites) { [void]$prefixes.Add($prefix) } }
        if (@($script:SelfMapped | Where-Object { $path.StartsWith($_) }).Count -gt 0) { [void]$prefixes.Add('SelfMap') }
    }
    return , @($Suites | Where-Object {
        $base = $_.BaseName.Substring(0, $_.BaseName.Length - 6)
        $exact.Contains($base) -or @($prefixes | Where-Object { $base.StartsWith($_) }).Count -gt 0
    })
}

# The seconds each suite took on its last run, by suite name; a suite never timed counts as 10.
function Get-SuiteTimes {
    $times = @{}
    if (Test-Path $script:SuiteTimes) {
        foreach ($line in [System.IO.File]::ReadAllLines($script:SuiteTimes)) {
            $cells = $line.Split("`t")
            if ($cells.Count -eq 2) { $times[$cells[0]] = [double]::Parse($cells[1], [Globalization.CultureInfo]::InvariantCulture) }
        }
    }
    return $times
}

# What this run measured, merged over what earlier runs did, so a partial run never forgets the rest.
function Save-SuiteTimes([hashtable]$Measured) {
    $times = Get-SuiteTimes
    foreach ($key in $Measured.Keys) { $times[$key] = $Measured[$key] }
    [void](New-Item -ItemType Directory -Force -Path (Split-Path $script:SuiteTimes -Parent))
    $lines = $times.Keys | Sort-Object | ForEach-Object { "$_`t$($times[$_].ToString('0.0', [Globalization.CultureInfo]::InvariantCulture))" }
    [System.IO.File]::WriteAllLines($script:SuiteTimes, [string[]]@($lines))
}

# The suites dealt to N processes: longest first onto the least loaded, each share back in NAME order.
function Split-Suites($Suites, [int]$Jobs) {
    $times = Get-SuiteTimes
    $shares = @(1..$Jobs | ForEach-Object { [pscustomobject]@{ Load = 0.0; Suites = New-Object System.Collections.ArrayList } })
    $byTime = $Suites | Sort-Object @{ Expression = { if ($times.ContainsKey($_.BaseName)) { $times[$_.BaseName] } else { 10.0 } }; Descending = $true }, Name
    foreach ($suite in $byTime) {
        $least = $shares | Sort-Object Load | Select-Object -First 1
        [void]$least.Suites.Add($suite)
        $least.Load += if ($times.ContainsKey($suite.BaseName)) { $times[$suite.BaseName] } else { 10.0 }
    }
    return , @($shares | Where-Object { $_.Suites.Count -gt 0 } | ForEach-Object { , @($_.Suites | Sort-Object Name) })
}

# A CACHE TWO PROCESSES FILL AT ONCE is a corrupt cache: `npm install` into one prefix from two shards races.
# The body runs holding an exclusive lock on `<cache>.lock`, so the second process waits, then finds it filled.
function Use-CacheLock([string]$Cache, [scriptblock]$Body) {
    $lockPath = "$($Cache.TrimEnd('\', '/')).lock"
    $lock = $null
    while ($null -eq $lock) {
        try { $lock = [System.IO.File]::Open($lockPath, 'OpenOrCreate', 'ReadWrite', 'None') }
        catch [System.IO.IOException] { Start-Sleep -Milliseconds 250 }
    }
    try { & $Body }
    finally { $lock.Dispose() }
}
