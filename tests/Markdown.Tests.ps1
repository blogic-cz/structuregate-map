<#
    The MARKDOWN half: `pulldown-cmark`, inside fbtcore, inside the exe. One question - what does each doc
    POINT A READER AT, and is it there.

    WHAT IS ASSERTED IS THE EDGE AND ITS ABSENCE. A mention is not an import, so the case that matters most is
    a dead file a doc names staying dead. The rest pins which texts are a claim at all - a bare name, a
    placeholder and a JSON example are not - and which docs may report one: only a context doc does.

    Helpers here are prefixed `Md`: suites share ONE scope, and `Get-Map` is the map suite's.
#>

function Get-MdMap {
    param([Parameter(ValueFromRemainingArguments)][object[]]$GateArgs)
    $path = Join-Path ([System.IO.Path]::GetTempPath()) "sgmd-$([System.Guid]::NewGuid().ToString('N').Substring(0,8)).json"
    $result = Invoke-Gate @GateArgs --ext .rs --map --map-check --map-out $path
    if (-not (Test-Path $path)) { throw "no map was written. Output:`n$($result.Text)" }
    $map = Get-Content $path -Raw | ConvertFrom-Json
    Remove-Item $path -Force -ErrorAction SilentlyContinue
    return [pscustomobject]@{ Exit = $result.Exit; Lines = $result.Lines; Text = $result.Text; Map = $map }
}

# `,` so a one-entry answer stays an array - see Get-Measured in Assert.ps1.
function Get-MdSection($Map, [string]$Section, [string]$Rel) {
    $property = $Map.$Section.PSObject.Properties | Where-Object { $_.Name -eq $Rel }
    if (-not $property) { return ,@() }
    return ,@($property.Value)
}

Test-Case 'md map: a doc mentioning a file is an edge in mentioned_by, and never makes a dead file look read' {
    $tree = Use-Tree @{
        'CLAUDE.md'      = "# t`n`nThe helper is ``src/lonely.rs``.`n"
        'src/lib.rs'     = "pub fn entry() {}`n"
        'src/lonely.rs'  = "pub fn lonely() {}`n"
    }
    $found = Get-MdMap --root $tree
    Assert-Exit $found 0
    Assert-Equal ((Get-MdSection $found.Map 'mentioned_by' 'src/lonely.rs') -join ',') 'CLAUDE.md' 'the doc points at it'
    Assert-Equal ((Get-MdSection $found.Map 'imported_by' 'src/lonely.rs').Count) 0 'a mention is not an import'
    Assert-Line $found 'NO READER src/lonely.rs'
    Assert-NoLine $found 'NO READER CLAUDE.md'
}

Test-Case 'md map: a context doc naming a path that is not there is a note, and --map-check still passes' {
    $tree = Use-Tree @{
        'CLAUDE.md'    = "# t`n`nThe map lives in ``gen/buildmap.json``; see [the notes](notes.md).`n"
        'docs/page.md' = "# p`n`nOver in the other tree: ``tools/other/run.py``.`n"
        'src/lib.rs'   = "pub fn entry() {}`n"
    }
    $found = Get-MdMap --root $tree
    Assert-Exit $found 0
    Assert-Line $found 'DOC-MISSING CLAUDE.md:3: `gen/buildmap.json`'
    Assert-Line $found 'DOC-MISSING CLAUDE.md:3: `notes.md`'
    # A `docs/` page is opened by choice and may describe another tree on purpose - mapped, never reported.
    Assert-NoLine $found 'DOC-MISSING docs/page.md'
    Assert-Equal ($found.Map.files.'docs/page.md'.language) 'markdown' 'the page is still mapped'
}

Test-Case 'md map: a bare name, a placeholder, a symbol and a JSON example are not claims' {
    $tree = Use-Tree @{
        'CLAUDE.md'  = "# t`n`nRun ``Map.cs``, ``MapGraph.Resolve``, ``<tree>\buildmap.json``, ``--map``, ``/S``, " +
                       "and call ``api/v1/login`` then ``/auth/token``; the build writes ``bin\Debug\App.Tests.dll``.`n`n" +
                       "``````json`n{ `"x`": `"never/there.json`" }`n```````n"
        'src/lib.rs' = "pub fn entry() {}`n"
    }
    $found = Get-MdMap --root $tree
    Assert-Exit $found 0
    Assert-NoLine $found 'DOC-MISSING'
}

