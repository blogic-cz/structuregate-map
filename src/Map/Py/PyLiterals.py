"""PyLiterals.py - what a LITERAL is doing where it is written, for `string_literals` and `number_literals`.

WHY A `use`. A literal alone says nothing about whether it is magic: `TIMEOUT = 30` names its number, and
`if retries > 30:` hides one. So every literal row carries what its parent does with it - `compare`, `arith`,
`index`, `argument`, `receiver` (`",".join`), `assign`, `return`, `default`, `declared` (a module CAPS constant's value),
`doc`, `format` or `other` - and, for an argument, the `callee` it is passed to; for an index or a method's
argument, the `target` source it applies to. `line.split(":")[2]` is then two rows that read as what they
are: a ":" passed to `split`, and a 2 indexing `line.split(":")`.

The vocabulary is SHARED with the C#, TypeScript and rust halves, so one `--magic` query reads all of them. The
column is `use`, not `context`: the Angular half's `string_literals.context` already means its parent's kind.

IMPORTED, never launched, and embedded with `PyRows.py` - see the PYROWS set in rust/fbtcore/src/embedded/.
"""
from __future__ import annotations

import ast

# A literal inside one of these takes the context of the container: `SIZES = (1, 2, 3)` declares all three.
CONTAINERS = (ast.List, ast.Tuple, ast.Set, ast.Dict)

# AND THROUGH A BUILT-IN THAT ONLY BUILDS ONE: `KINDS = frozenset({2, 3, 6})` declares its members exactly as
# `SIZES = (1, 2)` does. Stopped at the call, each member read as its argument and then as `other` - dozens of a real
# tree's magic numbers were one such constant. Only the builtin names, and only a positional argument.
COLLECTIONS = ("frozenset", "set", "tuple", "list", "dict")


def collects(node: ast.AST, child: ast.AST) -> bool:
    """Whether `node` is `frozenset(...)` / `tuple(...)` &c. building a collection out of `child`."""
    return (isinstance(node, ast.Call) and isinstance(node.func, ast.Name) and node.func.id in COLLECTIONS
            and any(arg is child for arg in node.args))


def add_literal(rows, where: dict, node: ast.Constant, parents: dict, src, key: str = "") -> None:
    """A string or number literal as its row, with what it is DOING there - the half of a magic-value question
    the value alone cannot answer. A string a dict is looked up by carries its dotted `key` - see PyKeys."""
    if isinstance(node.value, str) and node.value.strip():
        rows.add("string_literals", "s", dict(where, line=node.lineno, value=node.value, length=len(node.value),
                                              key=key, **context_of(node, parents, src)))
    elif number_of(node) is not None:
        rows.add("number_literals", "n", dict(where, line=node.lineno, value=src(node), number=float(number_of(node)),
                                              **context_of(node, parents, src)))


def parents_of(tree: ast.AST) -> dict:
    """{id(child): parent} for every node - python's `ast` keeps no parent pointer of its own."""
    parents = {}
    for node in ast.walk(tree):
        for child in ast.iter_child_nodes(node):
            parents[id(child)] = node
    return parents


def number_of(node: ast.Constant):
    """The number a constant holds, or None: a bool is not a number here, and neither is a complex."""
    value = node.value
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        return None
    return value


def context_of(node: ast.AST, parents: dict, src) -> dict:
    """{use, callee, target} of one literal, read off the nodes above it."""
    child, parent = node, parents.get(id(node))
    # A SIGN IS PART OF THE NUMBER: `-1` is one literal, not a 1 with something done to it.
    if isinstance(parent, ast.UnaryOp) and isinstance(parent.op, (ast.USub, ast.UAdd)):
        child, parent = parent, parents.get(id(parent))
    contained = isinstance(parent, CONTAINERS)
    while isinstance(parent, CONTAINERS) or (contained and collects(parent, child)):
        child, parent = parent, parents.get(id(parent))
    found = {"use": "other", "callee": "", "target": ""}
    if isinstance(parent, ast.Compare) or isinstance(parent, ast.MatchValue):
        found["use"] = "compare"
    elif isinstance(parent, (ast.BinOp, ast.AugAssign)) and child is not getattr(parent, "target", None):
        found["use"] = "arith"
    elif isinstance(parent, ast.Subscript) and parent.slice is child:
        found.update(use="index", target=src(parent.value))
    elif isinstance(parent, ast.Call) and child is not parent.func:
        found.update(use="argument", **callee_of(parent, src))
    elif isinstance(parent, ast.Attribute) and isinstance(parents.get(id(parent)), ast.Call) \
            and parents.get(id(parent)).func is parent:
        # `",".join(parts)`: the literal is what the method is CALLED ON - python's separator is a receiver.
        found.update(use="receiver", callee=parent.attr)
    elif isinstance(parent, ast.keyword):
        call = parents.get(id(parent))
        found.update(use="argument", **(callee_of(call, src) if isinstance(call, ast.Call) else {}))
    elif isinstance(parent, (ast.Assign, ast.AnnAssign)) and parent.value is child:
        found["use"] = "declared" if declares(parent, parents) else "assign"
    elif isinstance(parent, ast.Return):
        found["use"] = "return"
    elif isinstance(parent, ast.arguments):
        found["use"] = "default"
    elif isinstance(parent, ast.Expr):
        found["use"] = "doc"
    elif isinstance(parent, (ast.JoinedStr, ast.FormattedValue)):
        found["use"] = "format"
    # AN ELEMENT IS NOT THE ARGUMENT: the strings of `",".join([a, "x"])` are not separators. Only a constant's
    # value is the container's - `SIZES = (1, 2)` declares both.
    if contained and found["use"] != "declared":
        return {"use": "other", "callee": "", "target": ""}
    return found


def callee_of(call: ast.Call, src) -> dict:
    """The NAME a call is made to - `split`, `join`, `int` - and, for a method, what it is called on."""
    func = call.func
    if isinstance(func, ast.Attribute):
        return {"callee": func.attr, "target": src(func.value)}
    if isinstance(func, ast.Name):
        return {"callee": func.id, "target": ""}
    return {"callee": "", "target": ""}


def declares(assign: ast.AST, parents: dict) -> bool:
    """A module-level assignment to CAPITAL names - what the `consts` table calls a constant."""
    if not isinstance(parents.get(id(assign)), ast.Module):
        return False
    targets = assign.targets if isinstance(assign, ast.Assign) else [assign.target]
    names = [n.id for t in targets for n in ast.walk(t) if isinstance(n, ast.Name)]
    return bool(names) and all(n.isupper() for n in names)
