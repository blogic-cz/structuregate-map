<#
    THE FILE MAP RE-PARSES ONLY WHAT MOVED (`rust/fbtcore/src/mapper/kept.rs`). A Stop hook after a one-line edit
    re-parsed every file: 5.1 s of a 6.5 s turn on this repo was PowerShell reading 75 scripts to learn one answer.

    WHAT IS HELD: an incremental map is BYTE-IDENTICAL to the one a full parse writes, a half is handed only the files
    whose content moved, and what an answer depends on beyond its file - the file set, a TypeScript project file -
    asks every file again. How many files a half parsed is read off the run's trace (`structuregate.parsed`).

    Its helpers are its own - `-Only MapKept` runs this suite alone.
#>

# The map over $Tree, traced; the `map: <half>` stages as { half -> parsed }.
function Invoke-MapKept([string]$Tree, [string]$Ext) {
    $trace = Join-Path $Tree "trace-$([guid]::NewGuid().ToString('N').Substring(0, 6)).jsonl"
    $saved = $env:STRUCTUREGATE_TRACE
    try {
        $env:STRUCTUREGATE_TRACE = $trace
        $result = Invoke-Gate --root $Tree --ext $Ext --map --map-check --map-out (Join-Path $Tree 'buildmap.json')
    } finally {
        if ($null -eq $saved) { Remove-Item Env:STRUCTUREGATE_TRACE -ErrorAction SilentlyContinue } else { $env:STRUCTUREGATE_TRACE = $saved }
    }
    $parsed = @{}
    $request = [System.IO.File]::ReadAllText($trace) | ConvertFrom-Json
    Remove-Item $trace
    foreach ($span in $request.resourceSpans[0].scopeSpans[0].spans) {
        if (-not $span.name.StartsWith('map: ')) { continue }
        $value = ($span.attributes | Where-Object { $_.key -eq 'structuregate.parsed' }).value
        if ($value) { $parsed[$span.name.Substring(5)] = [int]$value.intValue }
    }
    return [pscustomobject]@{ Result = $result; Parsed = $parsed; Map = [System.IO.File]::ReadAllText((Join-Path $Tree 'buildmap.json')) }
}

function New-MapKeptTree {
    return Use-Tree @{
        'lib/One.ps1'  = "function Get-One { 1 }`n"
        'lib/Two.ps1'  = "function Get-Two { Get-One }`n"
        'run.ps1'      = ". `$PSScriptRoot/lib/One.ps1`n. `$PSScriptRoot/lib/Two.ps1`nGet-Two`n"
    }
}

Test-Case 'map: an edit re-parses that file alone, and the map is byte-identical to a full parse' {
    $tree = New-MapKeptTree
    $first = Invoke-MapKept $tree '.ps1'
    Assert-Exit $first.Result 0
    Assert-Equal $first.Parsed['powershell'] 3 'parsed on the first run'
    $quiet = Invoke-MapKept $tree '.ps1'
    Assert-Equal $quiet.Parsed['powershell'] 0 'parsed with nothing moved - the host is not started'
    # THE FILE THAT DOT-SOURCES THE OTHERS: parsed alone, its edges still need the files it was not handed.
    [System.IO.File]::AppendAllText((Join-Path $tree 'run.ps1'), "function Get-Three { Get-Two }`n")
    $edited = Invoke-MapKept $tree '.ps1'
    Assert-Equal $edited.Parsed['powershell'] 1 'parsed after one edit'
    Assert-Equal ($edited.Map.Contains('Get-Three')) $true 'the edit is in the map'
    # THE SAME MAP A FULL PARSE WRITES: the cache gone, everything read again.
    Remove-Item -Recurse -Force (Join-Path $tree '.fbt')
    $full = Invoke-MapKept $tree '.ps1'
    Assert-Equal $full.Parsed['powershell'] 3 'parsed with the cache gone'
    Assert-Equal ($edited.Map -ceq $full.Map) $true 'the incremental map equals the full one'
}

