<#
    --map end to end, continued: the python, TypeScript and PowerShell halves, and generated C#. Split from
    `Map` at its size limit; the map-reading helpers are `Map.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot 'Map.Helpers.ps1')

# ---------------------------------------------------------------------------------------------------
# The python half
# ---------------------------------------------------------------------------------------------------

if ($script:MapPython) {

Test-Case 'map: python imports resolve by package PATH and by flat name' {
    $tree = Use-Tree @{
        'pkg/__init__.py' = "`n"
        'pkg/leaf.py'     = "VALUE = 1`n"
        'app.py'          = "from pkg import leaf`nimport flat`n"
        'flat.py'         = "OTHER = 2`n"
    }
    $found = Get-Map --root $tree --ext .py
    Assert-Exit $found 0
    $imports = Get-Imports $found.Map 'app.py'
    if ($imports -notcontains 'pkg/leaf.py') { throw "the package import is missing: $($imports -join ', ')" }
    if ($imports -notcontains 'flat.py') { throw "the flat import is missing: $($imports -join ', ')" }
}

Test-Case 'map: with SEVERAL roots each half resolves against its own root' {
    # A half answers in paths relative to the root it was GIVEN, and with several roots the map keys carry a
    # folder prefix. Handing one half the whole set and one root resolved every import against the wrong
    # tree: `import _paths` came back as `_paths.py` while the map held `tools/_paths.py`, and a
    # correct edge was reported BROKEN.
    $left = Use-Tree @{
        'shared.py' = "X = 1" + [char]10
        'app.py'    = "import shared" + [char]10
    }
    $right = Use-Tree @{
        'shared.py' = "Y = 2" + [char]10
        'tool.py'   = "import shared" + [char]10
    }
    $path = Join-Path ([System.IO.Path]::GetTempPath()) "sgmap-$([System.Guid]::NewGuid().ToString('N').Substring(0,8)).json"
    $result = Invoke-Gate --root $left --root $right --ext .py --map --map-out $path --map-check
    Assert-Exit $result 0
    Assert-NoLine $result 'BROKEN'
    $map = Get-Content $path -Raw | ConvertFrom-Json
    Remove-Item $path -Force -ErrorAction SilentlyContinue
    Remove-Item $right -Recurse -Force -ErrorAction SilentlyContinue
    $leftName = Split-Path $left -Leaf
    $rightName = Split-Path $right -Leaf
    Assert-Equal $map.imports."$leftName/app.py" "$leftName/shared.py" 'the left root resolved to its own file'
    Assert-Equal $map.imports."$rightName/tool.py" "$rightName/shared.py" 'and the right root to its own'
}

Test-Case 'map: a python module path must match the file NAME, case included' {
    # `os.path.isfile` says yes on a case-insensitive filesystem for a name differing only in case, which
    # turns a NAME import into a MODULE one: `from pkg import STORE` found `pkg/store.py` on Windows and drew
    # an edge to a file the code never imports - the real STORE is an object inside `__init__.py`.
    $tree = Use-Tree @{
        'pkg/__init__.py' = "STORE = object()" + [char]10
        'pkg/store.py'    = "UNRELATED = 1" + [char]10
        'app.py'          = "from pkg import STORE" + [char]10
    }
    $found = Get-Map --root $tree --ext .py --map-check
    Assert-Exit $found 0
    Assert-NoLine $found 'BROKEN'
    Assert-Equal (Get-Imports $found.Map 'app.py') 'pkg/__init__.py' 'the package, not the same-named module'
}

Test-Case 'map: a python import written INSIDE a function still counts' {
    # In a flat-import tree those are the majority: a module-level import of a sibling would be a cycle, so
    # the import lives in the function that needs it. Reading only module level reports most of such a tree
    # as importing nothing.
    $tree = Use-Tree @{
        'caller.py' = "def go():`n    import helper`n    return helper.X`n"
        'helper.py' = "X = 1`n"
    }
    $found = Get-Map --root $tree --ext .py
    Assert-Equal (Get-Imports $found.Map 'caller.py') 'helper.py' 'the function-level import is an edge'
}

Test-Case 'map: a third-party python import is external, not an edge' {
    $tree = Use-Tree @{ 'a.py' = "import json`nimport requests`n" }
    $found = Get-Map --root $tree --ext .py
    $external = @($found.Map.external.'a.py')
    if ($external -notcontains 'requests') { throw "requests is not listed as external: $($external -join ', ')" }
    Assert-Equal (Get-Imports $found.Map 'a.py').Count 0 'nothing in the tree is imported'
}

