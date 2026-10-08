<#
    --map end to end: the walk, the three language halves, the join, the findings and the JSON.

    WHAT IS ASSERTED IS THE EDGE, not the sentence about it. An import graph that is merely plausible is
    worse than none: every claim made from it - what would break if I move this, is this file dead - is acted
    on directly. So the cases pin which edges exist, which ones deliberately do NOT (an ambiguous name, a
    `using` segment, a member after a dot), and what the map says when a half cannot run at all.

    The python cases need a `python` on PATH and the TypeScript ones a resolvable compiler; both are skipped
    with a printed line rather than silently, because a suite that quietly stops covering a half is the
    failure this whole tool is about.
#>

. (Join-Path $PSScriptRoot 'map/Map.Helpers.ps1')

# ---------------------------------------------------------------------------------------------------
# C#: an edge is a type reference, and the two things that are deliberately not one
# ---------------------------------------------------------------------------------------------------

Test-Case 'map: a C# file that names another file''s type imports it' {
    $tree = Use-Tree @{
        'Reader.cs' = "namespace N;`npublic static class Reader { public static int Go() => Store.Value; }`n"
        'Store.cs'  = "namespace N;`npublic static class Store { public static int Value => 1; }`n"
    }
    $found = Get-Map --root $tree
    Assert-Exit $found 0
    Assert-Equal (Get-Imports $found.Map 'Reader.cs') 'Store.cs' 'Reader imports Store'
    Assert-Equal $found.Map.imported_by.'Store.cs' 'Reader.cs' 'Store is imported by Reader'
}

Test-Case 'map: a MEMBER that happens to spell a type name is not an edge' {
    # `thing.Store` names a property, not the file below. Counting it would invent an edge from a name that
    # only looks like a type - the failure mode a grep has and this graph exists not to have.
    $tree = Use-Tree @{
        'User.cs'  = "namespace N;`npublic class User { public int Go(Bag b) { return b.Store; } }`n"
        'Bag.cs'   = "namespace N;`npublic class Bag { public int Store => 1; }`n"
        'Store.cs' = "namespace N;`npublic class Store { }`n"
    }
    $found = Get-Map --root $tree
    Assert-Equal (Get-Imports $found.Map 'User.cs') 'Bag.cs' 'only the parameter type is an edge'
}

Test-Case 'map: a name declared TWICE draws no edge and is reported' {
    $tree = Use-Tree @{
        'Use.cs' = "namespace N;`npublic class Use { public Result Go() => default; }`n"
        'A.cs'   = "namespace A;`npublic class Result { }`n"
        'B.cs'   = "namespace B;`npublic class Result { }`n"
    }
    $found = Get-Map --root $tree --map-check
    Assert-Exit $found 0
    Assert-Equal (Get-Imports $found.Map 'Use.cs').Count 0 'no edge is guessed'
    Assert-Line $found 'AMBIGUOUS Result'
    Assert-Equal $found.Map.ambiguous.Result.Count 2 'both candidates are named'
}

Test-Case 'map: a using directive segment is not an edge' {
    $tree = Use-Tree @{
        'Uses.cs'  = "using Store.Deep;`nnamespace N;`npublic class Uses { }`n"
        'Store.cs' = "namespace N;`npublic class Store { }`n"
    }
    $found = Get-Map --root $tree
    Assert-Equal (Get-Imports $found.Map 'Uses.cs').Count 0 'a namespace segment is not a type reference'
}

Test-Case 'map: the file headline comes off the doc comment' {
    $tree = Use-Tree @{ 'A.cs' = "namespace N;`n/// <summary>`n/// What this file is for.`n/// </summary>`npublic class A { }`n" }
    $found = Get-Map --root $tree
    Assert-Equal $found.Map.files.'A.cs'.summary 'What this file is for.' 'summary'
    Assert-Equal $found.Map.files.'A.cs'.language 'csharp' 'language'
}

# ---------------------------------------------------------------------------------------------------
# The findings, and which of them may fail a gate
# ---------------------------------------------------------------------------------------------------