Test-Case 'md map: a command after cd runs where the cd went, and a skill is summarised by its frontmatter' {
    $tree = Use-Tree @{
        '.claude/skills/run/SKILL.md' = "---`nname: run-app`ndescription: `"Run the app. Then more.`"`n---`n# Run`n`n" +
                                        "``````bash`ncd tools/demo`npython run.py --check   # the check`n```````n"
        'tools/demo/run.py'            = "print(1)`n"
        'src/lib.rs'                  = "pub fn entry() {}`n"
    }
    $found = Get-MdMap --root $tree
    Assert-Exit $found 0
    Assert-Equal ((Get-MdSection $found.Map 'mentioned_by' 'tools/demo/run.py') -join ',') '.claude/skills/run/SKILL.md' 'resolved under the cd'
    Assert-NoLine $found 'DOC-MISSING'
    Assert-Equal ($found.Map.files.'.claude/skills/run/SKILL.md'.summary) 'run-app: Run the app.' 'frontmatter name and first sentence'
}

Test-Case 'md map: a cd into a folder this tree does not have is missing - the first line a reader runs' {
    $tree = Use-Tree @{
        'CLAUDE.md'  = "# t`n`n``````bash`ncd tools/other`npython run.py`n```````n"
        'src/lib.rs' = "pub fn entry() {}`n"
    }
    $found = Get-MdMap --root $tree
    Assert-Exit $found 0
    Assert-Line $found 'DOC-MISSING CLAUDE.md:4: `tools/other`'
}

Test-Case 'md map: a nested doc names paths from a folder above it, and from the project under it' {
    $tree = Use-Tree @{
        'src/App/Models/CLAUDE.md'                         = "# m`n`nThe table is ``Db/Tables/Items.sql``.`n"
        'src/Db/Tables/Items.sql'                          = "create table x (a int)`n"
        'Shop/CLAUDE.md'                                   = "# i`n`nSettings: ``Infrastructure/Auth/Jwt.cs``; not ``Shared/Only.cs``.`n"
        'Shop/src/Shop.Domain/Infrastructure/Auth/Jwt.cs'  = "class Jwt {}`n"
        'Other/Shared/Only.cs'                             = "class Only {}`n"
        'src/lib.rs'                                       = "pub fn entry() {}`n"
    }
    $found = Get-MdMap --root $tree
    Assert-Exit $found 0
    Assert-Equal ((Get-MdSection $found.Map 'mentioned_by' 'src/Db/Tables/Items.sql') -join ',') 'src/App/Models/CLAUDE.md' 'from src/, above the doc'
    Assert-Equal ((Get-MdSection $found.Map 'mentioned_by' 'Shop/src/Shop.Domain/Infrastructure/Auth/Jwt.cs') -join ',') 'Shop/CLAUDE.md' 'from the project under the doc'
    Assert-NoLine $found 'DOC-MISSING src/App/Models/CLAUDE.md'
    Assert-NoLine $found '`Infrastructure/Auth/Jwt.cs`'
    # A SIBLING's file is not the doc's: neither above it nor under it, so it is still missing.
    Assert-Line $found 'DOC-MISSING Shop/CLAUDE.md:3: `Shared/Only.cs`'
}

Test-Case 'md map: an unlabelled fence is a folder tree or a message, never a command line' {
    $tree = Use-Tree @{
        'CLAUDE.md'  = "# t`n`n```````nServices/Api/`n  Mail/Text sender`n  Views/Gone/Page.razor`n```````n"
        'src/lib.rs' = "pub fn entry() {}`n"
    }
    $found = Get-MdMap --root $tree
    Assert-Exit $found 0
    Assert-NoLine $found 'DOC-MISSING'
}

Test-Case 'md map: the docs mapped are the docs the gate measures, so --doc-scope context leaves a README out' {
    $tree = Use-Tree @{
        'CLAUDE.md'  = "# t`n"
        'README.md'  = "# r`n"
        'src/lib.rs' = "pub fn entry() {}`n"
    }
    $found = Get-MdMap --root $tree --doc-scope context
    Assert-Equal ([bool]$found.Map.files.'CLAUDE.md') $true 'the context doc is mapped'
    Assert-Equal ([bool]$found.Map.files.'README.md') $false 'a README is outside the scope'
}