Test-Case 'map: a python file with a __main__ guard is an entry point' {
    $tree = Use-Tree @{ 'cli.py' = "def main():`n    return 0`n`nif __name__ == '__main__':`n    main()`n" }
    $found = Get-Map --root $tree --ext .py --map-check
    Assert-Exit $found 0
    Assert-NoLine $found 'NO READER cli.py'
}

Test-Case 'map: python that does not parse fails --map-check' {
    $tree = Use-Tree @{ 'bad.py' = "def broken(`n" }
    $found = Get-Map --root $tree --ext .py --map-check
    Assert-Exit $found 1
    Assert-Line $found 'UNPARSED  bad.py'
}

Test-Case 'map: a python host that will not launch leaves the reason on every file' {
    # NOT a silent skip: a map missing every python file looks exactly like a map of a tree with no python.
    $tree = Use-Tree @{ 'a.py' = "X = 1`n" }
    $found = Get-Map --root $tree --ext .py --py-host no-such-python.exe --map-check
    Assert-Line $found 'UNMAPPED  a.py'
    Assert-Line $found 'no-such-python.exe'
}

Test-Case 'map: two python bodies written twice are one duplicate group' {
    $body = "    total = 1`n    total = total + 2`n    return total`n"
    $tree = Use-Tree @{
        'one.py' = "def first():`n$body"
        'two.py' = "def second():`n    ""a docstring the other copy does not have""`n$body"
    }
    $found = Get-Map --root $tree --ext .py
    Assert-Equal $found.Map.duplicate_bodies.Count 1 'the docstring is not part of what a body DOES'
}

# THE NEAR COPY. A body digest keeps literals, so two pasted functions whose one edit was a number are
# NOT a duplicate - and were invisible. Their per-statement shingles (names AND literals blanked) are the
# same set, so they are a SIMILAR pair at 100: reported beside the duplicates, never as one of them.
Test-Case 'map: two python bodies that differ in ONE literal are a similar pair, not a duplicate' {
    $tree = Use-Tree @{
        'one.py' = "def first(items):`n    total = len(items)`n    limit = total * 2`n    return retry(limit, 3)`n"
        'two.py' = "def second(rows):`n    count = len(rows)`n    cap = count * 2`n    return retry(cap, 5)`n"
    }
    $found = Get-Map --root $tree --ext .py --map-check
    Assert-Equal $found.Map.duplicate_bodies.Count 0 'a changed literal is not an identical body'
    Assert-Equal $found.Map.similar_bodies.Count 1 'one near copy'
    Assert-Equal $found.Map.similar_bodies[0].score 100 'every statement shape shared'
    Assert-Line $found 'SIMILAR   2 function bodies'
}

# ONE LINE ADDED. Four of five statement shapes shared is 80 of 100; a function sharing two of four with
# either is vocabulary (`return x`), below the threshold, and must not be paired with them.
Test-Case 'map: a python body with one statement added is still a near copy, and a loose one is not' {
    # Four statements of four different SHAPES: `b = parse(a)` after `a = load(path)` would be the same
    # `_ = _(_)` shingle twice, and a set of two cannot clear the shared-count threshold.
    $body = "    a = load(path)`n    b = a.parse()`n    c = check(b, limit=3)`n    return c`n"
    $tree = Use-Tree @{
        'one.py' = "def first(path):`n$body"
        'two.py' = "def second(path):`n    log(path)`n$body"
        'other.py' = "def third(path):`n    a = load(path)`n    x = 7`n    y = x + 1`n    return y`n"
    }
    $found = Get-Map --root $tree --ext .py
    Assert-Equal $found.Map.similar_bodies.Count 1 'one pair, the loose function in none'
    Assert-Equal $found.Map.similar_bodies[0].score 80 '4 shared of 5 distinct shapes'
    Assert-Equal $found.Map.similar_bodies[0].shared 4 'the shared count is reported'
    if ($found.Map.similar_bodies[0].at -join ' ' -match 'other.py') { throw 'the loose function was paired' }
}