Test-Case 'map: a file added or gone asks every file again - an answer resolves against the file set' {
    $tree = New-MapKeptTree
    Invoke-MapKept $tree '.ps1' | Out-Null
    [System.IO.File]::WriteAllText((Join-Path $tree 'lib/Four.ps1'), "function Get-Four { 4 }`n")
    $added = Invoke-MapKept $tree '.ps1'
    Assert-Equal $added.Parsed['powershell'] 4 'parsed after a file was added'
}

$script:MapKeptTs = Get-TsModules
if ($script:MapKeptTs) {
    Test-Case 'map: a TypeScript project file that moved asks every TypeScript file again' {
        $tree = Use-Tree @{
            'tsconfig.json' = '{"compilerOptions":{"strict":true}}'
            'src/a.ts'      = "export const A = 1;`n"
            'src/b.ts'      = "import { A } from './a';`nexport const B = A;`n"
        }
        $previous = $env:NODE_PATH
        $env:NODE_PATH = $script:MapKeptTs
        try {
            Assert-Equal (Invoke-MapKept $tree '.ts').Parsed['typescript'] 2 'parsed on the first run'
            Assert-Equal (Invoke-MapKept $tree '.ts').Parsed['typescript'] 0 'parsed with nothing moved'
            [System.IO.File]::WriteAllText((Join-Path $tree 'tsconfig.json'), '{"compilerOptions":{"strict":true,"baseUrl":"."}}')
            Assert-Equal (Invoke-MapKept $tree '.ts').Parsed['typescript'] 2 'parsed after tsconfig.json moved'
        } finally { $env:NODE_PATH = $previous }
    }
}

# PYTHON IS KEPT PER FILE, AND READ AGAIN WITH WHAT IMPORTS IT: its answer about a file reads the modules that file
# imports, so an edit re-reads the file and every file importing it through any chain (by the last map's edges) - and
# a launcher (`pyproject.toml`) moving re-reads everything. Its entry points are the RUN's, kept apart from the files'.
if ($script:Python) {
    Test-Case 'map: python re-reads the edited module and what imports it, and keeps a launcher entry point' {
        $tree = Use-Tree @{
            'pkg/__init__.py' = "`n"
            'pkg/leaf.py'     = "VALUE = 1`n"
            'app.py'          = "from pkg import leaf`n`ndef main():`n    return leaf.VALUE`n"
            'alone.py'        = "Y = 2`n"
            'run.ps1'         = "function Get-One { 1 }`n"
            'pyproject.toml'  = "[project]`nname = 'demo'`n[project.scripts]`ndemo = 'app:main'`n"
        }
        Assert-Equal (Invoke-MapKept $tree '.py,.ps1').Parsed['python'] 4 'parsed on the first run'
        [System.IO.File]::AppendAllText((Join-Path $tree 'run.ps1'), "function Get-Two { 2 }`n")
        $other = Invoke-MapKept $tree '.py,.ps1'
        Assert-Equal $other.Parsed['python'] 0 'parsed after a .ps1 edit'
        # THE LAUNCHER'S ENTRY SURVIVES A RUN THAT PARSED NOTHING: `app.py` is entered through `demo`.
        $map = $other.Map | ConvertFrom-Json
        Assert-Equal ([bool]$map.files.'app.py'.entry) $true 'the launcher entry point after a replay'
        [System.IO.File]::AppendAllText((Join-Path $tree 'pkg/leaf.py'), "OTHER = 2`n")
        Assert-Equal (Invoke-MapKept $tree '.py,.ps1').Parsed['python'] 2 'parsed after editing a module app.py imports'
        [System.IO.File]::AppendAllText((Join-Path $tree 'alone.py'), "Z = 3`n")
        Assert-Equal (Invoke-MapKept $tree '.py,.ps1').Parsed['python'] 1 'parsed after editing a module nothing imports'
        [System.IO.File]::AppendAllText((Join-Path $tree 'pyproject.toml'), "# moved`n")
        Assert-Equal (Invoke-MapKept $tree '.py,.ps1').Parsed['python'] 4 'parsed after pyproject.toml moved'
        [System.IO.File]::AppendAllText((Join-Path $tree 'pkg/leaf.py'), "THIRD = 3`n")
        $edited = Invoke-MapKept $tree '.py,.ps1'
        Remove-Item -Recurse -Force (Join-Path $tree '.fbt')
        $full = Invoke-MapKept $tree '.py,.ps1'
        Assert-Equal ($edited.Map -ceq $full.Map) $true 'the incremental map equals a full parse'
    }
}

