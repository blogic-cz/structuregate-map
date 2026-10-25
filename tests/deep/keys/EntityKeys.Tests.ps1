<#
    A KEY FOR EVERY ENTITY, THE SAME IN EVERY BUILD (#130, `rust/fbtcore/src/rows/keys/`): `entity_keys` keys each file, C# type,
    member, method (its parameter types telling overloads apart), enum and enum member, SQL object and column from what the source
    declares - so a fresh map of a tree with one more file gives every old entity its old key while its row id moves - and `_meta`
    says which source and gate the map describes.

    Its helpers are its own - `-Only EntityKeys` runs this suite alone.
#>

$script:EntityKeysFiles = @{
    'app/App.csproj' = '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework></PropertyGroup></Project>'
    'app/OrderService.cs' = "namespace Demo;`npublic class OrderService`n{`n    public int Run(int a) => a;`n    public int Run(string s) => s.Length;`n    public string Name { get; set; } = `"`";`n}`npublic enum Hue { Red = 1, Blue = 2 }`n"
    'db/Sales/Sales.sqlproj' = '<Project><ItemGroup><Build Include="Customers.sql" /></ItemGroup></Project>'
    'db/Sales/Customers.sql' = "CREATE TABLE [Sales].[Customers] ([CustomerID] INT NOT NULL PRIMARY KEY, [Code] VARCHAR(20) NOT NULL);`n"
}

# Every key with the id it names, as `key|id` lines, for one map.
function Get-EntityKeysRows([string]$Db) {
    $r = Invoke-Gate --map-query $Db --width 0 --limit 0 --sql "SELECT 'k=' || key || '|' || id || '|' || tbl AS k FROM entity_keys ORDER BY key"
    Assert-Exit $r 0
    return @($r.Lines | Where-Object { $_.Trim().StartsWith('k=') } | ForEach-Object { $_.Trim().Substring(2) })
}

Test-Case 'entitykeys: a fresh map of a tree with one more file keeps every key while the row ids move (#130)' {
    $tree = Use-Tree $script:EntityKeysFiles
    $db = Join-Path $tree 'map.sqlite'
    Assert-Exit (Invoke-Gate --root $tree --ext '.cs,.sql' --map-sqlite $db) 0
    $first = Get-EntityKeysRows $db
    # OVERLOADS ARE TWO KEYS, by their parameter types; a property, an enum member and a SQL column each have their own.
    foreach ($key in 'cs:Demo.OrderService.Run(int)|', 'cs:Demo.OrderService.Run(string)|', 'cs:Demo.OrderService|', 'cs:Demo.OrderService.Name|',
        'cs:Demo.Hue.Red|', 'file:app/OrderService.cs|') {
        if (-not ($first | Where-Object { $_.StartsWith($key) })) { throw "no key $key in:`n$($first -join "`n")" }
    }
    if (-not ($first | Where-Object { $_.StartsWith('sql:Sales.Customers.Code') })) { throw "no SQL column key in:`n$($first -join "`n")" }
    # A LINK BY ROW ID IS ONE JOIN AWAY: the column's object, by key.
    $j = Invoke-Gate --map-query $db --width 0 --sql "SELECT 'obj=' || k.key AS o FROM sql_columns c JOIN entity_keys k ON k.id = c.object WHERE c.name = 'Code'"
    Assert-Line $j 'obj=sql:Sales.Customers'
    # A FILE SORTING FIRST, then a fresh map: the ids move, the keys do not.
    Set-Content -LiteralPath (Join-Path $tree 'app/Billing.cs') -Value "namespace Demo;`npublic class Billing { public int Total() => 1; }" -NoNewline
    Remove-Item -LiteralPath $db -Force
    Assert-Exit (Invoke-Gate --root $tree --ext '.cs,.sql' --map-sqlite $db) 0
    $second = Get-EntityKeysRows $db
    $keyOf = { param($rows) $map = @{}; foreach ($r in $rows) { $p = $r.Split('|'); $map[$p[0]] = $p[1] }; $map }
    $a = & $keyOf $first
    $b = & $keyOf $second
    foreach ($key in $a.Keys) { if (-not $b.ContainsKey($key)) { throw "key $key is gone after a fresh map" } }
    if ($a['cs:Demo.OrderService'] -eq $b['cs:Demo.OrderService']) { throw "the class's row id did not move - the case proves nothing" }
    if (-not $b.ContainsKey('cs:Demo.Billing.Total()')) { throw "the new method has no key:`n$($second -join "`n")" }
}

Test-Case 'entitykeys: _meta names the commit, the dirty flag, the gate and the build time (#130)' {
    $tree = Use-Tree $script:EntityKeysFiles
    $db = Join-Path $tree 'map.sqlite'
    & git -C $tree init -q 2>$null; & git -C $tree -c user.email=t@t -c user.name=t add app db 2>$null
    & git -C $tree -c user.email=t@t -c user.name=t commit -q -m one 2>$null
    $commit = (& git -C $tree rev-parse HEAD).Trim()
    Assert-Exit (Invoke-Gate --root $tree --ext '.cs,.sql' --map-sqlite $db) 0
    $m = Invoke-Gate --map-query $db --width 0 --sql "SELECT 'm=' || key || '|' || value AS m FROM _meta WHERE key LIKE 'source_%' OR key IN ('gate_version', 'built_at')"
    $root = (Resolve-Path $tree).Path
    Assert-Line $m "m=source_commit:$root|$commit"
    Assert-Line $m "m=source_dirty:$root|0"
    Assert-Line $m 'm=gate_version|dev'
    Assert-Line $m 'm=built_at|20'
    # A TRACKED FILE EDITED is a dirty tree.
    Add-Content -LiteralPath (Join-Path $tree 'app/OrderService.cs') -Value '// edited'
    Assert-Exit (Invoke-Gate --root $tree --ext '.cs,.sql' --map-sqlite $db) 0
    $m = Invoke-Gate --map-query $db --width 0 --sql "SELECT 'm=' || key || '|' || value AS m FROM _meta WHERE key LIKE 'source_dirty%'"
    Assert-Line $m "m=source_dirty:$root|1"
}