Test-Case 'map: a python span is CHARACTERS, not the bytes col_offset is measured in' {
    # `col_offset` is a UTF-8 BYTE offset and every span is reported in characters. An accent INSIDE the
    # measured line is where the two part company: this body is 56 characters and 57 bytes.
    $tail = '    return alpha + gamm' + [char]0xE9
    $body = '    alpha = 1' + [char]10 + '    gamma = alpha + 2' + [char]10 + $tail + [char]10
    $tree = Use-Tree @{
        'one.py' = 'def first():' + [char]10 + $body
        'two.py' = 'def second():' + [char]10 + $body
    }
    $found = Get-Map --root $tree --ext .py
    Assert-Equal $found.Map.duplicate_bodies.Count 1 'one group'
    Assert-Equal $found.Map.duplicate_bodies[0].size 56 'span in characters, not bytes'
}

Test-Case 'map: a python import named at RUN TIME is reported, not silently dropped' {
    # `importlib.import_module(name)` is still an import and this pass cannot say of what. Guessing would
    # invent an edge; dropping it would leave the dead-file finding unqualified. It is counted instead, and
    # the NO READER note names the count.
    $tree = Use-Tree @{
        'app.py'    = "import importlib" + [char]10 + "def load(name):" + [char]10 + "    return importlib.import_module(name)" + [char]10
        'plugin.py' = "X = 1" + [char]10
    }
    $found = Get-Map --root $tree --ext .py --map-check
    Assert-Exit $found 0
    Assert-Line $found 'COMPUTED  1 import(s) name their target at run time'
    Assert-Line $found 'name built at run time'
    Assert-Line $found 'NO READER plugin.py'
    Assert-Line $found '1 import(s) in this tree name their target at run time'
    Assert-Equal $found.Map.computed_imports.Count 1 'listed in the map as well'
}

Test-Case 'map: a LITERAL importlib call is a real edge' {
    $tree = Use-Tree @{
        'app.py'    = "import importlib" + [char]10 + "mod = importlib.import_module('plugin')" + [char]10
        'plugin.py' = "X = 1" + [char]10
    }
    $found = Get-Map --root $tree --ext .py --map-check
    Assert-Exit $found 0
    $imports = Get-Imports $found.Map 'app.py'
    if ($imports -notcontains 'plugin.py') { throw "the literal name did not resolve: $($imports -join ', ')" }
    Assert-Equal $found.Map.computed_imports.Count 0 'nothing was computed'
}

Test-Case 'map: a GUARDED python import is optional, not a dependency' {
    # An import under `except ImportError` is a degradation the file already handles. Counting it as a
    # dependency answers "can these two trees ship apart" with a no that is not true.
    $tree = Use-Tree @{
        'app.py'      = "try:" + [char]10 + "    import plugin" + [char]10 + "except ImportError:" + [char]10 + "    plugin = None" + [char]10
        'plugin.py'   = "X = 1" + [char]10
    }
    $found = Get-Map --root $tree --ext .py --map-check
    Assert-Exit $found 0
    Assert-Equal (Get-Imports $found.Map 'app.py').Count 0 'not a hard edge'
    Assert-Equal $found.Map.soft_imports.'app.py' 'plugin.py' 'an optional edge'
    Assert-Equal $found.Map.imported_softly_by.'plugin.py' 'app.py' 'read the other way round'
    Assert-NoLine $found 'NO READER plugin.py'
}

Test-Case 'map: a module imported BOTH ways is hard' {
    # One unguarded site is enough to need it; listing it as optional as well would say the file runs
    # without something it plainly does not.
    $tree = Use-Tree @{
        'app.py'    = "import plugin" + [char]10 + "try:" + [char]10 + "    import plugin" + [char]10 + "except ImportError:" + [char]10 + "    plugin = None" + [char]10
        'plugin.py' = "X = 1" + [char]10
    }
    $found = Get-Map --root $tree --ext .py
    Assert-Equal (Get-Imports $found.Map 'app.py') 'plugin.py' 'the hard edge stands'
    $soft = $found.Map.soft_imports.PSObject.Properties | Where-Object { $_.Name -eq 'app.py' }
    if ($soft) { throw "the same module is listed as optional as well: $($soft.Value -join ', ')" }
}

