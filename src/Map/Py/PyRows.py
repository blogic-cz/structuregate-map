"""PyRows.py - the DEEP python half of `structuregate --map --map-sqlite`.

TWO GRANULARITIES, ON PURPOSE. `PyMap.py` beside this answers questions about FILES: what imports what,
what is written twice, what is unread. It carries no code at all, deliberately, because a copy of the
source in a JSON file is a copy that drifts. That leaves a whole class of question unaskable - where do we
build an id, which calls pass a flag, what reads this constant - because those are about
EXPRESSIONS, and a file-level graph has none.

This is that other granularity, modelled on the Angular map of this same tool: every call, branch,
assignment, decorator and expression as a ROW, carrying its own source text beside the facts extracted
from it, in a SQLite file with an index on every join column.

THE FACTS ARE EXTRACTED, THE TEXT IS CARRIED. A row holds `source` so a reader can see it, and `reads`,
`calls` and `strings` so a query never has to parse the text again. A consumer that pattern-matches over
`source` has re-implemented, badly, what this pass already resolved.

IT IS A CACHE, NEVER A SOURCE. The database is rebuilt from the tree and goes stale the moment a file
changes; `_meta` records the root and the file count it was built from, so a number taken from it can be
checked against the tree it claims to describe.

PYTHON PARSES, IT DOES NOT WRITE. The database is written by `rust/fbtcore`, linked into the exe - the same
store the C# half writes through - so this script never opens it. It reads what the database already holds
from `--state`, parses the files that moved, and hands the rows over as one JSON payload at `--rows`. It
used to write the file itself through `sqlite3`; one store instead of two is one answer to "what is
recorded" instead of two to keep in step.

Contract with the caller (structuregate.exe):
  in   --list-file <path>  UTF-8, one file per line: <relative-path><TAB><absolute-path>
       --root <dir>        the tree being mapped
       --roots-file <path> with several roots: a JSON list of `[prefix, dir]`, so a call is bound
                           across all of them as their `sys.path` would, and named with their prefix
       --state <path>      what the database already holds, as the store published it (JSON)
       --rows <path>       where the payload goes (JSON)
       --rebuild           read every file, whatever the state says
       --reset             the retry: drop every file this half records and write them again
  out  MAP-ERROR|rel|line|message
       MAP-FATAL|message       the state could not be read or the payload not written
       MAP-READ|<parsed>|<files>
       MAP-DONE|<files>        LAST line; its absence means this half died half way
The `MAP-DB|<table>|<rows>` lines come from the store, once it has written the payload.
"""
from __future__ import annotations

import ast
import hashlib
import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from PyAst import MIN_EXPRESSION_NODES, NEWLINE, dotted, facts, segment, shape, sizes, used_names  # noqa: E402
import PyDeps  # noqa: E402
from PyBind import Binder, Tree  # noqa: E402
from PyLaunch import launched  # noqa: E402
from PyArgs import arguments  # noqa: E402
from PyInputs import handed_of, options, roots_of, rows_of, text_of  # noqa: E402
from PyComments import record_comments  # noqa: E402
from PyStmts import Statements  # noqa: E402
from PyRegex import regex_call  # noqa: E402
from PyLiterals import add_literal, parents_of  # noqa: E402
from PyKeys import Sections  # noqa: E402

# What the `files` rows of this half are stamped with, so the other extractor writing the same database
# can tell its own files from these - the store scopes what is recorded, gone and disagreeing by it.
LANG = "python"

# WHAT A ROW MEANS, as a version folded into every file's sha. The store rebuilds on its own SCHEMA_VERSION,
# which every half shares, so bumping that replaces the C# and Angular rows too. A change to THESE rows only
# has to re-read THESE files: a new salt makes every recorded python sha disagree, and each file is read
# again on the next run. Without it `calls.target_path` existed only for files edited since the upgrade,
# and a query naming it failed with `no such column` on every tree that had a database already.
ROWS_VERSION = "16"


def emit(*fields: object) -> None:
    parts = []
    for field in fields:
        text = str(field)
        parts.append(text.replace("|", "/").replace("\r", " ").replace("\n", " "))
    print("|".join(parts))