Test-Case 'map: a type named by REFLECTION with a literal is a real edge' {
    # The only C# blind spot the other three halves did not have: `Type.GetType("Reader")` reaches the same
    # file `new Reader()` would, and the identifier walk cannot see inside a string.
    $tree = Use-Tree @{
        'Loader.cs' = "namespace N;`npublic class Loader { public object Go() => System.Type.GetType(""Reader""); }`n"
        'Reader.cs' = "namespace N;`npublic class Reader { }`n"
    }
    $found = Get-Map --root $tree --map-check
    Assert-Exit $found 0
    Assert-Equal (Get-Imports $found.Map 'Loader.cs') 'Reader.cs' 'the literal type name is an edge'
    Assert-NoLine $found 'NO READER Reader.cs'
}

Test-Case 'map: an ASSEMBLY-QUALIFIED and a generic reflection name are reduced to the declared name' {
    $tree = Use-Tree @{
        'Loader.cs' = "namespace N;`npublic class Loader { public object Go() => System.Type.GetType(""N.Deep.Reader, MyAsm""); }`n"
        'Reader.cs' = "namespace N.Deep;`npublic class Reader { }`n"
    }
    $found = Get-Map --root $tree
    Assert-Equal (Get-Imports $found.Map 'Loader.cs') 'Reader.cs' 'namespace and assembly are stripped'
}

Test-Case 'map: a C# reflection name built at RUN TIME is counted, and typeof is not' {
    # `typeof(X)` is already an edge from the identifier walk. Counting it as computed would inflate the
    # number the dead-file finding is qualified by.
    $tree = Use-Tree @{
        'Loader.cs' = "namespace N;`npublic class Loader {`n  public object Go(string n) => System.Type.GetType(n);`n  public object Fixed() => System.Activator.CreateInstance(typeof(Reader));`n}`n"
        'Reader.cs' = "namespace N;`npublic class Reader { }`n"
    }
    $found = Get-Map --root $tree --map-check
    Assert-Exit $found 0
    Assert-Equal $found.Map.computed_imports.Count 1 'only the run-time name is counted'
    Assert-Line $found 'Type.GetType() with a name built at run time'
    Assert-Equal (Get-Imports $found.Map 'Loader.cs') 'Reader.cs' 'typeof still draws its edge'
}

Test-Case 'map: a file nothing imports is a NOTE, and an entry point is not one' {
    $tree = Use-Tree @{
        'Dead.cs'    = "namespace N;`npublic class Dead { public int A() => 1; }`n"
        'Program.cs' = "System.Console.WriteLine(1);`n"
    }
    $found = Get-Map --root $tree --map-check
    Assert-Exit $found 0
    Assert-Line $found 'NO READER Dead.cs'
    Assert-NoLine $found 'NO READER Program.cs'
}

Test-Case 'map: a file that does not PARSE fails --map-check' {
    # Roslyn error-recovers into a PARTIAL tree, so every edge read out of such a file is a guess. That is
    # the one C# state here that cannot be legitimate.
    $tree = Use-Tree @{ 'Broken.cs' = "namespace N;`npublic class Broken { public void A( { }`n" }
    $found = Get-Map --root $tree --map-check
    Assert-Exit $found 1
    Assert-Line $found 'UNPARSED  Broken.cs'
    Assert-Line $found 'error:'
}

Test-Case 'map: an import CYCLE is reported and does not fail the check' {
    $tree = Use-Tree @{
        'A.cs' = "namespace N;`npublic class A { public B Next() => null; }`n"
        'B.cs' = "namespace N;`npublic class B { public A Back() => null; }`n"
    }
    $found = Get-Map --root $tree --map-check
    Assert-Exit $found 0
    Assert-Line $found 'CYCLE'
    Assert-Equal $found.Map.cycles.Count 1 'one cycle, not one per member'
}

Test-Case 'map: two identical bodies are ONE duplicate group, comments and names aside' {
    $body = "var a = 1;`n        var b = a + 2;`n        return a + b;"
    $tree = Use-Tree @{
        'One.cs' = "namespace N;`npublic class One { public int First() {`n        $body`n    } }`n"
        'Two.cs' = "namespace N;`npublic class Two { public int Second() {`n        // a comment the other copy does not have`n        $body`n    } }`n"
    }
    $found = Get-Map --root $tree --map-check
    Assert-Exit $found 0
    Assert-Equal $found.Map.duplicate_bodies.Count 1 'one group'
    Assert-Line $found 'DUPLICATE 2 function bodies'
}

