<#
    THE TRACE: `STRUCTUREGATE_TRACE=<file>` appends one OTLP/JSON line per run - the run as the root span, each
    stage a child, the slowest single files spans of their own - and `--trace-report` reads the file back.

    What is held: nothing is written without the variable; a line is what an OpenTelemetry Collector's
    `otlpjsonfile` receiver reads (ids, parents, times as strings); a trace that cannot be written never changes
    the run's exit; and the report names the tree, the stage and the file.
#>

# The gate with the trace variable set for this one run, and the OTel ones a case asks for - restored after.
function Invoke-TracedGate([string]$Trace, [hashtable]$Environment = @{}, [object[]]$GateArgs) {
    $names = @('STRUCTUREGATE_TRACE', 'STRUCTUREGATE_TRACE_MAX_BYTES', 'OTEL_RESOURCE_ATTRIBUTES', 'OTEL_SERVICE_NAME')
    $saved = @{}
    foreach ($name in $names) { $saved[$name] = [Environment]::GetEnvironmentVariable($name) }
    try {
        foreach ($name in $names) { [Environment]::SetEnvironmentVariable($name, $null) }
        if ($Trace) { $env:STRUCTUREGATE_TRACE = $Trace }
        foreach ($name in $Environment.Keys) { [Environment]::SetEnvironmentVariable($name, $Environment[$name]) }
        return Invoke-Gate @GateArgs
    } finally {
        foreach ($name in $names) { [Environment]::SetEnvironmentVariable($name, $saved[$name]) }
    }
}

# Every run in a trace file, each as `{ Resource; Spans }`.
function Read-TraceRuns([string]$Path) {
    foreach ($line in [System.IO.File]::ReadAllLines($Path)) {
        if (-not $line.Trim()) { continue }
        $request = $line | ConvertFrom-Json
        $resource = $request.resourceSpans[0]
        [pscustomobject]@{ Resource = $resource.resource.attributes; Spans = @($resource.scopeSpans[0].spans) }
    }
}

# An attribute's value as text, whichever AnyValue it was written as.
function Get-TraceAttribute($Attributes, [string]$Key) {
    $found = @($Attributes) | Where-Object { $_.key -eq $Key } | Select-Object -First 1
    if (-not $found) { return $null }
    foreach ($kind in 'stringValue', 'intValue', 'boolValue') {
        if ($null -ne $found.value.$kind) { return "$($found.value.$kind)" }
    }
    return $null
}

Test-Case 'no STRUCTUREGATE_TRACE, no trace file' {
    $tree = Use-Tree @{ 'a.cs' = (New-Code 3) }
    $trace = Join-Path $tree 'trace.jsonl'
    $result = Invoke-TracedGate '' @{} @('--root', $tree, '--no-gate-cache')
    Assert-Exit $result 0
    Assert-Equal (-not (Test-Path $trace)) $true 'a run without the variable wrote a trace'
    Assert-NoLine $result 'trace'
}