class Rows:
    """The tables, filled as the walk goes. Ids are handed out on append and mean nothing outside one run:
    the database is a cache, so a row id is a handle for joining within one build, never a name to keep."""

    def __init__(self, counters: dict = None) -> None:
        # CONTINUED, NOT RESTARTED, on an incremental run: ids already in the database must not be handed
        # out a second time, or a join would pull two unrelated rows.
        self.counters: dict = dict(counters or {})
        self.tables: dict = {}
        # The Binder of the file being walked, which the walk keeps at the scope the row sits in.
        self.binder = None

    def add(self, table: str, prefix: str, row: dict) -> str:
        # `binds` IS `reads` RESOLVED, position for position: the file and qualname of the symbol each
        # read names, or "" where the tree cannot say. Added here, where every row passes, so no table that
        # carries `reads` can be written without it - `--reads ROOT` then tells config.py's ROOT from
        # every other file's, and `--dead` counts a callback by the def it is rather than by its name.
        if self.binder is not None and isinstance(row.get("reads"), list):
            row["binds"] = [self.bound(name) for name in row["reads"]]
        n = self.counters.get(prefix, 0) + 1
        self.counters[prefix] = n
        row["id"] = "%s:%d" % (prefix, n)
        self.tables.setdefault(table, []).append(row)
        return row["id"]

    def bound(self, name: str) -> str:
        path, qual = self.binder.resolve_read(name)
        return path + "::" + qual if path else ""


