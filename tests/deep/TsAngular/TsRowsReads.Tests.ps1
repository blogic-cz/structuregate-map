<#
    A partial run reads a file again when something it READ changed (`files.reads`), and follows a change further
    only while what a file shows (`files.surface`, its declaration emit) keeps moving - see
    `src/TsRows/TsSetup/TsReads.mjs`. A full run proves the reads against every row it wrote before any partial
    run may stop short. Its helpers are `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '../TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

# A chain of three: c sees a only through what b shows, never a itself - the type its member is inferred to.
$script:TsReadsChain = @{
    'apps/shop/src/a.ts' = "export function f(x: number) { return x + 1; }`n"
    'apps/shop/src/b.ts' = "import { f } from './a';`nexport function g(n: number) { return f(n); }`n"
    'apps/shop/src/c.ts' = "import { g } from './b';`nexport class H { h = g; }`n"
}

function Get-TsReadsType([string]$Db) {
    Invoke-TsRowsQ $Db ("SELECT 'h=' || m.type AS r FROM members m JOIN files f ON f.id = m.file " +
        "WHERE m.name = 'h' AND f.path = 'apps/shop/src/c.ts'")
}

# THE READS ARE PROVED BEFORE THEY ARE USED: a full run holds them against every row it wrote.
Test-Case 'tsrows: a full run proves the recorded reads before a partial run may stop short' {
    $tree = New-TsRowsWorkspace $script:TsReadsChain
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    Assert-NoLine $made.Result 'recorded reads miss'
    Assert-Line (Invoke-TsRowsQ $made.Db "SELECT 'reads=' || value AS r FROM _meta WHERE key = 'reads:typescript'") 'reads=proven'
    $b = Invoke-TsRowsQ $made.Db "SELECT 'b=' || reads AS r FROM files WHERE path = 'apps/shop/src/b.ts'"
    Assert-Line $b 'apps/shop/src/a.ts'
}

# A BODY THAT MOVED shows nothing new: its readers are read again, and nothing past them.
Test-Case 'tsrows: an edit that leaves what a file shows alone reads its readers and stops there' {
    $tree = New-TsRowsWorkspace $script:TsReadsChain
    Assert-Exit (New-TsRowsDb $tree).Result 0
    Set-Content -LiteralPath (Join-Path $tree 'apps/shop/src/a.ts') -Value 'export function f(x: number) { return x + 2; }'
    $again = New-TsRowsDb $tree
    Assert-Exit $again.Result 0
    Assert-Line $again.Result 'read 2 file(s) again for 1 changed'
    Assert-Line (Get-TsReadsType $again.Db) 'h=(n: number) => number'
}

# WHAT A FILE SHOWS MOVED, and it moved what its reader shows: the reader's reader is read again too, and gets
# the type the chain now infers.
Test-Case 'tsrows: an edit that moves what a file shows follows it through every file it moves' {
    $tree = New-TsRowsWorkspace $script:TsReadsChain
    Assert-Exit (New-TsRowsDb $tree).Result 0
    Set-Content -LiteralPath (Join-Path $tree 'apps/shop/src/a.ts') -Value 'export function f(x: number) { return String(x); }'
    $again = New-TsRowsDb $tree
    Assert-Exit $again.Result 0
    Assert-Line $again.Result 'read 3 file(s) again for 1 changed'
    Assert-Line (Get-TsReadsType $again.Db) 'h=(n: number) => string'
}

# A CHANGED FILE IS MEASURED AGAIN: its `files` row is handed back with the rest, and a size that was only
# filled where empty kept the old one.
Test-Case 'tsrows: a changed file carries the size it has now, not the one it had' {
    $tree = New-TsRowsWorkspace $script:TsReadsChain
    Assert-Exit (New-TsRowsDb $tree).Result 0
    $text = 'export function f(x: number) { return x + 1000000; }'
    Set-Content -LiteralPath (Join-Path $tree 'apps/shop/src/a.ts') -Value $text -NoNewline
    $again = New-TsRowsDb $tree
    Assert-Exit $again.Result 0
    Assert-Line (Invoke-TsRowsQ $again.Db "SELECT 'bytes=' || bytes AS r FROM files WHERE path = 'apps/shop/src/a.ts'") "bytes=$($text.Length)"
}