Test-Case 'map: a SUBMODULE under another root resolves to the file, not to its package' {
    # A half runs per root and can only resolve against the root it was GIVEN, so `from client_pkg.importer
    # import load` written under one root fell back to the first segment as a NAME - which matched the
    # package's __init__.py and left importer.py reported as read by nobody, twice in one day. The spelling
    # is emitted as a path suffix and joined here, where every root is held.
    $left = Use-Tree @{
        'caller.py' = "from client_pkg.importer import load" + [char]10
    }
    $right = Use-Tree @{
        'client_pkg/__init__.py' = [char]10
        'client_pkg/importer.py' = "def load():" + [char]10 + "    return 2" + [char]10
    }
    $path = Join-Path ([System.IO.Path]::GetTempPath()) "sgmap-$([System.Guid]::NewGuid().ToString('N').Substring(0,8)).json"
    $result = Invoke-Gate --root $left --root $right --ext .py --map --map-out $path --map-check
    Assert-Exit $result 0
    $map = Get-Content $path -Raw | ConvertFrom-Json
    Remove-Item $path -Force -ErrorAction SilentlyContinue
    Remove-Item $right -Recurse -Force -ErrorAction SilentlyContinue
    $leftName = Split-Path $left -Leaf
    $rightName = Split-Path $right -Leaf
    $imports = Get-Imports $map "$leftName/caller.py"
    if ($imports -notcontains "$rightName/client_pkg/importer.py") {
        throw "the submodule under the other root is missing: $($imports -join ', ')"
    }
    # And the package still gets its edge: importing pkg.sub RUNS pkg/__init__.py first.
    if ($imports -notcontains "$rightName/client_pkg/__init__.py") {
        throw "the package __init__ lost its edge: $($imports -join ', ')"
    }
    Assert-NoLine $result "NO READER $rightName/client_pkg/importer.py"
}

Test-Case 'map: a name spelled in a STRING is an OPTIONAL edge, never a hard one' {
    # A registry keyed by a string leaves no import in any parse tree: several constants read as dead in one
    # session for exactly this reason. The literal is evidence, not proof, so it lands on the optional side.
    # BOTH places a string is a key are covered: a module-level registry and a namespace call. A word that
    # merely appears in prose is not one - see `keyed` - so `mention.py` draws nothing.
    $tree = Use-Tree @{
        'catalog.py'  = "STEPS = ['worker']" + [char]10
        'dispatch.py' = "def pick(mod):" + [char]10 + "    return getattr(mod, 'worker')" + [char]10
        'mention.py'  = "def why():" + [char]10 + "    return 'worker queue is slow'" + [char]10
        'worker.py'   = "def run():" + [char]10 + "    return 1" + [char]10
    }
    $found = Get-Map --root $tree --ext .py --map-check
    Assert-Exit $found 0
    Assert-Equal (Get-Imports $found.Map 'catalog.py').Count 0 'a literal is not a hard import'
    foreach ($from in 'catalog.py', 'dispatch.py') {
        $soft = $found.Map.soft_imports.PSObject.Properties | Where-Object { $_.Name -eq $from }
        if (-not $soft) { throw "$from drew no optional edge at all. Output:`n$($found.Text)" }
        Assert-Equal ($soft.Value -join ',') 'worker.py' "the file whose name the string in $from spells"
    }
    $prose = $found.Map.soft_imports.PSObject.Properties | Where-Object { $_.Name -eq 'mention.py' }
    if ($prose) { throw "a word inside a sentence drew an edge: $($prose.Value -join ', ')" }
    Assert-NoLine $found 'NO READER worker.py'
}

Test-Case 'map: a DICT LOOKUP key is data, not a name - even in a module-level assignment' {
    # `_S["server"]` and `_S.get("worker")` in a shared settings module drew optional edges to server.py and
    # worker.py, and the config read as importing both apps. The registry beside them still draws one.
    $tree = Use-Tree @{
        # AND A PATH SEGMENT: `join(ROOT, 'data', 'build')` names a folder, not build.py - what v1.5.13 still drew.
        'config.py'   = "import os" + [char]10 + "_S = {}" + [char]10 + "HOST = _S['server']['host']" + [char]10 + "W = _S.get('worker') or {}" + [char]10 +
            "OUT = os.path.join('root', 'data', 'build')" + [char]10
        'build.py'    = "def run():" + [char]10 + "    return 3" + [char]10
        'registry.py' = "STEPS = {'server': 1}" + [char]10
        'server.py'   = "def run():" + [char]10 + "    return 1" + [char]10
        'worker.py'   = "def run():" + [char]10 + "    return 2" + [char]10
    }
    $found = Get-Map --root $tree --ext .py
    $soft = $found.Map.soft_imports.PSObject.Properties | Where-Object { $_.Name -eq 'config.py' }
    if ($soft) { throw "a lookup key drew an edge: $($soft.Value -join ', ')" }
    Assert-Equal ($found.Map.soft_imports.'registry.py' -join ',') 'server.py' 'a registry key is still a name'
}

