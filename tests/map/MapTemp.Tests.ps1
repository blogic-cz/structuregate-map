<#
    AN EMPTY TMPDIR (set, but to nothing) made rust's `temp_dir()` answer "", so the embedded scripts were staged into
    whatever folder the run started in - beside the user's own files - and a host started in the tree's root could not
    find them: every file UNMAPPED. Every temp path now comes from `hosts::temp_dir()`, which falls back to /tmp.

    Off Windows only: Windows takes its temp folder from TEMP/TMP, never TMPDIR. Its helpers are its own.
#>

if (-not $script:OnWindows -and $script:Python) {
    Test-Case 'map: an empty TMPDIR stages nothing into the working folder, and the halves still run' {
        $tree = Use-Tree @{ 'pkg/a.py' = "X = 1`n"; 'pkg/b.py' = "from pkg import a`n"; 'pkg/__init__.py' = "`n" }
        $cwd = Join-Path $tree 'cwd'
        New-Item -ItemType Directory -Path $cwd | Out-Null
        $saved = [Environment]::GetEnvironmentVariable('TMPDIR')
        Push-Location $cwd
        try {
            # SET, TO NOTHING: a child of this process sees `TMPDIR=` - the state the bug needs.
            [Environment]::SetEnvironmentVariable('TMPDIR', '')
            $result = Invoke-Gate --root $tree --ext .py --map --map-check --map-out (Join-Path $tree 'm.json')
        } finally {
            Pop-Location
            if ($null -eq $saved) { Remove-Item Env:TMPDIR -ErrorAction SilentlyContinue } else { [Environment]::SetEnvironmentVariable('TMPDIR', $saved) }
        }
        Assert-Exit $result 0
        Assert-NoLine $result 'UNMAPPED'
        $staged = @(Get-ChildItem $cwd -Force | Where-Object { $_.Name.StartsWith('structuregate-') })
        Assert-Equal $staged.Count 0 'files staged into the working folder'
    }
}
