<#
    THE GATE'S OWN SOURCES, ASKED THROUGH ITS OWN DEEP MAP. Two rules no grep can hold, because a grep cannot
    tell a string the code runs from a comment explaining why it must not:

    * NO JSON IN THE MAP. A list or an object is NODE ROWS (`rust/fbtcore/src/rows/cells/mod.rs`), so no query of
      this repo may need a SQLite JSON function: a string literal naming one is a reader still treating a cell
      as JSON text.
    * NO CAP ON A PARSE. A constant named for a depth (`*DEPTH*`, `*LEVELS*`) and a literal compared with a
      `depth` (or a bare `level`) are how a tree was cut short without a word - python's `node.level`, an import's relative level, is not one; a walk that could loop takes a visited set.

    The rules are proved on a planted tree first, then held over the repo. Its helpers are its own.
#>

# The SQLite JSON functions, as a query would call them. `->`/`->>` need a JSON text on the left, which a node
# column never is.
$script:SelfMapJsonCalls = @('json(', 'json_array(', 'json_array_length(', 'json_each(', 'json_error_position(',
    'json_extract(', 'json_group_array(', 'json_group_object(', 'json_insert(', 'json_object(', 'json_patch(',
    'json_pretty(', 'json_quote(', 'json_remove(', 'json_replace(', 'json_set(', 'json_tree(', 'json_type(',
    'json_valid(', 'jsonb(', 'jsonb_')

# Every violation the deep map at $Db holds, one `<rule> <path>:<line> <what>` per line.
function Get-SelfMapViolation([string]$Db) {
    $json = ($script:SelfMapJsonCalls | ForEach-Object { "instr(lower(s.value), '$_') > 0" }) -join ' OR '
    $queries = @(
        "SELECT 'v=json ' || f.path || ':' || s.line || ' ' || substr(s.value, 1, 60) FROM string_literals s JOIN files f ON f.id = s.file WHERE $json",
        "SELECT 'v=cap ' || f.path || ':' || c.line || ' ' || c.name FROM consts c JOIN files f ON f.id = c.file WHERE instr(upper(c.name), 'DEPTH') > 0 OR instr(upper(c.name), 'LEVELS') > 0",
        "SELECT 'v=cap ' || f.path || ':' || n.line || ' ' || n.target || ' vs ' || n.value FROM number_literals n JOIN files f ON f.id = n.file WHERE n.use = 'compare' AND (instr(lower(n.target), 'depth') > 0 OR lower(n.target) IN ('level', 'levels'))"
    )
    $found = @()
    foreach ($sql in $queries) {
        $r = Invoke-Gate --map-query $Db --sql $sql --width 100000
        Assert-Exit $r 0
        $found += @($r.Lines | Where-Object { $_.TrimStart().StartsWith('v=') } | ForEach-Object { $_.Trim().Substring(2) })
    }
    return $found
}

Test-Case 'selfmap: a JSON function, a depth constant and a depth comparison are each found' {
    $nl = [string][char]10
    $tree = Use-Tree @{
        'lib.rs' = "const MAX_DEPTH: usize = 4;$nl" +
            "pub fn q() -> &'static str { `"SELECT value FROM json_each(t.reads)`" }$nl" +
            "pub fn walk(depth: usize) -> bool { depth > 60 }$nl"
    }
    $db = Join-Path $tree 'self.sqlite'
    Assert-Exit (Invoke-Gate --root $tree --ext .rs --map-sqlite $db) 0
    $found = Get-SelfMapViolation $db
    $text = $found -join "`n"
    if (-not ($found | Where-Object { $_.StartsWith('json lib.rs:2') })) { throw "the json_each literal was not found:`n$text" }
    if (-not ($found | Where-Object { $_.StartsWith('cap lib.rs:1 MAX_DEPTH') })) { throw "the depth constant was not found:`n$text" }
    if (-not ($found | Where-Object { $_.StartsWith('cap lib.rs:3 depth') })) { throw "the depth comparison was not found:`n$text" }
}

Test-Case 'selfmap: the gate''s own sources use no JSON function on the map and cap no parse' {
    $out = New-Tree @{}
    $db = Join-Path $out 'self.sqlite'
    $roots = @('rust/fbtcore/src', 'rust/fbt/src', 'src', 'scripts') | ForEach-Object { '--root'; (Join-Path $script:Root $_) }
    $modules = Get-TsModules
    $ts = if ($modules) { @('--ts-node-modules', $modules) } else { @() }
    Assert-Exit (Invoke-Gate @roots --ext '.rs,.cs,.mjs,.py' --map-sqlite $db @ts) 0
    $found = Get-SelfMapViolation $db
    if ($found) { throw "$($found.Count) violation(s):`n$($found -join "`n")" }
}