Test-Case 'a gate run is one OTLP line: the run as the root span, its stages and files as children' {
    $tree = Use-Tree @{ 'big.cs' = (New-Code 20); 'small.cs' = (New-Code 2) }
    $trace = Join-Path $tree 'out/trace.jsonl'
    $result = Invoke-TracedGate $trace @{} @('--root', $tree, '--max-lines', '5', '--no-gate-cache')
    Assert-Exit $result 1
    $runs = @(Read-TraceRuns $trace)
    Assert-Equal $runs.Count 1 'lines after one run'
    $spans = $runs[0].Spans
    $root = @($spans | Where-Object { -not $_.parentSpanId })
    Assert-Equal $root.Count 1 'root spans'
    $root = $root[0]
    Assert-Equal $root.name 'structuregate' 'root span name'
    Assert-Equal $root.traceId.Length 32 'trace id length'
    Assert-Equal $root.spanId.Length 16 'span id length'
    Assert-Equal (Get-TraceAttribute $root.attributes 'structuregate.root') $tree 'the tree'
    Assert-Equal (Get-TraceAttribute $root.attributes 'structuregate.mode') 'gate' 'the mode'
    Assert-Equal (Get-TraceAttribute $root.attributes 'process.exit.code') '1' 'the exit code'
    Assert-Equal (Get-TraceAttribute $runs[0].Resource 'service.name') 'structuregate' 'service.name'
    # TIMES ARE STRINGS of nanoseconds, as the protobuf JSON mapping writes an int64, and a run takes time.
    Assert-Equal ($root.startTimeUnixNano -is [string]) $true 'startTimeUnixNano is a string'
    Assert-Equal ([decimal]$root.endTimeUnixNano -gt [decimal]$root.startTimeUnixNano) $true 'the run took no time'
    $walk = @($spans | Where-Object { $_.name -eq 'walk and count' })
    Assert-Equal $walk.Count 1 'walk and count spans'
    Assert-Equal $walk[0].parentSpanId $root.spanId 'the stage is a child of the run'
    Assert-Equal (Get-TraceAttribute $walk[0].attributes 'structuregate.files') '2' 'files the walk measured'
    # A STAGE THAT HAS ENDED IS NO LONGER A PARENT: the judge runs after the walk, beside it and not under it.
    $judge = @($spans | Where-Object { $_.name -eq 'judge' })
    Assert-Equal $judge.Count 1 'judge spans'
    Assert-Equal $judge[0].parentSpanId $root.spanId 'the judge is a child of the run'
    $files = @($spans | Where-Object { (Get-TraceAttribute $_.attributes 'code.filepath') -eq 'big.cs' })
    Assert-Equal $files.Count 1 'a span for the C# file Roslyn counted'
    Assert-Equal $files[0].parentSpanId $walk[0].spanId 'the file is a child of the stage that read it'
    Assert-Equal @($spans.spanId | Select-Object -Unique).Count $spans.Count 'span ids are unique'
}

Test-Case 'every run APPENDS its own line, under its own trace id' {
    $tree = Use-Tree @{ 'a.cs' = (New-Code 3) }
    $trace = Join-Path $tree 'trace.jsonl'
    Invoke-TracedGate $trace @{} @('--root', $tree, '--no-gate-cache') | Out-Null
    Invoke-TracedGate $trace @{} @('--root', $tree, '--no-gate-cache') | Out-Null
    $runs = @(Read-TraceRuns $trace)
    Assert-Equal $runs.Count 2 'lines after two runs'
    Assert-Equal ($runs[0].Spans[0].traceId -ne $runs[1].Spans[0].traceId) $true 'two runs shared a trace id'
}

Test-Case 'a trace past its limit rolls over into .1, keeping one file before it - and the report reads both' {
    # A MACHINE-WIDE TRACE (`Update-Gate.ps1 -Trace`) is written by every run of every consumer: never cut, it only grew.
    $tree = Use-Tree @{ 'a.cs' = (New-Code 3) }
    $trace = Join-Path $tree 'trace.jsonl'
    $small = @{ STRUCTUREGATE_TRACE_MAX_BYTES = '1' }
    foreach ($run in 1..3) { Invoke-TracedGate $trace $small @('--root', $tree, '--no-gate-cache') | Out-Null }
    Assert-Equal @(Read-TraceRuns $trace).Count 1 'the current file holds the last run'
    Assert-Equal @(Read-TraceRuns (Join-Path $tree 'trace.1.jsonl')).Count 1 'the rolled file holds the one before'
    Assert-Equal (@(Get-ChildItem $tree -Filter 'trace*.jsonl').Count) 2 'never more than two files'
    $report = Invoke-Gate --trace-report $trace
    Assert-Exit $report 0
    Assert-Line $report '2 run(s)'
}

Test-Case 'the map traces its walk, its halves and its write' {
    $tree = Use-Tree @{ 'a.cs' = "class A { }`n"; 'CLAUDE.md' = "# d`n" }
    $trace = Join-Path $tree 'trace.jsonl'
    $result = Invoke-TracedGate $trace @{} @('--root', $tree, '--map', '--ext', '.cs')
    Assert-Exit $result 0
    $spans = @(Read-TraceRuns $trace)[0].Spans
    $root = $spans | Where-Object { -not $_.parentSpanId }
    Assert-Equal (Get-TraceAttribute $root.attributes 'structuregate.mode') 'map' 'the mode'
    foreach ($name in 'walk', 'map: csharp', 'map: markdown', 'write the map') {
        Assert-Equal (@($spans | Where-Object { $_.name -eq $name }).Count -eq 1) $true "no '$name' span"
    }
}

