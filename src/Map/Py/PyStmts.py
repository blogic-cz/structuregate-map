"""PyStmts.py - the STATEMENTS of `PyRows.py` that carry no expression of their own worth a row: `try` and each
of its handlers, `with`, `global`/`nonlocal`, `match`, `del` and `assert`.

WHY. A bare name in one of them - `with LOCK:`, `assert READY`, `except _ERRORS:` - is under the size of an
`expressions` row, so with no row of its own it was not read anywhere, and a module-state or
context-manager audit had nothing to run on. Each statement is a row here, carrying what it reads.

WHAT A HANDLER CATCHES IS READ OFF THE CHAIN. `except subprocess.TimeoutExpired:` is an attribute, not a
name; keeping names only left many `try` rows with no handler at all.

`import` and `from .. import` are rows here too, and so is `mod = importlib.import_module("mod")` - the import a
string names (`PyAst.IMPORTERS`).

A mixin of `Walker`, so every row is written while the scope stack stands - see `PyRows.Walker`. It is
IMPORTED, never launched, and staged and embedded with `PyRows.py` - see PyBind.py.
"""
from __future__ import annotations

import ast

from PyAst import IMPORTERS, arg_names, comprehension_names, dotted, reads_of, taken_names
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
    """The visitors; `Walker` supplies `rows`, `where`, `src`, `touched`, `branch`, `guarded`, `comments`, `binder`, `used`."""

    def visit_Import(self, node) -> None:
        for alias in node.names:
            local = alias.asname or alias.name.split(".")[0]
            self.rows.add("imports", "i", dict(self.where(), line=node.lineno, module=alias.name,
                                               name="", alias=alias.asname or "", level=0, bind="",
                                               used=1 if local in self.used else 0,
                                               from_path=self.binder.tree.module(self.binder.rel, alias.name),
                                               guarded=1 if id(node) in self.guarded else 0))
        self.generic_visit(node)

    def visit_ImportFrom(self, node) -> None:
        for alias in node.names:
            # `bind` is the SYMBOL the name imports, as `binds` is for a read: `from jobs import work` is
            # jobs.py's `work`, and by name alone it protected every `work` in the tree from --dead.
            # Resolved from THIS statement, not from the scope: a def of the same name later in the function is what
            # the scope binds, and read from there the import named the def and the imported one read as dead.
            path, qual = self.binder.resolve_import(node, alias.asname or alias.name) if alias.name != "*" else ("", "")
            bind = path + "::" + qual if path else ""
            # `from_path` is the FILE the name is taken from - `pkg/__init__.py` for a re-export - so
            # --unused-imports can follow a re-export to whoever imports it next.
            tree, rel, module = self.binder.tree, self.binder.rel, node.module or ""
            # The NAMES it takes decide between several files spelling the module (`Tree.unique`), as they do for `bind`.
            taken = taken_names(node)
            source = tree.relative(rel, node.level, module) if node.level else tree.module(rel, module, taken)
            if not source and alias.name != "*":
                # A FOLDER WITH NO `__init__.py` IS A NAMESPACE PACKAGE: `from pkg.sub import mod` takes the file `pkg/sub/mod.py`.
                nested = (module + "." if module else "") + alias.name
                source = tree.relative(rel, node.level, nested) if node.level else tree.module(rel, nested)
            self.rows.add("imports", "i", dict(self.where(), line=node.lineno, module=module,
                                               name=alias.name, alias=alias.asname or "", bind=bind,
                                               level=node.level or 0, from_path=source,
                                               used=1 if (alias.asname or alias.name) in self.used else 0,
                                               guarded=1 if id(node) in self.guarded else 0))
        self.generic_visit(node)

    def string_import(self, node) -> None:
        """`mod = pytest.importorskip("mod")` IS `import mod as mod`: a row of its own, so `mod.tool` read in this
        file takes `tool` from mod.py, and the re-export there stops reading as unused (`--unused-imports`)."""
        value = node.value
        if len(node.targets) != 1 or not isinstance(node.targets[0], ast.Name) or not isinstance(value, ast.Call) \
                or dotted(value.func) not in IMPORTERS or not value.args or not isinstance(value.args[0], ast.Constant) \
                or not isinstance(value.args[0].value, str):
            return
        local, module = node.targets[0].id, value.args[0].value
        self.string_imports.add(id(value))
        if dotted(value.func) == "__import__":
            module = module.split(".")[0]
        self.rows.add("imports", "i", dict(self.where(), line=node.lineno, module=module, name="", alias=local, level=0,
                                           bind="", used=1 if local in self.used else 0,
                                           from_path=self.binder.tree.module(self.binder.rel, module),
                                           guarded=1 if id(node) in self.guarded else 0))

    def loose_import(self, node) -> None:
        """`return importlib.import_module("mod")`, `pytest.importorskip("mod").tool` - an import a string names that no
        name keeps: a row with no alias, used where it stands. One a name keeps is `string_import`'s."""
        if id(node) in self.string_imports or dotted(node.func) not in IMPORTERS or not node.args \
                or not isinstance(node.args[0], ast.Constant) or not isinstance(node.args[0].value, str):
            return
        module = node.args[0].value
        if dotted(node.func) == "__import__":
            module = module.split(".")[0]
        self.rows.add("imports", "i", dict(self.where(), line=node.lineno, module=module, name="", alias="", level=0, bind="",
                                           used=1, from_path=self.binder.tree.module(self.binder.rel, module),
                                           guarded=1 if id(node) in self.guarded else 0))

    def own_scope(self, node, names: set) -> None:
        """A COMPREHENSION OR A LAMBDA BINDS ITS OWN NAMES: every row inside it - a nested expression, an argument -
        reads them as locals, never as the module's variable of the same name."""
        self.binder.scopes.append(("func", "", {name: None for name in names}))
        try:
            self.generic_visit(node)
        finally:
            self.binder.leave()

    def visit_ListComp(self, node) -> None:
        self.own_scope(node, comprehension_names(node))

    visit_SetComp = visit_DictComp = visit_GeneratorExp = visit_ListComp

    def visit_Lambda(self, node) -> None:
        self.own_scope(node, set(arg_names(node.args)))

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
                                            **reads_of(handler.type)))

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