Test-Case 'map: a duplicate is sized in SOURCE CHARACTERS, not in whatever the parser counts' {
    # Four halves, four native units - C# tokens, python AST nodes, PowerShell tokens, TypeScript leaves -
    # and one ranked list of groups. Four units cannot be sorted together, so the threshold stays native and
    # the REPORTED size is a source span. Long names make the span large while the token count stays small,
    # which is exactly what tells the two apart.
    $body = "        var theFirstAccumulatedValue = 1;`n        var theSecondAccumulatedValue = theFirstAccumulatedValue + 2;`n        return theFirstAccumulatedValue + theSecondAccumulatedValue;"
    $tree = Use-Tree @{
        'One.cs' = "namespace N;`npublic class One { public int First() {`n$body`n    } }`n"
        'Two.cs' = "namespace N;`npublic class Two { public int Second() {`n$body`n    } }`n"
    }
    $found = Get-Map --root $tree
    Assert-Equal $found.Map.duplicate_bodies.Count 1 'one group'
    # ~20 tokens, ~180 characters: only a character measure can be this big.
    if ([int]$found.Map.duplicate_bodies[0].size -lt 100) {
        throw "size $($found.Map.duplicate_bodies[0].size) looks like a token count, not a source span"
    }
}

Test-Case 'map: a body under the statement threshold is NOT a duplicate' {
    $tree = Use-Tree @{
        'One.cs' = "namespace N;`npublic class One { public int A() { return 1; } }`n"
        'Two.cs' = "namespace N;`npublic class Two { public int B() { return 1; } }`n"
    }
    $found = Get-Map --root $tree
    Assert-Equal $found.Map.duplicate_bodies.Count 0 'two one-line wrappers are alike by coincidence'
}

# ---------------------------------------------------------------------------------------------------
# The mode itself: where the map goes, when it is rebuilt, and what the flags refuse
# ---------------------------------------------------------------------------------------------------

Test-Case 'map: --map alone writes buildmap.json beside the tree' {
    $tree = Use-Tree @{ 'A.cs' = "namespace N;`npublic class A { }`n" }
    $result = Invoke-Gate --root $tree --map
    Assert-Exit $result 0
    if (-not (Test-Path (Join-Path $tree 'buildmap.json'))) { throw "no buildmap.json. Output:`n$($result.Text)" }
}

Test-Case 'map: --map-if-stale re-parses when a file is newer, and does nothing when none is' {
    $tree = Use-Tree @{ 'A.cs' = "namespace N;`npublic class A { }`n" }
    $path = Join-Path $tree 'map.json'
    Assert-Exit (Invoke-Gate --root $tree --map --map-out $path) 0
    $again = Invoke-Gate --root $tree --map --map-out $path --map-if-stale
    Assert-Exit $again 0
    Assert-Line $again 'nothing re-parsed'

    Start-Sleep -Milliseconds 1100
    [System.IO.File]::WriteAllText((Join-Path $tree 'A.cs'), "namespace N;`npublic class A { public int B; }`n")
    $fresh = Invoke-Gate --root $tree --map --map-out $path --map-if-stale
    Assert-Exit $fresh 0
    Assert-NoLine $fresh 'nothing re-parsed'
    Assert-Line $fresh 'wrote'
}