Test-Case 'the deep map says per root how it hashed the tree: journal or walk, entries, files read' {
    $tree = Use-Tree @{ 'a.py' = "X = 1`n" }
    $trace = Join-Path $tree 'trace.jsonl'
    Invoke-TracedGate $trace @{} @('--root', $tree, '--ext', '.py', '--map-sqlite', (Join-Path $tree 'map.sqlite')) | Out-Null
    $spans = @((Read-TraceRuns $trace)[0].Spans)
    $hash = $spans | Where-Object { $_.name -eq ('deep: hash ' + (Split-Path $tree -Leaf)) }
    Assert-Equal ($null -ne $hash) $true 'a stage per root'
    Assert-Equal ((Get-TraceAttribute $hash.attributes 'structuregate.hash.method').Length -gt 0) $true 'how it was refreshed'
    Assert-Equal ((Get-TraceAttribute $hash.attributes 'structuregate.hash.entries') -ge 1) $true 'over how many entries'
}

Test-Case 'a deep map keeps its LAST run''s trace beside its database, asked for or not, replaced by each run' {
    $tree = Use-Tree @{ 'a.py' = "X = 1`n" }
    $db = Join-Path $tree 'map.sqlite'
    $last = "$db.last-run.jsonl"
    foreach ($run in 1..2) { Assert-Exit (Invoke-TracedGate '' @{} @('--root', $tree, '--ext', '.py', '--map-sqlite', $db)) 0 }
    $runs = @(Read-TraceRuns $last)
    Assert-Equal $runs.Count 1 'runs in the last-run trace after two runs'
    $root = $runs[0].Spans | Where-Object { -not $_.parentSpanId }
    Assert-Equal (Get-TraceAttribute $root.attributes 'structuregate.mode') 'deep' 'the mode'
    # A TRACE ASKED FOR STILL GETS ITS LINE, and the last run's file is written beside it.
    $asked = Join-Path $tree 'asked.jsonl'
    Assert-Exit (Invoke-TracedGate $asked @{} @('--root', $tree, '--ext', '.py', '--map-sqlite', $db)) 0
    Assert-Equal @(Read-TraceRuns $asked).Count 1 'runs in the trace asked for'
    Assert-Equal @(Read-TraceRuns $last).Count 1 'runs in the last-run trace'
    # A QUERY READS THE DATABASE; it is no run of the map and leaves the last run's trace alone.
    $before = [System.IO.File]::ReadAllText($last)
    Assert-Exit (Invoke-TracedGate '' @{} @('--map-query', $db, '--sql', 'SELECT 1')) 0
    Assert-Equal ([System.IO.File]::ReadAllText($last) -ceq $before) $true 'a query replaced the last run''s trace'
}

Test-Case 'the deep C# half says per project where its compile time went: references, sources, razor, declarations' {
    $tree = Use-Tree @{
        'lib/Lib.csproj' = '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework></PropertyGroup></Project>'
        'lib/Lib.cs'     = 'namespace Lib; public class Engine { public int Run() { return 1; } }'
    }
    $trace = Join-Path $tree 'trace.jsonl'
    Invoke-TracedGate $trace @{} @('--root', $tree, '--ext', '.cs', '--map-sqlite', (Join-Path $tree 'map.sqlite')) | Out-Null
    $spans = @((Read-TraceRuns $trace)[0].Spans)
    foreach ($phase in 'load references', 'parse sources', 'razor', 'declarations') {
        Assert-Equal (@($spans | Where-Object { $_.name -eq "csharp: $phase" }).Count) 1 "the $phase total"
    }
    $project = $spans | Where-Object { $_.name -eq 'csharp project: lib/Lib.csproj' }
    Assert-Equal ($null -ne $project) $true 'a span for the project'
    Assert-Equal (Get-TraceAttribute $project.attributes 'structuregate.files') '1' 'with the files it parsed'
}