class Walker(Statements, ast.NodeVisitor):
    """Every row a single file contributes.

    SCOPE IS RECORDED, NOT INFERRED. A row says which class and function it sits in, because "which calls
    happen inside `build`" is the question people actually have, and recovering that from line
    numbers afterwards means re-deriving what the tree already knew.
    """

    def __init__(self, rows: Rows, file_id: str, text: str, binder: Binder, entries: set) -> None:
        self.rows = rows
        self.binder = binder
        # The qualnames a LAUNCHER calls in this file - `[project.scripts]`, `setup(entry_points=...)`.
        self.entries = entries
        self.file = file_id
        # SPLIT ONCE, sliced many times - see `segment`. Holding the lines is what makes the expression
        # table affordable at all.
        self.lines = text.split(NEWLINE)
        # {line -> comment}, for the `handlers` row an `except` line's waiver belongs to - see PyComments.
        self.comments: dict = {}
        # {id(node): nodes in its subtree}, counted once for the file (`PyAst.sizes`).
        self.sizes: dict = {}
        self.stack: list = []
        self.guarded: set = set()
        self.used: set = set()
        # (value, target) of the single-target assignment being walked - what a regex row is `used_by`.
        self.value_of = None
        # {id(child): parent}, for what a literal is DOING - see PyLiterals.
        self.parents: dict = {}
        # The settings section each name holds, for a lookup key's dotted path - see PyKeys.
        self.sections = None

    def where(self) -> dict:
        cls = next((n for k, n in self.stack if k == "class"), "")
        func = next((n for k, n in reversed(self.stack) if k == "func"), "")
        return {"file": self.file, "cls": cls, "func": func}

    def src(self, node) -> str:
        return segment(self.lines, node)

    def qual(self, name: str) -> str:
        return ".".join([n for _, n in self.stack] + [name])

    def visit_ClassDef(self, node) -> None:
        bases = [dotted(b) or self.src(b) for b in node.bases]
        # `bases_bind` IS `bases` RESOLVED, like `binds` beside `reads`: the class each base is, or "" for one
        # this tree does not hold. Matched by NAME, two classes called `Engine` were one ancestor, and an
        # override of the wrong one read as dead.
        self.rows.add("classes", "c", dict(self.where(), line=node.lineno, name=node.name,
                                           qualname=self.qual(node.name),
                                           bases=bases, bases_bind=[self.rows.bound(b) for b in bases],
                                           doc=(ast.get_docstring(node) or "")))
        self.decorators(node, node.name, "class")
        self.outer(node.decorator_list + list(node.bases) + [k.value for k in node.keywords])
        self.binder.enter_class(node, self.qual(node.name))
        self.sections.enter("class")
        self.stack.append(("class", node.name))
        self.inner(node.body)
        self.stack.pop()
        self.sections.leave()
        self.binder.leave()

    def visit_FunctionDef(self, node) -> None:
        self.function(node, 0)

    def visit_AsyncFunctionDef(self, node) -> None:
        self.function(node, 1)

    def function(self, node, is_async: int) -> None:
        spec = node.args
        args = [a.arg for a in spec.posonlyargs + spec.args + spec.kwonlyargs]
        if spec.vararg:
            args.append("*" + spec.vararg.arg)
        if spec.kwarg:
            args.append("**" + spec.kwarg.arg)
        self.rows.add("functions", "fn", dict(self.where(), line=node.lineno, name=node.name,
                                              qualname=self.qual(node.name), args=args,
                                              returns=self.src(node.returns) if node.returns else "",
                                              is_async=is_async,
                                              launched=1 if self.qual(node.name) in self.entries else 0,
                                              doc=(ast.get_docstring(node) or ""),
                                              end_line=getattr(node, "end_lineno", node.lineno)))
        self.decorators(node, node.name, "function")
        self.parameters(node)
        # A DECORATOR, A DEFAULT AND AN ANNOTATION ARE EVALUATED WHERE THE `def` IS WRITTEN, not inside the
        # function, so each is walked BEFORE the scope is pushed. Walking them after it recorded
        # `def load(path=SETTINGS)` as reading SETTINGS inside `load` - a function whose body never mentions
        # it - so "which function reads this constant" answered with the wrong one.
        self.outer(node.decorator_list + [a.annotation for a in self.args_of(spec)]
                   + list(spec.defaults) + list(spec.kw_defaults) + [node.returns])
        self.binder.enter_function(node, self.qual(node.name))
        self.sections.enter("func")
        self.stack.append(("func", node.name))
        self.inner(node.body)
        self.stack.pop()
        self.sections.leave()
        self.binder.leave()

    @staticmethod
    def args_of(spec) -> list:
        """Every `ast.arg` of a signature, in the order a caller writes them."""
        listed = spec.posonlyargs + spec.args + spec.kwonlyargs
        return listed + [a for a in (spec.vararg, spec.kwarg) if a is not None]

    def outer(self, nodes: list) -> None:
        """Sub-trees that belong to the ENCLOSING scope, walked before the scope is pushed."""
        for node in nodes:
            if node is not None:
                self.visit(node)

    def inner(self, body: list) -> None:
        """The statements the pushed scope owns. `generic_visit` would walk the signature a second time."""
        for statement in body:
            self.visit(statement)

    def parameters(self, node) -> None:
        """One row per parameter, with its annotation and its DEFAULT EXPRESSION.

        `functions.args` carries the NAMES only, and a name says nothing about what a signature READS. In a
        tree whose defaults are constants - `def load(path=_CONFIG_PATH, local=_LOCAL_PATH)` - every one
        of those constants looked unread, and a dead-code pass built on that offered dozens of them for
        deletion. `reads` here is the default's own, resolved the way an expression's is.
        """
        spec = node.args
        positional = spec.posonlyargs + spec.args
        # A DEFAULT BELONGS TO THE LAST PARAMETERS. `spec.defaults` is right-aligned against the positional
        # list, so pairing the two from the left labels the wrong parameter with the wrong default.
        pad = [None] * (len(positional) - len(spec.defaults))
        listed = list(zip(positional, pad + list(spec.defaults), ["positional"] * len(positional)))
        listed += list(zip(spec.kwonlyargs, spec.kw_defaults, ["keyword"] * len(spec.kwonlyargs)))
        listed += [(a, None, k) for a, k in ((spec.vararg, "vararg"), (spec.kwarg, "kwarg"))
                   if a is not None]
        where = self.where()
        for position, (arg, default, kind) in enumerate(listed):
            self.rows.add("parameters", "p", dict(
                where, func=node.name, qualname=self.qual(node.name),
                line=getattr(arg, "lineno", node.lineno), position=position, name=arg.arg, kind=kind,
                annotation=self.src(arg.annotation) if arg.annotation else "",
                default_expr=self.src(default) if default is not None else "",
                reads=facts(default)["reads"] if default is not None else []))

    def decorators(self, node, target: str, target_kind: str) -> None:
        for dec in getattr(node, "decorator_list", []):
            call = dec.func if isinstance(dec, ast.Call) else dec
            # `args` is what the decorator is CALLED WITH, one source text per argument - the route of
            # `@app.post("/items")` - so "which routes does this file serve" needs no --cat.
            given = list(dec.args) + list(dec.keywords) if isinstance(dec, ast.Call) else []
            self.rows.add("decorators", "d", dict(self.where(), line=dec.lineno,
                                                  name=dotted(call) or self.src(call),
                                                  target=target, target_kind=target_kind,
                                                  args=[self.src(a) for a in given], source=self.src(dec)))

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
            bind = self.rows.bound(alias.asname or alias.name) if alias.name != "*" else ""
            # `from_path` is the FILE the name is taken from - `pkg/__init__.py` for a re-export - so
            # --unused-imports can follow a re-export to whoever imports it next.
            tree, rel, module = self.binder.tree, self.binder.rel, node.module or ""
            source = tree.relative(rel, node.level, module) if node.level else tree.module(rel, module)
            self.rows.add("imports", "i", dict(self.where(), line=node.lineno, module=module,
                                               name=alias.name, alias=alias.asname or "", bind=bind,
                                               level=node.level or 0, from_path=source,
                                               used=1 if (alias.asname or alias.name) in self.used else 0,
                                               guarded=1 if id(node) in self.guarded else 0))
        self.generic_visit(node)

    def visit_Assign(self, node) -> None:
        for target in node.targets:
            self.assignment(node, target, "assign")
        if len(node.targets) == 1:
            self.value_of = (node.value, dotted(node.targets[0]))
        self.generic_visit(node)
        # BOUND AFTER THE VALUE IS WALKED: `cfg = cfg["inner"]` looks "inner" up in the OLD `cfg`.
        for target in node.targets:
            self.sections.bind(target, node.value)

    def visit_AnnAssign(self, node) -> None:
        self.assignment(node, node.target, "annotated")
        self.value_of = (node.value, dotted(node.target))
        self.generic_visit(node)
        self.sections.bind(node.target, node.value)

    def visit_AugAssign(self, node) -> None:
        self.assignment(node, node.target, "augmented")
        self.generic_visit(node)

    def assignment(self, node, target, kind: str) -> None:
        name = dotted(target) or self.src(target)
        where = self.where()
        # WHAT THE RIGHT-HAND SIDE READS, resolved here rather than left in `source` for a query to
        # re-parse. `_safe = inspect.getsource` recorded no read of `inspect.getsource` anywhere, so a name
        # bound by assignment looked unused however often it was assigned.
        rhs = facts(node.value) if node.value is not None else {"reads": [], "calls": []}
        # `keys` are the settings keys the value looks up, dotted - what `--key` builds a name from (PyKeys).
        keys = self.sections.keys_in(node.value)
        self.rows.add("assignments", "a", dict(where, line=node.lineno, target=name, kind=kind,
                                               source=self.src(node.value) if node.value else "",
                                               reads=rhs["reads"], calls=rhs["calls"], keys=keys))
        # A MODULE-LEVEL CAPITAL NAME is a constant, and a constant is a fact about this codebase somebody
        # will want to find without already knowing which file holds it.
        # ONE ROW PER NAME. `QUEUED, RUNNING, DONE, FAILED = range(4)` was one row named by the whole target
        # text, so a question about RUNNING found nothing and the four read as one unused constant.
        names = [target.id] if isinstance(target, ast.Name) else [
            n.id for n in ast.walk(target) if isinstance(n, ast.Name) and isinstance(n.ctx, ast.Store)]
        for const in names if not self.stack else []:
            if const.isupper():
                self.rows.add("consts", "k", dict(where, line=node.lineno, name=const,
                                                  source=self.src(node.value) if node.value else "",
                                                  reads=rhs["reads"], calls=rhs["calls"], keys=keys))
        if not self.stack and name == "__all__" and isinstance(node.value, (ast.List, ast.Tuple)):
            for element in node.value.elts:
                if isinstance(element, ast.Constant) and isinstance(element.value, str):
                    self.rows.add("exports", "e", dict(where, line=node.lineno, name=element.value))

    def visit_Return(self, node) -> None:
        self.rows.add("returns", "r", dict(self.where(), line=node.lineno,
                                           source=self.src(node.value) if node.value else "",
                                           **self.touched(node.value)))
        self.generic_visit(node)

    @staticmethod
    def touched(node) -> dict:
        """What a sub-tree reads and calls, or empty columns when there is no sub-tree at all.

        A ROW WITHOUT THE COLUMNS IS NOT THE SAME ROW: the table takes its shape from the first rows
        written, so a bare `return` in the first file would decide that `returns` has no `reads` column at
        all and every later row would lose it.
        """
        if node is None:
            return {"reads": [], "calls": []}
        found = facts(node)
        return {"reads": found["reads"], "calls": found["calls"]}

    def visit_Raise(self, node) -> None:
        name = ""
        if node.exc is not None:
            name = dotted(node.exc.func) if isinstance(node.exc, ast.Call) else dotted(node.exc)
        self.rows.add("raises", "rs", dict(self.where(), line=node.lineno, name=name,
                                           source=self.src(node.exc) if node.exc else "",
                                           **self.touched(node.exc)))
        self.generic_visit(node)

    def visit_If(self, node) -> None:
        self.branch(node, "if", self.src(node.test), node.test)
        self.generic_visit(node)

    def visit_While(self, node) -> None:
        self.branch(node, "while", self.src(node.test), node.test)
        self.generic_visit(node)

    def visit_For(self, node) -> None:
        self.branch(node, "for", self.src(node.iter), node.iter)
        self.generic_visit(node)

    visit_AsyncFor = visit_For

    def branch(self, node, kind: str, test: str, tested) -> None:
        # `if DATA_PATH:` reads the name and produces no expression row of its own, for the same reason a
        # bare `return` does - see `touched`.
        self.rows.add("branches", "br", dict(self.where(), line=node.lineno, kind=kind, test=test,
                                             end_line=getattr(node, "end_lineno", node.lineno),
                                             **self.touched(tested)))

    def visit_Call(self, node) -> None:
        # `target_path` + `target_name` are the def this call RUNS, bound through this file's own imports
        # and scopes - see PyBind. Empty when the file alone cannot say, never a guess by name.
        target_path, target_name = self.binder.resolve(dotted(node.func))
        # `getattr(plugins, name)` runs SOME def of plugins.py and nobody can say which: `*` says every def
        # there may be reached. A literal name is not that - it is `plugins.name`, bound like any call.
        if dotted(node.func) == "getattr" and len(node.args) >= 2:
            name = node.args[1]
            if isinstance(name, ast.Constant) and isinstance(name.value, str):
                target_path, target_name = self.binder.resolve(dotted(node.args[0]) + "." + name.value)
            else:
                target_path = self.binder.module_of(dotted(node.args[0]))
                target_name = "*" if target_path else ""
        callee = dotted(node.func) or self.src(node.func)
        call = self.rows.add("calls", "call", dict(self.where(), line=node.lineno, callee=callee,
                                                   target_path=target_path, target_name=target_name,
                                                   args=len(node.args), kwargs=len(node.keywords),
                                                   source=self.src(node)))
        # ONE ROW PER ARGUMENT, naming the parameter the CALLEE declares - see PyArgs. What the value reads
        # rides along, so "which calls pass FLAG" is a `binds` query like any other.
        for fields, value in arguments(self.binder.tree, node, callee, target_path, target_name, self.src):
            self.rows.add("arguments", "arg", dict(self.where(), call=call, line=node.lineno, **fields,
                                                   reads=facts(value)["reads"]))
        # A CALL THAT BUILDS A REGEX is a `regexes` row too, beside its call row - see PyRegex.
        found = regex_call(self.binder, node, self.src)
        if found:
            assigned = self.value_of is not None and self.value_of[0] is node and self.value_of[1]
            self.rows.add("regexes", "rx", dict(self.where(), line=node.lineno, source=self.src(node),
                                                used_by="= " + assigned if assigned else "", **found))
        self.generic_visit(node)

    def visit_Constant(self, node) -> None:
        add_literal(self.rows, self.where(), node, self.parents, self.src, self.sections.key_of(node))
        self.generic_visit(node)

    def generic_visit(self, node) -> None:
        """Every expression row comes through here, DURING the walk - see `expression`."""
        if isinstance(node, ast.expr):
            self.expression(node)
        super().generic_visit(node)

    def expression(self, node) -> None:
        """One expression worth a row of its own, with its text, its shape and what it touches.

        EMITTED WHILE THE SCOPE STACK IS STANDING, which is the whole difference. These rows used to come
        from a second `ast.walk` run AFTER the visit had finished, and by then the stack was empty: every
        row in a real tree carried `cls=""` and `func=""` - all of them - so the one worked query this
        table exists for, "which functions read this constant", could only ever answer with a file.
        """
        if isinstance(node, (ast.Slice, ast.Name, ast.Constant)):
            return
        size = self.sizes[id(node)]
        if size < MIN_EXPRESSION_NODES:
            return
        source = self.src(node)
        if not source:
            return
        self.rows.add("expressions", "x", dict(self.where(), line=node.lineno,
                                               role=type(node).__name__, source=source,
                                               size=size, shape=shape(node), **facts(node)))