Test-Case 'map: a def HANDED TO an object is not dead code, and an ordinary decorator is no excuse' {
    # `@app.route(...)` is entered over HTTP and has no importer by design. The test is structural - an
    # attribute of an object this tree makes or imports - so `@functools.lru_cache` is not one of them and
    # `plain.py` stays reported.
    $tree = Use-Tree @{
        'routes.py' = @(
            'app = Flask()',
            '',
            '',
            '@app.route("/x")',
            'def index_html():',
            '    return "x"') -join [char]10
        'plain.py'  = @(
            'import functools',
            '',
            '',
            '@functools.lru_cache',
            'def cached():',
            '    return 1') -join [char]10
    }
    $found = Get-Map --root $tree --ext .py --map-check
    Assert-Exit $found 0
    Assert-Line $found 'NO READER plain.py'
    Assert-NoLine $found 'NO READER routes.py'
    # The map SAYS WHY, rather than quietly treating it as an entry point: the reason differs and a reader
    # of the JSON has to be able to tell the two apart.
    Assert-Equal ($found.Map.files.'routes.py'.registered -join ',') 'app.route' 'the decorator that registers it'
}

}

# ---------------------------------------------------------------------------------------------------
# The TypeScript half
# ---------------------------------------------------------------------------------------------------

if (Get-TsModules) {

Test-Case 'map: a relative TypeScript import is an EXACT edge' {
    $tree = Use-Tree @{
        'src/app.ts'  = "import { value } from './util';`nexport const x = value;`n"
        'src/util.ts' = "export const value = 1;`n"
    }
    $found = Get-Map --root $tree
    Assert-Exit $found 0
    Assert-Equal (Get-Imports $found.Map 'src/app.ts') 'src/util.ts' 'the specifier resolved to the file'
}

Test-Case 'map: an ESM `./x.js` specifier resolves to x.ts' {
    # TypeScript under NodeNext writes the emitted extension. Following the literal suffix would report every
    # such import as broken in exactly the repos that got their module config right.
    $tree = Use-Tree @{
        'a.ts' = "import { v } from './b.js';`nexport const x = v;`n"
        'b.ts' = "export const v = 1;`n"
    }
    $found = Get-Map --root $tree
    Assert-Equal (Get-Imports $found.Map 'a.ts') 'b.ts' 'the .js specifier resolved to the .ts file'
}

Test-Case 'map: a .jsx file is parsed AS JSX - its elements parse and its imports are edges' {
    # Through BOTH compilers: 7.x's native one picks the kind from the file name, but 5.x/6.x parse in this
    # process with the kind the gate passes - which made every `.jsx` TS, so the first `<tag>` was UNPARSED.
    $tree = Use-Tree @{
        'src/App.jsx' = "import { api } from './api';`nexport default function App() {`n  return <div class=`"x`">{api.name}</div>;`n}`n"
        'src/api.js'  = "export const api = { name: 'a' };`n"
    }
    foreach ($modules in @((Get-TsModules), (Get-TypeScriptPath 'typescript@5'))) {
        if (-not $modules) { continue }
        $script:MapTsModules = $modules
        try { $found = Get-Map --root $tree --ext '.js,.jsx' --map-check }
        finally { $script:MapTsModules = $null }
        Assert-NoLine $found 'UNPARSED'
        Assert-Equal (Get-Imports $found.Map 'src/App.jsx') 'src/api.js' "the .jsx import resolved ($modules)"
    }
}

Test-Case 'map: an import naming nothing at all FAILS --map-check' {
    $tree = Use-Tree @{ 'a.ts' = "import { v } from './missing';`nexport const x = v;`n" }
    $found = Get-Map --root $tree --map-check
    Assert-Exit $found 1
    Assert-Line $found 'BROKEN'
    Assert-Line $found 'missing'
}

Test-Case 'map: a file with NO module syntax is an entry point, not dead code' {
    # An Apps Script server file, a classic browser script: one global scope, no import and no export. Such a
    # file CANNOT be imported by anything, so "nothing imports it" is true of every one of them and says
    # nothing.
    $tree = Use-Tree @{
        'server/log.ts'  = "function logLine_(line: string): void { }`n"
        'web/app.ts'     = "import { v } from './lib';`nexport const x = v;`n"
        'web/lib.ts'     = "export const v = 1;`n"
    }
    $found = Get-Map --root $tree --map-check
    Assert-Exit $found 0
    Assert-NoLine $found 'NO READER server/log.ts'
    Assert-Line $found 'NO READER web/app.ts'
}

