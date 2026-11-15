<#
    TELEMETRY: `STRUCTUREGATE_TELEMETRY=<root>` (or `1` for telemetry-agent's own root under local app data) makes a run
    write ONE NDJSON file into `<root>/spool/` at exit, in telemetry-agent's record shapes, and start the agent named by
    `STRUCTUREGATE_TELEMETRY_AGENT` (or found on PATH) with `TELEMETRY_ROOT=<root>`.

    What is held: nothing is written without the variable; the file is whole (no `.part` left) and carries session
    start/end, the run's event and its stage timings; nothing of the TREE is in it (its folder, a file's name, the
    command line); the install id is made once per root; the agent is started with the root; a spool that cannot
    be written is a NOTE and never the run's exit.
#>

$script:TelemetryNames = @('STRUCTUREGATE_TELEMETRY', 'STRUCTUREGATE_TELEMETRY_AGENT', 'STRUCTUREGATE_TRACE', 'XDG_DATA_HOME', 'LOCALAPPDATA')

# The gate with the telemetry variables set for this one run, and every other one of them cleared - restored after.
function Invoke-TelemetryGate([hashtable]$Environment, [object[]]$GateArgs) {
    $saved = @{}
    foreach ($name in $script:TelemetryNames) { $saved[$name] = [Environment]::GetEnvironmentVariable($name) }
    try {
        foreach ($name in @('STRUCTUREGATE_TELEMETRY', 'STRUCTUREGATE_TELEMETRY_AGENT', 'STRUCTUREGATE_TRACE')) {
            [Environment]::SetEnvironmentVariable($name, $null)
        }
        foreach ($name in $Environment.Keys) { [Environment]::SetEnvironmentVariable($name, $Environment[$name]) }
        return Invoke-Gate @GateArgs
    } finally {
        foreach ($name in $script:TelemetryNames) { [Environment]::SetEnvironmentVariable($name, $saved[$name]) }
    }
}

# Every record of every spool file under $Root, parsed; the files themselves in `Files`.
function Read-Spool([string]$Root) {
    $spool = Join-Path $Root 'spool'
    $files = @(Get-ChildItem -LiteralPath $spool -File -ErrorAction SilentlyContinue | Sort-Object Name)
    $records = @(foreach ($file in $files | Where-Object { $_.Name.EndsWith('.ndjson') }) {
        foreach ($line in [System.IO.File]::ReadAllLines($file.FullName)) { if ($line.Trim()) { $line | ConvertFrom-Json } }
    })
    [pscustomobject]@{ Files = $files; Records = $records }
}

function Get-SpoolRecord($Spool, [string]$Kind, [string]$Name) {
    @($Spool.Records | Where-Object { $_.k -eq $Kind -and $_.n -eq $Name })
}

Test-Case 'no STRUCTUREGATE_TELEMETRY, nothing spooled - not even under the default root' {
    $tree = Use-Tree @{ 'a.cs' = (New-Code 3) }
    $data = Join-Path $tree 'data'
    $result = Invoke-TelemetryGate @{ XDG_DATA_HOME = $data; LOCALAPPDATA = $data; STRUCTUREGATE_TELEMETRY_AGENT = 'off' } @('--root', $tree, '--no-gate-cache')
    Assert-Exit $result 0
    Assert-Equal (Test-Path (Join-Path $data 'structuregate/telemetry')) $false 'a run without the variable made a telemetry root'
    Assert-NoLine $result 'telemetry'
}