Test-Case '--trace-detail puts every step of the C# half in a span, and the report names what no child covers' {
    $tree = Use-Tree @{
        'lib/Lib.csproj' = '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework></PropertyGroup></Project>'
        'lib/Lib.cs'     = 'namespace Lib; public class Engine { public int Run() { return 1; } }'
    }
    $trace = Join-Path $tree 'detail.jsonl'
    # THE FLAGS, NOT THE VARIABLE: --trace names the file.
    $run = Invoke-TracedGate '' @{} @('--root', $tree, '--ext', '.cs', '--map-sqlite', (Join-Path $tree 'map.sqlite'), '--trace', $trace, '--trace-detail')
    Assert-Exit $run 0
    $spans = @((Read-TraceRuns $trace)[0].Spans)
    foreach ($name in 'csharp: hash the files and projects', 'csharp: open the session', 'csharp: batches', 'csharp batch', 'pass: sql links', 'pass: count the tables') {
        Assert-Equal (@($spans | Where-Object { $_.name -eq $name }).Count -ge 1) $true "a span '$name'"
    }
    $batch = $spans | Where-Object { $_.name -eq 'csharp batch' } | Select-Object -First 1
    $inBatch = @($spans | Where-Object { $_.parentSpanId -eq $batch.spanId } | ForEach-Object { $_.name })
    Assert-Equal ($inBatch -contains 'csharp: read the sources') $true "the batch's own reads: $($inBatch -join ', ')"
    $total = $spans | Where-Object { $_.name -eq 'csharp: compile' -and (Get-TraceAttribute $_.attributes 'structuregate.timing') -eq 'breakdown' }
    Assert-Equal ($null -ne $total) $true 'the compile total only explains the batches'
    $report = Invoke-Gate --trace-report $trace
    Assert-Exit $report 0
    Assert-Line $report 'deep: csharp (untraced)'
    Assert-Line $report 'as a tree (total, and what no child covers)'
    # THE ROWS' TOTAL IS ITS OWN LINE: added to the batches' `csharp: rows`, a large consumer's table read about twice the real time.
    Assert-Line $report 'csharp: rows (breakdown)'
    Assert-Exit (Invoke-TracedGate '' @{} @('--root', $tree, '--trace-detail')) 2
}

Test-Case 'an assembly copied into a second project''s bin is opened once, not once per copy' {
    # A HOST OR TEST PROJECT'S BUILD COPIES EVERY PACKAGE INTO ITS OWN bin, and that copy is the reference it takes.
    $project = '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework></PropertyGroup></Project>'
    $tree = Use-Tree @{
        'a/A.csproj' = $project
        'a/A.cs'     = 'namespace A; public class One { }'
        'b/B.csproj' = $project
        'b/B.cs'     = 'namespace B; public class Two { }'
    }
    $assembly = [psobject].Assembly.Location
    foreach ($name in 'a', 'b') {
        $bin = Join-Path $tree "$name/bin/Debug/net10.0"
        New-Item -ItemType Directory -Path $bin -Force | Out-Null
        # A COPY, with the time a build's copy keeps.
        Copy-Item -LiteralPath $assembly -Destination $bin
        (Get-Item (Join-Path $bin (Split-Path $assembly -Leaf))).LastWriteTimeUtc = (Get-Item $assembly).LastWriteTimeUtc
    }
    $trace = Join-Path $tree 'trace.jsonl'
    Invoke-TracedGate $trace @{} @('--root', $tree, '--ext', '.cs', '--map-sqlite', (Join-Path $tree 'map.sqlite')) | Out-Null
    $spans = @((Read-TraceRuns $trace)[0].Spans)
    $first = $spans | Where-Object { $_.name -eq 'csharp project: a/A.csproj' }
    $second = $spans | Where-Object { $_.name -eq 'csharp project: b/B.csproj' }
    Assert-Equal ((Get-TraceAttribute $first.attributes 'structuregate.csharp.assemblies_opened') -ge 1) $true 'the first project opens its references'
    Assert-Equal (Get-TraceAttribute $second.attributes 'structuregate.csharp.assemblies_opened') '0' 'the second opens none of its own'
    Assert-Equal ((Get-TraceAttribute $second.attributes 'structuregate.csharp.assemblies_shared') -ge 1) $true 'it shares them'
}

