<#
    Install-Dependencies.ps1 - put every BUILD-box tool this repo needs on the machine, repeatably.

      powershell -NoProfile -File scripts\Install-Dependencies.ps1            # Windows
      pwsh -NoProfile -File scripts/Install-Dependencies.ps1                   # Linux
      pwsh -NoProfile -File scripts/Install-Dependencies.ps1 -DryRun           # print the plan, install nothing

    WHAT IT INSTALLS, each only when it is missing or too old:

      dotnet   the .NET 10 SDK - src/StructureGate.csproj targets net10.0
      cargo    the rust toolchain, 1.85 or newer - rust/fbtcore is edition 2024 and the build runs cargo
      node     the TypeScript half runs in node
      python   the python half runs in python's own `ast`
      cc       LINUX ONLY: build-essential, clang and zlib1g-dev - rustc links through `cc`, and a
               NativeAOT publish links with clang against zlib
      msvc     WINDOWS ONLY: the C++ build tools - the NativeAOT link and the rust MSVC target both need
               link.exe, and a publish needs vswhere.exe on PATH (see CLAUDE.md)
      sccache  caches rustc's output, so a `cargo clean` or a branch switch does not cost the whole
               fbtcore compile again (minutes on a laptop)
      mold     LINUX ONLY, from apt: a faster linker for rust's own link steps

    SCCACHE AND MOLD DO NOTHING UNTIL CARGO IS TOLD, so the user's ~/.cargo/config.toml is pointed at them -
    only once each one is installed: a `rustc-wrapper` naming a missing sccache fails every cargo build on the
    machine. A config that already has the table is left alone and the lines to add are printed instead.
    Nothing is written into this repo, so a machine without them still builds it.

    A CONSUMER NEEDS NONE OF THIS: it receives the published exe and the targets file, nothing else.

    On Windows everything goes through winget. On Linux everything lands under the user's home
    (~/.dotnet, ~/.cargo, ~/.local) so no root is needed, and the tools are linked into ~/.local/bin;
    python and the C toolchain are the exceptions, taken from the system package manager. PowerShell itself has to exist before
    this can run - on Linux, `sudo snap install powershell --classic`, or unpack the release tarball.
#>
[CmdletBinding()]
param(
    [switch]$DryRun
)

$ErrorActionPreference = 'Stop'
$onWindows = $env:OS -eq 'Windows_NT'
$minRust = [version]'1.85'
$localBin = Join-Path $HOME '.local/bin'

function Write-Step([string]$Verb, [string]$What) { Write-Host ('{0,-10} {1}' -f $Verb, $What) }

function Test-Tool([string]$Name) { [bool](Get-Command $Name -ErrorAction SilentlyContinue) }

# Run a step unless -DryRun; a step either installs or throws, so a half-installed machine says where.
function Invoke-Step([string]$What, [scriptblock]$Action) {
    if ($DryRun) { Write-Step 'would' $What; return }
    Write-Step 'install' $What
    & $Action
}

function Invoke-Winget([string]$Id, [string[]]$Extra = @()) {
    & winget install --id $Id --exact --silent --accept-source-agreements --accept-package-agreements @Extra
    # -1978335189 is winget's "already installed, no newer version" - not a failure.
    if ($LASTEXITCODE -ne 0 -and $LASTEXITCODE -ne -1978335189) { throw "winget $Id exited $LASTEXITCODE" }
}

# A tool installed under the home is reached through ~/.local/bin, which most shells already have on PATH.
function Add-LocalLink([string]$Target) {
    New-Item -ItemType Directory -Force -Path $localBin | Out-Null
    $link = Join-Path $localBin (Split-Path $Target -Leaf)
    if (Test-Path $link) { Remove-Item $link -Force }
    New-Item -ItemType SymbolicLink -Path $link -Target $Target | Out-Null
}

function Get-Download([string]$Url, [string]$Name) {
    $file = Join-Path ([IO.Path]::GetTempPath()) $Name
    Invoke-WebRequest -Uri $Url -OutFile $file -UseBasicParsing
    $file
}

function Test-DotnetSdk {
    if (-not (Test-Tool 'dotnet')) { return $false }
    foreach ($line in @(& dotnet --list-sdks)) { if ($line.StartsWith('10.')) { return $true } }
    $false
}

function Get-RustVersion {
    if (-not (Test-Tool 'rustc')) { return $null }
    # "rustc 1.90.0 (1159e78c4 2025-09-14)" - the second word is the version.
    $parts = (& rustc --version).Split([char]' ')
    if ($parts.Count -lt 2) { return $null }
    [version]$parts[1].Split([char]'-')[0]
}

