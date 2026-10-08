<#
    The python deep map's `regexes` rows: a call that builds a regex, named by the REAL module and function
    whatever alias the file used, with its pattern when a literal gives it.

    Its helpers are its own - `-Only PyRegex` runs this suite alone.
#>

$script:PyRegexPython = [bool]$script:Python
if (-not $script:PyRegexPython) { Write-Host '    (no python on this machine - the regex cases are not run)' }

# The deep map of a throwaway tree, as the tree and its database.
function New-PyRegexDb([hashtable]$Files) {
    $tree = Use-Tree $Files
    $db = Join-Path $tree 'regex.sqlite'
    Assert-Exit (Invoke-Gate --root $tree --ext .py --map-sqlite $db) 0
    return [pscustomobject]@{ Tree = $tree; Db = $db }
}

# One query through the exe, every cell whole.
function Invoke-PyRegexQ([string]$Path, [Parameter(ValueFromRemainingArguments)][object[]]$LensArgs) {
    $result = Invoke-Gate --map-query $Path --width 0 --limit 0 @LensArgs
    Assert-Exit $result 0
    return $result
}

# `|`-joined cells, so a case compares one exact line and a containment check cannot pass on a neighbour.
function Assert-PyRegexRow($Result, [string]$Expected) {
    $hits = @($Result.Lines | Where-Object { $_.Trim() -ceq $Expected })
    if ($hits.Count -eq 0) { throw "no line is exactly '$Expected'. Output:`n$($Result.Text)" }
}

if ($script:PyRegexPython) {

$nl = [string][char]10

Test-Case 'pyregex: a regex call is a row, the module resolved through its alias' {
    $made = New-PyRegexDb @{
        'a.py' = "import re as r$nl" + "from re import sub as s, compile$nl" + "import regex$nl" +
                 'WORDS = r.compile(r"\d+", re.I)' + $nl + "def go(t):$nl" +
                 '    return s("a", "b", t), compile("x"), regex.search(f"{t}", t)' + $nl
    }
    $r = Invoke-PyRegexQ $made.Db --sql ("SELECT 'rx=' || api || '|' || pattern || '|' || pattern_kind || '|' || flags " +
        "|| '|' || used_by || '|' || func FROM regexes ORDER BY line, id")
    Assert-PyRegexRow $r 'rx=re.compile|\d+|literal|re.I|= WORDS|'
    Assert-PyRegexRow $r 'rx=re.sub|a|literal|||go'
    Assert-PyRegexRow $r 'rx=re.compile|x|literal|||go'
    # A pattern built at run time stays EMPTY - never the f-string's text.
    Assert-PyRegexRow $r 'rx=regex.search|||||go'
    Assert-PyRegexRow (Invoke-PyRegexQ $made.Db --sql "SELECT 'n=' || count(*) FROM regexes") 'n=4'
}

Test-Case 'pyregex: a name bound to a file of the tree is not the module' {
    # `b.py` is there so the table is: a database with no regex row has no `regexes` table to count.
    $made = New-PyRegexDb @{
        're.py' = "def compile(p):$nl    return p$nl"
        'a.py'  = "from re import compile$nl" + 'X = compile("a")' + $nl
        'b.py'  = "import regex$nl" + 'Y = regex.compile("b")' + $nl
    }
    $r = Invoke-PyRegexQ $made.Db --sql "SELECT 'n=' || count(*) FROM regexes WHERE api = 're.compile'"
    Assert-PyRegexRow $r 'n=0'
    $r = Invoke-PyRegexQ $made.Db --sql "SELECT 'to=' || target_path FROM calls WHERE callee = 'compile'"
    Assert-PyRegexRow $r 'to=re.py'
}

}