Test-Case 'map: a package import is external, and a file on disk but outside --ext is neither' {
    $tree = Use-Tree @{
        'a.ts'    = "import x from 'left-pad';`nimport data from './data.json';`nexport const y = x;`n"
        'data.json' = "{}`n"
    }
    $found = Get-Map --root $tree --map-check
    Assert-Exit $found 0
    $external = @($found.Map.external.'a.ts')
    if ($external -notcontains 'left-pad') { throw "the package is not external: $($external -join ', ')" }
    Assert-NoLine $found 'BROKEN'
}


Test-Case 'map: a tsconfig that EXTENDS a package still yields its aliases' {
    # A shared base config is only ever published as a package, so a resolver that handles the relative form
    # alone leaves every repo using one with no aliases at all. The specifier is asked of node's own resolver
    # rather than walked by hand.
    $tree = Use-Tree @{
        'node_modules/@base/cfg/package.json'  = "{ ""name"": ""@base/cfg"", ""version"": ""1.0.0"" }" + [char]10
        # A baseUrl inside a shared base config is relative to THAT file - TypeScript's own rule - so a
        # published one has to write its way back out to the repo that extends it.
        'node_modules/@base/cfg/tsconfig.json' = "{ ""compilerOptions"": { ""baseUrl"": ""../../.."", ""paths"": { ""~/*"": [""src/*""] } } }" + [char]10
        'tsconfig.json' = "{ ""extends"": ""@base/cfg/tsconfig.json"" }" + [char]10
        'src/app.ts'    = "import { v } from '~/util';" + [char]10 + "export const x = v;" + [char]10
        'src/util.ts'   = "export const v = 1;" + [char]10
    }
    $found = Get-Map --root $tree --map-check
    Assert-Exit $found 0
    Assert-Equal (Get-Imports $found.Map 'src/app.ts') 'src/util.ts' 'the inherited alias resolved'
}

Test-Case 'map: an alias defined by a REFERENCED project resolves too' {
    # A composite build reaches its siblings through `references`, and the alias a referenced project
    # defines is the one its own files are imported by. Each config keeps its OWN baseUrl - two configs'
    # patterns cannot share a base.
    $tree = Use-Tree @{
        'tsconfig.json'     = "{ ""references"": [ { ""path"": ""./lib"" } ] }" + [char]10
        'lib/tsconfig.json' = "{ ""compilerOptions"": { ""baseUrl"": ""."", ""paths"": { ""@lib/*"": [""src/*""] } } }" + [char]10
        'lib/src/util.ts'   = "export const v = 1;" + [char]10
        'app.ts'            = "import { v } from '@lib/util';" + [char]10 + "export const x = v;" + [char]10
    }
    $found = Get-Map --root $tree --map-check
    Assert-Exit $found 0
    Assert-Equal (Get-Imports $found.Map 'app.ts') 'lib/src/util.ts' 'the referenced project''s alias resolved'
}

Test-Case 'map: a dynamic import with a computed specifier is reported' {
    $tree = Use-Tree @{
        'a.ts' = "export async function load(name: string) { return await import(name); }" + [char]10
    }
    $found = Get-Map --root $tree --map-check
    Assert-Exit $found 0
    Assert-Line $found 'COMPUTED'
    Assert-Line $found 'specifier built at run time'
}

Test-Case 'map: a tsconfig `paths` alias resolves to the file, and is not a package' {
    # An aliased import is not third-party, and calling it one hides a real edge - on a repo that aliases
    # everything, that is every edge in the tree. The tsconfig is read as JSONC, because `tsc --init` writes
    # comments and JSON.parse rejects all of them.
    $tree = Use-Tree @{
        'tsconfig.json' = "{" + [char]10 + "  // the alias every file in this repo imports through" + [char]10 + "  ""compilerOptions"": { ""baseUrl"": ""."", ""paths"": { ""~/*"": [""src/*""] } }," + [char]10 + "}" + [char]10
        'src/app.ts'    = "import { v } from '~/util';" + [char]10 + "export const x = v;" + [char]10
        'src/util.ts'   = "export const v = 1;" + [char]10
    }
    $found = Get-Map --root $tree --map-check
    Assert-Exit $found 0
    Assert-Equal (Get-Imports $found.Map 'src/app.ts') 'src/util.ts' 'the alias resolved to the file'
    $external = $found.Map.external.PSObject.Properties | Where-Object { $_.Name -eq 'src/app.ts' }
    if ($external) { throw "the alias was reported as a package: $($external.Value -join ', ')" }
}