Test-Case 'a run is one whole spool file: session start and end, the run, its stage timings' {
    $tree = Use-Tree @{ 'big.cs' = (New-Code 20); 'small.cs' = (New-Code 2) }
    $root = Join-Path $tree 'telemetry'
    $result = Invoke-TelemetryGate @{ STRUCTUREGATE_TELEMETRY = $root; STRUCTUREGATE_TELEMETRY_AGENT = 'off' } @('--root', $tree, '--max-lines', '5', '--no-gate-cache')
    Assert-Exit $result 1
    $spool = Read-Spool $root
    Assert-Equal $spool.Files.Count 1 'files in the spool after one run'
    Assert-Equal ($spool.Files[0].Name.EndsWith('.ndjson')) $true "the file is whole, not $($spool.Files[0].Name)"
    Assert-Equal ($spool.Files[0].Name.Split('-')[0].Length) 13 'the name starts with 13 digits of epoch ms'
    Assert-Equal (Get-SpoolRecord $spool 'session' 'start').Count 1 'session start'
    Assert-Equal "$((Get-SpoolRecord $spool 'session' 'start')[0].d.pid -gt 0)" 'True' 'session start carries the pid'
    $end = Get-SpoolRecord $spool 'session' 'end'
    Assert-Equal $end.Count 1 'session end'
    Assert-Equal "$($end[0].d.exit_code)" '1' 'the end carries the exit code'
    $run = Get-SpoolRecord $spool 'event' 'run'
    Assert-Equal "$($run[0].d.mode)" 'gate' 'the mode'
    Assert-Equal (@($spool.Records | Select-Object -ExpandProperty s -Unique)).Count 1 'every record is one session'
    Assert-Equal (@($spool.Records | Select-Object -ExpandProperty i -Unique)).Count 1 'every record is one install'
    # THE RELEASE TAG (`dev` in a build of the working tree), never the .NET assembly's 1.0.0.0 every release carries.
    Assert-Equal "$($spool.Records[0].v)" $(if ($env:STRUCTUREGATE_VERSION) { $env:STRUCTUREGATE_VERSION } else { 'dev' }) 'the version'
    $stats = @((Get-SpoolRecord $spool 'metric' 'run')[0].d.stats)
    $walk = @($stats | Where-Object { $_.name -eq 'stage.walk and count' })
    Assert-Equal $walk.Count 1 'the walk as a timing series'
    Assert-Equal "$($walk[0].kind)/$($walk[0].n)" 'timing/1' 'its kind and count'
    Assert-Equal (@($stats | Where-Object { $_.name -eq 'run' })).Count 1 'the run as a timing series'
    Assert-Equal (Get-SpoolRecord $spool 'crash' 'exit_2').Count 0 'an exit 1 is a verdict, not a crash'
    Assert-Equal (Test-Path (Join-Path $root 'install_id')) $true 'the install id is kept in the root'
}

Test-Case 'nothing of the tree leaves: no folder, file name, path or command line in the spool, a deep map included' {
    $tree = Use-Tree @{ 'zebracorn_orders.py' = "ZEBRACORN_LIMIT = 3`n`ndef zebracorn_total(x):`n    return x + ZEBRACORN_LIMIT`n" }
    $root = Join-Path ([System.IO.Path]::GetTempPath()) "sgtelemetry-$([System.Guid]::NewGuid().ToString('N').Substring(0,8))"
    Register-Tree $root
    $db = Join-Path $tree 'map.sqlite'
    $result = Invoke-TelemetryGate @{ STRUCTUREGATE_TELEMETRY = $root; STRUCTUREGATE_TELEMETRY_AGENT = 'off' } @('--root', $tree, '--ext', '.py', '--map-sqlite', $db)
    Assert-Exit $result 0
    $spool = Read-Spool $root
    Assert-Equal "$((Get-SpoolRecord $spool 'event' 'run')[0].d.mode)" 'deep' 'the deep map run was spooled'
    $text = (Get-Content -LiteralPath $spool.Files[0].FullName -Raw)
    $leaf = Split-Path $tree -Leaf
    foreach ($word in @($leaf, 'zebracorn', 'map.sqlite', '--root')) {
        Assert-Equal ($text.IndexOf($word, [System.StringComparison]::OrdinalIgnoreCase) -ge 0) $false "the spool carries '$word'"
    }
    # NO PATH AT ALL outside a crash's backtrace: the exe's own folder, the host's, a stamp - none is the run's to send.
    foreach ($record in $spool.Records | Where-Object { $_.k -ne 'crash' }) {
        $json = $record | ConvertTo-Json -Depth 8 -Compress
        Assert-Equal ($json.Contains('/') -or $json.Contains('\\')) $false "a path in a $($record.k)/$($record.n) record: $json"
    }
    $stats = @((Get-SpoolRecord $spool 'metric' 'run')[0].d.stats | ForEach-Object { $_.name })
    Assert-Equal ($stats -contains 'stage.deep: hash') $true "the tree's hash stage is counted under its prefix: $($stats -join ', ')"
}

