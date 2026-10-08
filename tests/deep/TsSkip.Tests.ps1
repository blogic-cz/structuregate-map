<#
    The gate's `--skip` reaches the TYPESCRIPT half. It walked the workspace skipping only `node_modules`,
    so on a large Angular workspace the `.angular` build cache, `dist` and `.nx` were most of its "source" files.

    Its helpers are its own - `-Only` runs this suite alone. The compiler cache is the one TsRows fills.
#>

function Get-TsSkipModules {
    $modules = Join-Path ([System.IO.Path]::GetTempPath()) 'sgtest-tsrows\node_modules'
    if (-not (Get-Command node -ErrorAction SilentlyContinue)) { return $null }
    if ((Test-Path (Join-Path $modules 'typescript\package.json')) -and
        (Test-Path (Join-Path $modules '@angular\compiler\package.json'))) { return $modules }
    if (-not (Get-Command npm -ErrorAction SilentlyContinue)) { return $null }
    $cache = Split-Path $modules
    [void](New-Item -ItemType Directory -Path $cache -Force)
    & npm install --no-save --silent --prefix $cache 'typescript@5' '@angular/compiler@17' '@angular/core@17' '@angular/router@17' '@ngrx/store@17' 2>&1 | Out-Null
    if (Test-Path (Join-Path $modules '@angular\compiler\package.json')) { return $modules }
    return $null
}

$script:TsSkipModules = Get-TsSkipModules
if (-not $script:TsSkipModules) { Write-Host '    (no node/npm-installed typescript + @angular/compiler - the typescript skip cases are not run)' }

function Invoke-TsSkipMap([string]$Tree, [string]$Skip) {
    $db = Join-Path $Tree 'map.sqlite'
    $run = Invoke-Gate --root $Tree --ext .ts --skip $Skip --map-sqlite $db --ts-node-modules $script:TsSkipModules
    Assert-Exit $run 0
    $r = Invoke-Gate --map-query $db --width 0 --sql "SELECT 'path=' || path AS p FROM files WHERE lang = 'typescript' ORDER BY path"
    Assert-Exit $r 0
    return [pscustomobject]@{ Run = $run; Files = $r }
}