Test-Case 'map: the same expression in two TypeScript files is COPIED' {
    $shape = "const config = { retries: 3, timeout: 1000, backoff: 2, label: 'run', verbose: true, tag: 'x' };"
    $tree = Use-Tree @{
        'a.ts' = "export function one() { $shape return config; }" + [char]10
        'b.ts' = "export function two() { $shape return config; }" + [char]10
    }
    $found = Get-Map --root $tree --map-check
    Assert-Exit $found 0
    if ($found.Map.duplicate_expressions.Count -lt 1) { throw 'the shared object literal was not reported' }
    Assert-Line $found 'COPIED'
}

}

# ---------------------------------------------------------------------------------------------------
# The PowerShell half - two kinds of edge, because the language has two
# ---------------------------------------------------------------------------------------------------

Test-Case 'map: a dot-source is an edge, and so is calling the function it brought in' {
    # PowerShell has no module graph: a tree is wired by dot-sourcing, and once a file is dot-sourced its
    # functions are GLOBAL - so the caller usually names a function, not a file. Reading only one of the two
    # would either draw a star out of the entry script or miss the wiring entirely.
    $tree = Use-Tree @{
        'lib/Tools.ps1' = "function Get-Thing { return 1 }`n"
        'main.ps1'      = ". `$PSScriptRoot\lib\Tools.ps1`nGet-Thing`n"
        'other.ps1'     = "function Invoke-Other { Get-Thing }`n"
    }
    $found = Get-Map --root $tree --ext '.ps1' --map-check
    Assert-Exit $found 0
    Assert-Equal (Get-Imports $found.Map 'main.ps1') 'lib/Tools.ps1' 'the dot-source is an edge'
    Assert-Equal (Get-Imports $found.Map 'other.ps1') 'lib/Tools.ps1' 'the function call is an edge too'
}

Test-Case 'map: launching an EXECUTABLE is not an import' {
    # `& arp.exe` is a process launch written with the same operator as a script call. Counting one as an
    # import reported executables as broken imports on the first repo this ran against.
    $tree = Use-Tree @{ 'run.ps1' = "& arp.exe -a`n& powershell.exe -NoProfile -Command 'exit'`n" }
    $found = Get-Map --root $tree --ext '.ps1' --map-check
    Assert-Exit $found 0
    Assert-NoLine $found 'BROKEN'
}

Test-Case 'map: dot-sourcing a file that is not there FAILS --map-check' {
    $tree = Use-Tree @{ 'main.ps1' = ". `$PSScriptRoot\lib\Missing.ps1`n" }
    $found = Get-Map --root $tree --ext '.ps1' --map-check
    Assert-Exit $found 1
    Assert-Line $found 'BROKEN'
    Assert-Line $found 'Missing.ps1'
}

Test-Case 'map: a PowerShell file that defines no function is an entry point' {
    $tree = Use-Tree @{ 'publish.ps1' = "Write-Host 'go'`nWrite-Host 'done'`n" }
    $found = Get-Map --root $tree --ext '.ps1' --map-check
    Assert-Exit $found 0
    Assert-NoLine $found 'NO READER publish.ps1'
}

Test-WindowsCase 'map: PowerShell that does not parse under this host is UNPARSED' {
    # The version floor as well as a parse check: `??` is PS7-only, so a file using it cannot even be read
    # by the 5.1 that launches these apps.
    $tree = Use-Tree @{ 'new.ps1' = "function Get-X { `$a = `$null ?? 1; return `$a }`n" }
    $found = Get-Map --root $tree --ext '.ps1' --map-check --ps-host powershell.exe
    Assert-Exit $found 1
    Assert-Line $found 'UNPARSED  new.ps1'
}

Test-Case 'map: a computed path that still SPELLS the file is an edge' {
    # `. (Join-Path $AppRoot 'lib\Thing.ps1')` is how a PowerShell app normally dot-sources. Treating the whole
    # expression as unknowable made such edges invisible.
    $tree = Use-Tree @{
        'App.ps1'     = "`$AppRoot = `$PSScriptRoot`n. (Join-Path `$AppRoot 'lib\Thing.ps1')`n"
        'lib/Thing.ps1'     = "function Get-Thing { return 1 }`n"
    }
    $found = Get-Map --root $tree --ext '.ps1' --map-check
    Assert-Exit $found 0
    Assert-Equal (Get-Imports $found.Map 'App.ps1') 'lib/Thing.ps1' 'the literal named the file'
    Assert-NoLine $found 'COMPUTED'
}

