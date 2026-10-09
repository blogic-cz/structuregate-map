<#
    Connect-Gate.ps1 end to end: what lands in a consumer, and whether it actually RUNS.

    WHAT IS ASSERTED IS THE WIRING, not the report line beside it. A connect that prints six green verbs over
    a tree where nothing runs the gate is the failure this script exists to prevent, so each case drives the
    entry point it wrote: the Stop hook wrapper is EXECUTED against a tree with a violation in it, and the
    .csproj import is asserted to name the tree rather than the project folder.

    A junction is created in one case, so these cases never point the script at this repo's own `skills\`: a
    throwaway origin under TEMP is used instead. Remove-Item -Recurse over a junction has deleted the
    TARGET's files on Windows PowerShell before now, and the target here would be the origin every consumer
    links to.
#>

$script:ConnectScript = Join-Path $script:Root 'scripts\Connect-Gate.ps1'
# The exe a publish leaves for THIS OS: out\win-x64\publish\structuregate.exe on Windows, the host's own
# runtime and no extension elsewhere - the payload Connect-Gate.ps1 deploys.
$script:ConnectExe = if ($script:OnWindows) { 'structuregate.exe' } else { 'structuregate' }
$script:ConnectRid = if ($script:OnWindows) { 'win-x64' } else { [System.Runtime.InteropServices.RuntimeInformation]::RuntimeIdentifier }
$script:ConnectReady = (Test-Path (Join-Path $script:Out "$script:ConnectRid/publish/$script:ConnectExe"))
if (-not $script:ConnectReady) {
    Write-Host "    (no published exe under out/$script:ConnectRid/publish - the connect cases are not run)"
}

# How many names a file has on disk: `fsutil`'s list on Windows, the link count `stat` reports elsewhere.
function Get-ConnectNameCount([string]$Path) {
    if ($script:OnWindows) { return @(fsutil hardlink list $Path).Count }
    return [int](& stat -c '%h' $Path)
}

# LAST RUN'S FIXTURE FOLDERS, swept at load. Each case's registry copy and skills origin sit BESIDE its tree
# (see Invoke-Connect), so the harness's own cleanup - which knows about the tree only - cannot reach them.
foreach ($stale in Get-ChildItem ([System.IO.Path]::GetTempPath()) -Directory -Filter 'sgtest-*_env' -ErrorAction SilentlyContinue) {
    Remove-Item $stale.FullName -Recurse -Force -ErrorAction SilentlyContinue
}

# The script against a throwaway tree, with the consumer list and the skills origin pointed at throwaway
# copies too. Every case gets its own of each, so no case can see another's writes.
function Invoke-Connect {
    param([string]$Tree, [object[]]$Extra)
    # OUTSIDE THE TREE, both of them. A registry copy left inside made the tree hold a .csproj, so the entry
    # detection reported msbuild for a python-only tree and the hook cases asserted against wiring that was
    # never written. The fixtures of a case must not be visible to the thing under test.
    $side = $Tree + '_env'
    if (-not (Test-Path $side)) { [void](New-Item -ItemType Directory -Path $side -Force) }
    Register-Tree $side
    $registry = Join-Path $side 'registry.csproj'
    if (-not (Test-Path $registry)) {
        Copy-Item (Join-Path $script:Root 'src\StructureGate.csproj') $registry -Force
    }
    $origin = Join-Path $side 'skills'
    if (-not (Test-Path $origin)) {
        [void](New-Item -ItemType Directory -Path (Join-Path $origin 'demo-skill') -Force)
        [System.IO.File]::WriteAllText((Join-Path $origin 'demo-skill\SKILL.md'), "# demo")
    }
    # THE RELEASE TOO: the script writes the payload into it, and the real one is this repo's buildtools\.
    $release = Join-Path $side 'release'
    $all = @('-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-File', $script:ConnectScript,
             '-Path', $Tree, '-Registry', $registry, '-SkillsRoot', $origin, '-Release', $release)
    foreach ($argument in $Extra) { foreach ($part in @($argument)) { $all += "$part" } }
    $previous = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try { $out = & $script:PowerShell @all 2>&1 | ForEach-Object { "$_".TrimEnd() } }
    finally { $ErrorActionPreference = $previous }
    return [pscustomobject]@{ Exit = $LASTEXITCODE; Lines = @($out); Text = ($out -join "`n")
                              Registry = $registry; Origin = $origin; Release = $release }
}