Test-Case 'OTEL_RESOURCE_ATTRIBUTES and OTEL_SERVICE_NAME name the run as the OpenTelemetry SDKs read them' {
    $tree = Use-Tree @{ 'a.cs' = (New-Code 3) }
    $trace = Join-Path $tree 'trace.jsonl'
    $otel = @{ OTEL_RESOURCE_ATTRIBUTES = 'team=build, deployment.environment=ci'; OTEL_SERVICE_NAME = 'gate-ci' }
    Invoke-TracedGate $trace $otel @('--root', $tree, '--no-gate-cache') | Out-Null
    $resource = @(Read-TraceRuns $trace)[0].Resource
    Assert-Equal (Get-TraceAttribute $resource 'team') 'build' 'team'
    Assert-Equal (Get-TraceAttribute $resource 'deployment.environment') 'ci' 'deployment.environment'
    Assert-Equal (Get-TraceAttribute $resource 'service.name') 'gate-ci' 'service.name'
    Assert-Equal @($resource | Where-Object { $_.key -eq 'service.name' }).Count 1 'service.name written once'
}

Test-Case 'a trace that cannot be written is a note - the run keeps its own exit' {
    $tree = Use-Tree @{ 'big.cs' = (New-Code 20) }
    # A FOLDER where the file should be: nothing can append to it.
    $trace = Join-Path $tree 'taken'
    New-Item -ItemType Directory -Path $trace | Out-Null
    $result = Invoke-TracedGate $trace @{} @('--root', $tree, '--max-lines', '5', '--no-gate-cache')
    Assert-Exit $result 1
    Assert-Line $result 'NOTE: the trace was not written to'
    Assert-Line $result 'big.cs'
}

Test-Case '--trace-report names each tree, its stages and the slowest files - and passes over a torn line' {
    $tree = Use-Tree @{ 'big.cs' = (New-Code 20) }
    $trace = Join-Path $tree 'trace.jsonl'
    Invoke-TracedGate $trace @{} @('--root', $tree, '--no-gate-cache') | Out-Null
    Invoke-TracedGate $trace @{} @('--root', $tree, '--no-gate-cache') | Out-Null
    [System.IO.File]::AppendAllText($trace, "{`"resourceSp`n")
    $report = Invoke-Gate --trace-report $trace
    Assert-Exit $report 0
    Assert-Line $report '2 run(s) in'
    Assert-Line $report '1 line(s) that are not a run passed over'
    Assert-Line $report $tree
    Assert-Line $report 'whole run (gate)'
    Assert-Line $report 'walk and count'
    Assert-Line $report 'slowest files'
    Assert-Line $report 'big.cs'
}

Test-Case '--trace-report over a missing file exits 2 and says which' {
    $tree = Use-Tree @{ 'a.cs' = (New-Code 3) }
    $report = Invoke-Gate --trace-report (Join-Path $tree 'none.jsonl')
    Assert-Exit $report 2
    Assert-Line $report 'none.jsonl'
}

# THE TARGETS EVERY CONSUMER IMPORTS, parsed as MSBuild parses them: this repo's own build imports the DEPLOYED
# copy in buildtools/, so a comment holding `--` here broke only the release, never a build in this repo.
Test-Case 'StructureGate.targets is XML, and passes StructureGateTrace to the gate only when it is set' {
    $targets = [xml][System.IO.File]::ReadAllText((Join-Path $script:Root 'StructureGate.targets'))
    $env = @($targets.Project.PropertyGroup._StructureGateEnv)[0]
    Assert-Equal $env.Condition "'`$(StructureGateTrace)' != ''" 'the condition on the trace variable'
    $exec = @($targets.Project.Target | Where-Object { $_.Name -eq 'StructureGateCheck' })[0].Exec
    Assert-Equal $exec.EnvironmentVariables '$(_StructureGateEnv)' 'the Exec environment'
}