Test-Case 'the install id is made once per root: a second run is the same install, in a second file' {
    $tree = Use-Tree @{ 'a.cs' = (New-Code 3) }
    $root = Join-Path $tree 'telemetry'
    $env = @{ STRUCTUREGATE_TELEMETRY = $root; STRUCTUREGATE_TELEMETRY_AGENT = 'off' }
    Assert-Exit (Invoke-TelemetryGate $env @('--root', $tree, '--no-gate-cache')) 0
    Assert-Exit (Invoke-TelemetryGate $env @('--root', $tree, '--no-gate-cache')) 0
    $spool = Read-Spool $root
    Assert-Equal $spool.Files.Count 2 'one file per run'
    Assert-Equal (@($spool.Records | Select-Object -ExpandProperty s -Unique)).Count 2 'two sessions'
    Assert-Equal (@($spool.Records | Select-Object -ExpandProperty i -Unique)).Count 1 'one install'
    Assert-Equal "$($spool.Records[0].i)" ((Get-Content -LiteralPath (Join-Path $root 'install_id') -Raw).Trim()) 'the id is the kept one'
}

Test-Case '1 is telemetry-agent''s own root under local app data' {
    $tree = Use-Tree @{ 'a.cs' = (New-Code 3) }
    $data = Join-Path $tree 'data'
    $result = Invoke-TelemetryGate @{ STRUCTUREGATE_TELEMETRY = '1'; XDG_DATA_HOME = $data; LOCALAPPDATA = $data; STRUCTUREGATE_TELEMETRY_AGENT = 'off' } @('--root', $tree, '--no-gate-cache')
    Assert-Exit $result 0
    Assert-Equal (Read-Spool (Join-Path $data 'structuregate/telemetry')).Files.Count 1 'files spooled under the default root'
}

Test-Case 'the agent is started with the root, after the file is in the spool' {
    $tree = Use-Tree @{ 'a.cs' = (New-Code 3) }
    $root = Join-Path $tree 'telemetry'
    if ($IsWindows -or $env:OS -eq 'Windows_NT') {
        $agent = Join-Path $tree 'agent.cmd'
        Set-Content -LiteralPath $agent -Value "@dir /b `"%TELEMETRY_ROOT%\spool\*.ndjson`" > `"%TELEMETRY_ROOT%\agent-saw`"" -Encoding ASCII
    } else {
        $agent = Join-Path $tree 'agent.sh'
        Set-Content -LiteralPath $agent -Value "#!/bin/sh`nls `"`$TELEMETRY_ROOT/spool`" > `"`$TELEMETRY_ROOT/agent-saw`"`n" -Encoding ASCII
        & chmod +x $agent
    }
    $result = Invoke-TelemetryGate @{ STRUCTUREGATE_TELEMETRY = $root; STRUCTUREGATE_TELEMETRY_AGENT = $agent } @('--root', $tree, '--no-gate-cache')
    Assert-Exit $result 0
    $saw = Join-Path $root 'agent-saw'
    $deadline = (Get-Date).AddSeconds(15)
    while (-not (Test-Path $saw) -and (Get-Date) -lt $deadline) { Start-Sleep -Milliseconds 100 }
    Assert-Equal (Test-Path $saw) $true 'the agent was not started in the root'
    Start-Sleep -Milliseconds 200
    $listed = @(Get-Content -LiteralPath $saw | Where-Object { $_.Trim() })
    Assert-Equal $listed.Count 1 "what the agent found in the spool: $($listed -join ', ')"
    Assert-Equal ($listed[0].Trim().EndsWith('.ndjson')) $true "the agent found $($listed[0])"
}

Test-Case 'a spool that cannot be written is a NOTE, and the run keeps its exit' {
    $tree = Use-Tree @{ 'big.cs' = (New-Code 20) }
    $root = Join-Path $tree 'not-a-folder'
    Set-Content -LiteralPath $root -Value 'a file where the root should be'
    $result = Invoke-TelemetryGate @{ STRUCTUREGATE_TELEMETRY = $root; STRUCTUREGATE_TELEMETRY_AGENT = 'off' } @('--root', $tree, '--max-lines', '5', '--no-gate-cache')
    Assert-Exit $result 1
    Assert-Line $result 'NOTE: telemetry was not written'
}
