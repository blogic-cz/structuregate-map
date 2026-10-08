<#
    The RUST half: `syn`, inside fbtcore, inside the exe. Two questions, as for every other language - how
    many source lines a file has, and what its file-level graph is.

    WHAT IS ASSERTED IS THE NUMBER AND THE EDGE. A count that is merely close is what the `//` scanner
    already gave rust before this half existed; the cases that matter are the ones where that scanner and a
    lexer disagree. The map cases pin which edges exist and which do not, because a `mod` declaration is an
    exact import and everything else is a name.

    Helpers here are prefixed `Rs`: suites share ONE scope, and `Get-Map` is the map suite's.
#>

function Get-RsMap {
    param([Parameter(ValueFromRemainingArguments)][object[]]$GateArgs)
    $path = Join-Path ([System.IO.Path]::GetTempPath()) "sgrs-$([System.Guid]::NewGuid().ToString('N').Substring(0,8)).json"
    # `--ext .rs` unless the case names its own - a mixed tree has to be asked for its other languages.
    $ext = if ($GateArgs -contains '--ext') { @() } else { @('--ext', '.rs') }
    $result = Invoke-Gate @GateArgs @ext --map --map-out $path
    if (-not (Test-Path $path)) { throw "no map was written. Output:`n$($result.Text)" }
    $map = Get-Content $path -Raw | ConvertFrom-Json
    Remove-Item $path -Force -ErrorAction SilentlyContinue
    return [pscustomobject]@{ Exit = $result.Exit; Lines = $result.Lines; Text = $result.Text; Map = $map }
}

# `,` so a one-edge answer stays an array - see Get-Measured in Assert.ps1.
function Get-RsImports($Map, [string]$Rel) {
    $property = $Map.imports.PSObject.Properties | Where-Object { $_.Name -eq $Rel }
    if (-not $property) { return ,@() }
    return ,@($property.Value)
}

# ---------------------------------------------------------------------------------------------------
# Counting - a line a TOKEN sits on, as Roslyn counts C#
# ---------------------------------------------------------------------------------------------------

Test-Case 'rs: doc comments, block comments and blanks are trivia; a lone closing brace is a line' {
    $tree = Use-Tree @{
        'a.rs' = @"
//! crate doc
/// item doc
/* block
   still block */

#[derive(Debug)]
pub struct A;

fn f() {
    1;
}
"@
    }
    $dump = Get-Dump --root $tree --ext .rs
    Assert-Equal (Get-Count $dump 'files' 'a.rs') 5 'a.rs source lines'
}

