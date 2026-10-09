<#
    Update-Gate.ps1 - bring this machine's ONE release up to a GitHub release, and point every consumer at it.

      pwsh -NoProfile -File scripts/Update-Gate.ps1
      pwsh -NoProfile -File scripts/Update-Gate.ps1 -Tag v1.2.3
      pwsh -NoProfile -File scripts/Update-Gate.ps1 -Zip <local release archive>
      pwsh -NoProfile -File scripts/Update-Gate.ps1 -DryRun
      pwsh -NoProfile -File scripts/Update-Gate.ps1 -Trace         # and every later one
      pwsh -NoProfile -File scripts/Update-Gate.ps1 -NoTrace       # stop tracing

    Windows PowerShell 5.1 (`powershell`) runs it too. The replacement for `dotnet publish` deploying from a
    clone: no clone has to stay on disk. Keep a copy wherever is handy (the release's `scripts/`, or a
    folder of your own); it reads and writes `structuregate/` under local app data (`%LOCALAPPDATA%` on
    Windows, `~/.local/share` elsewhere):

      consumers.txt  one gate folder (a consumer's `buildtools/`) per line; `#` starts a comment
      release/       the unpacked release - the gate exe, StructureGate.targets, skills/, agents/

    WHAT IT DOES, in order:

      1  download this host's archive (win-x64 zip, linux-x64 tar.gz) and SHA256SUMS of the release (the
         latest, or -Tag) - anonymously, the repository is public; -Account <gh login> sends that account's token
         (a private fork, or GitHub's anonymous rate limit of 60 calls an hour); -Zip takes a local archive instead
         and skips the download and the checksum
      2  refuse an archive whose SHA256 is not the one SHA256SUMS lists
      3  write every file of it INTO release/ IN PLACE, so each hard link a consumer holds sees the new bytes
      4  per consumer: hard-link the two gate files, re-point each junction (symbolic link off Windows) in
         <tree>/.claude/skills and each agent in <tree>/.claude/agents whose name the release carries - a
         tree's OWN skills are not touched
      5  drop from consumers.txt each gate folder that no longer exists (a deleted or moved tree), so it does
         not fail every later run; a folder on a drive or root that is itself missing (an unplugged disk) is
         kept, and -DryRun only reports it

      6  -Trace, OPT-IN: on Windows set the user's STRUCTUREGATE_TRACE to `trace.jsonl` beside release/, so every gate
         run on the machine writes where its time went (rust/fbtcore/src/trace/CLAUDE.md); the exe rolls the file over at 20 MB.
         A value the user set themselves is kept. -NoTrace removes the one this script set (so does -Trace:$false
         where PowerShell parses it - `powershell -File` passes `$false` as a string and 5.1 refuses it); without
         either nothing changes, so a later update keeps tracing on. Off Windows it prints the line for a shell profile.

    A consumer's tree is the nearest folder above its gate folder that holds `.claude`. A skill or agent the
    tree never had is not added: which of them a tree carries was decided when it was wired.

    5.1-safe on purpose, like GatePlatform.ps1: `$env:OS` rather than `$IsWindows`, which 5.1 does not have.
#>
param(
    [string]$Tag = 'latest',
    [string]$Repo = 'blogic-cz/structuregate-map',
    [string]$Account = '',
    [string]$Zip = '',
    [string]$Root = '',
    [switch]$DryRun,
    [switch]$Trace,
    [switch]$NoTrace
)
$ErrorActionPreference = 'Stop'
# REFUSED BEFORE ANYTHING IS DOWNLOADED OR LINKED: both switches is no answer to act on.
if ($Trace.IsPresent -and $NoTrace.IsPresent) { Write-Host 'structuregate: -Trace and -NoTrace together - pick one'; exit 2 }

$onWindows = $env:OS -eq 'Windows_NT'
$exeName = if ($onWindows) { 'structuregate.exe' } else { 'structuregate' }
# The release names its archives by runtime: win-x64 and linux-x64 today. Off Windows this is pwsh 7, where
# .NET names the host's runtime itself - a host the release does not build for is refused by name below.
$rid = if ($onWindows) { 'win-x64' } else { [System.Runtime.InteropServices.RuntimeInformation]::RuntimeIdentifier }
if (-not $Root) {
    $appData = if ($env:LOCALAPPDATA) { $env:LOCALAPPDATA } else { [Environment]::GetFolderPath('LocalApplicationData') }
    $Root = Join-Path $appData 'structuregate'
}
$release = Join-Path $Root 'release'
$consumerList = Join-Path $Root 'consumers.txt'
$traceFile = Join-Path $Root 'trace.jsonl'

# STEP 6. The USER's variable, not the machine's: no elevation, and another account on the box is not traced. A
# program started before it - Claude Code, Visual Studio, an open terminal - does not see it until it restarts.
function Set-GateTrace([bool]$On) {
    $name = 'STRUCTUREGATE_TRACE'
    if (-not $onWindows) {
        if ($On) { Write-Host "structuregate: trace - add to your shell profile: export $name=$traceFile" }
        else { Write-Host "structuregate: trace - remove $name from your shell profile" }
        return
    }
    $current = [Environment]::GetEnvironmentVariable($name, 'User')
    if ($On -and $current -and $current -ne $traceFile) { Write-Host "structuregate: trace - kept your own $name=$current"; return }
    if (-not $On -and $current -and $current -ne $traceFile) { Write-Host "structuregate: trace - $name=$current was not set by this script, left as it is"; return }
    $want = if ($On) { $traceFile } else { $null }
    if ($current -eq $want) { Write-Host "structuregate: trace - already $(if ($On) { "on -> $traceFile" } else { 'off' })"; return }
    if ($DryRun) { Write-Host "structuregate: trace - would turn $(if ($On) { "on -> $traceFile" } else { 'off' })"; return }
    [Environment]::SetEnvironmentVariable($name, $want, 'User')
    Write-Host "structuregate: trace - $(if ($On) { "on -> $traceFile (restart Claude Code, Visual Studio and open terminals to pick it up)" } else { 'off' })"
}

# curl, not Invoke-WebRequest: an asset download is redirected to storage that refuses a second
# Authorization header, and curl drops the header on a redirect to another host where 5.1's cmdlet does not.
# `curl.exe` by name on Windows, where 5.1 aliases bare `curl` to that same cmdlet.
function Get-GateAsset([string]$Url, [string]$Token, [string]$OutFile, [string]$Accept) {
    $curl = if ($onWindows) { 'curl.exe' } else { 'curl' }
    $auth = if ($Token) { @('-H', "Authorization: Bearer $Token") } else { @() }
    & $curl -sSfL @auth -H "Accept: $Accept" -H 'X-GitHub-Api-Version: 2022-11-28' -o $OutFile $Url
    if ($LASTEXITCODE -ne 0) { throw "download failed ($LASTEXITCODE): $Url" }
}

function Test-GateArchive([string]$Name) {
    return $Name.EndsWith("-$rid.zip") -or $Name.EndsWith("-$rid.tar.gz")
}

function Get-GateRelease([string]$Work) {
    $token = ''
    if ($Account) {
        $token = (& gh auth token --user $Account 2>$null)
        if (-not $token) { throw "no gh token for the account $Account - run: gh auth login" }
    }
    $path = if ($Tag -eq 'latest') { 'latest' } else { "tags/$Tag" }
    $meta = Join-Path $Work 'release.json'
    Get-GateAsset "https://api.github.com/repos/$Repo/releases/$path" $token $meta 'application/vnd.github+json'
    $info = Get-Content $meta -Raw | ConvertFrom-Json
    $zipAsset = $info.assets | Where-Object { Test-GateArchive $_.name } | Select-Object -First 1
    $sumAsset = $info.assets | Where-Object { $_.name -eq 'SHA256SUMS' } | Select-Object -First 1
    if (-not $zipAsset -or -not $sumAsset) { throw "release $($info.tag_name) has no $rid archive or no SHA256SUMS" }
    $zipFile = Join-Path $Work $zipAsset.name
    $sumFile = Join-Path $Work 'SHA256SUMS'
    Get-GateAsset $zipAsset.url $token $zipFile 'application/octet-stream'
    Get-GateAsset $sumAsset.url $token $sumFile 'application/octet-stream'

    # A line is `<hash>  <name>`, as sha256sum writes it.
    $expected = $null
    foreach ($line in Get-Content $sumFile) {
        $parts = @($line.Trim().Split([char[]]' *', [System.StringSplitOptions]::RemoveEmptyEntries))
        if ($parts.Count -eq 2 -and $parts[1] -eq $zipAsset.name) { $expected = $parts[0] }
    }
    if (-not $expected) { throw "SHA256SUMS does not list $($zipAsset.name)" }
    $actual = (Get-FileHash $zipFile -Algorithm SHA256).Hash
    if ($actual -ine $expected) { throw "SHA256 mismatch for $($zipAsset.name): expected $expected, got $actual" }
    Write-Host "structuregate: $($info.tag_name) downloaded, SHA256 ok"
    return @{ Zip = $zipFile; Tag = $info.tag_name }
}

function Expand-GateArchive([string]$Archive, [string]$Destination) {
    if ($Archive.EndsWith('.zip')) { Expand-Archive -Path $Archive -DestinationPath $Destination; return }
    # tar keeps the exe's execute bit, which Expand-Archive would not.
    [void](New-Item -ItemType Directory -Path $Destination -Force)
    & tar -xzf $Archive -C $Destination
    if ($LASTEXITCODE -ne 0) { throw "tar failed ($LASTEXITCODE): $Archive" }
}

# Is `$B` the same file as `$A` - one more name of it, which is what a hard link is. By the file system's own
# record: `fsutil`'s list of the file's names on Windows, the device and inode `stat` reports elsewhere.
function Test-GateSameFile([string]$A, [string]$B) {
    if (-not (Test-Path $A) -or -not (Test-Path $B)) { return $false }
    $a = (Resolve-Path $A).Path
    $b = (Resolve-Path $B).Path
    if (-not $onWindows) {
        $format = if ((& uname) -eq 'Darwin') { @('-f', '%d:%i') } else { @('-c', '%d:%i') }
        return (& stat @format $a) -eq (& stat @format $b)
    }
    if ([System.IO.Path]::GetPathRoot($a) -ine [System.IO.Path]::GetPathRoot($b)) { return $false }
    # fsutil names every link WITHOUT its drive letter.
    foreach ($name in @(& fsutil hardlink list $a)) {
        if ($name.Trim() -ieq $b.Substring(2)) { return $true }
    }
    return $false
}

function Test-GateSameBytes([string]$A, [string]$B) {
    if (-not (Test-Path $B)) { return $false }
    if ((Get-Item $A).Length -ne (Get-Item $B).Length) { return $false }
    return (Get-FileHash $A -Algorithm SHA256).Hash -eq (Get-FileHash $B -Algorithm SHA256).Hash
}

# File.Copy over an existing file TRUNCATES THAT FILE, so every hard link to it sees the new bytes. A delete
# and re-create would leave each consumer holding the old file - the reason Link-Gate.ps1 writes the same way.
function Write-GateRelease([string]$Unpacked) {
    $changed = 0
    $wanted = @{}
    foreach ($file in Get-ChildItem $Unpacked -Recurse -File) {
        $relative = $file.FullName.Substring($Unpacked.Length).TrimStart([char]'\', [char]'/')
        $target = Join-Path $release $relative
        $wanted[$target.ToLowerInvariant()] = $true
        if (Test-GateSameBytes $file.FullName $target) { continue }
        $changed++
        Write-Host "  [release]  $relative"
        if ($DryRun) { continue }
        $folder = Split-Path $target -Parent
        if (-not (Test-Path $folder)) { [void](New-Item -ItemType Directory -Path $folder -Force) }
        try { [System.IO.File]::Copy($file.FullName, $target, $true) }
        catch { throw "cannot write $target - is a gate running from it? $($_.Exception.Message)" }
    }
    # A skill or agent the release no longer carries goes; a consumer's link to it keeps its own bytes. An
    # archive with no such folder at all (v1.0.0) says nothing about it, so the folder is kept as it is.
    foreach ($sub in @('skills', 'agents')) {
        $folder = Join-Path $release $sub
        if (-not (Test-Path $folder) -or -not (Test-Path (Join-Path $Unpacked $sub))) { continue }
        foreach ($file in Get-ChildItem $folder -Recurse -File) {
            if ($wanted.ContainsKey($file.FullName.ToLowerInvariant())) { continue }
            Write-Host "  [release]  removed $($file.FullName.Substring($release.Length + 1))"
            if (-not $DryRun) { Remove-Item $file.FullName -Force }
        }
    }
    return $changed
}

function Set-GateFileLink([string]$Source, [string]$Target) {
    if (Test-GateSameFile $Source $Target) { return 'present' }
    if ($DryRun) { return 'would link' }
    $folder = Split-Path $Target -Parent
    if (-not (Test-Path $folder)) { [void](New-Item -ItemType Directory -Path $folder -Force) }
    if (Test-Path $Target) { Remove-Item $Target -Force }
    if ([System.IO.Path]::GetPathRoot($Source) -ine [System.IO.Path]::GetPathRoot($Target)) {
        Copy-Item $Source $Target -Force
        return 'copied (another drive - no hard link)'
    }
    # Off Windows every path shares the root `/`, so another file system shows only when the link fails.
    try { [void](New-Item -ItemType HardLink -Path $Target -Target $Source) }
    catch {
        Copy-Item $Source $Target -Force
        return 'copied (another file system - no hard link)'
    }
    return 'linked'
}

# A junction on Windows (no privilege, unlike a symbolic link there), a symbolic link elsewhere. Removing the
# old one must remove the LINK: on Windows `rmdir`, since Remove-Item on 5.1 can walk INTO a junction and
# delete the origin; elsewhere `rm` without -r, which unlinks a symbolic link and never follows it.
function Set-GateFolderLink([string]$Link, [string]$Target) {
    if ($onWindows) {
        & cmd.exe /c rmdir "$Link"
        [void](New-Item -ItemType Junction -Path $Link -Target $Target)
    }
    else {
        & rm "$Link"
        if ($LASTEXITCODE -ne 0) { throw "cannot remove the link $Link" }
        [void](New-Item -ItemType SymbolicLink -Path $Link -Target $Target)
    }
}

function Find-GateTree([string]$GateDir) {
    $folder = Split-Path $GateDir -Parent
    while ($folder) {
        if (Test-Path (Join-Path $folder '.claude')) { return $folder }
        $folder = Split-Path $folder -Parent
    }
    return $null
}

function Update-GateConsumer([string]$GateDir) {
    foreach ($name in @($exeName, 'StructureGate.targets')) {
        Write-Host "  [gate]     $(Set-GateFileLink (Join-Path $release $name) (Join-Path $GateDir $name)) $name"
    }
    $tree = Find-GateTree $GateDir
    if (-not $tree) { return }
    $skills = Join-Path (Join-Path $tree '.claude') 'skills'
    if (Test-Path $skills) {
        foreach ($link in Get-ChildItem $skills -Directory) {
            $origin = Join-Path (Join-Path $release 'skills') $link.Name
            if (-not $link.LinkType -or -not (Test-Path $origin)) { continue }
            $now = @($link.Target)[0]
            if ($now -and $now.TrimEnd([char]'\', [char]'/') -ieq $origin) { continue }
            Write-Host "  [skill]    $($link.Name): $now -> $origin"
            if ($DryRun) { continue }
            Set-GateFolderLink $link.FullName $origin
        }
    }
    $agents = Join-Path (Join-Path $tree '.claude') 'agents'
    if (Test-Path $agents) {
        foreach ($file in Get-ChildItem $agents -File) {
            $origin = Join-Path (Join-Path $release 'agents') $file.Name
            if (-not (Test-Path $origin)) { continue }
            $verb = Set-GateFileLink $origin $file.FullName
            if ($verb -ne 'present') { Write-Host "  [agent]    $verb $($file.Name)" }
        }
    }
}

$work = Join-Path ([System.IO.Path]::GetTempPath()) ("structuregate-" + [guid]::NewGuid().ToString('N'))
[void](New-Item -ItemType Directory -Path $work)
try {
    if ($Zip) { $got = @{ Zip = (Resolve-Path $Zip).Path; Tag = 'local' } } else { $got = Get-GateRelease $work }
    $unpacked = Join-Path $work 'unpacked'
    Expand-GateArchive $got.Zip $unpacked
    foreach ($name in @($exeName, 'StructureGate.targets')) {
        if (-not (Test-Path (Join-Path $unpacked $name))) { throw "the archive has no $name - is it the $rid one?" }
    }
    if (-not (Test-Path (Join-Path $unpacked 'skills'))) { Write-Host 'structuregate: this release carries no skills/ - junctions are left where they point' }
    if (-not $DryRun -and -not (Test-Path $release)) { [void](New-Item -ItemType Directory -Path $release -Force) }
    $changed = Write-GateRelease $unpacked
    if (-not $DryRun) { [System.IO.File]::WriteAllText((Join-Path $release 'VERSION'), $got.Tag) }
    Write-Host "structuregate: release $($got.Tag) - $changed file(s) changed in $release"

    # The updater ships in the release too, and replaces whichever copy is running - wherever that copy
    # was installed, not a clone and not the release folder.
    $self = Join-Path $release 'Update-Gate.ps1'
    $runner = $PSCommandPath
    if (-not $DryRun -and (Test-Path $self) -and -not (Test-GateSameBytes $self $runner)) {
        Copy-Item $self $runner -Force
        Write-Host "structuregate: Update-Gate.ps1 updated - the next run uses it"
    }
} finally {
    Remove-Item $work -Recurse -Force -ErrorAction SilentlyContinue
}

if ($NoTrace.IsPresent) { Set-GateTrace $false }
elseif ($PSBoundParameters.ContainsKey('Trace')) { Set-GateTrace $Trace.IsPresent }

if (-not (Test-Path $consumerList)) { Write-Host "structuregate: no $consumerList - no consumer linked"; exit 0 }
$failed = 0
$gone = @()
foreach ($line in Get-Content $consumerList) {
    $dir = $line.Trim()
    if ($dir -eq '' -or $dir.StartsWith('#')) { continue }
    Write-Host "structuregate: $dir"
    if (-not (Test-Path -LiteralPath $dir -PathType Container)) {
        $drive = [System.IO.Path]::GetPathRoot($dir)
        if ($drive -and -not (Test-Path -LiteralPath $drive)) { Write-Host "  [kept]     $drive is not available"; continue }
        Write-Host "  [$(if ($DryRun) { 'would drop' } else { 'dropped' })] the folder is gone - removed from consumers.txt"
        $gone += $dir
        continue
    }
    try { Update-GateConsumer $dir }
    catch {
        # One locked tree does not stop the rest; the run still fails, so it does not read as complete.
        Write-Host "  FAILED - $($_.Exception.Message)"
        $failed++
    }
}
# Comments, blank lines and every other line stay as they were.
if ($gone.Count -and -not $DryRun) {
    $kept = @(Get-Content $consumerList | Where-Object { $gone -notcontains $_.Trim() })
    [System.IO.File]::WriteAllLines($consumerList, [string[]]$kept)
}
if ($failed) { exit 1 }