function Install-Dotnet {
    if (Test-DotnetSdk) { Write-Step 'have' '.NET 10 SDK'; return }
    Invoke-Step '.NET 10 SDK' {
        if ($onWindows) { Invoke-Winget 'Microsoft.DotNet.SDK.10'; return }
        $script = Get-Download 'https://dot.net/v1/dotnet-install.sh' 'dotnet-install.sh'
        $dir = Join-Path $HOME '.dotnet'
        & bash $script --channel 10.0 --install-dir $dir
        if ($LASTEXITCODE -ne 0) { throw "dotnet-install.sh exited $LASTEXITCODE" }
        Add-LocalLink (Join-Path $dir 'dotnet')
    }
}

function Install-Rust {
    $have = Get-RustVersion
    if ($have -and $have -ge $minRust) { Write-Step 'have' "rust $have"; return }
    if ($have -and (Test-Tool 'rustup')) {
        Invoke-Step "rust $have -> stable (edition 2024 needs $minRust)" {
            & rustup update stable
            if ($LASTEXITCODE -ne 0) { throw "rustup update exited $LASTEXITCODE" }
        }
        return
    }
    Invoke-Step 'rust (rustup, stable)' {
        if ($onWindows) { Invoke-Winget 'Rustlang.Rustup'; return }
        $init = Get-Download 'https://sh.rustup.rs' 'rustup-init.sh'
        & sh $init -y --profile minimal --default-toolchain stable
        if ($LASTEXITCODE -ne 0) { throw "rustup-init exited $LASTEXITCODE" }
        foreach ($tool in 'cargo', 'rustc', 'rustup') { Add-LocalLink (Join-Path $HOME ".cargo/bin/$tool") }
    }
}

function Install-Node {
    if (Test-Tool 'node') { Write-Step 'have' "node $(& node --version)"; return }
    Invoke-Step 'node (current LTS)' {
        if ($onWindows) { Invoke-Winget 'OpenJS.NodeJS.LTS'; return }
        # index.json lists releases newest first; `lts` is false on a non-LTS line and a codename on an LTS one.
        # Assigned first: PowerShell 7 hands a JSON array down a pipe as ONE object, a variable enumerates.
        $index = Invoke-RestMethod 'https://nodejs.org/dist/index.json'
        $release = $index | Where-Object { $_.lts } | Select-Object -First 1
        $name = "node-$($release.version)-linux-x64"
        $tarball = Get-Download "https://nodejs.org/dist/$($release.version)/$name.tar.xz" "$name.tar.xz"
        $root = Join-Path $HOME '.local/share/node'
        New-Item -ItemType Directory -Force -Path $root | Out-Null
        & tar -xJf $tarball -C $root
        if ($LASTEXITCODE -ne 0) { throw "tar exited $LASTEXITCODE" }
        foreach ($tool in 'node', 'npm', 'npx') { Add-LocalLink (Join-Path $root "$name/bin/$tool") }
    }
}

function Install-Python {
    $command = if ($onWindows) { 'python' } else { 'python3' }
    if (Test-Tool $command) { Write-Step 'have' (& $command --version); return }
    Invoke-Step 'python 3' {
        if ($onWindows) { Invoke-Winget 'Python.Python.3.13'; return }
        & sudo apt-get install -y python3
        if ($LASTEXITCODE -ne 0) { throw "apt-get exited $LASTEXITCODE" }
    }
}

# Linux only, from apt: rustc links through the system `cc` (rustup does not bring one), a NativeAOT publish
# links with clang against zlib, and mold links rust faster. One apt call, so sudo asks once.
function Install-NativeToolchain {
    $missing = @()
    if (-not (Test-Tool 'cc')) { $missing += 'build-essential' }
    if (-not (Test-Tool 'clang')) { $missing += 'clang' }
    if (-not (Test-Path '/usr/include/zlib.h')) { $missing += 'zlib1g-dev' }
    if (-not (Test-Tool 'mold')) { $missing += 'mold' }
    if ($missing.Count -eq 0) { Write-Step 'have' 'C toolchain (cc, clang, zlib, mold)'; return }
    Invoke-Step "$($missing -join ' ') (apt, needs sudo)" {
        & sudo apt-get install -y @missing
        if ($LASTEXITCODE -ne 0) { throw "apt-get exited $LASTEXITCODE" }
    }
}

