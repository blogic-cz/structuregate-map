"""PyAst.py - what an AST node MEANS, for the rows of `PyRows.py`.

WHY A FILE OF ITS OWN. Nothing here is about a table or a database: it is the reading of a node - the source
text it covers, the dotted name it is written as, its shape, and what it touches. `PyRows.py` grew past the
500-line ceiling this repo holds every hand-written file to, and this is the half of it that answers a
question about the TREE rather than about a row.

It is IMPORTED, never launched, so it is staged into the same folder as `PyRows.py` (the Launch in
`src/Map/Map.cs`) and embedded beside it (`src/StructureGate.csproj`). Missing either place is an
`ERR_MODULE_NOT_FOUND` at run time, in a consumer, where nobody is watching.
"""
from __future__ import annotations

import ast

# An expression smaller than this is a bare name or a literal - `x`, `1` - and a row for each would
# multiply the table while answering nothing the identifier columns cannot already answer.
MIN_EXPRESSION_NODES = 3

# How deep the structural skeleton of an expression goes. The shape is for recognising a pattern; the code
# itself is in `source`, in full, in the same row.
MAX_AST_DEPTH = 4

NEWLINE = chr(10)


def segment(lines: list, node) -> str:
    """The source text of a node, sliced from lines SPLIT ONCE per file.

    `ast.get_source_segment` splits the whole file on every call, so asking it for tens of thousands of
    expressions re-splits the tree as many times - measured at over ten times slower than
    once the split is hoisted. The columns are UTF-8 BYTE offsets, which is why each end is sliced on the
    encoded line and decoded back.
    """
    start = getattr(node, "lineno", None)
    end = getattr(node, "end_lineno", None)
    if start is None or end is None or start > len(lines) or end > len(lines):
        return ""
    if start == end:
        raw = lines[start - 1].encode("utf-8")[node.col_offset:node.end_col_offset]
        return raw.decode("utf-8", "replace").strip()
    first = lines[start - 1].encode("utf-8")[node.col_offset:].decode("utf-8", "replace")
    last = lines[end - 1].encode("utf-8")[:node.end_col_offset].decode("utf-8", "replace")
    return NEWLINE.join([first] + lines[start:end - 1] + [last]).strip()


def dotted(node) -> str:
    """`a.b.c` for an attribute chain, `a` for a name, "" for anything computed.

    Read off the chain and never off the text, so a call written across three lines reads the same as one
    written on a single line, and a subscript in the middle yields "" instead of a guess.
    """
    parts = []
    while isinstance(node, ast.Attribute):
        parts.append(node.attr)
        node = node.value
    if isinstance(node, ast.Name):
        parts.append(node.id)
        parts.reverse()
        return ".".join(parts)
    return ""


# THE FILE'S NODES, walked ONCE: a dozen passes each walked the same tree again through `ast.walk`, the half's
# single largest cost. Same order as `ast.walk`, so nothing a pass emits moves. Keyed by the tree OBJECT, held
# while it is in use - an `id` alone could name the next file's tree once this one is freed.
_WALKED: list = [None, []]


def walked(tree: ast.AST) -> list:
    if _WALKED[0] is not tree:
        _WALKED[0], _WALKED[1] = tree, list(ast.walk(tree))
    return _WALKED[1]


def looked_up(tree: ast.AST) -> dict:
    """{id(key): receiver} for every string a DICT LOOKUP is made with - `S["server"]`, `S.get("worker")`.

    A KEY IS DATA, NOT A NAME. Read by the file map as a name literal, `_S["server"]` in a settings module drew
    an optional edge to a `server.py` it never imports; read by the deep map, it is the key `--key`
    traces, with `receiver` the section it is looked up in."""
    found = {}
    for node in walked(tree):
        if isinstance(node, ast.Subscript):
            key, receiver = node.slice, node.value
        elif isinstance(node, ast.Call) and isinstance(node.func, ast.Attribute) and node.func.attr == "get" \
                and node.args:
            key, receiver = node.args[0], node.func.value
        else:
            continue
        if isinstance(key, ast.Constant) and isinstance(key.value, str):
            found[id(key)] = receiver
    return found


PATH_CALLS = ("join", "joinpath", "Path", "PurePath", "PurePosixPath", "PureWindowsPath", "PosixPath", "WindowsPath")


def path_parts(tree: ast.AST) -> set:
    """{id(string)} for every string that is a SEGMENT OF A PATH - an argument of `os.path.join`, `Path(...)`,
    `.joinpath(...)`, or an operand of `/`. `os.path.join(ROOT, "data", "build")` names a folder, not a module: read
    as a name it drew an edge from a settings module to `tools/build.py`."""
    found = set()
    for node in walked(tree):
        parts = []
        if isinstance(node, ast.Call) and dotted(node.func).split(".")[-1] in PATH_CALLS:
            parts = node.args
        elif isinstance(node, ast.BinOp) and isinstance(node.op, ast.Div):
            parts = [node.left, node.right]
        for part in parts:
            if isinstance(part, ast.Constant) and isinstance(part.value, str):
                found.add(id(part))
    return found


def sizes(tree: ast.AST) -> dict:
    """{id(node): nodes in its subtree} for every node, in ONE pass from the leaves up. Walking each expression's
    subtree to count it made the count grow with the square of a function's size - in
    both halves: the deep one's `expressions` rows did it again."""
    order = walked(tree)
    counted = {}
    for node in reversed(order):
        counted[id(node)] = 1 + sum(counted[id(child)] for child in ast.iter_child_nodes(node))
    return counted


