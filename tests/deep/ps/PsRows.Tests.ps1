<#
    THE POWERSHELL HALF OF THE DEEP MAP (`src/Map/PsRows.ps1`, launched by `rust/fbtcore/src/mapper/deep/own/ps.rs`) and
    the statement shingles the file map's half (`src/Map/PsMap.ps1`) now sends beside each body digest.

    - EVERY FUNCTION, METHOD, CALL, LITERAL, REGEX, BRANCH AND HANDLER IS A ROW with the class and function it sits in.
    - ONE FINGERPRINT WITH THE FILE MAP (`src/Map/PsShape.ps1`): a `body_shape` group in SQL is a `duplicate_bodies` group.
    - INCREMENTAL like every payload half: an unchanged run starts no host, and a file that has gone loses its rows.
    - A NEAR COPY - one literal changed, one statement added - is a `similar_bodies` pair.

    Its helpers are its own - `-Only PsRows` runs this suite alone.
#>

# The file map and the deep map over $Tree's PowerShell, and what the run said.
function Invoke-PsRowsMap([string]$Tree) {
    $result = Invoke-Gate --root $Tree --ext .ps1 --map --map-out (Join-Path $Tree 'm.json') --map-sqlite (Join-Path $Tree 'map.sqlite')
    Assert-Exit $result 0
    return $result
}

function Get-PsRowsCells([string]$Tree, [string]$Sql) {
    return Get-MapQueryRows (Join-Path $Tree 'map.sqlite') $Sql
}

$script:PsRowsLibrary = @'
<# Demo library. #>
class Order {
    [int] Total([int]$n) { return $n * 2 }
}
function Get-Alpha([string]$Name, $Count = 3) {
    $items = Get-ChildItem -Path 'src'
    if ($Name -match '^a\d+$') { Write-Host "alpha $Name" } elseif ($Count -gt 7) { throw 'too many' }
    try { . (Join-Path $PSScriptRoot 'other.ps1') } catch [System.IO.IOException] { }
    return [regex]::Replace($Name, 'b+', 'c')
}
'@

Test-Case 'psrows: every function, call, literal, regex, branch and handler is a row in the class and function it sits in' {
    $tree = Use-Tree @{ 'lib.ps1' = $script:PsRowsLibrary }
    [void](Invoke-PsRowsMap $tree)
    $rows = Get-PsRowsCells $tree ("SELECT 'v=' || f.lang || '|' || f.path || '|' || f.doc || '|' || f.entry FROM files f")
    Assert-Equal ($rows -join ';') 'powershell|lib.ps1|Demo library.|0' 'the files row'
    $rows = Get-PsRowsCells $tree ("SELECT 'v=' || qualname || '|' || kind || '|' || cls || '|' || " +
        "(SELECT group_concat(a.value, ',') FROM functions__args a WHERE a.owner = fn.id AND a.parent = 0) FROM functions fn ORDER BY line")
    Assert-Equal ($rows -join ';') 'Order.Total|method|Order|$n;Get-Alpha|function||$Name,$Count' 'the functions, a method once'
    $rows = Get-PsRowsCells $tree "SELECT 'v=' || callee || '|' || kind || '|' || func FROM calls ORDER BY line, callee"
    Assert-Equal ($rows -join ';') 'Get-ChildItem|command|Get-Alpha;Write-Host|command|Get-Alpha;Join-Path|command|Get-Alpha;[regex]::Replace|static|Get-Alpha' 'the calls'
    $rows = Get-PsRowsCells $tree "SELECT 'v=' || value || '|' || use || '|' || callee FROM string_literals WHERE value IN ('src', 'b+') ORDER BY line"
    Assert-Equal ($rows -join ';') 'src|argument|Get-ChildItem;b+|argument|Replace' 'what a literal is doing'
    $rows = Get-PsRowsCells $tree "SELECT 'v=' || kind || '|' || api || '|' || pattern || '|' || func FROM regexes ORDER BY line"
    Assert-Equal ($rows -join ';') 'operator|-match|^a\d+$|Get-Alpha;call|regex::Replace|b+|Get-Alpha' 'the regexes'
    $rows = Get-PsRowsCells $tree "SELECT 'v=' || kind || '|' || test FROM branches ORDER BY line, id"
    Assert-Equal ($rows -join ';') "if|`$Name -match '^a\d+$';elseif|`$Count -gt 7;try|" 'the branches'
    $rows = Get-PsRowsCells $tree ("SELECT 'v=' || h.kind || '|' || h.statements || '|' || (SELECT group_concat(t.value) FROM handlers__types t " +
        "WHERE t.owner = h.id AND t.parent = 0) FROM handlers h")
    Assert-Equal ($rows -join ';') 'catch|0|System.IO.IOException' 'the empty catch'
    $rows = Get-PsRowsCells $tree "SELECT 'v=' || kind || '|' || module FROM imports"
    Assert-Equal ($rows -join ';') "dot|(Join-Path `$PSScriptRoot 'other.ps1')" 'the dot-source'
}

