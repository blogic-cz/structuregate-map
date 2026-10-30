<#
    A LONE SURROGATE IN A STRING (`src/Map/Py/PyAst.py::payload_text`): `"[\ud800-\udfff]"` is valid python, and the value
    it spells is no UTF-8 at all - writing the payload refused it, and the whole python half stored nothing. A surrogate
    alone is now U+FFFD, and a pair written as two escapes is the one character it spells.
#>

if ($script:Python) {

Test-Case 'pysurrogate: a pattern holding a lone surrogate is stored, and the file with it' {
    $tree = Use-Tree @{ 'scan.py' = "import re`nLONE = re.compile(`"[\ud800-\udfff]`")`nPAIR = re.compile(`"😀`")`n`n" +
        "def scan(text):`n    return LONE.search(text)`n" }
    $db = Join-Path $tree 'map.sqlite'
    Assert-Exit (Invoke-Gate --root $tree --ext .py --map-sqlite $db) 0
    $q = Invoke-Gate --map-query $db --width 0 --limit 0 --sql ("SELECT 'fns=' || (SELECT count(*) FROM functions WHERE name = 'scan') || " +
        "' lone=' || (SELECT count(*) FROM regexes WHERE pattern = '[' || char(65533) || '-' || char(65533) || ']') || " +
        "' pair=' || (SELECT count(*) FROM regexes WHERE pattern = char(128512)) AS r")
    Assert-Line $q 'fns=1 lone=1 pair=1'
}

}