if ($script:TsSkipModules) {

Test-Case 'tsskip: a folder the gate skips is not walked by the typescript half, and a re-run reads nothing' {
    $tree = Use-Tree @{
        'angular.json' = '{"projects":{"shop":{"projectType":"application","root":"apps/shop","sourceRoot":"apps/shop/src",' +
            '"architect":{"build":{"options":{"tsConfig":"apps/shop/tsconfig.app.json"}}}}}}'
        'apps/shop/tsconfig.app.json' = '{"compilerOptions":{"strict":true},"include":["src/**/*.ts"]}'
        'apps/shop/src/main.ts'       = "export const started = true;`n"
        '.angular/cache/stale.ts'     = "export const cached = 1;`n"
        'dist/shop/main.js'           = "var built = 1;`n"
    }
    $first = Invoke-TsSkipMap $tree 'node_modules,dist,.angular'
    Assert-Line $first.Files 'path=apps/shop/src/main.ts'
    Assert-NoLine $first.Files 'path=.angular/'
    Assert-NoLine $first.Files 'path=dist/'
    # THE HASH WALK SKIPS THE SAME FOLDERS: with the cache in one walk and not the other, every run would
    # read the tree as changed and rebuild the whole half.
    $again = Invoke-TsSkipMap $tree 'node_modules,dist,.angular'
    Assert-Line $again.Run 'the typescript half had nothing to do'
}


function New-TsSkipWorkspace {
    return Use-Tree @{
        'angular.json' = '{"projects":{"shop":{"projectType":"application","root":"apps/shop","sourceRoot":"apps/shop/src",' +
            '"architect":{"build":{"options":{"tsConfig":"apps/shop/tsconfig.app.json"}}}}}}'
        'apps/shop/tsconfig.app.json' = '{"compilerOptions":{"strict":true},"include":["src/**/*.ts"]}'
        'apps/shop/src/main.ts'       = "export const started = true;`n"
    }
}

function Invoke-TsSkipDeep([string]$Tree, [string[]]$Extra = @()) {
    $run = Invoke-Gate --root $Tree --ext .ts --map-sqlite (Join-Path $Tree 'map.sqlite') --ts-node-modules $script:TsSkipModules @Extra
    Assert-Exit $run 0
    return $run
}

$script:TsSkipQuiet = 'no file under its roots moved'

Test-Case 'tsskip: a tree where nothing moved does not start node at all' {
    $tree = New-TsSkipWorkspace
    Invoke-TsSkipDeep $tree | Out-Null
    Assert-Line (Invoke-TsSkipDeep $tree) $script:TsSkipQuiet
}

Test-Case 'tsskip: --map-reread typescript reads every file of a tree where nothing moved, and the next run is quiet' {
    $tree = New-TsSkipWorkspace
    Invoke-TsSkipDeep $tree | Out-Null
    $forced = Invoke-TsSkipDeep $tree @('--map-reread', 'typescript')
    Assert-Line $forced 're-reads every file: --map-reread typescript'
    Assert-NoLine $forced $script:TsSkipQuiet
    Assert-Line $forced 'its setup moved'
    # THE FORCED RUN'S TRACE, kept beside the database, names the closure's passes and the steps of its write.
    $report = Invoke-Gate --trace-report ((Join-Path $tree 'map.sqlite') + '.last-run.jsonl')
    Assert-Exit $report 0
    foreach ($name in 'closure: branches', 'branches: resolve the conditions', 'gate_values: props', 'write the rows: insert', 'write the rows: delete the old rows') {
        Assert-Line $report $name
    }
    Assert-Line (Invoke-TsSkipDeep $tree) $script:TsSkipQuiet
}

Test-Case 'tsskip: a file EDITED after a skipped run is parsed - the file list alone did not change' {
    $tree = New-TsSkipWorkspace
    Invoke-TsSkipDeep $tree | Out-Null
    Invoke-TsSkipDeep $tree | Out-Null
    [System.IO.File]::WriteAllText((Join-Path $tree 'apps/shop/src/main.ts'), "export const started = false;`nexport const more = 1;`n")
    Assert-NoLine (Invoke-TsSkipDeep $tree) $script:TsSkipQuiet
}

Test-Case 'tsskip: a run that starts node says why, and the last run''s trace beside the database is no change' {
    $tree = New-TsSkipWorkspace
    Invoke-TsSkipDeep $tree | Out-Null
    # ANOTHER --skip LAUNCHES NODE over a tree that did not move: its own walk must pass over the trace the last run
    # wrote beside the database, or the file reads as new and the half reads everything.
    Assert-Line (Invoke-TsSkipDeep $tree @('--skip', 'node_modules,nothing-here')) 'the typescript half had nothing to do: the tree has not moved'
    [System.IO.File]::WriteAllText((Join-Path $tree 'apps/shop/src/main.ts'), "export const started = false;`n")
    $edited = Invoke-TsSkipDeep $tree
    Assert-Line $edited 'the typescript half reads '
    Assert-Line $edited ': 1 file(s) moved since its rows were written (.ts 1)'
    $r = Invoke-Gate --map-query (Join-Path $tree 'map.sqlite') --width 0 --sql "SELECT 'v=' || value FROM _meta WHERE key = 'last_refresh'"
    $kept = ($r.Lines | Where-Object { $_.TrimStart().StartsWith('v=') } | Select-Object -First 1).Trim().Substring(2) | ConvertFrom-Json
    Assert-Equal $kept.typescript.changed 1 'files the typescript half saw move'
    Assert-Equal ($null -ne $kept.steps_ms.typescript) $true 'the typescript half is timed'
}

Test-Case 'tsskip: an edited config OUTSIDE the tree runs the half again - the tree hash cannot see it' {
    $tree = New-TsSkipWorkspace
    $outside = Use-Tree @{ 'structuregate.ts.json' = '{"locales":[]}' }
    $config = @('--ts-config', (Join-Path $outside 'structuregate.ts.json'))
    Invoke-TsSkipDeep $tree $config | Out-Null
    Assert-Line (Invoke-TsSkipDeep $tree $config) $script:TsSkipQuiet
    [System.IO.File]::WriteAllText((Join-Path $outside 'structuregate.ts.json'), '{"locales":[],"i18nCarriers":["translate"]}')
    Assert-NoLine (Invoke-TsSkipDeep $tree $config) $script:TsSkipQuiet
}

Test-Case 'tsskip: an edit BESIDE the workspace does not start node, and one inside it still does' {
    # KEYED BY THE WORKSPACE THE LAST RUN MAPPED (`web`), not by every root: a C# edit next to it launched node
    # only for its plan to say nothing had moved.
    $tree = Use-Tree @{
        'web/angular.json' = '{"projects":{"shop":{"projectType":"application","root":"apps/shop","sourceRoot":"apps/shop/src",' +
            '"architect":{"build":{"options":{"tsConfig":"apps/shop/tsconfig.app.json"}}}}}}'
        'web/apps/shop/tsconfig.app.json' = '{"compilerOptions":{"strict":true},"include":["src/**/*.ts"]}'
        'web/apps/shop/src/main.ts'       = "export const started = true;`n"
        'svc/Program.cs'                  = "namespace Demo;`npublic class Program {}`n"
    }
    $mixed = @('--ext', '.ts,.cs')
    Invoke-TsSkipDeep $tree $mixed | Out-Null
    [System.IO.File]::WriteAllText((Join-Path $tree 'svc/Program.cs'), "namespace Demo;`npublic class Program { int x; }`n")
    Assert-Line (Invoke-TsSkipDeep $tree $mixed) 'the typescript half had nothing to do: no file under its roots moved'
    [System.IO.File]::WriteAllText((Join-Path $tree 'web/apps/shop/src/main.ts'), "export const started = false;`n")
    Assert-NoLine (Invoke-TsSkipDeep $tree $mixed) $script:TsSkipQuiet
}

}