Test-Case 'map: a computed path whose literal matches TWO files draws no edge' {
    # The ambiguity rule again: which one `$Root` points at is a question about run time that this pass does
    # not ask, so it reports rather than picks.
    $tree = Use-Tree @{
        'main.ps1'      = ". (Join-Path `$Root 'Thing.ps1')`n"
        'a/Thing.ps1'   = "function Get-A { return 1 }`n"
        'b/Thing.ps1'   = "function Get-B { return 2 }`n"
    }
    $found = Get-Map --root $tree --ext '.ps1' --map-check
    Assert-Exit $found 0
    Assert-Equal (Get-Imports $found.Map 'main.ps1').Count 0 'no edge is guessed'
    Assert-Line $found 'COMPUTED'
}

Test-Case 'map: a dot-source whose path is built at run time is reported' {
    # `. $lib` is certainly an import and certainly unresolvable here. `& $exe` is not an import at all, so
    # only the dot-source is counted - the difference is what keeps the count meaningful.
    $tree = Use-Tree @{
        'main.ps1' = "`$lib = Join-Path `$PSScriptRoot 'lib\Thing.ps1'`n. `$lib`n& `$env:ComSpec /c echo hi`n"
    }
    $found = Get-Map --root $tree --ext '.ps1' --map-check
    Assert-Exit $found 0
    Assert-Line $found 'COMPUTED  1 import(s) name their target at run time'
    Assert-Line $found 'a path built at run time'
}

Test-Case 'map: two identical PowerShell bodies are one duplicate group' {
    $body = "    `$total = 1`n    `$total = `$total + 2`n    return `$total`n"
    $tree = Use-Tree @{
        'one.ps1' = "function Get-First {`n$body}`n"
        'two.ps1' = "function Get-Second {`n    # a comment the other copy does not have`n$body}`n"
    }
    $found = Get-Map --root $tree --ext '.ps1'
    Assert-Equal $found.Map.duplicate_bodies.Count 1 'a comment is not part of what a body DOES'
}

# ---------------------------------------------------------------------------------------------------
# GENERATED C#: joined like any other file, never fingerprinted
# ---------------------------------------------------------------------------------------------------

Test-Case 'map: a generated file is still JOINED - what it declares is reachable' {
    # An NSwag client or an EF snapshot is written by a tool, and hand-written code calls into it. Dropping
    # it from the graph would make the caller import nothing and the generated file dead.
    $tree = Use-Tree @{
        'Client.Designer.cs' = "namespace N;`npublic class ApiClient { public int Send() { return 1; } }`n"
        'Hand.cs'            = "namespace N;`npublic class Hand { public int Go() { return new ApiClient().Send(); } }`n"
    }
    $found = Get-Map --root $tree
    Assert-Exit $found 0
    Assert-Equal (Get-Imports $found.Map 'Hand.cs') 'Client.Designer.cs' 'the edge into the generated file'
    Assert-Equal $found.Map.files.'Client.Designer.cs'.generated 'True' 'and it says it was generated'
}

Test-Case 'map: a generated file is NOT fingerprinted, so it reports no duplicate' {
    # Every EF model snapshot resembles every other one. Fingerprinting them fills the duplicate report
    # with groups nobody can act on - and on a real solution it was most of the time this pass spent.
    $body = "var a = 1;`n        var b = a + 2;`n        return a + b;"
    $tree = Use-Tree @{
        'One.Designer.cs' = "namespace N;`npublic class One { public int First() {`n        $body`n    } }`n"
        'Two.Designer.cs' = "namespace N;`npublic class Two { public int Second() {`n        $body`n    } }`n"
    }
    $found = Get-Map --root $tree --map-check
    Assert-Exit $found 0
    Assert-Equal $found.Map.duplicate_bodies.Count 0 'no group from two generated copies'
    Assert-NoLine $found 'DUPLICATE'
}

Test-Case 'map: the marker decides it, not the folder it sits in' {
    $body = "var a = 1;`n        var b = a + 2;`n        return a + b;"
    $tree = Use-Tree @{
        'Migrations/One.cs' = "namespace N;`npublic class One { public int First() {`n        $body`n    } }`n"
        'Migrations/Two.cs' = "namespace N;`npublic class Two { public int Second() {`n        $body`n    } }`n"
    }
    $found = Get-Map --root $tree
    Assert-Exit $found 0
    # A folder name is a convention; these two are hand-written and copying one into the other is a finding.
    Assert-Equal $found.Map.duplicate_bodies.Count 1 'hand-written files in Migrations are fingerprinted'
}