Test-Case 'rs: a string literal that spells a comment is source - the lexer, not a // scanner' {
    # The case the fallback scanner gets wrong: it drops the middle line because it starts with `//`.
    $tree = Use-Tree @{ 'a.rs' = "const A: &str = `"one`n// two`nthree`";`n" }
    $dump = Get-Dump --root $tree --ext .rs
    Assert-Equal (Get-Count $dump 'files' 'a.rs') 3 'a.rs source lines'
}

# ---------------------------------------------------------------------------------------------------
# The map - `mod` is an exact edge, the rest is a name
# ---------------------------------------------------------------------------------------------------

Test-Case 'rs map: a mod declaration is an edge to the file rustc would load, and the crate root is an entry' {
    $tree = Use-Tree @{
        'src/lib.rs'        = "mod scan;`nmod rows;`n"
        'src/scan.rs'       = "pub fn walk() {}`n"
        'src/rows/mod.rs'   = "mod half;`n"
        'src/rows/half.rs'  = "pub fn alone() {}`n"
    }
    $found = Get-RsMap --root $tree --map-check
    Assert-Exit $found 0
    Assert-Equal ((Get-RsImports $found.Map 'src/lib.rs') -join ',') 'src/rows/mod.rs,src/scan.rs' 'lib.rs pulls in both'
    Assert-Equal ((Get-RsImports $found.Map 'src/rows/mod.rs') -join ',') 'src/rows/half.rs' 'a mod.rs owns its folder'
    Assert-NoLine $found 'NO READER src/lib.rs'
    Assert-NoLine $found 'NO READER src/scan.rs'
}

Test-Case 'rs map: a #[path] attribute moves the file and the edge follows it' {
    $tree = Use-Tree @{
        'src/lib.rs'        = "#[path = `"gate/values.rs`"]`nmod values;`n"
        'src/gate/values.rs' = "pub fn permit() {}`n"
    }
    $found = Get-RsMap --root $tree --map-check
    Assert-Exit $found 0
    Assert-Equal ((Get-RsImports $found.Map 'src/lib.rs') -join ',') 'src/gate/values.rs' 'lib.rs imports the moved file'
}

Test-Case 'rs map: a mod that names no file is BROKEN and fails --map-check' {
    $tree = Use-Tree @{ 'src/main.rs' = "mod gone;`nfn main() {}`n" }
    $found = Get-RsMap --root $tree --map-check
    Assert-Exit $found 1
    Assert-Line $found 'src/main.rs: imports src/gone.rs, which is not a file in the mapped tree'
}

Test-Case 'rs map: a use of a PUB item is an edge, and a private item draws none' {
    $tree = Use-Tree @{
        'src/lib.rs'    = "mod store;`nmod reader;`nmod quiet;`n"
        'src/store.rs'  = "pub struct Store;`n"
        'src/quiet.rs'  = "struct Hidden;`n"
        'src/reader.rs' = "use crate::store::Store;`nfn go() -> Store { let _h = Hidden; Store }`n"
    }
    $found = Get-RsMap --root $tree
    Assert-Exit $found 0
    Assert-Equal ((Get-RsImports $found.Map 'src/reader.rs') -join ',') 'src/store.rs' 'reader imports store and not quiet'
}

Test-Case 'rs map: a method call that spells a free function''s name is not an edge' {
    # `x.walk()` names a method of whatever `x` is - the member rule every half keeps.
    $tree = Use-Tree @{
        'src/lib.rs'    = "mod scan;`nmod reader;`n"
        'src/scan.rs'   = "pub fn walk() {}`n"
        'src/reader.rs' = "fn go(x: Vec<u8>) { x.walk(); }`n"
    }
    $found = Get-RsMap --root $tree
    Assert-Equal (Get-RsImports $found.Map 'src/reader.rs').Count 0 'reader imports nothing'
}

Test-Case 'rs map: a rust item does not make a C# name ambiguous - the languages join apart' {
    # Found on this repo's own tree: `pub struct Check` in a rust file made the C# class `Check`
    # ambiguous, the edge was dropped, and Check.cs was reported as having no reader.
    $tree = Use-Tree @{
        'Reader.cs'  = "namespace N;`npublic static class Reader { public static int Go() => Check.Value; }`n"
        'Check.cs'   = "namespace N;`npublic static class Check { public static int Value => 1; }`n"
        'src/lib.rs' = "pub struct Check;`n"
    }
    $found = Get-RsMap --root $tree --ext '.cs,.rs' --map-check
    Assert-Equal ((Get-RsImports $found.Map 'Reader.cs') -join ',') 'Check.cs' 'Reader imports Check'
    Assert-NoLine $found 'AMBIGUOUS Check'
}

Test-Case 'rs map: a file that does not PARSE fails --map-check' {
    $tree = Use-Tree @{ 'src/lib.rs' = "pub fn a() {}`npub fn b( {}`n" }
    $found = Get-RsMap --root $tree --map-check
    Assert-Exit $found 1
    Assert-Line $found 'UNPARSED  src/lib.rs:2: does not parse as rust'
}

Test-Case 'rs map: two identical bodies are ONE duplicate group, local names and comments aside' {
    $a = "pub fn one(p: &str) -> usize {`n    let x = p.len();`n    let y = x + 1;`n    y * 2`n}`n"
    $b = "pub fn two(q: &str) -> usize {`n    // why`n    let m = q.len();`n    let n = m + 1;`n    n * 2`n}`n"
    $tree = Use-Tree @{ 'src/lib.rs' = "mod a;`nmod b;`n"; 'src/a.rs' = $a; 'src/b.rs' = $b }
    $found = Get-RsMap --root $tree --map-check
    Assert-Line $found 'DUPLICATE 2 function bodies are identical'
    Assert-Line $found 'src/a.rs:1:one'
}

# ---------------------------------------------------------------------------------------------------
# The deep map - `consts`, in the columns the C# and python halves write
# ---------------------------------------------------------------------------------------------------

# One `v=` cell out of a deep map, the way the deep suites read one.
function Get-RsDeep([string]$Db, [string]$Sql) {
    $r = Invoke-Gate --map-query $Db --sql $Sql --width 0
    Assert-Exit $r 0
    $line = $r.Lines | Where-Object { $_.TrimStart().StartsWith('v=') } | Select-Object -First 1
    if ($null -eq $line) { return '' }
    return $line.Trim().Substring(2)
}

Test-Case 'rs rows: every const and static is a consts row - value, type, owner, file - and a local or a fixture is not' {
    $tree = Use-Tree @{
        'src/lib.rs'    = "pub mod limits;`n"
        'src/limits.rs' = "pub const MAX: usize = 450;`nstatic NAME: &str = `"gate`";`npub const SUM: usize = MAX * 2;`n" +
                          "pub struct Cfg;`nimpl Cfg { pub const STEP: i32 = -3; }`n" +
                          "fn f() { const LOCAL: u8 = 1; }`n#[cfg(test)]`nmod tests { const FIXTURE: u8 = 2; }`n"
    }
    $db = Join-Path $tree 'map.sqlite'
    Assert-Exit (Invoke-Gate --root $tree --ext .rs --map-sqlite $db) 0
    $all = "SELECT 'v=' || group_concat(name, ',') FROM (SELECT k.name FROM consts k ORDER BY k.line)"
    Assert-Equal (Get-RsDeep $db $all) 'MAX,NAME,SUM,STEP' 'the constants, and neither the local nor the fixture'
    $max = "SELECT 'v=' || k.kind || '|' || k.type || '|' || k.value || '|' || k.exported || '|' || f.path || '|' || f.lang " +
           "FROM consts k JOIN files f ON f.id = k.file WHERE k.name = 'MAX'"
    Assert-Equal (Get-RsDeep $db $max) 'const|usize|450|1|src/limits.rs|rust' 'MAX'
    $name = "SELECT 'v=' || kind || '|' || value || '|' || exported FROM consts WHERE name = 'NAME'"
    Assert-Equal (Get-RsDeep $db $name) 'static|gate|0' 'NAME, a private static'
    $sum = "SELECT 'v=' || source || '|' || value || '|' || reads FROM consts WHERE name = 'SUM'"
    Assert-Equal (Get-RsDeep $db $sum) 'MAX * 2||["MAX"]' 'SUM is an expression: no value, and it reads MAX'
    Assert-Equal (Get-RsDeep $db "SELECT 'v=' || owner || '|' || value FROM consts WHERE name = 'STEP'") 'Cfg|-3' 'STEP belongs to Cfg'
}

Test-Case 'rs rows: an edit replaces that file''s rows only, and the last .rs gone takes the rust rows and leaves the rest' {
    $tree = Use-Tree @{ 'src/a.rs' = "pub const A: u8 = 1;`npub const B: u8 = 2;`n"; 'src/b.rs' = "pub const C: u8 = 3;`n"
                        'Other.cs' = "namespace N; public static class Other { public const int D = 4; }`n" }
    $db = Join-Path $tree 'map.sqlite'
    Assert-Exit (Invoke-Gate --root $tree --ext '.rs,.cs' --map-sqlite $db) 0
    $c = Get-RsDeep $db "SELECT 'v=' || id FROM consts WHERE name = 'C'"
    [System.IO.File]::WriteAllText((Join-Path $tree 'src/a.rs'), "pub const A: u8 = 9;`n")
    Assert-Exit (Invoke-Gate --root $tree --ext '.rs,.cs' --map-sqlite $db) 0
    Assert-Equal (Get-RsDeep $db "SELECT 'v=' || group_concat(name || '=' || value, ',') FROM (SELECT k.* FROM consts k JOIN files f ON f.id = k.file WHERE f.lang = 'rust' ORDER BY k.name)") 'A=9,C=3' 'the rust constants after the edit'
    Assert-Equal (Get-RsDeep $db "SELECT 'v=' || id FROM consts WHERE name = 'C'") $c 'the untouched file keeps its row'
    Remove-Item (Join-Path $tree 'src') -Recurse -Force
    Assert-Exit (Invoke-Gate --root $tree --ext '.rs,.cs' --map-sqlite $db) 0
    Assert-Equal (Get-RsDeep $db "SELECT 'v=' || count(*) FROM files WHERE lang = 'rust'") '0' 'no rust file is recorded'
    Assert-Equal (Get-RsDeep $db "SELECT 'v=' || count(*) FROM consts k JOIN files f ON f.id = k.file WHERE f.lang = 'rust'") '0' 'no rust constant is left'
    Assert-Equal (Get-RsDeep $db "SELECT 'v=' || group_concat(name) FROM consts") 'D' 'the C# constant stayed'
}

Test-Case 'rs rows: a file that does not parse keeps its files row, marked, and fails the deep map run' {
    $tree = Use-Tree @{ 'src/lib.rs' = "pub const A: u8 = 1;`npub fn b( {}`n" }
    $db = Join-Path $tree 'map.sqlite'
    $result = Invoke-Gate --root $tree --ext .rs --map-sqlite $db
    Assert-Exit $result 1
    Assert-Line $result 'src/lib.rs:2: does not parse as rust'
    Assert-Equal (Get-RsDeep $db "SELECT 'v=' || errors FROM files WHERE path = 'src/lib.rs'") '1' 'the file is recorded, with its error'
}

Test-Case 'rs rows: every way rust deals with an error is a handlers row, beside C#''s catch in the same table' {
    $nl = [string][char]10
    $tree = Use-Tree @{
        'src/lib.rs' = "pub fn open(p: &str) -> Result<u8, E> {" + $nl +
                       "    let Ok(f) = load(p) else { return Ok(0) };" + $nl +
                       "    match f.read() {" + $nl + "        Ok(v) => keep(v)," + $nl +
                       "        Err(Kind::Gone) => {}," + $nl + "        Err(e) => return Err(e)," + $nl + "    }" + $nl +
                       "    let n = parse(p)?;" + $nl + "    let _ = flush();" + $nl +
                       "    let d = n.checked_add(1).unwrap_or_default();" + $nl + "    Ok(d)" + $nl + "}" + $nl +
                       "#[cfg(test)]" + $nl + "mod tests { #[test] fn t() { super::open(`"x`").unwrap(); } }" + $nl
        'Other.cs'   = "namespace N; public static class Other { public static void Go() { try { } catch (System.Exception ex) { } } }" + $nl
    }
    $db = Join-Path $tree 'map.sqlite'
    Assert-Exit (Invoke-Gate --root $tree --ext '.rs,.cs' --map-sqlite $db) 0
    $rust = "SELECT 'v=' || group_concat(r, ' ') FROM (SELECT h.line || ':' || h.shape || ':' || h.name || ':' || h.bare || h.passes || " +
            "h.reraises || h.panics || h.test || ':' || h.func AS r FROM handlers h JOIN files f ON f.id = h.file " +
            "WHERE f.lang = 'rust' ORDER BY h.line, h.shape)"
    Assert-Equal (Get-RsDeep $db $rust) ('2:let_else::10000:open 5:match::01000:open 6:match:e:10100:open 8:question::10100:open ' +
        '9:discard::11000:open 10:fallback::10000:open 14:unwrap::10011:t') 'line:shape:name:bare passes reraises panics test:func'
    Assert-Equal (Get-RsDeep $db "SELECT 'v=' || types FROM handlers WHERE line = 5 AND shape = 'match'") '["Kind::Gone"]' 'what the arm catches'
    $sharp = "SELECT 'v=' || count(*) FROM handlers h JOIN files f ON f.id = h.file WHERE f.lang = 'csharp'"
    Assert-Equal (Get-RsDeep $db $sharp) '1' 'the C# catch is in the same table'
}