# THE DEEP TYPESCRIPT HALVES WITH NO WORKSPACE are keyed by what they read - the script files, the project files,
# the file set - never the whole tree: keyed by the tree, a `.py` edit started node twice to hear "unchanged".
$script:MapKeptTs5 = Get-TypeScriptPath 'typescript@5'
if ($script:Python -and $script:MapKeptTs5) {
    Test-Case 'deep: a .py edit does not start the TypeScript halves of a tree with no workspace; a .ts edit does' {
        $tree = Use-Tree @{ 'src/a.ts' = "export const A = 1;`n"; 'tool.py' = "X = 1`n" }
        $spans = {
            $trace = Join-Path $tree "deep-$([guid]::NewGuid().ToString('N').Substring(0, 6)).jsonl"
            $saved = $env:STRUCTUREGATE_TRACE
            try {
                $env:STRUCTUREGATE_TRACE = $trace
                $run = Invoke-Gate --root $tree --ext '.ts,.py' --map-sqlite (Join-Path $tree 'm.sqlite') --ts-node-modules $script:MapKeptTs5
                Assert-Exit $run 0
            } finally {
                if ($null -eq $saved) { Remove-Item Env:STRUCTUREGATE_TRACE -ErrorAction SilentlyContinue } else { $env:STRUCTUREGATE_TRACE = $saved }
            }
            $names = @(([System.IO.File]::ReadAllText($trace) | ConvertFrom-Json).resourceSpans[0].scopeSpans[0].spans.name)
            Remove-Item $trace
            return , $names
        }
        & $spans | Out-Null
        & $spans | Out-Null
        [System.IO.File]::AppendAllText((Join-Path $tree 'tool.py'), "Y = 2`n")
        $python = & $spans
        Assert-Equal ($python -contains 'map: plain-ts rows') $false 'the plain TypeScript half started for a .py edit'
        Assert-Equal ($python -contains 'typescript: plan (node)') $false 'the Angular half started for a .py edit'
        [System.IO.File]::AppendAllText((Join-Path $tree 'src/a.ts'), "export const B = 2;`n")
        Assert-Equal ((& $spans) -contains 'map: plain-ts rows') $true 'the plain TypeScript half for a .ts edit'
    }
}

# A TYPESCRIPT HALF WITH NO COMPILER fails the same way every turn, so its failure is kept too - until a compiler is
# installed, which the key sees through `node_modules/typescript/package.json` (the tree map skips `node_modules`).
if ($script:MapKeptTs) {
    Test-Case 'map: a TypeScript run with no compiler is replayed, and run again once one is installed' {
        $tree = Use-Tree @{ 'src/a.ts' = "export const A = 1;`n" }
        $previous = $env:NODE_PATH
        Remove-Item Env:NODE_PATH -ErrorAction SilentlyContinue
        try {
            $first = Invoke-MapKept $tree '.ts'
            Assert-Equal $first.Parsed['typescript'] 1 'parsed with no compiler'
            Assert-Equal ($first.Map.Contains('could not be resolved')) $true 'the file is UNMAPPED, with why'
            $again = Invoke-MapKept $tree '.ts'
            Assert-Equal $again.Parsed['typescript'] 0 'parsed again with nothing moved'
            Assert-Equal ($again.Map -ceq $first.Map) $true 'the replayed map equals the one the failed run wrote'
            $installed = Join-Path $tree 'node_modules'
            New-Item -ItemType Directory -Path $installed | Out-Null
            $kind = if ($script:OnWindows) { 'Junction' } else { 'SymbolicLink' }
            New-Item -ItemType $kind -Path (Join-Path $installed 'typescript') -Target (Join-Path $script:MapKeptTs 'typescript') | Out-Null
            $at = Join-Path $script:MapKeptTs '@typescript'
            if (Test-Path $at) { New-Item -ItemType $kind -Path (Join-Path $installed '@typescript') -Target $at | Out-Null }
            $fixed = Invoke-MapKept $tree '.ts'
            Assert-Equal $fixed.Parsed['typescript'] 1 'parsed once a compiler was installed'
            Assert-Equal ($fixed.Map.Contains('could not be resolved')) $false 'still UNMAPPED with a compiler installed'
        } finally { if ($null -ne $previous) { $env:NODE_PATH = $previous } }
    }
}