# A .ps1 run for its EXIT CODE, which is the whole contract of the hook wrapper.
function Invoke-ConnectHook([string]$Wrapper) {
    $previous = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try { $out = & $script:PowerShell -NoProfile -NonInteractive -ExecutionPolicy Bypass -File $Wrapper 2>&1 }
    finally { $ErrorActionPreference = $previous }
    return [pscustomobject]@{ Exit = $LASTEXITCODE; Text = (@($out) -join "`n") }
}

# How many times a needle is in a file. Counted with IndexOf rather than a match count for the reason the
# whole repo has no regex: a wiring block inserted twice must be caught as twice, exactly.
function Get-ConnectCount([string]$Path, [string]$Needle) {
    $text = [System.IO.File]::ReadAllText($Path)
    $count = 0
    $at = $text.IndexOf($Needle)
    while ($at -ge 0) { $count++; $at = $text.IndexOf($Needle, $at + 1) }
    return $count
}

# 600 lines of one language, to put a violation in a tree the wrapper is then run against.
function New-ConnectOversize([string]$Prefix) {
    $sb = New-Object System.Text.StringBuilder
    for ($i = 1; $i -le 600; $i++) { [void]$sb.AppendLine("$Prefix$i = $i") }
    return $sb.ToString()
}

if ($script:ConnectReady) {

# ---------------------------------------------------------------------------------------------------
# -DryRun, and which exe gets deployed
# ---------------------------------------------------------------------------------------------------

Test-Case '-DryRun writes nothing at all' {
    $tree = Use-Tree @{ 'a.py' = "VALUE = 1" }
    $result = Invoke-Connect $tree @('-DryRun')
    Assert-Exit $result 0
    Assert-Line $result '-DryRun: nothing was written'
    if (Test-Path (Join-Path $tree 'buildtools')) { throw 'a dry run created the gate folder' }
    if (Test-Path (Join-Path $tree '.claude')) { throw 'a dry run created .claude' }
    if (Test-Path (Join-Path $tree 'buildmap.json')) { throw 'a dry run wrote a map' }
}

# The deployed exe must be the single-file NativeAOT build. The framework-dependent one in `out\` has
# the same NAME and needs structuregate.dll and a dotnet runtime beside it, so a consumer given that one
# holds a gate that cannot start - and the size is the cheapest proof of which file was copied.
Test-Case 'the payload is the published exe, not the build output beside it' {
    $tree = Use-Tree @{ 'a.py' = "VALUE = 1" }
    [void](Invoke-Connect $tree @('-SkipMap', '-SkipSkill', '-SkipPrereq'))
    $deployed = Join-Path $tree "buildtools/$script:ConnectExe"
    if (-not (Test-Path $deployed)) { throw "no $script:ConnectExe in the gate folder" }
    $size = (Get-Item $deployed).Length
    if ($size -lt 5000000) { throw "the deployed exe is $size bytes - that is the apphost, not the AOT build" }
    if (-not (Test-Path (Join-Path $tree 'buildtools\StructureGate.targets'))) {
        throw 'no StructureGate.targets deployed'
    }
}

# THE CONSUMER HOLDS A LINK TO THE ONE RELEASE, NOT A COPY: a publish writes the release in place and every
# tree sees the new bytes, with nothing for the consumer's git to carry. One file on disk under two names is
# what `fsutil hardlink list` (a link count of 2 off Windows) says, and a copy of equal size and date would not pass it.
Test-Case 'the gate folder holds hard links to the release, and the release is written in place' {
    $tree = Use-Tree @{ 'a.py' = "VALUE = 1" }
    $made = Invoke-Connect $tree @('-SkipMap', '-SkipSkill', '-SkipPrereq')
    foreach ($name in @($script:ConnectExe, 'StructureGate.targets')) {
        $count = Get-ConnectNameCount (Join-Path $made.Release $name)
        if ($count -ne 2) { throw "$name has $count name(s) on disk, not the release plus the tree" }
    }
    # Written in place, the release's new bytes reach the tree without a second run.
    [System.IO.File]::Copy((Join-Path $script:Root 'StructureGate.targets'), (Join-Path $made.Release 'StructureGate.targets'), $true)
    Add-Content (Join-Path $made.Release 'StructureGate.targets') '<!-- touched -->'
    $seen = [System.IO.File]::ReadAllText((Join-Path $tree 'buildtools\StructureGate.targets'))
    if (-not $seen.Contains('<!-- touched -->')) { throw 'the tree kept its own copy of the targets' }
}

# ---------------------------------------------------------------------------------------------------
# the three entry points, each driven rather than believed
# ---------------------------------------------------------------------------------------------------

Test-Case 'one .csproj gets the import, and a second run does not add another' {
    $tree = Use-Tree @{ 'app\App.csproj' = "<Project Sdk=`"Microsoft.NET.Sdk`">`r`n</Project>"
                        'app\A.cs' = "class A { }" }
    $first = Invoke-Connect $tree @('-SkipMap', '-SkipSkill', '-SkipPrereq')
    Assert-Line $first '[added]  import in App.csproj'
    $second = Invoke-Connect $tree @('-SkipMap', '-SkipSkill', '-SkipPrereq')
    Assert-Line $second '[present]  import in App.csproj'
    Assert-Equal (Get-ConnectCount (Join-Path $tree 'app\App.csproj') 'StructureGate.targets') 1 `
                 'imports of StructureGate.targets'
}

# THE ROOT IS THE TREE, not the project's own folder. A project one level down that measures only itself
# leaves every sibling folder ungated, and the report would still say OK.
Test-Case 'the import points the root at the tree, not at the project folder' {
    $tree = Use-Tree @{ 'app\App.csproj' = "<Project Sdk=`"Microsoft.NET.Sdk`">`r`n</Project>"
                        'app\A.cs' = "class A { }"
                        'other\B.cs' = "class B { }" }
    [void](Invoke-Connect $tree @('-SkipMap', '-SkipSkill', '-SkipPrereq'))
    $text = [System.IO.File]::ReadAllText((Join-Path $tree 'app\App.csproj'))
    if (-not $text.Contains('<StructureGateRoot>$(MSBuildProjectDirectory)\..</StructureGateRoot>')) {
        throw "the root is not the tree. The file reads:`n$text"
    }
}

# TWO .csproj FILES ARE NOT GUESSED AT. Picking one would gate a tree from a project the owner did not
# choose, and the report would look identical to a correct wiring.
Test-Case 'two .csproj files are reported as ambiguous instead of picked' {
    $tree = Use-Tree @{ 'one\One.csproj' = "<Project Sdk=`"Microsoft.NET.Sdk`">`r`n</Project>"
                        'two\Two.csproj' = "<Project Sdk=`"Microsoft.NET.Sdk`">`r`n</Project>"
                        'one\A.cs' = "class A { }" }
    $result = Invoke-Connect $tree @('-SkipMap', '-SkipSkill', '-SkipPrereq')
    Assert-Exit $result 1
    Assert-Line $result 'cannot pick an entry point'
    if (Test-Path (Join-Path $tree 'buildtools')) { throw 'an ambiguous tree was wired anyway' }
}

Test-Case 'a package.json tree runs the gate from prebuild, and its build script is left alone' {
    $tree = Use-Tree @{ 'package.json' = "{ `"name`": `"t`", `"private`": true, `"scripts`": { `"build`": `"node build.mjs`" } }"
                        'src\a.mjs' = "export const A = 1;" }
    $result = Invoke-Connect $tree @('-SkipMap', '-SkipSkill', '-SkipPrereq')
    Assert-Line $result 'entry        npm'
    if (-not (Test-Path (Join-Path $tree 'scripts\checkStructure.mjs'))) { throw 'no launcher was written' }
    $manifest = Get-Content (Join-Path $tree 'package.json') -Raw | ConvertFrom-Json
    Assert-Equal $manifest.scripts.prebuild 'node scripts/checkStructure.mjs' 'the prebuild script'
    Assert-Equal $manifest.scripts.build 'node build.mjs' 'the build script'
}

<#
    THE HOOK WRAPPER EXISTS FOR EXIT 2 AND NOTHING ELSE. Claude Code hands a Stop hook's output back as
    something to fix only on exit 2; the gate exits 1, which is right for a build. So the case does not read
    the wrapper - it runs it over a tree with a 600-line file in it and asserts the code the harness would
    see. Exit 1 here means every violation in every hook-wired tree is printed and then ignored.
#>
Test-Case 'the Stop hook wrapper turns a violation into exit 2' {
    $tree = Use-Tree @{ 'a.py' = "VALUE = 1"; 'big.py' = (New-ConnectOversize 'VALUE_') }
    $result = Invoke-Connect $tree @('-SkipMap', '-SkipSkill', '-SkipPrereq')
    Assert-Line $result 'entry        hook'
    $wrapper = Join-Path $tree 'buildtools\StructureGate.Hook.ps1'
    if (-not (Test-Path $wrapper)) { throw 'no hook wrapper was written' }
    $settings = Get-Content (Join-Path $tree '.claude\settings.json') -Raw
    if (-not $settings.Contains('StructureGate.Hook.ps1')) { throw "the Stop hook is not in settings.json:`n$settings" }
    $hook = Invoke-ConnectHook $wrapper
    Assert-Equal $hook.Exit 2 'the wrapper exit code on a violation'
    if (-not $hook.Text.Contains('big.py')) { throw "the wrapper did not report the violation:`n$($hook.Text)" }
}

Test-Case 'the Stop hook wrapper exits 0 on a clean tree' {
    $tree = Use-Tree @{ 'a.py' = "VALUE = 1" }
    [void](Invoke-Connect $tree @('-SkipMap', '-SkipSkill', '-SkipPrereq'))
    $hook = Invoke-ConnectHook (Join-Path $tree 'buildtools\StructureGate.Hook.ps1')
    Assert-Equal $hook.Exit 0 'the wrapper exit code on a clean tree'
}

# THE PLUGIN IS DECLARED BY THE TREE, so every teammate who opens it is offered the hooks that point a session at the
# map - nobody has to know to install it. A plugin the tree already enables is kept, a second connect adds nothing,
# and -SkipPlugin writes none.
Test-Case 'connect enables the structuregate plugin beside what the tree already enables' {
    $tree = Use-Tree @{ 'a.py' = "VALUE = 1"; '.claude/settings.json' = '{"enabledPlugins": {"other@team": true}}' }
    $result = Invoke-Connect $tree @('-SkipMap', '-SkipSkill', '-SkipPrereq')
    Assert-Line $result 'plugin structuregate in .claude/settings.json'
    $settings = Get-Content (Join-Path $tree '.claude\settings.json') -Raw | ConvertFrom-Json
    Assert-Equal $settings.enabledPlugins.'structuregate@structuregate' $true 'enabled'
    Assert-Equal $settings.enabledPlugins.'other@team' $true 'the tree''s own plugin is kept'
    Assert-Equal $settings.extraKnownMarketplaces.structuregate.source.repo 'blogic-cz/structuregate-map' 'marketplace'
    $again = Invoke-Connect $tree @('-SkipMap', '-SkipSkill', '-SkipPrereq')
    Assert-Line $again '[present]  plugin structuregate'
    $skipped = Use-Tree @{ 'a.py' = "VALUE = 1" }
    [void](Invoke-Connect $skipped @('-SkipMap', '-SkipSkill', '-SkipPrereq', '-SkipPlugin'))
    $written = Get-Content (Join-Path $skipped '.claude\settings.json') -Raw
    if ($written.Contains('structuregate@structuregate')) { throw "-SkipPlugin still enabled it:`n$written" }
}

# GREEN ON ARRIVAL, AND A RATCHET AFTER. A file already over the limit is frozen by the first connect, so the
# wrapper passes; growing it fails, and a second connect does not re-freeze the grown file.
Test-Case 'a file over the limit at connect time is frozen, and may only shrink' {
    $tree = Use-Tree @{ 'a.py' = "import big`nVALUE = 1"; 'big.py' = (New-ConnectOversize 'VALUE_') }
    [void](Invoke-Connect $tree @('-SkipSkill', '-SkipPrereq'))
    if (-not (Test-Path (Join-Path $tree 'structure-baseline.json'))) { throw 'no structure-baseline.json was written' }
    $wrapper = Join-Path $tree 'buildtools\StructureGate.Hook.ps1'
    Assert-Equal (Invoke-ConnectHook $wrapper).Exit 0 'the wrapper exit code with the debt frozen'
    [System.IO.File]::AppendAllText((Join-Path $tree 'big.py'), "EXTRA = 1`nMORE = 2`n")
    Assert-Equal (Invoke-ConnectHook $wrapper).Exit 2 'the wrapper exit code after the frozen file grew'
    $again = Invoke-Connect $tree @('-SkipSkill', '-SkipPrereq')
    Assert-Line $again '[present]  structure-baseline.json'
    Assert-Equal (Invoke-ConnectHook $wrapper).Exit 2 'the wrapper exit code after a second connect'
}

# WHAT GIT SEES IS THE TREE. A build output or a vendored copy that .gitignore keeps out is not the tree's
# code; counted, it made a fresh consumer red on files nobody there wrote.
Test-Case 'in a git tree a file .gitignore keeps out is not measured' {
    if (-not (Get-Command git -ErrorAction SilentlyContinue)) { Write-Host '    (no git - skipped)'; return }
    $tree = Use-Tree @{ 'a.py' = "VALUE = 1"; 'gen\big.py' = (New-ConnectOversize 'VALUE_'); '.gitignore' = "gen/`n" }
    & git -C $tree init -q 2>&1 | Out-Null
    [void](Invoke-Connect $tree @('-SkipMap', '-SkipSkill', '-SkipPrereq'))
    $wrapper = Join-Path $tree 'buildtools\StructureGate.Hook.ps1'
    if (-not [System.IO.File]::ReadAllText($wrapper).Contains("'--tracked', '--include-untracked'")) {
        throw "the wrapper does not measure what git sees:`n$([System.IO.File]::ReadAllText($wrapper))"
    }
    Assert-Equal (Invoke-ConnectHook $wrapper).Exit 0 'the wrapper exit code with the oversized file ignored'
}

<#
    THE ENTRY POINT REBUILDS THE MAP, AS A SECOND RUN. The map was built once, at connect time, and nothing
    rebuilt it - so every connected tree's buildmap.json described the day it was connected. And `--map`
    REPLACES the limit check: a launcher that folded it into the gate's own run stopped checking limits.
    So each case deletes the connect-time map, runs the entry point, and asserts both: the map is back, and
    a violation still fails.
#>
Test-Case 'the Stop hook wrapper rebuilds the map, deep rows included, and still fails on a limit' {
    # `a.py` IMPORTS `big`, so the oversized file added later is a LIMIT violation and nothing else: the map
    # has no new finding to fail on, and only the limit check can turn the run red.
    $tree = Use-Tree @{ 'a.py' = "import big`nVALUE = 1" }
    [void](Invoke-Connect $tree @('-SkipSkill', '-SkipPrereq'))
    foreach ($made in 'buildmap.json', 'buildmap.sqlite') { Remove-Item (Join-Path $tree $made) -Force -ErrorAction SilentlyContinue }
    $wrapper = Join-Path $tree 'buildtools\StructureGate.Hook.ps1'
    Assert-Equal (Invoke-ConnectHook $wrapper).Exit 0 'the wrapper exit code on a clean tree'
    if (-not (Test-Path (Join-Path $tree 'buildmap.json'))) { throw 'the wrapper did not rebuild buildmap.json' }
    if (-not (Test-Path (Join-Path $tree 'buildmap.sqlite'))) { throw 'the wrapper did not rebuild buildmap.sqlite' }
    [System.IO.File]::WriteAllText((Join-Path $tree 'big.py'), (New-ConnectOversize 'VALUE_'))
    Assert-Equal (Invoke-ConnectHook $wrapper).Exit 2 'the wrapper exit code on a violation, with the map run after it'
}

Test-Case 'the npm launcher rebuilds the map, and still fails on a limit' {
    if (-not (Get-Command node -ErrorAction SilentlyContinue)) { Write-Host '    (no node - skipped)'; return }
    $tree = Use-Tree @{ 'package.json' = "{ `"name`": `"t`", `"private`": true }"; 'a.py' = "import big`nVALUE = 1" }
    [void](New-Item -ItemType Directory -Path (Join-Path $tree '.git') -Force)
    [void](Invoke-Connect $tree @('-SkipSkill', '-SkipPrereq'))
    foreach ($made in 'buildmap.json', 'buildmap.sqlite') { Remove-Item (Join-Path $tree $made) -Force -ErrorAction SilentlyContinue }
    $launcher = Join-Path $tree 'scripts\checkStructure.mjs'
    $previous = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try {
        $out = & node $launcher 2>&1
        Assert-Equal $LASTEXITCODE 0 "the launcher exit code on a clean tree ($($out -join ' '))"
        if (-not (Test-Path (Join-Path $tree 'buildmap.json'))) { throw 'the launcher did not rebuild buildmap.json' }
        if (-not (Test-Path (Join-Path $tree 'buildmap.sqlite'))) { throw 'the launcher did not rebuild buildmap.sqlite' }
        [System.IO.File]::WriteAllText((Join-Path $tree 'big.py'), (New-ConnectOversize 'VALUE_'))
        $out = & node $launcher 2>&1
        Assert-Equal $LASTEXITCODE 1 'the launcher exit code on a violation, with the map run after it'
    } finally { $ErrorActionPreference = $previous }
    # THE CACHES ARE IGNORED, ONCE: a second connect must not add the lines again.
    [void](Invoke-Connect $tree @('-SkipMap', '-SkipSkill', '-SkipPrereq'))
    $ignore = Join-Path $tree '.gitignore'
    Assert-Equal (Get-ConnectCount $ignore 'buildmap.sqlite') 1 'buildmap.sqlite lines in .gitignore'
    Assert-Equal (Get-ConnectCount $ignore 'map-baseline.json') 0 'the baseline is not ignored'
}

# ---------------------------------------------------------------------------------------------------
# the register, the scan, the hosts and the skills
# ---------------------------------------------------------------------------------------------------

# THE CONSUMER LIST IS THE ONLY THING THE DEPLOY READS, and a second entry for one tree makes the publish
# copy into the same folder twice and the list stop being a list of trees.
Test-Case 'the tree is registered once, however often the script is run' {
    $tree = Use-Tree @{ 'a.py' = "VALUE = 1" }
    $first = Invoke-Connect $tree @('-SkipMap', '-SkipSkill', '-SkipPrereq')
    Assert-Line $first '[added]  register'
    $second = Invoke-Connect $tree @('-SkipMap', '-SkipSkill', '-SkipPrereq')
    Assert-Line $second '[present]  register'
    Assert-Equal (Get-ConnectCount $first.Registry $tree) 1 'GateConsumer entries for this tree'
}

# FROM AN UNPACKED RELEASE, NO CLONE: the script sits in the release's scripts\, registers in the
# consumers.txt beside the release - the list Update-Gate.ps1 reads - and links the tree to that release.
Test-Case 'run from a release it registers in consumers.txt and links to that release' {
    $tree = Use-Tree @{ 'a.py' = "VALUE = 1" }
    $side = $tree + '_env'
    Register-Tree $side
    $unpacked = Join-Path $side 'structuregate\release'
    [void](New-Item -ItemType Directory -Path (Join-Path $unpacked 'scripts') -Force)
    foreach ($name in @('Connect-Gate.ps1', 'GateWiring.ps1', 'GateTemplates.ps1', 'GatePlatform.ps1', 'GateSettings.ps1')) {
        Copy-Item (Join-Path $script:Root "scripts\$name") (Join-Path $unpacked 'scripts') -Force
    }
    Copy-Item (Join-Path $script:Out "$script:ConnectRid/publish/$script:ConnectExe") $unpacked -Force
    Copy-Item (Join-Path $script:Root 'StructureGate.targets') $unpacked -Force
    $run = {
        $lines = @(& $script:PowerShell -NoProfile -NonInteractive -ExecutionPolicy Bypass -File (Join-Path $unpacked 'scripts\Connect-Gate.ps1') `
                     -Path $tree -SkipMap -SkipSkill -SkipPrereq 2>&1 | ForEach-Object { "$_" })
        [pscustomobject]@{ Lines = $lines; Text = ($lines -join "`n") }
    }
    $first = & $run
    Assert-Line $first '[added]  register in consumers.txt'
    $second = & $run
    Assert-Line $second '[present]  register in consumers.txt'
    Assert-Equal (Get-ConnectCount (Join-Path $side 'structuregate\consumers.txt') $tree) 1 'consumers.txt lines for this tree'
    $count = Get-ConnectNameCount (Join-Path $unpacked $script:ConnectExe)
    if ($count -ne 2) { throw "the release exe has $count name(s) on disk, not the release plus the tree" }
}

<#
    A TOOL'S OWN FOOTPRINT IS NOT EVIDENCE OF A LANGUAGE. The hook wrapper this script writes is a .ps1
    inside the gate folder, so a second run over a python-only tree once detected PowerShell, widened --ext
    to .ps1,.py, and would have put a tree's gate under rules for a language it does not use.
#>
Test-Case 'the gate folder does not widen --ext on a re-run' {
    $tree = Use-Tree @{ 'a.py' = "VALUE = 1" }
    $first = Invoke-Connect $tree @('-SkipMap', '-SkipSkill', '-SkipPrereq')
    Assert-Line $first 'extensions   .py'
    $second = Invoke-Connect $tree @('-SkipMap', '-SkipSkill', '-SkipPrereq')
    Assert-Line $second 'extensions   .py'
    Assert-NoLine $second 'extensions   .ps1,.py'
}

<#
    A BASELINE OVER AN UNREADABLE MAP IS THE WORST OUTCOME OF THIS SCRIPT. With no `typescript` resolvable
    from the tree, every .ts file is reported UNMAPPED - and freezing that records "nothing imports this"
    for files nobody could read. Measured on a real tree before this warning existed: nearly every file
    UNMAPPED, under a confident 0-edge summary.
#>
Test-Case 'a map with unreadable files says so instead of freezing quietly' {
    $tree = Use-Tree @{ 'src\a.ts' = "export const A = 1;" }
    $result = Invoke-Connect $tree @('-SkipSkill', '-SkipPrereq')
    Assert-Line $result 'UNMAPPED'
    Assert-Line $result 'that could not read them'
    if (-not (Test-Path (Join-Path $tree 'map-baseline.json'))) { throw 'no baseline was written at all' }
}

# TWO NAMES THAT DIFFER ONLY BY CASE: C# declares `Address` and `address`, so the map's `ambiguous` holds
# both as keys - and ConvertFrom-Json (5.1, and 7 without -AsHashtable) refuses a document with such keys. The script
# died after the map step: no UNMAPPED check, and no structure-baseline.json. The unreadable .ts proves the findings
# were really read, since only they carry the UNMAPPED that the warning counts.
Test-Case 'a map whose names differ only by case is still read - the wiring finishes' {
    $tree = Use-Tree @{
        'a\One.cs' = "namespace A; public class Address { }`npublic class address { }`n"
        'b\Two.cs' = "namespace B; public class Address { }`npublic class address { }`n"
        'U.cs'     = "namespace C; public class U { Address x; address y; }`n"
        'src\a.ts' = "export const A = 1;"
    }
    $result = Invoke-Connect $tree @('-SkipSkill', '-SkipPrereq')
    Assert-NoLine $result 'different casing'
    Assert-NoLine $result 'duplicated keys'
    Assert-Line $result 'that could not read them'
    if (-not (Test-Path (Join-Path $tree 'structure-baseline.json'))) { throw "the wiring stopped before the size baseline. Output:`n$($result.Text)" }
}

Test-Case 'the skills are junctioned, and a real folder of the same name is left alone' {
    $tree = Use-Tree @{ 'a.py' = "VALUE = 1"
                        '.claude\skills\demo-skill\OWN.md' = "# the consumer's own file" }
    $result = Invoke-Connect $tree @('-SkipMap', '-SkipPrereq')
    Assert-Line $result 'demo-skill IS A REAL FOLDER - left alone'
    if (-not (Test-Path (Join-Path $tree '.claude\skills\demo-skill\OWN.md'))) {
        throw "the consumer's own skill file was destroyed"
    }
}

Test-Case 'a skill with no folder in the way becomes a junction (a symbolic link off Windows), not a copy' {
    $tree = Use-Tree @{ 'a.py' = "VALUE = 1" }
    $result = Invoke-Connect $tree @('-SkipMap', '-SkipPrereq')
    Assert-Line $result 'demo-skill linked'
    $link = Get-Item (Join-Path $tree '.claude\skills\demo-skill') -Force
    if (-not $link.Attributes.ToString().Contains('ReparsePoint')) {
        throw "the skill is a copy, not a junction: $($link.Attributes)"
    }
}

}

# A CONNECTED FRONTEND GOT NO TYPESCRIPT ROWS: a bare `typescript` installs 7.x, which the gate reads and the deep
# map's plain half does not. The install asks for 5.x, and a 7.x already there is said at connect time.
$script:ConnectSeven = Get-TsModules
if ($script:ConnectSeven -and (Get-Command npm -ErrorAction SilentlyContinue)) {
    Test-Case 'connect: a 7.x typescript already in the tree is said to leave the deep map without TypeScript rows' {
        $tree = Use-Tree @{ 'package.json' = '{ "name": "t", "private": true }'; 'src\a.ts' = "export const A = 1;" }
        # LINKED, NOT INSTALLED: the cached 7.x is what `typescript` resolves to from this tree.
        $kind = if ($env:OS -eq 'Windows_NT') { 'Junction' } else { 'SymbolicLink' }
        [void](New-Item -ItemType $kind -Path (Join-Path $tree 'node_modules') -Target $script:ConnectSeven)
        $result = Invoke-Connect $tree @('-SkipMap', '-SkipSkill')
        Assert-Line $result 'resolves from this tree - but --map-sqlite stores NO TypeScript rows'
    }

    Test-Case 'connect: the typescript it installs is 5.x, and the files the install changed are named' {
        $tree = Use-Tree @{ 'package.json' = '{ "name": "t", "private": true }'; 'src\a.ts' = "export const A = 1;" }
        $result = Invoke-Connect $tree @('-SkipMap', '-SkipSkill')
        Assert-Line $result 'typescript: 5.'
        Assert-Line $result 'package.json and package-lock.json changed; commit or revert them'
        Assert-NoLine $result 'stores NO TypeScript rows'
    }
}
