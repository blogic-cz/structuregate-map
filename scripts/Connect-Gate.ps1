<#
    Connect-Gate.ps1 - wire a project into structuregate in one command, repeatably.

      powershell -NoProfile -File scripts\Connect-Gate.ps1 -Path C:\src\app
      powershell -NoProfile -File scripts\Connect-Gate.ps1 -Path C:\src\app -DryRun
      powershell -NoProfile -File scripts\Connect-Gate.ps1 -Path C:\src\app -Entry hook -GateArgs '--doc-scope context'

    WHAT IT DOES, in order, printing a verb per step:

      1  register the tree as a GateConsumer here, which is the only list the deploy reads
      2  hard-link the two files (structuregate.exe, StructureGate.targets) into <Path>\buildtools, to
         the ONE release in this repo's buildtools\ - never a copy, never in the consumer's git
      3  pick the entry point from what the tree holds, and write it
      4  probe the language hosts, and install `typescript` into the tree when the .ts/.js half needs it
      5  junction this repo's skills into <Path>\.claude\skills
      6  build the first map and FREEZE what it cannot resolve and every file already over a limit, so day
         one is green

    STEP 6 IS NOT DECORATION. A tree connected without it starts red on findings that were true before the
    gate arrived - entry points nothing imports, run-time imports, files already over a limit - and a gate
    that is red on arrival is a gate somebody switches off. `--update-map-baseline` and `--update-baseline`
    record those counts as the ratchet, so only NEW ones fail. In a git tree the gate measures what git
    sees (`--tracked --include-untracked`): a build output or a vendored copy .gitignore keeps out is not
    the tree's code. Measured on the trees connected without both: most were red on arrival.

    Everything is idempotent: run it again after every gate rebuild. -DryRun prints the plan and writes
    nothing.

    FROM A RELEASE, NO CLONE. The GitHub release ships this script and its helpers in `scripts\` of the
    unpacked release Update-Gate.ps1 keeps under local app data. Run from there it registers the tree in
    that folder's `consumers.txt` (the list Update-Gate.ps1 reads), links the gate folder to the release it
    sits in and junctions the release's skills; step 2 only links, since the release IS the payload.
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$Path,
    [string]$GateDir = 'buildtools',
    [ValidateSet('auto', 'msbuild', 'npm', 'hook', 'none')][string]$Entry = 'auto',
    [string]$Project = '',
    [string]$Ext = '',
    [string]$GateArgs = '',
    [string]$Registry = '',
    [string]$SkillsRoot = '',
    [string]$Release = '',
    # 5.x, NOT THE NEWEST: the deep map's plain TypeScript half reads only 5.x's in-process parser, and a bare
    # `typescript` installs 7.x - a connected tree then stored no TypeScript rows at all. The gate reads both.
    [string]$TypeScriptSpec = 'typescript@^5',
    [switch]$SkipRegister,
    [switch]$SkipSkill,
    [switch]$SkipPrereq,
    [switch]$SkipMap,
    [switch]$DryRun
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'GateWiring.ps1')

$repo = Split-Path $PSScriptRoot -Parent
# A CLONE HAS THE PROJECT; the unpacked release has the exe beside `scripts\` instead (see the header).
$fromRelease = -not (Test-Path (Join-Path $repo 'src\StructureGate.csproj'))
$release = if ($Release) { $Release } elseif ($fromRelease) { $repo } else { Join-Path $repo 'buildtools' }
if (-not (Test-Path $Path)) { Write-Host "no such tree: $Path"; exit 1 }
$tree = (Resolve-Path $Path).Path.TrimEnd([char]'\', [char]'/')
if (-not $Registry) {
    # NEVER THE TRACKED .csproj: a consumer's absolute path is this machine's, and the project is published.
    $Registry = if ($fromRelease) { Join-Path (Split-Path $repo -Parent) 'consumers.txt' } else { Join-Path $repo 'src\GateConsumers.local.props' }
}
if (-not $SkillsRoot) { $SkillsRoot = Join-Path $repo 'skills' }
$gateFolder = Join-Path $tree $GateDir

# WHAT THE TREE IS WRITTEN IN decides --ext, because the gate's default (.cs,.mjs,.js,.ts) carries neither
# .py nor the PowerShell set: a python tree connected with the default measures nothing and says so nowhere.
$extensions = @()
if ($Ext) { foreach ($part in $Ext.Split([char]',')) { if ($part.Trim()) { $extensions += $part.Trim() } } }
else { $extensions = @(Get-TreeExtension $tree $gateFolder) }
if (@($extensions).Count -eq 0) { Write-Host "no source files under $tree that this gate knows how to count"; exit 1 }
# WHAT GIT SEES, in a git tree. `rev-parse` rather than a `.git` test: a worktree holds a `.git` FILE, and an
# empty `.git` folder is no repository - `--tracked` there would measure nothing.
$scope = ''
if (Get-Command git -ErrorAction SilentlyContinue) {
    $ErrorActionPreference = 'Continue'
    & git -C $tree rev-parse --is-inside-work-tree *> $null
    if ($LASTEXITCODE -eq 0) { $scope = '--tracked --include-untracked' }
    $ErrorActionPreference = 'Stop'
}
$effective = (@('--ext ' + ($extensions -join ','), $scope, $GateArgs) | Where-Object { $_ }) -join ' '
# The size ratchet. Every entry point passes it; a baseline file that does not exist yet reads as empty.
$structureBaseline = Join-Path $tree 'structure-baseline.json'

# THE DEEP MAP (--map-sqlite) is asked for wherever a half can write rows: python with a host to parse it,
# and any TypeScript/JavaScript - the Angular half for a workspace, the plain half everywhere else. It was
# python only once; a Solid tree connected then got a graph and no rows.
$scriptExt = @('.ts', '.tsx', '.mts', '.cts', '.js', '.mjs', '.cjs', '.jsx')
$deep = (($extensions -contains '.py') -and [bool]$script:GatePython) -or
        [bool](@($extensions | Where-Object { $scriptExt -contains $_ }).Count)

$point = Get-GateEntryKind $tree $Entry
if ($Project) { $point = [pscustomobject]@{ Kind = 'msbuild'; Why = 'project named on the command line'; Project = (Resolve-Path $Project).Path } }
if ($point.Kind -eq 'ambiguous') { Write-Host "cannot pick an entry point: $($point.Why)"; exit 1 }

Write-Host "structuregate: connecting $tree"
Write-Host "  extensions   $($extensions -join ',')"
Write-Host "  entry        $($point.Kind) ($($point.Why))"
Write-Host "  gate folder  $gateFolder"
if ($DryRun) {
    Write-Host ''
    Write-Host '  -DryRun: nothing was written. The steps that would run:'
    Write-Host "    register in $Registry"
    Write-Host "    hard-link $script:GateExeName + StructureGate.targets into $gateFolder (release: $release)"
    Write-Host "    write the $($point.Kind) entry point"
    Write-Host '    probe node/python/powershell, install typescript if the .ts/.js half needs it'
    Write-Host '    junction the skills, then build the first map and freeze its baseline'
    exit 0
}

$payload = if ($fromRelease) { $null } else { Get-GatePayload $repo }
Write-Host ''

if ($SkipRegister) { Write-Host '  [skipped]  register' }
# THE GATE FOLDER, NOT THE TREE. `DeployGate` copies the payload straight into whatever this list
# names, and `Copy-GatePayload` below puts it in `$gateFolder` - registering `$tree` instead pointed
# the deploy at the repo root, two directories the wiring never reads, while the gate the build
# actually imports kept the exe it was connected with. Every consumer already in the list is a
# `\buildtools` path for this reason.
elseif ($Registry.EndsWith('.txt')) { Write-Host "  [$(Add-GateConsumerLine $Registry $gateFolder)]  register in $(Split-Path $Registry -Leaf)" }
else { Write-Host "  [$(Add-GateConsumer $Registry $gateFolder)]  register in $(Split-Path $Registry -Leaf)" }

# THE RELEASE FIRST, THEN THE LINK. A tree connected before any publish still gets the gate the last
# build published, and every later publish reaches it through the link.
if ($payload) { Write-Host "  [$(Copy-GatePayload $payload $release)]  payload into the release $release" }
else { Write-Host "  [release]  the payload is the release this script came with: $release" }
Write-Host "  [$(Set-GateLink $release $gateFolder)]  $GateDir -> release"

# THE ENTRY POINT IS THE GATE. Everything above only puts an exe on disk; this step is what makes something
# run it without being asked. `none` exists for a tree whose owner runs the gate from their own script.
switch ($point.Kind) {
    'msbuild' {
        $withBaseline = $effective + ' --baseline "$(StructureGateRoot)\structure-baseline.json"'
        Write-Host "  [$(Set-MsBuildWiring $tree $point.Project $gateFolder $withBaseline)]  import in $(Split-Path $point.Project -Leaf)"
    }
    'npm'     { Write-Host "  [$(Set-NpmWiring $tree $gateFolder $effective $deep)]  scripts/checkStructure.mjs + prebuild" }
    'hook'    { Write-Host "  [$(Set-HookWiring $tree $gateFolder $effective $deep)]  Stop hook + .claude/settings.json" }
    'none'    { Write-Host '  [skipped]  entry point (-Entry none)' }
}
# The npm launcher and the hook wrapper rebuild the map on every run, so its outputs are ignored there.
if ($point.Kind -eq 'npm' -or $point.Kind -eq 'hook') { Write-Host "  [$(Add-GateIgnore $tree)]  .gitignore: buildmap.json, buildmap.sqlite" }

if ($SkipPrereq) { Write-Host '  [skipped]  host probe' }
else { foreach ($note in Test-GatePrereq $tree $extensions $true $TypeScriptSpec) { Write-Host "  [host]     $note" } }

if ($SkipSkill) { Write-Host '  [skipped]  skills' }
else { foreach ($note in New-GateSkillJunction $tree $SkillsRoot) { Write-Host "  [skill]    $note" } }

<#
    THE FIRST MAP, AND THE RATCHET UNDER IT. --update-map-baseline records what this tree already cannot
    resolve - the entry points nothing imports, the imports built at run time - so the gate starts green and
    only a NEW blind spot fails. Without it a fresh consumer opens on findings that predate the gate, which
    is how a gate gets turned off in week one.

    The SQLite map is asked for where a half can write rows - see `$deep` above.
#>
if ($SkipMap) { Write-Host '  [skipped]  first map' }
else {
    $mapArgs = @('--root', $tree)
    foreach ($part in $effective.Split([char]' ')) { if ($part) { $mapArgs += $part } }
    $mapArgs += @('--map', '--map-out', (Join-Path $tree 'buildmap.json'),
                  '--map-baseline', (Join-Path $tree 'map-baseline.json'), '--update-map-baseline')
    if ($deep) { $mapArgs += @('--map-sqlite', (Join-Path $tree 'buildmap.sqlite')) }
    $exe = Join-Path $gateFolder $script:GateExeName
    $ErrorActionPreference = 'Continue'
    $out = & $exe @mapArgs 2>&1
    $ErrorActionPreference = 'Stop'
    foreach ($line in @($out)) { if ("$line".Trim()) { Write-Host "  [map]      $("$line".Trim())" } }
    if ($LASTEXITCODE -ne 0) { Write-Host "  [map]      the map run exited $LASTEXITCODE - read the lines above before trusting the wiring" }

    # A BASELINE OVER AN UNREADABLE MAP IS THE WORST OUTCOME OF THIS SCRIPT, so it is named here. Every file
    # a missing host could not parse is reported UNMAPPED, and freezing that state records "nothing imports
    # this" for files nobody could read - a map that looks built and answers wrongly: nearly every file
    # UNMAPPED because `typescript` did not resolve, with a confident 0-edge summary.
    $mapFile = Join-Path $tree 'buildmap.json'
    if (Test-Path $mapFile) {
        $unmapped = 0
        foreach ($finding in @(Read-MapFindings $mapFile)) {
            if ("$finding".StartsWith('UNMAPPED')) { $unmapped++ }
        }
        if ($unmapped -gt 0) {
            Write-Host "  [map]      WARNING: $unmapped file(s) UNMAPPED - a host is missing, so the baseline just froze a map"
            Write-Host '  [map]      that could not read them. Fix the [host] line above and re-run before trusting it.'
        }
    }

    # THE SIZE DEBT, frozen ONCE. An existing baseline is the ratchet itself: rewriting it on a re-run would
    # absorb every file that grew since, which is exactly what it exists to refuse.
    if (Test-Path $structureBaseline) { Write-Host "  [present]  structure-baseline.json" }
    else {
        $limitArgs = @('--root', $tree)
        foreach ($part in $effective.Split([char]' ')) { if ($part) { $limitArgs += $part } }
        $ErrorActionPreference = 'Continue'
        $out = & $exe @limitArgs --baseline $structureBaseline --update-baseline 2>&1
        $ErrorActionPreference = 'Stop'
        foreach ($line in @($out)) { if ("$line".Trim()) { Write-Host "  [limits]   $("$line".Trim())" } }
    }
}

# WHAT THE TREE RUNS NOW. Printed because the next person to ask "is this wired?" should not have to read
# three files to find out, and because the answer differs per entry point.
Write-Host ''
switch ($point.Kind) {
    'msbuild' { Write-Host "  now: dotnet build $(Split-Path $point.Project -Leaf) runs the gate before CoreCompile (-p:SkipStructureGate=true opts out)" }
    'npm'     { Write-Host '  now: npm run build runs the gate first (prebuild); npm run check:structure runs it alone' }
    'hook'    { Write-Host '  now: every Claude Code turn in this tree ends with the gate, and a violation blocks the stop' }
    'none'    { Write-Host "  now: nothing runs the gate by itself. Call it: $(Join-Path $GateDir $script:GateExeName) --root . $effective" }
}
Write-Host "  re-run this script after every gate rebuild; it is idempotent"
exit 0