# RUST IS PARSED IN PROCESS, and was re-parsed whole on every turn (127 ms of each in this repo): its answer about a
# file is the file and the file set, so it is kept per file like the script halves.
Test-Case 'map: rust re-parses only the .rs that moved, and the map equals a full parse' {
    $tree = Use-Tree @{ 'src/lib.rs' = "mod util;`npub fn go() -> u32 { util::one() }`n"; 'src/util.rs' = "pub fn one() -> u32 { 1 }`n" }
    Assert-Equal (Invoke-MapKept $tree '.rs').Parsed['rust'] 2 'parsed on the first run'
    Assert-Equal (Invoke-MapKept $tree '.rs').Parsed['rust'] 0 'parsed with nothing moved'
    [System.IO.File]::AppendAllText((Join-Path $tree 'src/util.rs'), "pub fn two() -> u32 { 2 }`n")
    $edited = Invoke-MapKept $tree '.rs'
    Assert-Equal $edited.Parsed['rust'] 1 'parsed after one edit'
    Remove-Item -Recurse -Force (Join-Path $tree '.fbt')
    $full = Invoke-MapKept $tree '.rs'
    Assert-Equal ($edited.Map -ceq $full.Map) $true 'the incremental map equals the full one'
}

# THE FIRST TURN AFTER A COLD RUN: the deep TypeScript half records its "nothing to do" key before any store has
# written, and a database with no `_meta` (and then a rebuild of it) dropped that key - so the turn right after every
# cold run started node again. No warm-up run here: a second one would record the key and hide exactly this.
if ($script:Python -and $script:MapKeptTs5) {
    Test-Case 'deep: the turn right after a cold run does not start the TypeScript halves for a .py edit' {
        $tree = Use-Tree @{ 'src/a.ts' = "export const A = 1;`n"; 'tool.py' = "X = 1`n" }
        $db = Join-Path $tree 'm.sqlite'
        Assert-Exit (Invoke-Gate --root $tree --ext '.ts,.py' --map-sqlite $db --ts-node-modules $script:MapKeptTs5) 0
        [System.IO.File]::AppendAllText((Join-Path $tree 'tool.py'), "Y = 2`n")
        $trace = Join-Path $tree 'turn.jsonl'
        $saved = $env:STRUCTUREGATE_TRACE
        try {
            $env:STRUCTUREGATE_TRACE = $trace
            Assert-Exit (Invoke-Gate --root $tree --ext '.ts,.py' --map-sqlite $db --ts-node-modules $script:MapKeptTs5) 0
        } finally {
            if ($null -eq $saved) { Remove-Item Env:STRUCTUREGATE_TRACE -ErrorAction SilentlyContinue } else { $env:STRUCTUREGATE_TRACE = $saved }
        }
        $names = @(([System.IO.File]::ReadAllText($trace) | ConvertFrom-Json).resourceSpans[0].scopeSpans[0].spans.name)
        Assert-Equal ($names -contains 'typescript: plan (node)') $false 'the Angular half started on the first turn'
        Assert-Equal ($names -contains 'map: plain-ts rows') $false 'the plain TypeScript half started on the first turn'
    }
}
