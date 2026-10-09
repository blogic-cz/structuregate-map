<#
    GateWiring.ps1 - every STEP of connecting a consumer, as one function each. Dot-sourced by
    Connect-Gate.ps1, which decides the order and prints the report.

    WHY THE STEPS ARE SEPARATE FUNCTIONS. Each one is what a person used to do by hand, and each one is
    what a test case drives directly against a throwaway tree. A single procedure would only be testable
    end to end, and the interesting failures here are per step: an Import inserted twice, a junction made
    over a real folder, a payload copied from a stale build.

    EVERY FUNCTION IS IDEMPOTENT AND SAYS WHAT IT DID. The return value is a short verb - `added`,
    `present`, `copied`, `current`, `skipped` - because connecting a tree is done again after every gate
    rebuild, and a run that cannot be repeated is a run nobody repeats.

    No regex here either, for the reason the whole repo has none: a wiring decision made by pattern match
    over a .csproj is a decision made over text that only looks like the file. String indexes and the JSON
    parser are exact.
#>

# Windows PowerShell 5.1 runs the suites, so nothing here may use PS7-only syntax.
Set-StrictMode -Version 2.0

# The two files this script writes INTO a consumer live next door, as text. They are the only part of the
# wiring that is not a decision, and keeping them here would have pushed this file past its own line limit -
# the rule it exists to enforce elsewhere.
. (Join-Path $PSScriptRoot 'GateTemplates.ps1')
# What differs per OS - the exe's name, its runtime, the shell, the hard-link and folder-link checks.
. (Join-Path $PSScriptRoot 'GatePlatform.ps1')
# What the wiring writes into a consumer's `.claude/settings.json` - the Stop hook and the plugin.
. (Join-Path $PSScriptRoot 'GateSettings.ps1')

# The directory names the gate itself never walks (rust/fbtcore/src/cli/options.rs). A tree is scanned for its extensions
# with the SAME blindness, or `node_modules` decides what languages a repo is written in.
$script:WiringSkip = @('out', 'bin', 'obj', 'dist', 'node_modules', '.git', '.vs', '.venv',
                       '__pycache__')

# Extensions worth putting on a --ext line, by the half that owns them. `.cs` and the C-style set are the
# gate's own default; `.py` and the PowerShell set are NOT, so a tree holding them needs them named.
$script:WiringKnown = @('.cs', '.ts', '.tsx', '.mts', '.cts', '.js', '.mjs', '.cjs', '.jsx',
                        '.py', '.ps1', '.psm1', '.psd1')

<#
    THE PAYLOAD IS THE PUBLISHED EXE, never the build output beside it. `src/StructureGate.csproj` writes a
    framework-dependent structuregate.exe into `out\` - an apphost that needs structuregate.dll and a
    dotnet runtime next to it. Deploying THAT into a consumer gives a tree a gate that cannot start. The
    single-file NativeAOT binary is only under out\<rid>\publish - out\win-x64\publish on Windows.
#>
function Get-GatePayload([string]$Repo) {
    $exe = Join-Path $Repo "out/$script:GateRid/publish/$script:GateExeName"
    $targets = Join-Path $Repo 'StructureGate.targets'
    if (-not (Test-Path $exe)) {
        throw "no published gate at $exe - publish it first:`n" `
            + "  dotnet publish src/StructureGate.csproj -c Release -r $script:GateRid -p:PublishAot=true -p:SkipGateDeploy=true"
    }
    if (-not (Test-Path $targets)) { throw "no StructureGate.targets at $targets" }
    return [pscustomobject]@{ Exe = (Resolve-Path $exe).Path; Targets = (Resolve-Path $targets).Path }
}

# What the tree is written in, as a --ext list. Sorted so two runs over one tree produce one string, which
# is what makes the wiring files comparable between runs.
function Get-TreeExtension([string]$Path, [string]$Exclude = '') {
    $found = New-Object 'System.Collections.Generic.HashSet[string]'
    # THE GATE FOLDER IS EXCLUDED, and the reason is a bug this had: the Stop hook wrapper this script
    # writes is a .ps1, so a second run over the same tree detected PowerShell in a tree that has none and
    # widened --ext to a language the project does not use. A tool's own footprint is not evidence.
    $skipDir = ''
    if ($Exclude) { $skipDir = (Split-Path $Exclude -Leaf).ToLowerInvariant() }
    foreach ($file in Get-ChildItem -Path $Path -Recurse -File -Force -ErrorAction SilentlyContinue) {
        $skipped = $false
        foreach ($part in (Split-GatePath $file.FullName)) {
            $lower = $part.ToLowerInvariant()
            if ($script:WiringSkip -contains $lower) { $skipped = $true; break }
            if ($skipDir -and $lower -eq $skipDir) { $skipped = $true; break }
        }
        if ($skipped) { continue }
        $extension = $file.Extension.ToLowerInvariant()
        if ($script:WiringKnown -contains $extension) { [void]$found.Add($extension) }
    }
    return @($found | Sort-Object)
}