def shape(node, depth: int = 0) -> dict:
    """The structural skeleton of an expression: kinds only, no names, capped."""
    out = {"k": type(node).__name__}
    if depth >= MAX_AST_DEPTH:
        return out
    kids = [shape(child, depth + 1) for child in ast.iter_child_nodes(node)
            if isinstance(child, (ast.expr, ast.operator, ast.cmpop, ast.boolop, ast.unaryop))]
    if kids:
        out["c"] = kids[:8]
    return out


def facts(node) -> dict:
    """What a subtree READS, CALLS and SPELLS OUT - so a query never has to look at `source` again."""
    reads, calls, strings = set(), set(), []
    for sub in ast.walk(node):
        # A NAME BEING ASSIGNED IS NOT READ. The target `LEFT, RIGHT` of a tuple assignment is an expression
        # row of its own, and counted as a read it made every tuple-assigned constant look used by the very
        # line that defines it. `obj.attr = v` still reads `obj`: that Name is in LOAD context.
        if isinstance(getattr(sub, "ctx", None), (ast.Store, ast.Del)):
            continue
        if isinstance(sub, ast.Name):
            reads.add(sub.id)
        elif isinstance(sub, ast.Attribute):
            name = dotted(sub)
            if name:
                reads.add(name)
        elif isinstance(sub, ast.Call):
            target = dotted(sub.func)
            if target:
                calls.add(target)
        elif isinstance(sub, ast.Constant) and isinstance(sub.value, str) and sub.value:
            strings.append(sub.value)
    return {"reads": sorted(reads), "calls": sorted(calls), "strings": strings}


def used_names(tree: ast.Module) -> set:
    """Every name the module READS anywhere, in any scope - what an import has to be for it to be used.

    AN ANNOTATION IS A USE, and the expression rows do not see one: `def f(x: Path)` makes no row for a bare
    `Path`, so an import used only in signatures read as unused. A QUOTED annotation (`"Path"`, the
    `TYPE_CHECKING` idiom) is parsed for its names, and every entry of a module-level `__all__` counts: a
    name the module declares public is not unused because the module itself never reads it.
    """
    found = set()
    for node in ast.walk(tree):
        if isinstance(node, ast.Name) and not isinstance(node.ctx, (ast.Store, ast.Del)):
            found.add(node.id)
        for hint in annotations_of(node):
            if isinstance(hint, ast.Constant) and isinstance(hint.value, str):
                try:
                    parsed = ast.parse(hint.value, mode="eval")
                except SyntaxError:
                    continue
                found.update(n.id for n in ast.walk(parsed) if isinstance(n, ast.Name))
    for node in tree.body:
        targets = node.targets if isinstance(node, ast.Assign) else []
        if any(isinstance(t, ast.Name) and t.id == "__all__" for t in targets) and \
                isinstance(node.value, (ast.List, ast.Tuple)):
            found.update(e.value for e in node.value.elts if isinstance(e, ast.Constant) and isinstance(e.value, str))
    return found


def annotations_of(node) -> list:
    """The annotation nodes a node carries itself: an argument's, a return, an annotated assignment's."""
    if isinstance(node, ast.arg):
        return [node.annotation] if node.annotation is not None else []
    if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
        return [node.returns] if node.returns is not None else []
    if isinstance(node, ast.AnnAssign):
        return [node.annotation]
    return []


def line_table(text: str) -> dict:
    """Where each line starts in CHARACTERS, plus the lines themselves.

    `col_offset` is a UTF-8 BYTE offset and every span here is reported as a character count, so the two
    have to be converted between - see `char_column`. Built once per file so a span stays arithmetic.
    """
    lines = text.splitlines()
    starts = [0]
    total = 0
    for line in lines:
        total += len(line) + 1
        starts.append(total)
    return {"starts": starts, "lines": lines}


def char_column(table: dict, line_number: int, byte_col: int) -> int:
    """A `col_offset` as a CHARACTER offset.

    THE TWO ARE THE SAME NUMBER FOR ASCII, which is almost every line, so the conversion is skipped where
    the line has no byte wider than one character. On the lines that do - a non-ASCII identifier, a comment
    with an em dash - a byte count overstates the span, and the span is what ranks one duplicate above
    another.
    """
    index = line_number - 1
    if index < 0 or index >= len(table["lines"]):
        return byte_col
    line = table["lines"][index]
    encoded = line.encode("utf-8")
    if len(encoded) == len(line):
        return byte_col
    return len(encoded[:byte_col].decode("utf-8", "replace"))


def span_start(table: dict, node: ast.AST) -> int:
    line = getattr(node, "lineno", None)
    if line is None or line > len(table["starts"]):
        return 0
    return table["starts"][line - 1] + char_column(table, line, node.col_offset)


def span_end(table: dict, node: ast.AST) -> int:
    line = getattr(node, "end_lineno", None)
    if line is None or line > len(table["starts"]):
        return 0
    return table["starts"][line - 1] + char_column(table, line, node.end_col_offset)


def span(table: dict, node: ast.AST) -> int:
    """How many CHARACTERS of source a node covers.

    THE THRESHOLDS STAY IN AST NODES and this is only what gets REPORTED. A node count is the right filter
    here - it cannot be inflated by a long name - but it is not comparable with what the other three halves
    count, and every group is ranked against them in one list. A source span is the same unit everywhere.
    """
    if getattr(node, "lineno", None) is None or getattr(node, "end_lineno", None) is None:
        return 0
    return max(0, span_end(table, node) - span_start(table, node))


def block_span(table: dict, body: list) -> int:
    """The same, for a list of statements: a body has no node of its own to measure."""
    if not body:
        return 0
    return max(0, span_end(table, body[-1]) - span_start(table, body[0]))