# A CURRENT MAP IS STILL CHECKED: every hook Connect-Gate writes passes --map-if-stale --map-check, and a
# map kept because nothing moved was passed without a look - so a red tree went green on the very next turn.
Test-Case 'map: --map-if-stale still checks a map it keeps, and a newer map baseline re-parses' {
    if (-not $script:Python) { return }
    $tree = Use-Tree @{ 'bad.py' = "def f(:`n    pass`n"; 'ok.py' = "X = 1`n" }
    $path = Join-Path $tree 'map.json'
    $first = Invoke-Gate --root $tree --ext .py --map --map-out $path --map-if-stale --map-check
    Assert-Exit $first 1
    Assert-Line $first 'UNPARSED  bad.py'
    $kept = Invoke-Gate --root $tree --ext .py --map --map-out $path --map-if-stale --map-check
    Assert-Line $kept 'nothing re-parsed'
    Assert-Exit $kept 1
    Assert-Line $kept 'error: UNPARSED  bad.py'
    # Without --map-check a kept map is still just kept - the check is what was asked for, not a re-parse.
    Assert-Exit (Invoke-Gate --root $tree --ext .py --map --map-out $path --map-if-stale) 0

    # THE RATCHET'S FINDINGS WERE JUDGED AGAINST THE BASELINE AS IT WAS: a baseline written after the map makes the
    # kept findings stale, so the map is parsed again rather than replayed.
    $baseline = Join-Path $tree 'map-baseline.json'
    Start-Sleep -Milliseconds 1100
    [System.IO.File]::WriteAllText($baseline, '{"unread":{},"computed":{}}')
    $rebaselined = Invoke-Gate --root $tree --ext .py --map --map-out $path --map-if-stale --map-check --map-baseline $baseline
    Assert-NoLine $rebaselined 'nothing re-parsed'
}

Test-Case 'map: a map flag without --map is refused, not ignored' {
    # A hook whose --map-check was silently dropped reports OK on a check it never made.
    $tree = Use-Tree @{ 'A.cs' = "namespace N;`npublic class A { }`n" }
    $result = Invoke-Gate --root $tree --map-check
    Assert-Exit $result 2
    Assert-Line $result 'need --map'
}

Test-Case 'map: a language with no parser here is LISTED as unmapped, never dropped' {
    $tree = Use-Tree @{ 'a.rb' = "class A`nend`n"; 'B.cs' = "namespace N;`npublic class B { }`n" }
    $found = Get-Map --root $tree --ext '.cs,.rb' --map-check
    Assert-Exit $found 0
    Assert-Line $found 'UNMAPPED  a.rb'
    Assert-Equal $found.Map.files.'a.rb'.language 'rb' 'the file is still in the map'
}

# ---------------------------------------------------------------------------------------------------
# --map-baseline - the ratchet over dynamic code
# ---------------------------------------------------------------------------------------------------

# A tree whose only run-time-named import is one `importlib` call, plus the baseline path beside it.
function New-DynamicTree([int]$Calls = 1) {
    $body = "import importlib" + [char]10
    for ($i = 1; $i -le $Calls; $i++) { $body += "m$i = importlib.import_module(name$i)" + [char]10 }
    return Use-Tree @{ 'app.py' = $body; 'plugin.py' = "X = 1" + [char]10 }
}

# A tree with one entry point, one module it imports, and one nothing imports at all.
function New-UnreadTree {
    return Use-Tree @{
        'app.py'    = "import lib" + [char]10 + "if __name__ == '__main__':" + [char]10 + "    lib.f()" + [char]10
        'lib.py'    = "def f():" + [char]10 + "    return 1" + [char]10
        'orphan.py' = "def g():" + [char]10 + "    return 2" + [char]10
    }
}

Test-Case 'map: --update-map-baseline records the files nothing imports' {
    $tree = New-UnreadTree
    $baseline = Join-Path $tree 'dynamic.json'
    $result = Invoke-Gate --root $tree --ext .py --map --map-out (Join-Path $tree 'm.json') `
        --map-baseline $baseline --update-map-baseline
    Assert-Exit $result 0
    Assert-Line $result 'file(s) nothing imports'
    $recorded = Get-Content $baseline -Raw | ConvertFrom-Json
    $listed = @($recorded.unread.PSObject.Properties.Name)
    Assert-Equal ($listed -join ',') 'orphan.py' 'only the orphan - an entry point is not unread, and lib has a reader'
}

Test-Case 'map: a recorded unread file passes, a NEW one fails' {
    $tree = New-UnreadTree
    $baseline = Join-Path $tree 'dynamic.json'
    Assert-Exit (Invoke-Gate --root $tree --ext .py --map --map-out (Join-Path $tree 'm.json') `
        --map-baseline $baseline --update-map-baseline) 0
    $held = Get-Map --root $tree --ext .py --map-check --map-baseline $baseline
    Assert-Exit $held 0
    Assert-NoLine $held 'UNREAD'

    [System.IO.File]::WriteAllText((Join-Path $tree 'stray.py'), "def h():" + [char]10 + "    return 3" + [char]10)
    $grown = Get-Map --root $tree --ext .py --map-check --map-baseline $baseline
    Assert-Exit $grown 1
    Assert-Line $grown 'UNREAD'
    Assert-Line $grown 'stray.py'
    Assert-Line $grown 'nothing in the tree imports it and nothing records that'
}