# A CACHED ANSWER STILL COUNTS AS READ. Two files fold the same constant through f into g; the second is
# answered from a cache the first filled, and asks the checker nothing.
Test-Case 'tsrows: a file answered from a cache still records what the answer read' {
    $files = @{
        'apps/shop/src/g.ts' = "export const BASE = 'b';`n"
        'apps/shop/src/f.ts' = "import { BASE } from './g';`nexport const URL = BASE + '/x';`n"
        'apps/shop/src/x.ts' = "import { URL } from './f';`nexport const X1 = URL;`n"
        'apps/shop/src/y.ts' = "import { URL } from './f';`nexport const Y1 = URL;`n"
    }
    $tree = New-TsRowsWorkspace $files
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    foreach ($reader in 'x', 'y') {
        $reads = Invoke-TsRowsQ $made.Db "SELECT '$reader=' || reads AS r FROM files WHERE path = 'apps/shop/src/$reader.ts'"
        Assert-Line $reads 'apps/shop/src/g.ts'
    }
    Set-Content -LiteralPath (Join-Path $tree 'apps/shop/src/g.ts') -Value "export const BASE = 'c';"
    $again = New-TsRowsDb $tree
    Assert-Exit $again.Result 0
    $values = Invoke-TsRowsQ $again.Db "SELECT name || '=' || value AS r FROM consts WHERE name IN ('X1', 'Y1')"
    Assert-Line $values 'X1=c/x'
    Assert-Line $values 'Y1=c/x'
}

# A BARREL IS NOT EVERY FILE BEHIND IT: importing one name reads the barrel and the file that declares it.
Test-Case 'tsrows: a name imported through a barrel reads the barrel and its declaring file, not the rest' {
    $files = @{
        'apps/shop/src/lib/x.ts' = "export const X = 1;`n"
        'apps/shop/src/lib/y.ts' = "export const Y = 2;`n"
        'apps/shop/src/lib/index.ts' = "export * from './x';`nexport * from './y';`n"
        'apps/shop/src/i.ts' = "import { Y } from './lib';`nexport const I = Y;`n"
    }
    $tree = New-TsRowsWorkspace $files
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    $reads = Invoke-TsRowsQ $made.Db "SELECT 'i=' || reads AS r FROM files WHERE path = 'apps/shop/src/i.ts'"
    # Y is behind the SECOND star: the walk looks in x first, and finding nothing there is not a read of x.
    Assert-Line $reads 'apps/shop/src/lib/index.ts'
    Assert-Line $reads 'apps/shop/src/lib/y.ts'
    Assert-NoLine $reads 'apps/shop/src/lib/x.ts'
    # A NAME ADDED BEHIND THE BARREL: x now declares Y too, so the star exports conflict and i's import no longer
    # lands on y. i asked the checker about the barrel's own nodes, and the barrel is read again for x - so i is
    # too, although neither it nor y changed.
    Set-Content -LiteralPath (Join-Path $tree 'apps/shop/src/lib/x.ts') -Value "export const X = 1;`nexport const Y = 3;"
    $again = New-TsRowsDb $tree
    Assert-Exit $again.Result 0
    Assert-Line $again.Result 'read 3 file(s) again for 1 changed'
}

# A ROW ANOTHER FILE NAMES KEEPS ITS ID: x's folded value names the row f's template literal has, and f is read
# again for a file x never read. Re-numbered, that reference pointed at nothing and the run had to read every hop
# instead (`TsSetup/TsReads.mjs`, `cutShort`); an expression is claimed by where it is now.
Test-Case 'tsrows: an expression another file names keeps its id when its own file is read again' {
    $files = @{
        'apps/shop/src/d.ts' = "export const D = 1;`n"
        # A CHAIN OF `||` TOO: each operand starts where the whole does, so position alone names three rows.
        'apps/shop/src/f.ts' = "import { D } from './d';`nexport const W = D;`n" + 'export const V = `v${Math.random()}`;' +
            "`nexport const L = Math.random() > 0.5 || Math.random() > 0.2 || Math.random() > 0.1;`n"
        'apps/shop/src/x.ts' = "import { V, L } from './f';`nexport class X { v = V; l = L; }`n"
    }
    $tree = New-TsRowsWorkspace $files
    Assert-Exit (New-TsRowsDb $tree).Result 0
    Set-Content -LiteralPath (Join-Path $tree 'apps/shop/src/d.ts') -Value "export const D = 1; // touched"
    $again = New-TsRowsDb $tree
    Assert-Exit $again.Result 0
    Assert-Line $again.Result 'read 2 file(s) again for 1 changed'
    Assert-NoLine $again.Result 'reading every hop of dependents again'
    # AND THE KEPT ROW NAMES A ROW THAT IS THERE.
    $joined = Invoke-TsRowsQ $again.Db ("SELECT 'found=' || count(*) AS r FROM members m JOIN expressions e " +
        "ON e.id = json_extract(m.value, '$.`$expr_id') WHERE m.name = 'v'")
    Assert-Line $joined 'found=1'
}

}