<#
    WHICH ENTRY POINT, and the reason printed beside it. The order is not a preference: MSBuild is the only
    entry that runs before a BINARY is produced, so a tree that has a .csproj gets it even when it also has
    a package.json. A tree with neither has no build step to hang the gate on, and the turn itself becomes
    the gate - a Claude Code Stop hook.
#>
function Get-GateEntryKind([string]$Path, [string]$Requested) {
    if ($Requested -and $Requested -ne 'auto') {
        return [pscustomobject]@{ Kind = $Requested; Why = 'named on the command line' }
    }
    $projects = @(Get-ChildItem -Path $Path -Filter '*.csproj' -Recurse -File -Depth 2 -ErrorAction SilentlyContinue |
                  Where-Object { $script:WiringSkip -notcontains $_.Directory.Name.ToLowerInvariant() })
    if ($projects.Count -eq 1) {
        return [pscustomobject]@{ Kind = 'msbuild'; Why = "$($projects[0].Name) builds this tree"; Project = $projects[0].FullName }
    }
    if ($projects.Count -gt 1) {
        return [pscustomobject]@{ Kind = 'ambiguous'
                                  Why = "$($projects.Count) .csproj files here - name one with -Project, or -Entry to pick another shape" }
    }
    if (Test-Path (Join-Path $Path 'package.json')) {
        return [pscustomobject]@{ Kind = 'npm'; Why = 'package.json, so npm run is the build step' }
    }
    return [pscustomobject]@{ Kind = 'hook'; Why = 'no build step in this tree, so the turn is the gate' }
}

<#
    REGISTER THE CONSUMER WHERE THE DEPLOY READS ITS LIST. Nothing else knows it: publishing the gate copies
    the two files into every `GateConsumer` and only into those, so a tree wired by hand but never registered
    is a tree that keeps a stale exe forever. Inserted as text before the closing tag of the item group that
    already holds the others - rewriting the .csproj through the XML DOM would reformat a file this repo reads
    every day.