def lines_of(text: str) -> int:
    """The lines a file HAS: a last newline ends the last line, it does not start one more. Counted on NEWLINE
    alone, as `--cat` splits - `splitlines` also breaks on a form feed and `\\u2028`."""
    return text.count(NEWLINE) + (1 if text and not text.endswith(NEWLINE) else 0)


def read_file(rows: Rows, where: Tree, rel: str, text: str, sha: str, entries: set = None) -> None:
    tree = ast.parse(text)
    entry = any(isinstance(n, ast.If) and any(isinstance(s, ast.Name) and s.id == "__name__"
                                              for s in ast.walk(n.test)) for n in tree.body)
    module = os.path.splitext(os.path.basename(rel))[0]
    if module == "__init__":
        module = os.path.basename(os.path.dirname(rel))
    file_id = rows.add("files", "f", {"path": rel, "module": module,
                                      "sha": sha,
                                      "lines": lines_of(text), "entry": 1 if entry or entries is not None else 0,
                                      "doc": (ast.get_docstring(tree) or "")})
    binder = Binder(where, rel, tree)
    rows.binder = binder
    walker = Walker(rows, file_id, text, binder, entries or set())
    walker.parents = parents_of(tree)
    walker.sections = Sections(tree)
    walker.sizes = sizes(tree)
    walker.comments = record_comments(rows, file_id, tree, text)
    # What the file reads ANYWHERE, annotations and `__all__` included: an import's `used`.
    walker.used = used_names(tree)
    walker.visit(tree)
    rows.binder = None


