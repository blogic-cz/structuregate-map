<#
    WHAT A CALL THROUGH A CLASS OR A MODULE REACHES (`src/Map/Py/PyBind.py`, `PyArgs.py`):
    - `Box.SLOTS.get()` is the dict's `get`, no def of the file - unbound, as it is across files (`is_def`);
    - `self.Inner()` makes the class written in the class body, as `self.go()` runs a method;
    - a def under an `if`, a loop or a `try` is still its scope's, and a call of it names its parameters;
    - `mod = pytest.importorskip("mod")` is `import mod`: `mod.tool()` reaches the def behind the re-export.
    Its helpers are its own - `-Only PyMembers` runs this suite alone.
#>

# One query through the exe, every cell whole.
function Invoke-PyMembersQ([string]$Db, [string]$Sql) {
    $result = Invoke-Gate --map-query $Db --width 0 --limit 0 --sql $Sql
    Assert-Exit $result 0
    return $result
}

if ($script:Python) {

$nl = [string][char]10

Test-Case 'pymembers: a member of a class attribute is no def, a class in the body is reached through self' {
    $tree = Use-Tree @{ 'box.py' = "class Box:$nl    SLOTS = {'a': 1}$nl    class Inner:$nl        pass$nl" +
        "    def pick(self, key):$nl        return Box.SLOTS.get(key)$nl    def make(self):$nl        return self.Inner()$nl" +
        "    def again(self):$nl        return Box.pick(self, 'a')$nl" +
        "def outer():$nl    class Local:$nl        seen = set()$nl    Local.seen.add(1)$nl" }
    $db = Join-Path $tree 'map.sqlite'
    Assert-Exit (Invoke-Gate --root $tree --ext .py --map-sqlite $db) 0
    $q = Invoke-PyMembersQ $db "SELECT 'call=' || callee || '|' || target_name || '|' AS r FROM calls ORDER BY line"
    Assert-Line $q 'call=Box.SLOTS.get||'
    Assert-Line $q 'call=self.Inner|Box.Inner|'
    Assert-Line $q 'call=Box.pick|Box.pick|'
    Assert-Line $q 'call=Local.seen.add||'
}

Test-Case 'pymembers: a call of a def written under an if names its parameters' {
    $tree = Use-Tree @{ 'run.py' = "def outer(flag):$nl    if flag:$nl        def helper(a, b):$nl            return a$nl" +
        "        return helper(1, b=2)$nl    return 0$nl" }
    $db = Join-Path $tree 'map.sqlite'
    Assert-Exit (Invoke-Gate --root $tree --ext .py --map-sqlite $db) 0
    $q = Invoke-PyMembersQ $db ("SELECT 'arg=' || k.target_name || '|' || a.param AS r FROM arguments a JOIN calls k ON k.id = a.call " +
        "WHERE k.callee = 'helper' ORDER BY a.position")
    Assert-Line $q 'arg=outer.helper|a'
    Assert-Line $q 'arg=outer.helper|b'
}

Test-Case 'pymembers: a module pytest.importorskip hands back is an import, so its re-export is used' {
    $tree = Use-Tree @{
        'lib/helper_mod.py' = "def tool():$nl    return 1$nl"
        'lib/mod_a.py'      = "from helper_mod import tool$nl"
        'tests/test_a.py'   = "import pytest${nl}mod_a = pytest.importorskip(`"mod_a`")$nl${nl}def test_x():$nl    assert mod_a.tool() == 1$nl"
    }
    $db = Join-Path $tree 'map.sqlite'
    Assert-Exit (Invoke-Gate --root $tree --ext .py --map-sqlite $db) 0
    $q = Invoke-PyMembersQ $db "SELECT 'call=' || target_path || '|' || target_name AS r FROM calls WHERE callee = 'mod_a.tool'"
    Assert-Line $q 'call=lib/helper_mod.py|tool'
    $unused = Invoke-Gate --map-query $db --unused-imports --limit 0
    Assert-Exit $unused 0
    Assert-NoLine $unused 'from helper_mod import tool'
}

}
