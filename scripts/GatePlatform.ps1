<#
    GatePlatform.ps1 - what the wiring asks of the OS it runs on. Dot-sourced by GateWiring.ps1.

    Windows is what every consumer runs today, and each answer there is exactly what the wiring always did:
    structuregate.exe from out\win-x64\publish, Windows PowerShell, a junction, `fsutil` for the hard links.
    Off Windows the exe has no extension and is published for the host's own runtime, PowerShell is `pwsh`,
    python is often `python3`, and a symbolic link needs no privilege, so it stands in for the junction.

    5.1-safe on purpose: `$env:OS` rather than `$IsWindows`, which 5.1 does not have.
#>

$script:GateOnWindows = $env:OS -eq 'Windows_NT'
$script:GateExeName = if ($script:GateOnWindows) { 'structuregate.exe' } else { 'structuregate' }
# The runtime a publish targets on this machine. Off Windows this is pwsh 7, where .NET names it itself.
$script:GateRid = if ($script:GateOnWindows) { 'win-x64' }
                  else { [System.Runtime.InteropServices.RuntimeInformation]::RuntimeIdentifier }
$script:GateShell = if ($script:GateOnWindows) { 'powershell' } else { 'pwsh' }
# `python` first, as on Windows; a distribution that ships only `python3` still has a python.
$script:GatePython = @('python', 'python3') | Where-Object { Get-Command $_ -ErrorAction SilentlyContinue } |
    Select-Object -First 1

# A path's segments, split on BOTH separators: a Windows path holds `\`, a unix one `/`, and a path written
# by hand on either can hold the other.
function Split-GatePath([string]$Path) {
    return @($Path.Split([char[]]@([char]'\', [char]'/')) | Where-Object { $_ -ne '' })
}

# Whether two paths are ONE file on disk - a hard link of each other. By the file system's own record of the
# file, never by size or date, which two separate copies share after a copy: `fsutil`'s list of the file's
# names on Windows, the device and inode `stat` reports elsewhere.
function Test-GateSameFile([string]$A, [string]$B) {
    if (-not (Test-Path $A) -or -not (Test-Path $B)) { return $false }
    $a = (Resolve-Path $A).Path
    if (-not $script:GateOnWindows) {
        $format = if ((& uname) -eq 'Darwin') { @('-f', '%d:%i') } else { @('-c', '%d:%i') }
        return (& stat @format $a) -eq (& stat @format (Resolve-Path $B).Path)
    }
    $names = @(fsutil hardlink list (Resolve-Path $B).Path 2>$null)
    foreach ($n in $names) {
        if ($a.Substring(2) -ieq $n.Trim()) { return $true }
    }
    return $false
}

# A folder that points at another one: a junction on Windows (no privilege, unlike a symbolic link there),
# a symbolic link elsewhere. Both are reparse points to .NET, which is what Test-GateFolderLink reads.
function New-GateFolderLink([string]$Link, [string]$Target) {
    $kind = if ($script:GateOnWindows) { 'Junction' } else { 'SymbolicLink' }
    [void](New-Item -ItemType $kind -Path $Link -Target $Target)
}

function Test-GateFolderLink([string]$Path) {
    return (Get-Item $Path -Force).Attributes.ToString().Contains('ReparsePoint')
}

# consumers.txt, the list Update-Gate.ps1 reads where the gate came from a GitHub release: one gate folder
# per line, `#` a comment. Compared as WHOLE lines, case-insensitively, for the reason Add-GateConsumer
# compares whole attribute values: `C:\X\Foo` is a prefix of `C:\X\FooBar`.
function Add-GateConsumerLine([string]$List, [string]$Path) {
    $wanted = $Path.TrimEnd([char]'\', [char]'/')
    $text = if (Test-Path $List) { [System.IO.File]::ReadAllText($List) } else { '' }
    foreach ($line in $text.Split([char]"`n")) {
        if ($line.Trim().TrimEnd([char]'\', [char]'/') -ieq $wanted) { return 'present' }
    }
    $folder = Split-Path $List -Parent
    if (-not (Test-Path $folder)) { [void](New-Item -ItemType Directory -Path $folder -Force) }
    $lead = if ($text -and -not $text.EndsWith("`n")) { [Environment]::NewLine } else { '' }
    [System.IO.File]::AppendAllText($List, $lead + $wanted + [Environment]::NewLine)
    return 'added'
}

# THE `findings` OF A WRITTEN MAP, read with KEYS THAT KEEP THEIR CASE. A map legitimately holds two names that differ
# only by case (C#'s `Address` and `address`, as keys of `ambiguous`), and ConvertFrom-Json refuses such a document:
# 5.1 always, 7 unless asked for a hashtable. The script died after the map step on a consumer solution. 7 is asked
# for its case-sensitive hashtable; 5.1 has .NET Framework's serializer, whose dictionary is case-sensitive too.
function Read-MapFindings([string]$MapFile) {
    $text = [System.IO.File]::ReadAllText($MapFile)
    if ($PSVersionTable.PSVersion.Major -ge 6) { return @((ConvertFrom-Json $text -AsHashtable)['findings']) }
    Add-Type -AssemblyName System.Web.Extensions
    $serializer = New-Object System.Web.Script.Serialization.JavaScriptSerializer
    # A map runs to megabytes, past the serializer's 2 MB default.
    $serializer.MaxJsonLength = [int]::MaxValue
    return @($serializer.DeserializeObject($text)['findings'])
}