Test-Case 'psrows: a body digest is the file map''s, so a body_shape group is a duplicate_bodies group' {
    $body = "        `$a = Get-Content `$p`n        `$b = `$a.Trim()`n        return `$b`n"
    $tree = Use-Tree @{
        'svc.ps1' = "class Svc {`n    [string] Load([string]`$p) {`n$body    }`n}`n"
        'one.ps1' = "function Read-One(`$p) {`n$body}`n"
    }
    [void](Invoke-PsRowsMap $tree)
    $shapes = Get-PsRowsCells $tree "SELECT 'v=' || qualname || '=' || body_shape FROM functions WHERE body_shape <> '' ORDER BY qualname"
    Assert-Equal $shapes.Count 2 'two digests'
    Assert-Equal ($shapes[0].Split('=')[1]) ($shapes[1].Split('=')[1]) 'one digest for the method and the function'
    $map = [System.IO.File]::ReadAllText((Join-Path $tree 'm.json')) | ConvertFrom-Json
    $group = @($map.duplicate_bodies | ForEach-Object { ($_.at | Sort-Object) -join ',' })
    Assert-Equal ($group -join ';') 'one.ps1:1:Read-One,svc.ps1:2:Load' 'the JSON group'
}

Test-Case 'psrows: an unchanged run starts no host, and a file that has gone loses its rows' {
    $tree = Use-Tree @{ 'lib.ps1' = $script:PsRowsLibrary; 'two.ps1' = "function Get-Two { 2 }`n" }
    # THE HALVES LINE NAMES A HALF THAT RAN: the first run starts the host, the second, over the same tree, does not.
    $first = Invoke-PsRowsMap $tree
    Assert-Line $first 'powershell rows via'
    $again = Invoke-PsRowsMap $tree
    Assert-NoLine $again 'powershell rows via'
    Remove-Item (Join-Path $tree 'lib.ps1')
    [void](Invoke-PsRowsMap $tree)
    $rows = Get-PsRowsCells $tree "SELECT 'v=' || f.path || '|' || count(fn.id) FROM files f LEFT JOIN functions fn ON fn.file = f.id GROUP BY f.path"
    Assert-Equal ($rows -join ';') 'two.ps1|1' 'only the file still there'
    Remove-Item (Join-Path $tree 'two.ps1')
    [void](Invoke-PsRowsMap $tree)
    Assert-Equal (Get-MapQueryNumber (Join-Path $tree 'map.sqlite') --sql "SELECT count(*) FROM functions") '0' 'no functions left'
}

Test-Case 'psmap: a body with one literal changed and one statement added is a similar_bodies pair' {
    $tree = Use-Tree @{
        'a.ps1' = "function Get-Alpha(`$Path) {`n    `$text = Get-Content -Path `$Path -Raw`n    `$lines = `$text.Split(',')`n" +
            "    `$kept = `$lines | Where-Object { `$_.Length -gt 3 }`n    Write-Output (`$kept -join ',')`n}`n"
        'b.ps1' = "function Get-Beta(`$File) {`n    `$body = Get-Content -Path `$File -Raw`n    `$rows = `$body.Split(',')`n" +
            "    `$kept = `$rows | Where-Object { `$_.Length -gt 9 }`n    Write-Verbose 'beta'`n    Write-Output (`$kept -join ',')`n}`n"
    }
    $result = Invoke-Gate --root $tree --ext .ps1 --map --map-out (Join-Path $tree 'm.json')
    Assert-Exit $result 0
    $map = [System.IO.File]::ReadAllText((Join-Path $tree 'm.json')) | ConvertFrom-Json
    $pairs = @($map.similar_bodies | ForEach-Object { ($_.at | Sort-Object) -join ',' })
    Assert-Equal ($pairs -join ';') 'a.ps1:1:Get-Alpha,b.ps1:1:Get-Beta' 'the near-copy pair'
}