Test-Case 'md map: a doc no CLAUDE.md leads to - by a chain of mentions or by its folder - is DOC-ORPHAN, a note' {
    $tree = Use-Tree @{
        'CLAUDE.md'           = "# t`n`nStart at [the readme](README.md); the rules are in ``guides/``.`n"
        'README.md'           = "# r`n`nMore in ``docs/a.md``.`n"
        'docs/a.md'           = "# a`n`nThen ``docs/b.md``.`n"
        'docs/b.md'           = "# b`n"
        'guides/g.md'         = "# g`n"
        'docs/lost.md'        = "# lost`n"
        '.claude/rules/x.md'  = "# x`n"
        'src/lib.rs'          = "pub fn entry() {}`n"
    }
    $found = Get-MdMap --root $tree
    Assert-Exit $found 0
    Assert-Line $found 'DOC-ORPHAN docs/lost.md'
    foreach ($reached in 'README.md', 'docs/a.md', 'docs/b.md', 'guides/g.md', '.claude/rules/x.md', 'CLAUDE.md') {
        Assert-NoLine $found "DOC-ORPHAN $reached"
    }
}

Test-Case 'md map: a .claude/rules or .claude/commands doc is a context doc - its dead path is DOC-MISSING' {
    $tree = Use-Tree @{
        'CLAUDE.md'              = "# t`n"
        '.claude/rules/r.md'     = "# r`n`nSee ``src/gone/rule.rs``.`n"
        '.claude/commands/c.md'  = "# c`n`nRun ``tools/gone/run.py``.`n"
        'src/lib.rs'             = "pub fn entry() {}`n"
    }
    $found = Get-MdMap --root $tree
    Assert-Exit $found 0
    Assert-Line $found 'DOC-MISSING .claude/rules/r.md:3: `src/gone/rule.rs`'
    Assert-Line $found 'DOC-MISSING .claude/commands/c.md:3: `tools/gone/run.py`'
}

Test-Case 'md map: --doc-skip makes a .md data - not measured by the gate, not mapped, never DOC-ORPHAN' {
    $long = "# corpus`n" + ((1..30 | ForEach-Object { "line $_" }) -join "`n") + "`n"
    $tree = Use-Tree @{
        'CLAUDE.md'                  = "# t`n"
        'assets/guides/faq.md'       = $long
        'src/lib.rs'                 = "pub fn entry() {}`n"
    }
    $found = Get-MdMap --root $tree
    Assert-Line $found 'DOC-ORPHAN assets/guides/faq.md'
    $skipped = Get-MdMap --root $tree --doc-skip 'assets/**'
    Assert-Exit $skipped 0
    Assert-NoLine $skipped 'faq.md'
    Assert-Equal ($null -eq $skipped.Map.files.'assets/guides/faq.md') $true 'the data file is not in the map'
    Assert-Exit (Invoke-Gate --root $tree --max-doc-lines 10) 1
    Assert-Exit (Invoke-Gate --root $tree --max-doc-lines 10 --doc-skip 'assets/**') 0
}

