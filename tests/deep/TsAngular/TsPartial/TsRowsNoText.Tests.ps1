<#
    `--map-no-text`: a deep map that does not store the source of its files (`rust/fbtcore/src/rows/cells/text.rs`). Every other
    table is a normal map's; `--cat` reads the disk, `--text` says there is no text, a partial run derives the closure
    again, a change of the setting is made true of an existing map, and the dead-key trace (which reads the disk) is
    unchanged. Its helpers are `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '../../TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

# Two dead keys (in the locale file, in no call). A WORD in code the trace reads off the disk makes the key `leaf`
# instead of `absent` - the one verdict a map without text could get wrong. `zed` is spelled here; `empty` is not.
$script:NoTextFiles = @{
    'apps/shop/src/kp.component.html' = "<b>{{ label }}</b>`n"
    'apps/shop/src/kp.component.ts' = "import { Component, Injectable } from '@angular/core';`n" +
        "@Injectable({ providedIn: 'root' })`n" +
        "export class Words {`n  instant(key: string): string {`n    return key;`n  }`n}`n" +
        "@Component({ selector: 'app-kp', templateUrl: './kp.component.html', standalone: true })`n" +
        "export class KpComponent {`n" +
        "  label = '';`n" +
        "  constructor(private words: Words) {}`n" +
        "  build(): void {`n    this.label = this.words.instant('kp.a');`n  }`n" +
        "}`n"
    'apps/shop/src/note.ts' = "export type zed = number;`n"
    'apps/shop/src/assets/locales/en.json' = '{"shop":{"title":"Shop","cart":{"empty":"Empty"}},"kp":{"a":"1","zed":"2"}}'
    'structuregate.ts.json' = '{"locales":["apps/shop/src/assets/locales"],"i18nCalls":["instant"]}'
}

# A folder BESIDE the tree for its databases: a database inside it is a file of the tree (`last-run.jsonl` is a `.json`).
function New-NoTextHome { return (Use-Tree @{ 'keep.txt' = "demo`n" }) }

function Invoke-NoTextMap([string]$Tree, [string]$Db, [string[]]$More = @()) {
    $arguments = @('--root', $Tree, '--map-sqlite', $Db, '--ts-node-modules', $script:TsRowsModules,
        '--ts-config', (Join-Path $Tree 'structuregate.ts.json')) + $More
    return Invoke-Gate $arguments
}

# `table=rows` for every table but the source text's own (`file_text` and the FTS5 shadows): what two maps must share.
function Get-NoTextCounts([string]$Db) {
    $r = Invoke-Gate --map-query $Db --tables --limit 0 --width 0
    Assert-Exit $r 0
    # THE TABLES END AT `TOTAL`; what follows is `_meta`'s keys, which a switch of the setting changes on purpose.
    $counts = @()
    foreach ($line in $r.Lines) {
        $cells = @($line.Trim() -split '\s+')
        if ($cells[0] -eq 'TOTAL') { break }
        if ($cells.Count -ge 2 -and $cells[1] -match '^\d+$' -and -not $cells[0].StartsWith('file_text')) { $counts += "$($cells[0])=$($cells[1])" }
    }
    return (@($counts | Sort-Object) -join "`n")
}

function Get-NoTextHeld([string]$Db) {
    $r = Invoke-TsRowsQ $Db "SELECT count(*) AS n FROM sqlite_master WHERE name LIKE 'file_text%'"
    return @($r.Lines | Where-Object { $_.Trim() -match '^\d+$' })[0].Trim()
}

function Get-NoTextReach([string]$Db) {
    return (Invoke-Gate --map-query $Db --width 0 --limit 0 --sql 'SELECT * FROM key_reach ORDER BY key').Text
}

Test-Case 'notext: no file_text rows or FTS tables, every other table is a normal map''s, and the file is smaller' {
    $tree = New-TsRowsWorkspace $script:NoTextFiles
    $dir = New-NoTextHome
    $plain = Join-Path $dir 'plain.sqlite'
    $bare = Join-Path $dir 'bare.sqlite'
    Assert-Exit (Invoke-NoTextMap $tree $plain) 0
    $made = Invoke-NoTextMap $tree $bare @('--map-no-text')
    Assert-Exit $made 0
    Assert-Equal (Get-NoTextHeld $plain) '6' 'a normal map holds file_text and its five shadows'
    Assert-Equal (Get-NoTextHeld $bare) '0' 'no file_text table, no FTS5 shadow'
    Assert-Equal (Get-NoTextCounts $bare) (Get-NoTextCounts $plain) 'every other table, row for row'
    Assert-Line (Invoke-Gate --map-query $bare --sql "SELECT key || '=' || value AS m FROM _meta WHERE key = 'options:text'") 'options:text=0'
    Assert-Line (Invoke-Gate --map-query $plain --sql "SELECT key || '=' || value AS m FROM _meta WHERE key = 'options:text'") 'options:text=1'
    if ((Get-Item $bare).Length -ge (Get-Item $plain).Length) { throw 'the map without text is not smaller than the one with it' }
}

