<#
    WHAT `--dead` KNOWS REACHES A DEF THAT NO CALL NAMES (`rust/fbtcore/src/query/dead.rs`, `src/Map/Py/PyLaunch.py`):
    - pytest enters `test_*.py`, `*_test.py` and `conftest.py` and calls their `test*` defs, the `test*` methods of a
      `Test*` class and every fixture - a launcher, as an entry point in `pyproject.toml` is (`functions.launched`),
      so a test file is no file read by nobody either;
    - a MIXIN'S method is run by a class that also inherits a base outside the tree: `ast.NodeVisitor.visit` calls
      `visit_Global` on whatever class supplies it.
    Its helpers are its own - `-Only PyReach` runs this suite alone.
#>

if ($script:Python) {

$nl = [string][char]10

Test-Case 'pyreach: pytest tests and fixtures are reached, a helper nothing calls is still dead' {
    $tree = Use-Tree @{
        'tests/test_shop.py' = "import pytest$nl${nl}@pytest.fixture${nl}def basket():$nl    return []$nl$nl" +
            "def total():$nl    return 0$nl${nl}def test_empty(basket):$nl    assert not basket$nl${nl}class TestOrder:$nl" +
            "    def test_total(self):$nl        assert total() == 0$nl$nl" +
            "def never_called():$nl    return 1$nl"
        'tests/conftest.py'  = "import pytest$nl${nl}@pytest.fixture${nl}def shared():$nl    return 1$nl"
        'tests/__init__.py'  = "VERSION = 1$nl"
    }
    $db = Join-Path $tree 'map.sqlite'
    Assert-Exit (Invoke-Gate --root $tree --ext .py --map-sqlite $db) 0
    $dead = Invoke-Gate --map-query $db --dead --limit 0
    Assert-Exit $dead 0
    Assert-Line $dead 'never_called'
    foreach ($live in 'basket', 'test_empty', 'TestOrder.test_total', 'shared') {
        if (@($dead.Lines | Where-Object { $_.TrimEnd().EndsWith(" $live") }).Count) { throw "--dead lists $live`:`n$($dead.Text)" }
    }
    $graph = Invoke-Gate --root $tree --ext .py --map --map-check --map-out (Join-Path $tree 'map.json')
    Assert-Exit $graph 0
    Assert-NoLine $graph 'NO READER tests/test_shop.py'
    Assert-NoLine $graph 'NO READER tests/conftest.py'
    Assert-NoLine $graph 'NO READER tests/__init__.py'
}

Test-Case 'pyreach: a mixin method a class with an outside base dispatches to is reached' {
    $tree = Use-Tree @{ 'walk.py' = "import ast$nl${nl}class Mixin:$nl    def visit_Global(self, node):$nl        return node$nl" +
        "    def unused_helper(self):$nl        return 0$nl${nl}class Walker(Mixin, ast.NodeVisitor):$nl    pass$nl$nl" +
        "def main(tree):$nl    Walker().visit(tree)$nl${nl}class Alone:$nl    def lonely(self):$nl        return 0$nl$nl" +
        "if __name__ == `"__main__`":$nl    main(ast.parse(`"x = 1`"))$nl" }
    $db = Join-Path $tree 'map.sqlite'
    Assert-Exit (Invoke-Gate --root $tree --ext .py --map-sqlite $db) 0
    $dead = Invoke-Gate --map-query $db --dead --limit 0
    Assert-Exit $dead 0
    Assert-NoLine $dead 'Mixin.visit_Global'
    Assert-Line $dead 'Alone.lonely'
}

}