#>
function Add-GateConsumer([string]$Csproj, [string]$Path) {
    # THE GITIGNORED LIST a clone keeps (src/GateConsumers.local.props) does not exist until the first connect.
    if (-not (Test-Path $Csproj)) {
        [System.IO.File]::WriteAllText($Csproj, "<Project>`r`n  <ItemGroup Condition=`"'`$(OS)' == 'Windows_NT'`">`r`n  </ItemGroup>`r`n</Project>`r`n")
    }
    $text = [System.IO.File]::ReadAllText($Csproj)
    $wanted = $Path.TrimEnd([char]'\')
    # THE WHOLE ATTRIBUTE VALUE, not a substring of one: `C:\X\Foo` is inside `C:\X\FooBar` and
    # inside `C:\X\Foo\buildtools`, so a bare Contains reports a tree present that is not listed.
    if ($text.ToLowerInvariant().Contains('"' + $wanted.ToLowerInvariant() + '"')) { return 'present' }
    $anchor = 'GateConsumer Include='
    $at = $text.IndexOf($anchor)
    # AN EMPTY LIST has no item to copy the indent from: the first entry goes in the first item group.
    if ($at -lt 0) { $at = $text.IndexOf('<ItemGroup') }
    if ($at -lt 0) { throw "no GateConsumer item group in $Csproj - the deploy list is not where this script expects it" }
    $lineStart = $text.LastIndexOf([char]"`n", $at) + 1
    $indent = ''
    for ($i = $lineStart; $i -lt $at; $i++) {
        if ($text[$i] -eq [char]' ' -or $text[$i] -eq [char]"`t") { $indent += $text[$i] } else { break }
    }
    $close = $text.IndexOf('</ItemGroup>', $at)
    if ($close -lt 0) { throw "the GateConsumer item group in $Csproj is not closed" }
    $insert = $indent + '<GateConsumer Include="' + $wanted + '" />' + "`r`n"
    # AT THE START OF THE CLOSING TAG'S OWN LINE. Backing up by the ITEM's indent assumed the closing
    # tag is indented exactly as far as the items are and sits on the line directly under the last
    # one. Neither holds here - `</ItemGroup>` is one level out and a blank line precedes it - so the
    # offset landed mid-whitespace and the entry was appended to the END of the previous item's line.
    $lineOfClose = $text.LastIndexOf([char]"`n", $close) + 1
    [System.IO.File]::WriteAllText($Csproj, $text.Insert($lineOfClose, $insert))
    return 'added'
}

# The two files, into the consumer's gate folder. SkipUnchanged by content date, so a re-run over every
# consumer is not one 12 MB write per tree.
function Copy-GatePayload($Payload, [string]$GateDir) {
    if (-not (Test-Path $GateDir)) { [void](New-Item -ItemType Directory -Path $GateDir -Force) }
    $verb = 'current'
    foreach ($source in @($Payload.Exe, $Payload.Targets)) {
        $target = Join-Path $GateDir (Split-Path $source -Leaf)
        if ((Test-Path $target) -and
            (Get-Item $target).Length -eq (Get-Item $source).Length -and
            (Get-Item $target).LastWriteTimeUtc -eq (Get-Item $source).LastWriteTimeUtc) { continue }
        Copy-Item $source $target -Force
        $verb = 'copied'
    }
    return $verb
}

<#
    THE CONSUMER HOLDS A HARD LINK TO THE ONE RELEASE, NEVER A COPY. Every tree on this machine then runs the
    same bytes the last publish wrote, and a publish is one write rather than one per tree. A hard link is
    an ordinary file to every reader - MSBuild, node, a Stop hook - and needs no privilege; a symbolic link
    needs admin or Developer Mode, and a junction cannot name a file. Overwriting the release in place keeps
    every link (File.Copy truncates the same file), which is why the release is written and never replaced.

    Across drives a hard link cannot exist, so the file is COPIED and the answer says so: that tree then
    keeps its copy until the next link run. Off Windows every path has the root `/`, so another FILE SYSTEM is
    only found by the link failing - and is copied the same way.
#>
function Set-GateLink([string]$Release, [string]$GateDir) {
    $release = (Resolve-Path $Release).Path.TrimEnd([char]'\', [char]'/')
    if (-not (Test-Path $GateDir)) { [void](New-Item -ItemType Directory -Path $GateDir -Force) }
    if ((Resolve-Path $GateDir).Path.TrimEnd([char]'\', [char]'/') -ieq $release) { return 'release' }
    $verb = 'linked'
    foreach ($name in @($script:GateExeName, 'StructureGate.targets')) {
        $source = Join-Path $release $name
        $target = Join-Path $GateDir $name
        if (-not (Test-Path $source)) { throw "no $name in the release $release - publish first" }
        if (Test-GateSameFile $source $target) { continue }
        if (Test-Path $target) { Remove-Item $target -Force }
        if ([System.IO.Path]::GetPathRoot($source) -ine [System.IO.Path]::GetPathRoot((Resolve-Path $GateDir).Path)) {
            Copy-Item $source $target -Force
            $verb = 'copied (another drive - no hard link)'
            continue
        }
        try { [void](New-Item -ItemType HardLink -Path $target -Target $source) }
        catch {
            Copy-Item $source $target -Force
            $verb = 'copied (another file system - no hard link)'
            continue
        }
        if ($verb -eq 'linked') { $verb = 'relinked' }
    }
    return $verb
}

# A path as the consumer would write it in its own file. Computed from the SEGMENTS, not with
# Uri.MakeRelativeUri: that returned `..\..	3` for a project one folder below its own tree, and an
# absolute path baked into a consumer's .csproj is a file that only builds on this machine anyway. Joined
# with `\` on every OS: MSBuild reads it as a separator everywhere, and the npm launcher turns it to `/`.
function Get-WiringRelative([string]$From, [string]$To) {
    $fromParts = @(Split-GatePath $From)
    $toParts = @(Split-GatePath $To)
    $same = 0
    while ($same -lt $fromParts.Count -and $same -lt $toParts.Count -and
           $fromParts[$same].ToLowerInvariant() -eq $toParts[$same].ToLowerInvariant()) { $same++ }
    $parts = @()
    for ($i = $same; $i -lt $fromParts.Count; $i++) { $parts += '..' }
    for ($i = $same; $i -lt $toParts.Count; $i++) { $parts += $toParts[$i] }
    if ($parts.Count -eq 0) { return '.' }
    return ($parts -join '\')
}

<#
    MSBUILD. The import goes in the consumer's own .csproj, before the closing tag, and it is deliberately
    unconditional for the reason the targets file states: a gate that disappears when its path is wrong is
    not a gate. `StructureGateRoot` is the TREE, which is usually the project's own folder but is a parent
    when the project sits in a subfolder of the repo being measured.
#>
# IS THE GATE ACTUALLY IMPORTED, or merely mentioned? `Contains('StructureGate.targets')` over the whole
# file said 'present' for this repo's own csproj, which names the targets file in a comment and again in
# the `GatePayload` it deploys - so the import was never written and `dotnet build` ran no gate while the
# connect reported success. Only an `<Import ...>` element counts. Scanned, not matched: this repo bans
# regex, and the element is found by reading to the tag's own `>`.
function Test-GateImported([string]$Text) {
    $at = 0
    while ($true) {
        $at = $Text.IndexOf('<Import', $at)
        if ($at -lt 0) { return $false }
        $end = $Text.IndexOf([char]'>', $at)
        if ($end -lt 0) { return $false }
        if ($Text.Substring($at, $end - $at).Contains('StructureGate.targets')) { return $true }
        $at = $end
    }
}

function Set-MsBuildWiring([string]$Path, [string]$Project, [string]$GateDir, [string]$GateArgs) {
    $text = [System.IO.File]::ReadAllText($Project)
    if (Test-GateImported $text) { return 'present' }
    $close = $text.LastIndexOf('</Project>')
    if ($close -lt 0) { throw "$Project has no </Project> to insert before" }
    $projectDir = Split-Path $Project -Parent
    # `.` means the project IS the tree, and `$(MSBuildProjectDirectory)\.` in a property is a path the eye
    # trips over in a log. The targets file defaults to the project directory, so the plain form is used.
    $rootValue = '$(MSBuildProjectDirectory)'
    $relative = Get-WiringRelative $projectDir $Path
    if ($relative -ne '.') { $rootValue = $rootValue + '\' + $relative }
    $block = "`r`n  <!-- structuregate: the structure limits are part of this build. Connect-Gate.ps1 wrote this. -->`r`n" `
           + "  <PropertyGroup>`r`n" `
           + "    <StructureGateRoot>" + $rootValue + "</StructureGateRoot>`r`n" `
           + "    <StructureGateArgs>" + $GateArgs + "</StructureGateArgs>`r`n" `
           + "  </PropertyGroup>`r`n" `
           + "  <Import Project=`"" + (Get-WiringRelative $projectDir $GateDir) + "\StructureGate.targets`" />`r`n"
    [System.IO.File]::WriteAllText($Project, $text.Insert($close, $block))
    return 'added'
}

<#
    NPM. Two parts, and the second is the one that makes it a gate.

    The launcher is a file because `npm run` has to work the same on every shell, and because an ABSENT exe
    is a real state that deserves the rebuild instruction rather than "is not recognized".

    The script entry is written with `npm pkg set`, npm's own editor, so the consumer's package.json keeps
    its formatting - ConvertTo-Json would rewrite every line of a file somebody reads. It is `prebuild`, not
    an edit to `build`: npm runs prebuild before build by itself, so the gate blocks `npm run build` (and
    anything that goes through it) without this script touching a command the project already owns.
#>
function Set-NpmWiring([string]$Path, [string]$GateDir, [string]$GateArgs, [bool]$Deep) {
    $launcherDir = Join-Path $Path 'scripts'
    if (-not (Test-Path $launcherDir)) { [void](New-Item -ItemType Directory -Path $launcherDir -Force) }
    $launcher = Join-Path $launcherDir 'checkStructure.mjs'
    $verb = 'present'
    if (-not (Test-Path $launcher)) {
        $relative = (Get-WiringRelative $Path $GateDir).Replace('\', '/')
        [System.IO.File]::WriteAllText($launcher, (Get-NpmLauncherText $relative $GateArgs $Deep))
        $verb = 'added'
    }
    $manifest = Get-Content (Join-Path $Path 'package.json') -Raw | ConvertFrom-Json
    $scripts = $manifest.PSObject.Properties | Where-Object { $_.Name -eq 'scripts' }
    if ($scripts -and $scripts.Value) {
        foreach ($name in @('prebuild', 'check:structure')) {
            $entry = $scripts.Value.PSObject.Properties | Where-Object { $_.Name -eq $name }
            if ($entry -and "$($entry.Value)".Contains('checkStructure')) { continue }
            Invoke-Npm $Path @('pkg', 'set', "scripts.$name=node scripts/checkStructure.mjs")
            $verb = 'added'
        }
    } else {
        Invoke-Npm $Path @('pkg', 'set', 'scripts.prebuild=node scripts/checkStructure.mjs',
                           'scripts.check:structure=node scripts/checkStructure.mjs')
        $verb = 'added'
    }
    return $verb
}

# npm from a given directory, with its output kept. A failure here is reported, never swallowed: a
# package.json that did not get the entry is a tree that builds green while measuring nothing.
function Invoke-Npm([string]$Path, [object[]]$NpmArgs) {
    Push-Location $Path
    try {
        $out = & npm @NpmArgs 2>&1
        if ($LASTEXITCODE -ne 0) { throw "npm $($NpmArgs -join ' ') failed in ${Path}:`n$out" }
    } finally { Pop-Location }
}

<#
    THE SKILLS, AS JUNCTIONS (symbolic links off Windows). A consumer never holds a copy: an edit to the origin here is live in every
    tree that points at it, and two properties come free - the gate does not walk a junction, so the skill
    costs the consumer no file budget, and the map does not list it, so a junctioned skill never turns up as
    a file nothing imports.

    An EXISTING real folder is left alone and reported. Replacing one would delete a consumer's own skill.
#>
function New-GateSkillJunction([string]$Path, [string]$SkillsRoot) {
    $done = @()
    $target = Join-Path $Path '.claude\skills'
    if (-not (Test-Path $target)) { [void](New-Item -ItemType Directory -Path $target -Force) }
    foreach ($skill in Get-ChildItem -Path $SkillsRoot -Directory) {
        $link = Join-Path $target $skill.Name
        if (Test-Path $link) {
            if (Test-GateFolderLink $link) { $done += "$($skill.Name) present" } else { $done += "$($skill.Name) IS A REAL FOLDER - left alone" }
            continue
        }
        New-GateFolderLink $link $skill.FullName
        $done += "$($skill.Name) linked"
    }
    return $done
}

<#
    THE PREREQUISITES, PROBED RATHER THAN ASSUMED. Each language half runs inside the host that owns its
    parser, and a missing host is not a skipped check - every file that half owned is reported UNMAPPED, one
    line each. That is honest and useless: a tree connected without `typescript` resolvable produced an
    UNMAPPED line for nearly every file and a map with no edges at all.

    TypeScript is resolved FROM THE CONSUMER, because the compiler that may judge a repo is the one the repo
    compiles with. A tree that borrows it through createRequire from somewhere else therefore has none as far
    as the gate is concerned, however well the tree builds.
#>
function Test-GatePrereq([string]$Path, [object[]]$Extensions, [bool]$Install, [string]$Spec) {
    $notes = @()
    $script = @('.ts', '.tsx', '.mts', '.cts', '.js', '.mjs', '.cjs', '.jsx')
    $needsTs = $false
    foreach ($extension in $Extensions) { if ($script -contains $extension) { $needsTs = $true } }
    if ($needsTs) { $notes += (Resolve-GateTypeScript $Path $Install $Spec) }
    if ($Extensions -contains '.py') {
        if ($script:GatePython) { $notes += "python: ok ($script:GatePython)" }
        else { $notes += 'python: MISSING - .py files will be UNMAPPED and the SQLite map cannot be built' }
    }
    foreach ($shell in @('.ps1', '.psm1', '.psd1')) {
        if ($Extensions -notcontains $shell) { continue }
        $powershell = Get-Command $(if ($script:GateOnWindows) { 'powershell.exe' } else { 'pwsh' }) -ErrorAction SilentlyContinue
        if ($powershell) { $notes += 'powershell: ok' } else { $notes += 'powershell: MISSING - .ps1 files will be UNMAPPED' }
        break
    }
    return $notes
}

# Does `typescript` resolve from the consumer, and install it there when it does not. The probe is node's own
# resolver from the consumer's package.json - the same two origins TsGate.mjs uses - because anything else
# would answer a different question than the gate asks.
function Resolve-GateTypeScript([string]$Path, [bool]$Install, [string]$Spec) {
    $manifest = Join-Path $Path 'package.json'
    if (-not (Get-Command node -ErrorAction SilentlyContinue)) {
        return 'typescript: no node on PATH - the .ts/.js half cannot run at all'
    }
    if (-not (Test-Path $manifest)) {
        return 'typescript: NO package.json here, so nothing can be resolved from this tree - `npm init -y` then npm i -D typescript, or set NODE_PATH'
    }
    $version = Get-GateTypeScriptVersion $manifest
    if ($version -ne 'no') { return "typescript: $version resolves from this tree" + (Get-GateTypeScriptCaveat $version) }
    if (-not $Install) { return 'typescript: NOT RESOLVABLE - every .ts/.js file will be UNMAPPED (drop -SkipPrereq to install it)' }
    # A TREE THAT ALREADY DECLARES A VERSION KEEPS IT. `npm i -D typescript` would resolve to the newest
    # release - 7.x today - and the compiler that may judge a repo is the one the repo compiles with, so a
    # declared but uninstalled pin is restored with a plain `npm i` instead.
    if (Test-GateDeclaresTypeScript $manifest) { Invoke-Npm $Path @('i') }
    else { Invoke-Npm $Path @('i', '-D', $Spec) }
    $version = Get-GateTypeScriptVersion $manifest
    if ($version -eq 'no') { return "typescript: INSTALL of $Spec did not make it resolvable" }
    # THE INSTALL WRITES THE TREE'S OWN FILES: said, so a consumer branch does not commit them by accident.
    return "typescript: $version installed here as a devDependency - package.json and package-lock.json changed; commit or revert them" `
        + (Get-GateTypeScriptCaveat $version)
}

# A 7.x COMPILER IS NOT AN ERROR - the gate and the file map read it - but the deep map's plain TypeScript half reads
# only 5.x's in-process parser and stores no TypeScript row over it. Said here, at connect time, not after a map.
function Get-GateTypeScriptCaveat([string]$Version) {
    $major = 0
    if (-not [int]::TryParse(($Version.Split('.')[0]), [ref]$major) -or $major -lt 6) { return '' }
    return " - but --map-sqlite stores NO TypeScript rows over $Version (its plain half reads 5.x only): npm i -D typescript@^5"
}

# Is `typescript` in this manifest's own dependency lists? A declared pin that simply is not installed is a
# different state from a tree that never asked for a compiler, and it is restored, never overridden.
function Test-GateDeclaresTypeScript([string]$Manifest) {
    $manifestObject = Get-Content $Manifest -Raw | ConvertFrom-Json
    foreach ($list in @('dependencies', 'devDependencies')) {
        $property = $manifestObject.PSObject.Properties | Where-Object { $_.Name -eq $list }
        if (-not $property -or -not $property.Value) { continue }
        $entry = $property.Value.PSObject.Properties | Where-Object { $_.Name -eq 'typescript' }
        if ($entry) { return $true }
    }
    return $false
}

<#
    THE VERSION `typescript` RESOLVES TO FROM ONE package.json, or the string `no`. It is node's own resolver
    from the consumer's manifest - the same origin TsGate.mjs uses - because any other way of looking answers
    a different question than the gate asks.

    THE PROBE IS A FILE, not `node -e`. PowerShell rewrites the quoting of a native command's arguments and
    there is no way to stop it, so the inline program reached node as a syntax error - which reads exactly
    like "typescript is missing" and would have installed a second copy over a tree that already had one.
#>
function Get-GateTypeScriptVersion([string]$Manifest) {
    $probeFile = Join-Path ([System.IO.Path]::GetTempPath()) `
        ("sgprobe-" + [System.Guid]::NewGuid().ToString('N').Substring(0, 8) + ".cjs")
    $probeText = "const {createRequire} = require('node:module');" + "`n" `
               + "try { console.log(createRequire(process.argv[2])('typescript').version) }" `
               + " catch (e) { console.log('no') }" + "`n"
    [System.IO.File]::WriteAllText($probeFile, $probeText)
    try { $printed = (& node $probeFile $Manifest 2>&1 | Select-Object -Last 1) }
    finally { Remove-Item $probeFile -Force -ErrorAction SilentlyContinue }
    if (-not $printed) { return 'no' }
    return "$printed".Trim()
}