Test-Case 'notext: --cat reads the file from disk, --text says the map holds no text' {
    $tree = New-TsRowsWorkspace $script:NoTextFiles
    $dir = New-NoTextHome
    $plain = Join-Path $dir 'plain.sqlite'
    $bare = Join-Path $dir 'bare.sqlite'
    Assert-Exit (Invoke-NoTextMap $tree $plain) 0
    Assert-Exit (Invoke-NoTextMap $tree $bare @('--map-no-text')) 0
    # THE CONTROL: the line a normal map prints is the line this one prints.
    $held = Invoke-Gate --map-query $plain --cat 'kp.component.ts' --lines 4-4
    $disk = Invoke-Gate --map-query $bare --cat 'kp.component.ts' --lines 4-4
    Assert-Exit $disk 0
    Assert-Line $disk 'apps/shop/src/kp.component.ts  lines 4-4 of 15'
    Assert-Equal $disk.Text $held.Text '--cat of a map with no text against one with it'
    # AS IT IS NOW: the disk is read, so an edit since the build shows.
    Add-Content -LiteralPath (Join-Path $tree 'apps/shop/src/kp.component.ts') -Value '// demo: edited after the map'
    Assert-Line (Invoke-Gate --map-query $bare --cat 'kp.component.ts' --lines 16-16) 'edited after the map'
    # A FILE THE ROWS NAME AND THE DISK DOES NOT HOLD is refused, saying why.
    Remove-Item -LiteralPath (Join-Path $tree 'apps/shop/src/note.ts') -Force
    $gone = Invoke-Gate --map-query $bare --cat 'note.ts'
    Assert-Equal $gone.Exit 2 'a mapped file that is gone from the disk fails'
    Assert-Line $gone 'built with --map-no-text'
    # `--text`: a refusal, never an empty answer that reads as "no file mentions it".
    $text = Invoke-Gate --map-query $bare --text 'Words'
    Assert-Equal $text.Exit 2 '--text over a map with no text fails'
    Assert-Line $text 'it was built with --map-no-text'
    Assert-Line (Invoke-Gate --map-query $plain --text 'Words') 'kp.component.ts'
}

Test-Case 'notext: the dead-key trace, which reads the disk, is a normal map''s' {
    $tree = New-TsRowsWorkspace $script:NoTextFiles
    $dir = New-NoTextHome
    $plain = Join-Path $dir 'plain.sqlite'
    $bare = Join-Path $dir 'bare.sqlite'
    Assert-Exit (Invoke-NoTextMap $tree $plain) 0
    Assert-Exit (Invoke-NoTextMap $tree $bare @('--map-no-text')) 0
    $reach = Get-NoTextReach $plain
    # NOT VACUOUS: the key whose last word the code spells is `leaf`, the one nothing mentions is `absent`.
    if (-not ($reach -match 'shop\.cart\.empty\s+.*absent')) { throw "the unmentioned key is not absent: $reach" }
    if (-not ($reach -match 'kp\.zed\s+.*leaf')) { throw "the key whose word the code spells is not leaf: $reach" }
    Assert-Equal (Get-NoTextReach $bare) $reach 'key_reach of a map with no text against one with it'
}