Test-Case 'map: an unread file that GAINS a reader must leave the baseline' {
    # The rule that stops the list being a permanent exemption: it may only shrink, and shrinking has to be
    # recorded or the next reader of the file has no idea it was ever in question.
    $tree = New-UnreadTree
    $baseline = Join-Path $tree 'dynamic.json'
    Assert-Exit (Invoke-Gate --root $tree --ext .py --map --map-out (Join-Path $tree 'm.json') `
        --map-baseline $baseline --update-map-baseline) 0
    [System.IO.File]::WriteAllText((Join-Path $tree 'app.py'),
        "import lib" + [char]10 + "import orphan" + [char]10 +
        "if __name__ == '__main__':" + [char]10 + "    lib.f()" + [char]10 + "    orphan.g()" + [char]10)
    $fixed = Get-Map --root $tree --ext .py --map-check --map-baseline $baseline
    Assert-Exit $fixed 1
    Assert-Line $fixed 'something imports it now'
    Assert-Line $fixed 'remove it from dynamic.json'
}

Test-Case 'map: both lists live in ONE baseline file and are judged together' {
    $tree = Use-Tree @{
        'app.py'    = "import importlib" + [char]10 + "import lib" + [char]10 +
                      "m = importlib.import_module(n)" + [char]10 +
                      "if __name__ == '__main__':" + [char]10 + "    lib.f()" + [char]10
        'lib.py'    = "def f():" + [char]10 + "    return 1" + [char]10
        'orphan.py' = "def g():" + [char]10 + "    return 2" + [char]10
    }
    $baseline = Join-Path $tree 'dynamic.json'
    Assert-Exit (Invoke-Gate --root $tree --ext .py --map --map-out (Join-Path $tree 'm.json') `
        --map-baseline $baseline --update-map-baseline) 0
    $recorded = Get-Content $baseline -Raw | ConvertFrom-Json
    Assert-Equal @($recorded.computed.PSObject.Properties.Name).Count 1 'the dynamic import is recorded'
    Assert-Equal @($recorded.unread.PSObject.Properties.Name).Count 1 'and the unread file, in the same file'
    Assert-Exit (Get-Map --root $tree --ext .py --map-check --map-baseline $baseline) 0
}

Test-Case 'map: --update-map-baseline records every run-time-named site' {
    $tree = New-DynamicTree 2
    $baseline = Join-Path $tree 'dynamic.json'
    $result = Invoke-Gate --root $tree --ext .py --map --map-out (Join-Path $tree 'm.json') `
        --map-baseline $baseline --update-map-baseline
    Assert-Exit $result 0
    Assert-Line $result 'map baseline written'
    $recorded = Get-Content $baseline -Raw | ConvertFrom-Json
    # Keyed by FILE AND SHAPE, never by line: an edit above a dynamic call must not move it.
    $key = $recorded.computed.PSObject.Properties.Name | Select-Object -First 1
    Assert-Equal $key 'app.py: importlib.import_module() with a name built at run time' 'the site key carries no line'
    Assert-Equal $recorded.computed.$key 2 'both calls counted'
}

Test-Case 'map: a recorded site passes, and NEW dynamic code fails' {
    $tree = New-DynamicTree 1
    $baseline = Join-Path $tree 'dynamic.json'
    Assert-Exit (Invoke-Gate --root $tree --ext .py --map --map-out (Join-Path $tree 'm.json') `
        --map-baseline $baseline --update-map-baseline) 0
    # Unchanged: the recorded site is allowed.
    $held = Get-Map --root $tree --ext .py --map-check --map-baseline $baseline
    Assert-Exit $held 0
    Assert-NoLine $held 'DYNAMIC'

    # A second file starts naming a module at run time. Nothing records it, so it is rejected.
    [System.IO.File]::WriteAllText((Join-Path $tree 'later.py'),
        "import importlib" + [char]10 + "m = importlib.import_module(other)" + [char]10)
    $grown = Get-Map --root $tree --ext .py --map-check --map-baseline $baseline
    Assert-Exit $grown 1
    Assert-Line $grown 'DYNAMIC'
    Assert-Line $grown 'later.py'
    Assert-Line $grown 'a name built at RUN TIME that nothing records'
}