def digest_of(text: str) -> str:
    return hashlib.sha256((ROWS_VERSION + NEWLINE + text).encode("utf-8")).hexdigest()[:16]


def load(rows: Rows, where: Tree, files: list, shas: dict, only: set, entries: dict, deps: dict) -> tuple:
    """Parse the files in `only`, putting what each was bound through in `deps`. Returns (texts kept, how
    many parsed) - the rest are left alone. A file handed by hash is read here, and only if it is parsed."""
    texts = {}
    parsed = 0
    for rel, abs_path, text in files:
        if rel not in only:
            continue
        if text is None:
            text = text_of(abs_path)
            if isinstance(text, OSError):
                emit("MAP-ERROR", rel, 1, "cannot be read - %s" % text)
                shas.pop(rel, None)
                continue
        deps.pop(rel, None)
        sha = shas[rel]
        try:
            deps[rel] = PyDeps.consulting(where, rel, lambda: read_file(rows, where, rel, text, sha, entries.get(rel)))
        except SyntaxError as error:
            # NOT a skip: `ast.parse` gives nothing at all on a syntax error, so every row this file would
            # have contributed is missing, and a file with no rows looks exactly like an empty one.
            emit("MAP-ERROR", rel, error.lineno or 1, "does not parse as python: %s" % error.msg)
            continue
        texts[rel] = text
        parsed += 1
    return texts, parsed


