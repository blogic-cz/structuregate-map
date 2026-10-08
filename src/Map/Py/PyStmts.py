"""PyStmts.py - the STATEMENTS of `PyRows.py` that carry no expression of their own worth a row: `try` and each
of its handlers, `with`, `global`/`nonlocal`, `match`, `del` and `assert`.

WHY. A bare name in one of them - `with LOCK:`, `assert READY`, `except _ERRORS:` - is under the size of an
`expressions` row, so with no row of its own it was not read anywhere, and a module-state or
context-manager audit had nothing to run on. Each statement is a row here, carrying what it reads.

WHAT A HANDLER CATCHES IS READ OFF THE CHAIN. `except subprocess.TimeoutExpired:` is an attribute, not a
name; keeping names only left many `try` rows with no handler at all.

A mixin of `Walker`, so every row is written while the scope stack stands - see `PyRows.Walker`. It is
IMPORTED, never launched, and staged and embedded with `PyRows.py` - see PyBind.py.
"""
from __future__ import annotations

import ast

from PyAst import dotted, facts
from PyComments import noqa_of

# What a handler can hold and still do nothing: `pass`, `...`, or a lone string.
NOTHING = (ast.Pass,)

# A nested scope's body runs later, or never: a `raise` in a def inside a handler is not the handler's.
SCOPES = (ast.FunctionDef, ast.AsyncFunctionDef, ast.Lambda, ast.ClassDef)


def own_nodes(body: list):
    """Every node of `body`, nested defs, lambdas and classes left out."""
    stack = list(reversed(body))
    while stack:
        node = stack.pop()
        yield node
        if not isinstance(node, SCOPES):
            stack.extend(reversed(list(ast.iter_child_nodes(node))))


