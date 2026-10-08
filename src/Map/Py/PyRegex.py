"""PyRegex.py - which calls BUILD a regex, for the `regexes` rows of `PyRows.py`.

WHY. "Where are the regexes, what pattern, what uses them" had no answer in the map: a call into `re` was a
`calls` row like any other, named by whatever alias the file used. A row per regex, naming the real module
and function, is that answer - the same table the TypeScript and C# halves fill.

THE MODULE IS RESOLVED THROUGH THE BINDER, never read off the text. `import re as r` then `r.compile` is
`re.compile`; `from re import compile as c` then `c` is `re.compile`; a file of the tree called `re.py`
binds to that file and is no regex at all.

THE PATTERN IS KNOWN ONLY FROM A LITERAL. A name, an f-string or a call builds it at run time: `pattern` and
`pattern_kind` are then "", an honest empty, never a guess. A method of a compiled pattern (`PAT.match`) is
not a row - the regex is recorded where it is compiled - and neither are `escape` and `purge`.

It is IMPORTED, never launched, and staged and embedded with `PyRows.py` - see PyBind.py.
"""
from __future__ import annotations

import ast
from PyAst import dotted

# The modules whose functions take a pattern: the standard one and its drop-in.
MODULES = ("re", "regex")

# Each function that builds a regex, and the POSITION its `flags` argument takes when passed bare.
FLAGS_AT = {"compile": 1, "match": 2, "search": 2, "fullmatch": 2, "findall": 2, "finditer": 2,
            "split": 3, "sub": 4, "subn": 4}


def api_of(binder, node: ast.Call) -> str:
    """`re.compile` & co for a call the binder places in one of MODULES, or ""."""
    parts = dotted(node.func).split(".")
    binding = binder.lookup(parts[0]) if parts[0] else None
    if len(parts) == 2 and binding is not None and binding[0] == "mod":
        module, name = binding[1], parts[1]
    elif len(parts) == 1 and binding is not None and binding[0] == "ext":
        module, name = binding[1], binding[2]
    else:
        return ""
    return module + "." + name if module in MODULES and name in FLAGS_AT else ""


def argument(node: ast.Call, position: int, keyword: str):
    """The value passed as `keyword` or at `position`, or None."""
    for passed in node.keywords:
        if passed.arg == keyword:
            return passed.value
    return node.args[position] if position < len(node.args) else None


def regex_call(binder, node: ast.Call, src) -> dict | None:
    """The regex columns of a call that builds one, or None for any other call."""
    api = api_of(binder, node)
    if not api:
        return None
    pattern, pattern_kind = "", ""
    value = argument(node, 0, "pattern")
    if isinstance(value, ast.Constant) and isinstance(value.value, (str, bytes)):
        text = value.value
        pattern = text if isinstance(text, str) else text.decode("utf-8", "replace")
        pattern_kind = "literal"
    flags = argument(node, FLAGS_AT[api.split(".")[1]], "flags")
    return {"kind": "call", "api": api, "pattern": pattern, "pattern_kind": pattern_kind,
            "flags": src(flags) if flags is not None else ""}
