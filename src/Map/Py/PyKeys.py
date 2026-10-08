"""PyKeys.py - the SETTINGS KEY a string looks up, dotted through every section it is read from.

`--key service.url` asks which code reads that key, and a settings module rarely spells it in one place:

    _svc = _S.get("service") or {}
    SERVICE_URL = _svc.get("url") or DEFAULT_URL

The "url" literal alone matches every `url` key in the tree, and the section it is read from sits on another
line, bound to a local. So each lookup key carries its PATH - the keys of the lookups its receiver was made by,
through a chain (`S["a"]["b"]`), an `or {}` fallback, or a name bound to one in this scope or an enclosing one.
`string_literals.key` is that path; `assignments.keys` and `consts.keys` are the paths a value reads, which is
how `--key` finds SERVICE_URL as the name built from `service.url`.

What a lookup IS comes from `PyAst.looked_up`, shared with the file map, which drops the same strings from
its name literals. IMPORTED, never launched, and embedded with `PyRows.py` - see the PYROWS set in
rust/fbtcore/src/embedded/.
"""
from __future__ import annotations

import ast

from PyAst import looked_up


class Sections:
    """The section each name in scope holds, for the file being walked - kept by the walk as it enters and
    leaves a def or a class, the way the Binder keeps what each name binds."""

    def __init__(self, tree: ast.AST) -> None:
        self.lookups = looked_up(tree)
        # (kind, {name: path}) innermost last. A path of "" is a name rebound to something that is NOT a
        # section, which shadows a section of the same name further out.
        self.scopes: list = [("module", {})]

    def enter(self, kind: str) -> None:
        self.scopes.append((kind, {}))

    def leave(self) -> None:
        self.scopes.pop()

    def key_of(self, node: ast.AST) -> str:
        """The dotted path a lookup key reads - `service.url` - or "" for a string that is no lookup key."""
        receiver = self.lookups.get(id(node))
        if receiver is None:
            return ""
        prefix = self.section(receiver)
        return prefix + "." + node.value if prefix else node.value

    def keys_in(self, value) -> list:
        """Every key path a value reads, for the row of the assignment that holds it."""
        if value is None:
            return []
        return sorted({path for node in ast.walk(value) if isinstance(node, ast.Constant)
                       for path in [self.key_of(node)] if path})

    def section(self, node: ast.AST) -> str:
        """The key path an expression evaluates to - a lookup, a fallback `x or {}`, a name holding one."""
        while isinstance(node, ast.BoolOp) and isinstance(node.op, ast.Or):
            node = node.values[0]
        if isinstance(node, ast.Name):
            # A CLASS BODY IS NOT AN ENCLOSING SCOPE, as the Binder says: only the innermost scope is asked
            # when it is a class.
            for depth, (kind, bound) in enumerate(reversed(self.scopes)):
                if kind == "class" and depth > 0:
                    continue
                if node.id in bound:
                    return bound[node.id]
            return ""
        if isinstance(node, ast.Subscript):
            return self.key_of(node.slice)
        if isinstance(node, ast.Call) and node.args:
            return self.key_of(node.args[0])
        return ""

    def bind(self, target: ast.AST, value) -> None:
        """`target = value` in the scope being walked: a name now holds the section the value is, or none."""
        if isinstance(target, ast.Name):
            self.scopes[-1][1][target.id] = self.section(value) if value is not None else ""
