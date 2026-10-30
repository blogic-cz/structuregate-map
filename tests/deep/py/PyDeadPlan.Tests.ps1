<#
    `--dead` ANSWERS IN TIME on a map with no `sqlite_stat1` (`rust/fbtcore/src/query/dead.rs`). Under the bundled SQLite
    (3.53) two shapes of its query planned into a scan per row: `p.bind = ''`, pushed into every member of the
    reads union, started from the value index; and `used_imports` as a correlated EXISTS ran the whole `uses`
    union again for each import. A tree of a few hundred defs then ran for minutes; the same answer takes a second.
    Its helper is its own - `-Only PyDeadPlan` runs this suite alone.
#>

# The gate, killed after `$Seconds`: a lens that never finishes would hang the suite, not fail it.
function Invoke-PyDeadPlanTimed([int]$Seconds, [Parameter(ValueFromRemainingArguments)][object[]]$GateArgs) {
    $gate = Get-GateInvocation
    $all = @($gate.Lead) + @($GateArgs | ForEach-Object { "$_" })
    $psi = New-Object System.Diagnostics.ProcessStartInfo
    $psi.FileName = $gate.File
    $psi.Arguments = ($all | ForEach-Object { '"' + $_ + '"' }) -join ' '
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.UseShellExecute = $false
    $process = [System.Diagnostics.Process]::Start($psi)
    $outTask = $process.StandardOutput.ReadToEndAsync()
    $errTask = $process.StandardError.ReadToEndAsync()
    if (-not $process.WaitForExit($Seconds * 1000)) {
        $process.Kill()
        $process.WaitForExit()
        return [pscustomobject]@{ Finished = $false; Exit = -1; Text = '' }
    }
    return [pscustomobject]@{ Finished = $true; Exit = $process.ExitCode; Text = $outTask.Result + $errTask.Result }
}

if ($script:Python) {

Test-Case 'pydeadplan: --dead over a few hundred bound defs answers in seconds, not minutes' {
    # Thirty modules, each importing ten defs of the others and calling one: every read bound, most imports used.
    $files = @{}
    $count = 30
    foreach ($i in 0..($count - 1)) {
        $lines = @(foreach ($k in 1..10) { "from m$(($i + $k) % $count) import f$(($i + $k) % $count)_$k" })
        foreach ($k in 0..39) {
            $lines += "def f${i}_$k(a, b):"
            $lines += "    x = a.b.c + b.d.e + f$(($i + 1) % $count)_$($k % 10 + 1)(a, b)"
            $lines += "    return x.y.z"
        }
        $files["m$i.py"] = ($lines -join "`n") + "`n"
    }
    $tree = Use-Tree $files
    $db = Join-Path $tree 'map.sqlite'
    Assert-Exit (Invoke-Gate --root $tree --ext .py --map-sqlite $db) 0
    $dead = Invoke-PyDeadPlanTimed 30 --map-query $db --dead --limit 0
    if (-not $dead.Finished) { throw '--dead did not answer within 30 s' }
    Assert-Equal $dead.Exit 0
}

}