Test-Case 'map: a recorded site may not GROW' {
    $tree = New-DynamicTree 1
    $baseline = Join-Path $tree 'dynamic.json'
    Assert-Exit (Invoke-Gate --root $tree --ext .py --map --map-out (Join-Path $tree 'm.json') `
        --map-baseline $baseline --update-map-baseline) 0
    [System.IO.File]::WriteAllText((Join-Path $tree 'app.py'),
        "import importlib" + [char]10 + "a = importlib.import_module(one)" + [char]10 +
        "b = importlib.import_module(two)" + [char]10)
    $grown = Get-Map --root $tree --ext .py --map-check --map-baseline $baseline
    Assert-Exit $grown 1
    Assert-Line $grown 'recorded dynamic code GREW'
}

Test-Case 'map: a site that is GONE must be removed from the baseline' {
    # The third rule, and the one that stops a baseline being a permanent exemption list with extra steps.
    $tree = New-DynamicTree 1
    $baseline = Join-Path $tree 'dynamic.json'
    Assert-Exit (Invoke-Gate --root $tree --ext .py --map --map-out (Join-Path $tree 'm.json') `
        --map-baseline $baseline --update-map-baseline) 0
    [System.IO.File]::WriteAllText((Join-Path $tree 'app.py'),
        "import plugin" + [char]10 + "m = plugin" + [char]10)
    $fixed = Get-Map --root $tree --ext .py --map-check --map-baseline $baseline
    Assert-Exit $fixed 1
    Assert-Line $fixed 'none left here: remove it from dynamic.json'
}

Test-Case 'map: the ratchet spans languages - a PowerShell dot-source counts too' {
    # One list, whatever the language: `. $lib`, `import(expr)`, `importlib.import_module(name)` and
    # `Type.GetType(name)` are the same blind spot wearing four syntaxes.
    $tree = Use-Tree @{ 'main.ps1' = "`$lib = 'x'`n. `$lib`n" }
    $baseline = Join-Path $tree 'dynamic.json'
    Assert-Exit (Invoke-Gate --root $tree --ext '.ps1' --map --map-out (Join-Path $tree 'm.json') `
        --map-baseline $baseline --update-map-baseline) 0
    $recorded = Get-Content $baseline -Raw | ConvertFrom-Json
    $key = $recorded.computed.PSObject.Properties.Name | Select-Object -First 1
    if ($key -notlike 'main.ps1: a path built at run time*') { throw "not recorded: $key" }
    Assert-Exit (Get-Map --root $tree --ext '.ps1' --map-check --map-baseline $baseline) 0
}

Test-Case 'map: an UNREADABLE map baseline is a violation, not a silent pass' {
    # A ratchet that is not being applied, with nobody told, is worse than one that never existed.
    $tree = New-DynamicTree 1
    $baseline = Join-Path $tree 'dynamic.json'
    [System.IO.File]::WriteAllText($baseline, '{ this is not json')
    $result = Get-Map --root $tree --ext .py --map-check --map-baseline $baseline
    Assert-Exit $result 1
    Assert-Line $result 'DYNAMIC'
    Assert-Line $result 'could not be read'
}

Test-Case 'map: --update-map-baseline without --map-baseline is refused' {
    $tree = Use-Tree @{ 'a.py' = "X = 1`n" }
    $result = Invoke-Gate --root $tree --ext .py --map --update-map-baseline
    Assert-Exit $result 2
    Assert-Line $result 'needs --map-baseline'
}