Test-Case 'md map: --doc-root maps the docs above several code roots once, and their paths join the code' {
    $tree = Use-Tree @{
        'CLAUDE.md'                = "# t`n`nThe alpha package is ``pkgs/alpha/src/lib.rs``; see [alpha](pkgs/alpha/CLAUDE.md).`n"
        'pkgs/alpha/CLAUDE.md'     = "# alpha`n`nEntry: ``src/lib.rs``; see [notes](notes.md).`n"
        # A DOC INSIDE A CODE ROOT, linked from the CLAUDE.md beside it: mapped from the doc root, joined there.
        'pkgs/alpha/notes.md'      = "# notes`n"
        # A path with a symbol after one colon, and a path from a code root only the whole tree has.
        '.claude/agents/a.md'      = "# a`n`nSee ``pkgs/beta/src/lib.rs:tool`` and ``alpha/src/lib.rs``.`n"
        'pkgs/alpha/src/lib.rs'    = "pub fn entry() {}`n"
        'pkgs/beta/src/lib.rs'     = "pub fn tool() {}`n"
    }
    $without = Get-MdMap --root (Join-Path $tree 'pkgs/alpha') --root (Join-Path $tree 'pkgs/beta')
    Assert-Equal ($null -eq $without.Map.files.'CLAUDE.md') $true 'the project CLAUDE.md is in no code root'
    $found = Get-MdMap --root (Join-Path $tree 'pkgs/alpha') --root (Join-Path $tree 'pkgs/beta') --doc-root $tree
    Assert-Exit $found 0
    Assert-Equal ($found.Map.files.'CLAUDE.md'.language) 'markdown' 'the project CLAUDE.md is mapped'
    Assert-Equal ($found.Map.files.'pkgs/alpha/CLAUDE.md'.language) 'markdown' 'a doc inside a code root is mapped once, from the doc root'
    Assert-Equal ($null -eq $found.Map.files.'alpha/CLAUDE.md') $true 'and not again under its code root'
    $by = (Get-MdSection $found.Map 'mentioned_by' 'alpha/src/lib.rs') -join ','
    Assert-Equal $by '.claude/agents/a.md,CLAUDE.md,pkgs/alpha/CLAUDE.md' 'every doc names the file the code root mapped - a.md by the one path in the tree that ends so'
    Assert-NoLine $found 'DOC-MISSING'
    Assert-NoLine $found 'DOC-ORPHAN'
    Assert-Equal ((Get-MdSection $found.Map 'mentioned_by' 'pkgs/alpha/notes.md') -join ',') 'pkgs/alpha/CLAUDE.md' 'a doc in a code root joins its link'
    Assert-Equal ((Get-MdSection $found.Map 'mentioned_by' 'beta/src/lib.rs') -join ',') '.claude/agents/a.md' 'a path with a :symbol'
}

Test-Case 'md map: under --doc-root, a git-ignored .md is no doc, and a path from a code root or the repo top resolves' {
    $tree = Use-Tree @{
        'proj/CLAUDE.md'                  = "# t`n"
        # NEITHER THE DOC'S FOLDER NOR A UNIQUE TAIL reaches these: `engine/run.rs` is also a stale copy outside every
        # code root, and `other/notes.md` sits above the doc root, at the repository's top.
        'proj/.claude/agents/a.md'        = "# a`n`nSee ``engine/run.rs`` and ``other/notes.md``.`n"
        'proj/old/engine/run.rs'          = "pub fn stale() {}`n"
        'other/notes.md'                  = "# notes`n"
        'proj/pkgs/alpha/engine/run.rs'   = "pub fn run() {}`n"
        'proj/pkgs/alpha/src/lib.rs'      = "pub fn entry() {}`n"
        'proj/pkgs/beta/src/lib.rs'       = "pub fn tool() {}`n"
        'proj/.pytest_cache/README.md'    = "# pytest cache`n"
        'proj/.pytest_cache/.gitignore'   = "*`n"
    }
    & git -C $tree init -q 2>&1 | Out-Null
    $proj = Join-Path $tree 'proj'
    $found = Get-MdMap --root (Join-Path $proj 'pkgs/alpha') --root (Join-Path $proj 'pkgs/beta') --doc-root $proj
    Assert-Exit $found 0
    Assert-Equal ($null -eq $found.Map.files.'.pytest_cache/README.md') $true 'a git-ignored .md is not mapped'
    Assert-NoLine $found 'DOC-ORPHAN .pytest_cache'
    # A path from one code root resolves to that root's file; one from the repository's top to the doc itself.
    Assert-Equal ((Get-MdSection $found.Map 'mentioned_by' 'alpha/engine/run.rs') -join ',') '.claude/agents/a.md' 'a path from a code root'
    Assert-NoLine $found 'DOC-MISSING .claude/agents/a.md:3: `engine/run.rs`'
    Assert-NoLine $found 'DOC-MISSING .claude/agents/a.md:3: `other/notes.md`'
    Assert-Equal ($found.Map.files.'CLAUDE.md'.language) 'markdown' 'a doc keyed from the doc root, not the repository top'
}
