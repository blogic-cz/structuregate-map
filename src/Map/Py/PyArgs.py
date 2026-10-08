"""PyArgs.py - which PARAMETER each argument of a bound call fills, for the `arguments` rows of `PyRows.py`.

WHY. "Is this default ever changed by a caller?" had no answer in the map: `calls` counted its arguments and
named nothing. A row per argument, carrying the parameter the CALLEE declares, is the join that question
needs - the same one the C# half makes through Roslyn.

ONLY A BOUND CALL IS MAPPED. The parameter list is the target def's own signature, read from its file; a call
the binder could not place has no signature to map against, and its arguments keep `param = ""`. After a
`*args` splat every later position is unknown, and `**kwargs` can fill any keyword parameter - both are
recorded as they are, never guessed through.

It is IMPORTED, never launched, and staged and embedded with `PyRows.py` - see PyBind.py.
"""
from __future__ import annotations

import ast

from PyAst import dotted

# The signatures of every def in a file, by qualname, parsed once per file per run - see `signatures`.
SIGNATURES = {}


def signatures(tree, rel: str) -> dict:
    """{qualname -> signature} for every def in `rel`, classes marked with `"class": True`."""
    path = tree.abs_of(rel)
    if path in SIGNATURES:
        return SIGNATURES[path]
    found: dict = {}
    SIGNATURES[path] = found
    try:
        with open(path, "r", encoding="utf-8", errors="replace") as handle:
            parsed = ast.parse(handle.read())
    except (OSError, SyntaxError, ValueError):
        return found

    def walk(body: list, prefix: str, in_class: bool) -> None:
        for node in body:
            if isinstance(node, ast.ClassDef):
                found[prefix + node.name] = {"class": True}
                walk(node.body, prefix + node.name + ".", True)
            elif isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
                spec = node.args
                marks = {dotted(d) for d in node.decorator_list}
                found[prefix + node.name] = {
                    "pos": [a.arg for a in spec.posonlyargs + spec.args],
                    "kwonly": [a.arg for a in spec.kwonlyargs],
                    "var": spec.vararg.arg if spec.vararg else "",
                    "kw": spec.kwarg.arg if spec.kwarg else "",
                    "method": in_class and "staticmethod" not in marks,
                    "classmethod": "classmethod" in marks,
                }
                walk(node.body, prefix + node.name + ".", False)

    walk(parsed.body, "", False)
    return found


def signature_of(tree, path: str, qual: str, callee: str) -> tuple:
    """(signature, how many leading parameters the call supplies itself), or (None, 0).

    `Thing(x)` runs `Thing.__init__`, whose `self` the call never passes. `obj.go(x)` and `self.go(x)` pass
    `self` implicitly; `Thing.go(obj, x)` passes it by hand - told apart by whether the callee names the
    class right before the method. A static method takes no `self` either way, a class method always its
    `cls`. A class with no `__init__` of its own inherits one this file does not show: unmapped.
    """
    found = signatures(tree, path)
    sig = found.get(qual)
    if sig is not None and sig.get("class"):
        init = found.get(qual + ".__init__")
        return (init, 1) if init else (None, 0)
    if sig is None:
        return None, 0
    if not sig["method"]:
        return sig, 0
    if sig["classmethod"]:
        return sig, 1
    owner = qual.rsplit(".", 1)[0].rsplit(".", 1)[-1]
    parts = callee.split(".")
    explicit = len(parts) >= 2 and parts[-2] == owner
    return sig, 0 if explicit else 1


def arguments(tree, node: ast.Call, callee: str, path: str, qual: str, src) -> list:
    """(row, value node) per argument; a row is `position`, `keyword`, `star`, `source`, `value`, `param`."""
    sig, offset = signature_of(tree, path, qual, callee) if path and qual and qual != "*" else (None, 0)
    rows = []
    known = True
    for index, arg in enumerate(node.args):
        starred = isinstance(arg, ast.Starred)
        param = ""
        if starred:
            # Everything after a splat lands at a position nobody can count.
            known = False
        elif sig is not None and known:
            slot = index + offset
            if slot < len(sig["pos"]):
                param = sig["pos"][slot]
            elif sig["var"]:
                param = "*" + sig["var"]
        rows.append(row(index, "", "*" if starred else "", arg.value if starred else arg, param, src))
    for word in node.keywords:
        param = ""
        if word.arg is not None and sig is not None:
            if word.arg in sig["pos"][offset:] or word.arg in sig["kwonly"]:
                param = word.arg
            elif sig["kw"]:
                param = "**" + sig["kw"]
        rows.append(row(-1, word.arg or "", "" if word.arg else "**", word.value, param, src))
    return rows


def row(position: int, keyword: str, star: str, value, param: str, src) -> tuple:
    """`value` is the literal's repr - `None` is `'None'`, and "" means the argument is not a literal.
    NOTHING IS CUT: `source` is the argument as written, however long."""
    return ({"position": position, "keyword": keyword, "star": star, "source": src(value),
             "value": literal_of(value), "param": param}, value)


def literal_of(value) -> str:
    """The repr of what `value` evaluates to when python can say without running anything, or "".

    A LITERAL IS MORE THAN A CONSTANT NODE. `-1`, `(1, 2)`, `{"a": 1}` and `["x", "y"]` are what
    `ast.literal_eval` reads, and read as constants only they had no value at all - so "which calls pass
    retries=-1" found nothing. Anything that needs a name, a call or an operator on non-literals stays "".
    """
    if isinstance(value, ast.Constant):
        return repr(value.value)
    try:
        return repr(ast.literal_eval(value))
    except (ValueError, TypeError, SyntaxError, MemoryError, RecursionError):
        return ""