class Statements:
    """The visitors; `Walker` supplies `rows`, `where`, `src`, `touched`, `branch`, `guarded`, `comments`."""

    def caught(self, handler) -> list:
        """What one handler catches, one entry per type - `""` for a bare `except:`."""
        if handler.type is None:
            return []
        types = handler.type.elts if isinstance(handler.type, ast.Tuple) else [handler.type]
        return [dotted(t) or self.src(t) for t in types]

    def visit_Try(self, node) -> None:
        # WHICH IMPORTS ARE OPTIONAL is decided by the handler, never by the import: an import is optional
        # because of what encloses it, and nothing on the import node itself says so.
        caught = [name for handler in node.handlers for name in self.caught(handler)]
        if any(name in ("ImportError", "ModuleNotFoundError") for name in caught):
            for stmt in node.body:
                for sub in ast.walk(stmt):
                    if isinstance(sub, (ast.Import, ast.ImportFrom)):
                        self.guarded.add(id(sub))
        # WHAT A HANDLER CATCHES IS READ. `except _HTTP_ERRORS as e:` is the only use of that tuple, and
        # with no read recorded for it the constant looked unused.
        types = [handler.type for handler in node.handlers if handler.type is not None]
        self.branch(node, "try", ", ".join(caught), ast.Tuple(elts=types, ctx=ast.Load()) if types else None)
        star = 1 if type(node).__name__ == "TryStar" else 0
        for handler in node.handlers:
            self.handler(node, handler, star)
        self.generic_visit(node)

    visit_TryStar = visit_Try

    def handler(self, node, handler, star: int) -> None:
        """One row per `except` clause: what it catches, what it binds, and what its body DOES with it."""
        body = list(own_nodes(handler.body))
        calls = sorted({dotted(n.func) for n in body if isinstance(n, ast.Call) and dotted(n.func)})
        raised = [n for n in body if isinstance(n, ast.Raise)]
        name = handler.name or ""
        # `raise` alone, or `raise e` of the name this clause bound: the exception goes on as it came.
        reraises = any(r.exc is None or (name and isinstance(r.exc, ast.Name) and r.exc.id == name) for r in raised)
        exc_info = any(k.arg == "exc_info" and not (isinstance(k.value, ast.Constant) and k.value.value in (False, None))
                       for n in body if isinstance(n, ast.Call) for k in n.keywords)
        quiet = all(isinstance(s, NOTHING) or (isinstance(s, ast.Expr) and isinstance(s.value, ast.Constant))
                    for s in handler.body)
        comment = self.comments.get(handler.lineno, "")
        noqa, codes = noqa_of(comment)
        self.rows.add("handlers", "h", dict(self.where(), line=handler.lineno,
                                            end_line=getattr(handler, "end_lineno", handler.lineno),
                                            try_line=node.lineno, types=self.caught(handler),
                                            bare=1 if handler.type is None else 0, star=star, name=name,
                                            name_read=1 if name and any(isinstance(n, ast.Name) and n.id == name
                                                                        and isinstance(n.ctx, ast.Load)
                                                                        for n in body) else 0,
                                            passes=1 if quiet else 0, raises=len(raised),
                                            reraises=1 if reraises else 0, exc_info=1 if exc_info else 0,
                                            calls=calls, comment=comment, noqa=noqa, codes=codes,
                                            reads=facts(handler.type)["reads"] if handler.type else []))

    def visit_With(self, node) -> None:
        # ONE ROW PER ITEM: `with open(a) as f, LOCK:` enters two context managers, and the second is the one
        # a module-state audit asks about.
        for position, item in enumerate(node.items):
            self.rows.add("withs", "w", dict(self.where(), line=node.lineno,
                                             end_line=getattr(node, "end_lineno", node.lineno),
                                             position=position, source=self.src(item.context_expr),
                                             target=self.src(item.optional_vars) if item.optional_vars else "",
                                             is_async=1 if isinstance(node, ast.AsyncWith) else 0,
                                             **self.touched(item.context_expr)))
        self.generic_visit(node)

    visit_AsyncWith = visit_With

    def visit_Global(self, node) -> None:
        self.declared(node, "global")

    def visit_Nonlocal(self, node) -> None:
        self.declared(node, "nonlocal")

    def declared(self, node, kind: str) -> None:
        # A DECLARATION, NOT A READ: `global CACHE` says the function rebinds the module's name. It is the
        # row a module-state audit starts from, so it is not folded into `reads`.
        for name in node.names:
            self.rows.add("globals", "g", dict(self.where(), line=node.lineno, name=name, kind=kind))

    def visit_Delete(self, node) -> None:
        # `del CACHE[key]` reads CACHE; `del CACHE` unbinds it and reads nothing - `facts` skips a `Del` name.
        for target in node.targets:
            self.rows.add("deletes", "del", dict(self.where(), line=node.lineno,
                                                 target=dotted(target) or self.src(target),
                                                 **self.touched(target)))
        self.generic_visit(node)

    def visit_Assert(self, node) -> None:
        tested = node.test if node.msg is None else ast.Tuple(elts=[node.test, node.msg], ctx=ast.Load())
        self.branch(node, "assert", self.src(node.test), tested)
        self.generic_visit(node)

    def visit_Match(self, node) -> None:
        self.branch(node, "match", self.src(node.subject), node.subject)
        self.generic_visit(node)

    def visit_match_case(self, node) -> None:
        # A `match_case` has no position of its own; its pattern does. A pattern READS what it compares
        # against - `case Color.RED:` - and the guard reads like any test.
        test = self.src(node.pattern) + (" if " + self.src(node.guard) if node.guard else "")
        parts = [node.pattern] + ([node.guard] if node.guard else [])
        end = getattr(node.body[-1], "end_lineno", node.pattern.lineno) if node.body else node.pattern.lineno
        self.rows.add("branches", "br", dict(self.where(), line=node.pattern.lineno, kind="case", test=test,
                                             end_line=end, **self.touched(ast.Tuple(elts=parts, ctx=ast.Load()))))
        self.generic_visit(node)