Test-Case 'notext: a partial run derives the closure again and equals a full run of the edited tree' {
    # A WORD THE TRACE READS: `empty` is what makes `shop.cart.empty` leaf, and a type alias moves no row the closure reads,
    # so only the text says so. A comment would not (`unchanged.rs`). In a file no template resolves through: a PARTIAL run.
    $edit = { param($ts) Add-Content -LiteralPath $ts -Value 'export type empty = number;' }
    $tree = New-TsRowsWorkspace $script:NoTextFiles
    $bare = Join-Path (New-NoTextHome) 'bare.sqlite'
    Assert-Exit (Invoke-NoTextMap $tree $bare @('--map-no-text')) 0
    & $edit (Join-Path $tree 'apps/shop/src/note.ts')
    $again = Invoke-NoTextMap $tree $bare @('--map-no-text')
    Assert-Exit $again 0
    # NO TEXT TO COMPARE, SO THE CLOSURE IS DERIVED AGAIN - where a map with text keeps it (the control just below).
    Assert-Line $again 'derives its closure again: no text was held for'
    Assert-NoLine $again 'kept the closure it stored'
    Assert-Equal (Get-NoTextHeld $bare) '0' 'a partial run adds no text'

    $fresh = New-TsRowsWorkspace $script:NoTextFiles
    & $edit (Join-Path $fresh 'apps/shop/src/note.ts')
    $full = Join-Path (New-NoTextHome) 'full.sqlite'
    Assert-Exit (Invoke-NoTextMap $fresh $full @('--map-no-text')) 0
    Assert-Equal (Get-NoTextCounts $bare) (Get-NoTextCounts $full) 'every table, partial against full'
    $reach = Get-NoTextReach $full
    Assert-Equal (Get-NoTextReach $bare) $reach 'key_reach, partial against full'
    # THE EDIT MOVED A VERDICT, which a closure kept past it would have missed.
    if (-not ($reach -match 'shop\.cart\.empty\s+.*leaf')) { throw "the word the edit adds did not move the key's trace: $reach" }

    # A COMMENT MOVES NOTHING THE TRACE READS, and with no text there is still nothing to prove it by.
    $quiet = New-NoTextHome
    $quietDb = Join-Path $quiet 'bare.sqlite'
    $tree2 = New-TsRowsWorkspace $script:NoTextFiles
    Assert-Exit (Invoke-NoTextMap $tree2 $quietDb @('--map-no-text')) 0
    Add-Content -LiteralPath (Join-Path $tree2 'apps/shop/src/note.ts') -Value '// demo: a remark'
    Assert-Line (Invoke-NoTextMap $tree2 $quietDb @('--map-no-text')) 'derives its closure again: no text was held for'

    # THE CONTROL: with text, a comment that spells nothing keeps the closure.
    $kept = New-TsRowsWorkspace $script:NoTextFiles
    $plain = Join-Path (New-NoTextHome) 'plain.sqlite'
    Assert-Exit (Invoke-NoTextMap $kept $plain) 0
    Add-Content -LiteralPath (Join-Path $kept 'apps/shop/src/note.ts') -Value '// demo: a remark'
    Assert-Line (Invoke-NoTextMap $kept $plain) 'kept the closure it stored'
}

Test-Case 'notext: switching the option on an existing map drops the text, and switching it back restores it' {
    $tree = New-TsRowsWorkspace $script:NoTextFiles
    $db = Join-Path (New-NoTextHome) 'map.sqlite'
    Assert-Exit (Invoke-NoTextMap $tree $db) 0
    $counts = Get-NoTextCounts $db
    $reach = Get-NoTextReach $db
    $before = (Get-Item $db).Length

    $off = Invoke-NoTextMap $tree $db @('--map-no-text')
    Assert-Exit $off 0
    Assert-Line $off 'file_text dropped (--map-no-text)'
    Assert-Equal (Get-NoTextHeld $db) '0' 'no stale text after the switch'
    Assert-Equal (Get-NoTextCounts $db) $counts 'the rows did not move'
    Assert-Equal (Get-NoTextReach $db) $reach 'nor the trace'
    if ((Get-Item $db).Length -ge $before) { throw 'the switch did not shrink the file' }
    Assert-Equal (Invoke-Gate --map-query $db --text 'Words').Exit 2 '--text refuses'
    # A RUN THAT REPEATS THE SETTING DOES NOTHING TO IT.
    $same = Invoke-NoTextMap $tree $db @('--map-no-text')
    Assert-NoLine $same 'file_text dropped'
    Assert-NoLine $same 'rebuilt from scratch'

    $on = Invoke-NoTextMap $tree $db
    Assert-Exit $on 0
    Assert-Line $on 'rebuilt from scratch'
    Assert-Equal (Get-NoTextHeld $db) '6' 'no missing text after the switch back'
    Assert-Line (Invoke-Gate --map-query $db --text 'Words') 'kp.component.ts'
    Assert-Equal (Get-NoTextCounts $db) $counts 'the rows are the first run''s again'
}

}