# The upstream release binary: static (musl), so it runs on any distribution, and no compile of its own -
# `cargo install sccache` would spend minutes building the tool meant to save them.
function Install-Sccache {
    if (Test-Tool 'sccache') { Write-Step 'have' "$(& sccache --version)"; return }
    Invoke-Step 'sccache' {
        if ($onWindows) { Invoke-Winget 'Mozilla.sccache'; return }
        $tag = (Invoke-RestMethod 'https://api.github.com/repos/mozilla/sccache/releases/latest').tag_name
        $name = "sccache-$tag-x86_64-unknown-linux-musl"
        $tarball = Get-Download "https://github.com/mozilla/sccache/releases/download/$tag/$name.tar.gz" "$name.tar.gz"
        $root = Join-Path $HOME '.local/share/sccache'
        New-Item -ItemType Directory -Force -Path $root | Out-Null
        & tar -xzf $tarball -C $root
        if ($LASTEXITCODE -ne 0) { throw "tar exited $LASTEXITCODE" }
        Add-LocalLink (Join-Path $root "$name/sccache")
    }
}

# One TOML table into the user's cargo config, unless the table is already there in any form - merging into
# a table someone wrote by hand is how a config gets broken, so that case prints the lines instead.
function Add-CargoTable([string]$Header, [string[]]$Lines, [string]$What) {
    $config = Join-Path $HOME '.cargo/config.toml'
    $text = if (Test-Path $config) { [IO.File]::ReadAllText($config) } else { '' }
    $block = (@($Header) + $Lines) -join "`n"
    if ($text.Contains($block)) { Write-Step 'have' "cargo uses $What"; return }
    if ($text.Contains($Header)) {
        Write-Step 'skip' "$config already has $Header - add by hand to use ${What}:"
        foreach ($line in $Lines) { Write-Host "             $line" }
        return
    }
    if ($DryRun) { Write-Step 'would' "point cargo at $What ($config)"; return }
    Write-Step 'config' "cargo uses $What ($config)"
    New-Item -ItemType Directory -Force -Path (Split-Path $config -Parent) | Out-Null
    $lead = if ($text.Length -gt 0 -and -not $text.EndsWith("`n")) { "`n`n" } elseif ($text.Length -gt 0) { "`n" } else { '' }
    [IO.File]::AppendAllText($config, "$lead$block`n")
}

# A dry run reports the config as if the installs above had happened, since that is the plan it prints.
function Set-CargoSpeedups {
    if ($DryRun -or (Test-Tool 'sccache')) { Add-CargoTable '[build]' @('rustc-wrapper = "sccache"') 'sccache' }
    else { Write-Step 'skip' 'cargo config for sccache - it is not installed' }
    if ($onWindows) { return }
    # clang drives the link (gcc 12+ knows -fuse-ld=mold too, clang on every version that matters).
    if ($DryRun -or ((Test-Tool 'mold') -and (Test-Tool 'clang'))) {
        Add-CargoTable '[target.x86_64-unknown-linux-gnu]' @('linker = "clang"', 'rustflags = ["-C", "link-arg=-fuse-ld=mold"]') 'mold'
    }
    else { Write-Step 'skip' 'cargo config for mold - mold or clang is not installed' }
}

# Windows only: link.exe for NativeAOT and the rust MSVC target, and vswhere.exe for a publish.
function Install-Msvc {
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
    if (Test-Path $vswhere) {
        $found = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
        if ($found) { Write-Step 'have' "C++ build tools ($found)"; return }
    }
    Invoke-Step 'C++ build tools (VS 2022 Build Tools, VCTools workload)' {
        $override = '--quiet --wait --norestart --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended'
        Invoke-Winget 'Microsoft.VisualStudio.2022.BuildTools' @('--override', $override)
    }
}

if ($onWindows -and -not (Test-Tool 'winget')) { Write-Host 'winget is missing - install App Installer from the Microsoft Store'; exit 1 }

# What needs root goes LAST on Linux, so a sudo that cannot prompt still leaves the user-local tools in.
if ($onWindows) { Install-Msvc }
Install-Dotnet
Install-Rust
Install-Node
Install-Python
Install-Sccache
# A sudo that cannot prompt fails the apt step, and that must not also cost the cargo config for what DID
# install - so it is reported, the run goes on, and the exit code says it at the end.
$aptFailed = $null
if (-not $onWindows) {
    try { Install-NativeToolchain }
    catch { $aptFailed = $_.Exception.Message; Write-Step 'FAILED' "apt step: $aptFailed - run it in a terminal that can ask for the password" }
}
Set-CargoSpeedups
if ($aptFailed) { exit 1 }

if (-not $DryRun) {
    Write-Host ''
    Write-Host 'done. open a NEW shell so PATH picks up what was installed, then:'
    Write-Host '  dotnet build src/StructureGate.csproj -p:SkipTests=true'
    if (-not $onWindows) { Write-Host "  (on Linux, $localBin must be on PATH)" }
}