def main() -> int:
    # --roots-file: with several roots, a JSON list of [prefix, dir]. --state: what the database already holds,
    # as the storing end published it. --rows: where the payload goes. --hashes: {rel: content hash} for every
    # file the tree map already hashed. --rebuild reads every file; --reset is the retry, dropping every file
    # this half records.
    args = options({"list-file": None, "root": ".", "roots-file": "", "state": None, "rows": None, "hashes": ""},
                   ("rebuild", "reset"))
    if hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(encoding="utf-8", newline=NEWLINE)

    # HASH FIRST, decide second. PARSING is seconds, and the whole point of an incremental run is to not pay
    # it for a file that has not moved. A file the tree map has already hashed is not even READ unless it is
    # parsed: its sha is that hash under this half's version, and only the rest are read and hashed here.
    handed = handed_of(args.hashes)
    files = []
    shas = {}
    for rel, abs_path in rows_of(args.list_file):
        if rel in handed:
            files.append((rel, abs_path, None))
            shas[rel] = digest_of("tree " + handed[rel])
            continue
        text = text_of(abs_path)
        if isinstance(text, OSError):
            emit("MAP-ERROR", rel, 1, "cannot be read - %s" % text)
            continue
        files.append((rel, abs_path, text))
        shas[rel] = digest_of(text)

    # WHAT IS ALREADY THERE IS THE STORING END'S ANSWER, not this end's. The python side once opened the database
    # and answered for itself; the store is now the one in the exe, the same one the C# half writes
    # through, and a second implementation of "what is recorded" is a second thing to keep in step.
    try:
        with open(args.state, encoding="utf-8") as handle:
            state = json.load(handle)
    except (OSError, ValueError) as error:
        emit("MAP-FATAL", "the database state could not be read - %s" % error)
        return 0

    before = {} if (args.rebuild or args.reset) else dict(state.get("shas") or {})
    # FULL IS ABOUT THE DATABASE, NOT ABOUT MY ROWS. A full run REPLACES the file, and the file may already
    # hold the C# half's rows: a python half that rebuilt because it found no python in there would delete
    # every one of them. So it rebuilds when the database cannot be built on, and only then.
    full = args.rebuild or bool(state.get("rebuild"))
    stale = {rel for rel, sha in shas.items() if before.get(rel) != sha}
    # A FILE THAT DID NOT MOVE IS RE-READ WHEN WHAT IT WAS BOUND THROUGH DID - see PyDeps. Its own sha says
    # nothing about the module it imports having gone to another folder.
    deps = {} if (full or args.reset) else PyDeps.recorded(state)
    deps = {rel: through for rel, through in deps.items() if rel in shas}
    stale |= PyDeps.rebound(deps, stale | (set(before) - set(shas)))

    rows = Rows({} if (full or args.reset) else dict(state.get("counters") or {}))
    roots = roots_of(args.roots_file) or [("", os.path.abspath(args.root))]
    where = Tree(roots, [rel for rel, _, _ in files])
    # A LAUNCHER ENTRY IS READ WITH THE FILE IT NAMES: a changed pyproject.toml reaches the rows of an
    # unchanged cli.py the next time that file is re-read, not before.
    entries = launched(where, [rel for rel, _, _ in files], [])
    texts, parsed = load(rows, where, files, shas, set(shas) if (full or args.reset) else stale, entries, deps)

    # THE TEXT IS NOT SENT. The store reads each file itself off `read`, because the file is already on
    # disk and carrying it would put every byte of the tree through the payload a second time.
    where = dict(rows_of(args.list_file))
    payload = {
        "all": True,
        "first": True,
        "final": True,
        "reset": bool(args.reset),
        "shas": shas,
        "read": [[rel, where[rel]] for rel in sorted(texts) if rel in where],
        "counters": rows.counters,
        "tables": rows.tables,
        "lang": LANG,
        "deps": PyDeps.stored(deps),
        # Every root and its prefix, for a lens handed a path as a person spells it - see `Batch::roots`.
        "roots": json.dumps([[prefix, root] for prefix, root in roots]),
    }
    try:
        with open(args.rows, "w", encoding="utf-8", newline=NEWLINE) as handle:
            # ONE STRING, THEN ONE WRITE. `json.dump` streams through the pure-python encoder, chunk by chunk:
            # ~3x slower than `dumps`, which encodes in C, on a payload of thousands of rows.
            handle.write(json.dumps(payload, ensure_ascii=False))
    except OSError as error:
        emit("MAP-FATAL", "the payload could not be written - %s" % error)
        return 0

    emit("MAP-READ", parsed, len(shas))
    emit("MAP-DONE", len(shas))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
