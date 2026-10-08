"""PyBind.py - which FILE a python name is, for both python halves.

WHY A FILE OF ITS OWN. `PyMap.py` resolves an import to a file for the file-level graph, and `PyRows.py`
has to resolve a CALL to the def it runs for the deep map. Both are the same question - which file does
this dotted name reach - and two copies of the answer are two places for the case-sensitivity rule below
to be got wrong.

A CALL IS BOUND THROUGH THE FILE'S OWN SCOPES, NEVER BY ITS BARE NAME. `run()` in a file that imports
`run` from `jobs.py` calls `jobs.py::run`, and a second `run` in `legacy.py` is not called by it. Joined by
name, the dead `legacy.py::run` looked live for as long as anything called any `run`. A name this pass
cannot bind - an instance attribute, a parameter, an inherited method - stays UNBOUND rather than guessed:
a caller that asks "is this def called" has to count an unbound call of the same name as a maybe.

It is IMPORTED, never launched, so it is staged beside both halves - it is in both python sets of
`rust/fbtcore/src/embedded/mod.rs`, which compiles every script into the exe.
"""
from __future__ import annotations

import ast
import os

import PyDeps
from PyAst import dotted

DIRECTORY_CACHE = {}

# What each file binds at module level, parsed once per run - see `module_scope`.
MODULE_SCOPE = {}

# What each top-level class of a file defines in its own body, by the same key - see `is_def`.
CLASS_MEMBERS = {}

# The module-level Binder of each file, by the same key, so a name bound THERE is resolved there - see
# `module_binder`.
MODULE_BINDER = {}

# How many re-exports are followed before giving up. A chain longer than this is a cycle in practice.
MAX_HOPS = 8

# The names a method reaches its own class through. Not a keyword in python, only the convention every
# tree follows - which is why a method written `def go(this)` simply stays unbound.
RECEIVERS = ("self", "cls")


def exists_exact(candidate: str) -> bool:
    """Is there a file at this path with THIS EXACT NAME?

    `os.path.isfile` answers yes on a case-insensitive filesystem for a name that differs only in case, and
    that turns a NAME import into a MODULE one: `from registry import STORE` found `registry/store.py`
    on Windows and drew an edge to a file the code never imports - the real `STORE` is an object inside the
    package's `__init__.py`. The directory listing is the only authority on how a file is really spelled.
    """
    # A MISS IS RECORDED TOO: a module that does not exist yet is exactly the file that, once written,
    # changes what this import binds.
    PyDeps.note(candidate)
    return os.path.basename(candidate) in listing(os.path.dirname(candidate) or ".")


def listing(folder: str) -> set:
    """The names in a folder, listed once per run."""
    names = DIRECTORY_CACHE.get(folder)
    if names is None:
        try:
            names = set(os.listdir(folder))
        except OSError:
            names = set()
        DIRECTORY_CACHE[folder] = names
    return names


def module_path(root: str, dotted: str) -> str:
    """A dotted import as the FILE it names, or "" - `pkg.mod` -> `pkg/mod.py` or `pkg/mod/__init__.py`.

    Tried before the flat name join in `PyMap.py`, and the two answer different layouts: a package import
    resolves here exactly, while a flat bootstrap (every tree on sys.path, siblings imported by bare name)
    resolves by name there. Guessing between them is not needed - the filesystem says which one this is.
    """
    parts = dotted.split(".")
    base = os.path.join(root, *parts)
    for candidate in (base + ".py", os.path.join(base, "__init__.py")):
        if exists_exact(candidate):
            return os.path.relpath(candidate, root).replace(os.sep, "/")
    return ""


def relative_path(root: str, rel: str, level: int, module: str) -> str:
    """`from ..pkg import x`, as the file it names. The level counts directories UP from the importing
    file's own package, which is what makes a relative import unambiguous where a bare name is not."""
    folder = os.path.dirname(os.path.join(root, rel))
    for _ in range(level - 1):
        folder = os.path.dirname(folder)
    parts = module.split(".") if module else []
    base = os.path.join(folder, *parts) if parts else folder
    for candidate in (base + ".py", os.path.join(base, "__init__.py")):
        if exists_exact(candidate):
            return os.path.relpath(candidate, root).replace(os.sep, "/")
    return ""


class Tree:
    """Where each mapped rel lives on disk, for ONE ROOT OR SEVERAL.

    A tree mapped with several `--root`s names its files with a prefix per root - `scripts/lib/x.py`,
    `core/text.py` - and each root is on `sys.path` in that project. Resolved against the first root
    alone, `client_pkg.http` imported from `scripts/` bound to nothing, and `_paths` bound to
    `_paths.py` - a path no `files` row carries. Every lookup here goes to disk through the root that
    owns the importer and comes back as a rel with that root's prefix on it.
    """

    def __init__(self, roots: list, rels: list = ()) -> None:
        self.roots = [(prefix, os.path.abspath(folder)) for prefix, folder in roots]
        # The LONGEST prefix and the DEEPEST folder claim first, so a root nested in another wins its files.
        self.by_prefix = sorted(self.roots, key=lambda root: -len(root[0]))
        self.by_folder = sorted(self.roots, key=lambda root: -len(root[1]))
        # Every mapped file by its last name, for `unique` below.
        self.by_name: dict = {}
        for rel in rels:
            self.by_name.setdefault(rel.rsplit("/", 1)[-1], []).append(rel)

    def root_of(self, rel: str):
        for prefix, folder in self.by_prefix:
            if rel.startswith(prefix) or rel + "/" == prefix:
                return prefix, folder
        return None

    def abs_of(self, rel: str) -> str:
        owner = self.root_of(rel)
        if owner is None:
            return ""
        tail = rel[len(owner[0]):] if rel.startswith(owner[0]) else ""
        return os.path.join(owner[1], *tail.split("/")) if tail else owner[1]

    def rel_of(self, path: str) -> str:
        path = os.path.abspath(path)
        for prefix, folder in self.by_folder:
            try:
                inside = os.path.relpath(path, folder)
            except ValueError:
                # Another drive: `relpath` has no answer, and this root does not hold the file.
                continue
            if inside != ".." and not inside.startswith(".." + os.sep):
                return prefix + ("" if inside == "." else inside.replace(os.sep, "/"))
        return ""

    def found(self, base: str) -> str:
        """`base.py` or `base/__init__.py` as a rel, or ""."""
        for candidate in (base + ".py", os.path.join(base, "__init__.py")):
            if exists_exact(candidate):
                return self.rel_of(candidate)
        return ""

    def module(self, rel: str, dotted: str, names: tuple = ()) -> str:
        """A dotted module the way `sys.path` finds it here: the importer's own root, then the IMPORTER'S
        OWN FOLDER - the sibling a flat tree reads, because a script's folder is first on `sys.path` - and
        then every other root. `names` are what the import takes from it, for `unique`."""
        parts = dotted.split(".")
        owner = self.root_of(rel)
        if owner is not None:
            hit = self.found(os.path.join(owner[1], *parts))
            if hit:
                return hit
        home = os.path.dirname(self.abs_of(rel))
        if home:
            hit = self.found(os.path.join(home, *parts))
            if hit:
                return hit
        for root in self.roots:
            if root != owner:
                hit = self.found(os.path.join(root[1], *parts))
                if hit:
                    return hit
        return self.unique(parts, names)

    def unique(self, parts: list, names: tuple = ()) -> str:
        """The ONE mapped file this module can be, wherever it sits, or "".

        A BOOTSTRAP PUTS FOLDERS ON `sys.path` THAT NO ROOT NAMES. `from shared import ...` works in
        `lib/` only because the project adds `vendor/` at start-up, which no parse tree
        says - so the re-export bound to nothing and the def behind it read as dead. The file-level graph
        answers such a name with the same join: exactly one file spelling it is the answer, two are none.
        """
        joined = "/".join(parts)
        found = set()
        for spelling in (joined + ".py", joined + "/__init__.py"):
            PyDeps.note(PyDeps.ANY + spelling)
            for rel in self.by_name.get(spelling.rsplit("/", 1)[-1], []):
                if rel == spelling or rel.endswith("/" + spelling):
                    found.add(rel)
        if len(found) > 1 and names:
            # SEVERAL FILES SPELL IT, AND THE IMPORT SAYS WHICH. `from util import alpha,
            # beta` has several `util.py` to choose from and exactly one of them binds both names. The
            # others would raise ImportError, so they are not what this line reads.
            found = {rel for rel in found if all(declares(self, rel, name) for name in names)}
        return found.pop() if len(found) == 1 else ""

    def relative(self, rel: str, level: int, module: str) -> str:
        """`from ..pkg import x`, counted up from the importing file's own folder."""
        folder = os.path.dirname(self.abs_of(rel))
        for _ in range(level - 1):
            folder = os.path.dirname(folder)
        parts = module.split(".") if module else []
        return self.found(os.path.join(folder, *parts)) if folder else ""

    def descend(self, rel: str, rest: list) -> tuple:
        """`pkg/__init__.py` + `["sub", "go"]` -> (`pkg/sub.py`, "go"): an attribute of a PACKAGE may be one
        of its submodules, and the def lives in that file rather than in the `__init__`."""
        while rest and rel.endswith("__init__.py"):
            hit = self.found(os.path.join(os.path.dirname(self.abs_of(rel)), rest[0]))
            if not hit:
                break
            rel, rest = hit, rest[1:]
        return rel, ".".join(rest)


def module_scope(tree: Tree, rel: str) -> dict:
    """What `rel` binds at module level - its defs, its imports, its assignments - or {} when it cannot be
    read. Cached by the FILE ON DISK, and marked empty BEFORE it is parsed, so a cycle of re-exports ends
    instead of recursing."""
    path = tree.abs_of(rel)
    PyDeps.note(path)
    if path in MODULE_SCOPE:
        PyDeps.cached(path)
        return MODULE_SCOPE[path]
    MODULE_SCOPE[path] = {}
    CLASS_MEMBERS[path] = {}
    PyDeps.reading(path)
    try:
        return scope_of(tree, rel, path)
    finally:
        PyDeps.closing()


def scope_of(tree: Tree, rel: str, path: str) -> dict:
    """`module_scope`'s parse, once the cache has missed."""
    try:
        with open(path, "r", encoding="utf-8", errors="replace") as handle:
            parsed = ast.parse(handle.read())
    except (OSError, SyntaxError, ValueError):
        return {}
    CLASS_MEMBERS[path] = {node.name: {n.name for n in node.body
                                       if isinstance(n, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef))}
                           for node in parsed.body if isinstance(node, ast.ClassDef)}
    binder = Binder(tree, rel, parsed)
    MODULE_BINDER[path] = binder
    MODULE_SCOPE[path] = binder.scopes[0][2]
    return MODULE_SCOPE[path]


def module_binder(tree: Tree, rel: str):
    """The module-level Binder of `rel`, or None when it cannot be read."""
    module_scope(tree, rel)
    return MODULE_BINDER.get(tree.abs_of(rel))


def is_def(tree: Tree, rel: str, qual: str) -> bool:
    """Is `qual` a def or class that `rel` writes itself - `run`, or `Thing.go` with `go` in Thing's body?

    AN ATTRIBUTE OF A CLASS IS NOT ALWAYS ITS METHOD. `Engine._cache` is inherited or assigned,
    and `Box._SLOTS.get` is a dict's; bound as if they were defs, their reads counted as explained and the
    method they really reach could read as dead. Deeper than one member is not placed at all.
    """
    head, _, tail = qual.partition(".")
    if module_scope(tree, rel).get(head) != ("def", rel, head):
        return False
    if not tail:
        return True
    return "." not in tail and tail in CLASS_MEMBERS.get(tree.abs_of(rel), {}).get(head, set())


def declares(tree: Tree, rel: str, name: str) -> bool:
    """Does `rel` bind `name` at module level at all - a def, a class, an import or an assignment?"""
    return name in module_scope(tree, rel)


def follow(tree: Tree, rel: str, qual: str) -> tuple:
    """(file, qualname) where a name imported FROM `rel` is really defined.

    A PACKAGE RE-EXPORTS. `from rx import run` binds to `rx/__init__.py`, and that file only says
    `from .impl import run` - the def is in `rx/impl.py`. Stopping at the `__init__` bound the call to a file
    that defines nothing, and the def it really runs read as called by nobody.
    """
    head, _, tail = qual.partition(".")
    for _ in range(MAX_HOPS):
        binding = module_scope(tree, rel).get(head)
        if not binding:
            break
        if binding[0] == "path" and tail:
            rel = binding[1]
            head, _, tail = tail.partition(".")
            continue
        if binding[0] != "def" or (binding[1] == rel and binding[2] == head):
            break
        rel, head = binding[1], binding[2]
    return rel, head + ("." + tail if tail else "")


class Binder:
    """The names each scope of ONE file binds, and the `(file, qualname)` a callee resolves to.

    A binding is ("mod", dotted) for an absolute import, ("path", file) for a relative one, ("def", file,
    qualname) for a def, a class or a name imported from a module, ("ext", module, name) for a name
    imported from a module outside the tree, and None for anything else - an assignment, a parameter, a
    loop variable - which SHADOWS whatever the name meant further out.
    """

    def __init__(self, tree: Tree, rel: str, module: ast.Module) -> None:
        self.tree = tree
        self.rel = rel
        # The calls being typed right now - see `typed`.
        self.typing: set = set()
        self.scopes: list = [("module", "", self.names(module.body, "", []))]

    def enter_function(self, node, qualname: str) -> None:
        spec = node.args
        params = [a.arg for a in spec.posonlyargs + spec.args + spec.kwonlyargs]
        params += [a.arg for a in (spec.vararg, spec.kwarg) if a is not None]
        self.scopes.append(("func", qualname, self.names(node.body, qualname + ".", params)))

    def enter_class(self, node, qualname: str) -> None:
        methods = {n.name for n in node.body if isinstance(n, (ast.FunctionDef, ast.AsyncFunctionDef))}
        self.scopes.append(("class", qualname, methods))

    def leave(self) -> None:
        self.scopes.pop()

    def names(self, body: list, prefix: str, params: list) -> dict:
        """What a block binds, in source order so a later binding wins. A nested def or class binds its
        NAME here and nothing inside it; a comprehension or a lambda has a scope of its own."""
        bound = {name: None for name in params}
        pending = list(reversed(body))
        while pending:
            node = pending.pop()
            if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef)):
                bound[node.name] = ("def", self.rel, prefix + node.name)
                continue
            if isinstance(node, (ast.Lambda, ast.ListComp, ast.SetComp, ast.DictComp, ast.GeneratorExp)):
                continue
            if isinstance(node, (ast.Import, ast.ImportFrom)):
                bound.update(self.imported(node))
                continue
            target = node.targets[0] if isinstance(node, ast.Assign) and len(node.targets) == 1 else (
                node.target if isinstance(node, ast.AnnAssign) else None)
            if isinstance(target, ast.Name) and isinstance(getattr(node, "value", None), ast.Call):
                # `STORE = Store()` - the value is an instance of whatever the call names.
                bound[target.id] = self.variable(prefix, target.id, dotted(node.value.func))
                pending.append(node.value)
                continue
            if isinstance(target, ast.Name) and isinstance(getattr(node, "value", None), (ast.Name, ast.Attribute)) \
                    and dotted(node.value):
                # `_strip = fold` - the name IS another one, and a call through it runs what that one is.
                bound[target.id] = self.variable(prefix, target.id, "", dotted(node.value))
                continue
            if isinstance(node, ast.Name) and isinstance(node.ctx, ast.Store):
                bound[node.id] = self.variable(prefix, node.id, "")
            elif isinstance(node, ast.ExceptHandler) and node.name:
                bound[node.name] = None
            pending.extend(reversed(list(ast.iter_child_nodes(node))))
        return bound

    def variable(self, prefix: str, name: str, call: str, alias: str = ""):
        """An assigned name. AT MODULE LEVEL IT IS A SYMBOL another file can import and read -
        ("var", file, name, call, alias) - where a local is not. `call` is what made its value, which is how
        `STORE.get()` finds `Store.get`; `alias` is the name its value simply IS, which is
        how `_strip()` finds `fold` after `_strip = fold`. A local that is neither is None."""
        if not prefix:
            return ("var", self.rel, name, call, alias)
        return ("var", "", "", call, alias) if call or alias else None

    def imported(self, node) -> dict:
        out = {}
        if isinstance(node, ast.Import):
            for alias in node.names:
                if alias.asname:
                    out[alias.asname] = ("mod", alias.name)
                else:
                    head = alias.name.split(".")[0]
                    out[head] = ("mod", head)
            return out
        module = node.module or ""
        taken = tuple(alias.name for alias in node.names if alias.name != "*")
        for alias in node.names:
            if alias.name == "*":
                continue
            nested = (module + "." if module else "") + alias.name
            if node.level:
                sub = self.tree.relative(self.rel, node.level, nested)
                base = self.tree.relative(self.rel, node.level, module)
            else:
                sub = self.tree.module(self.rel, nested)
                base = self.tree.module(self.rel, module, taken)
            if sub:
                out[alias.asname or alias.name] = ("path", sub)
            elif base:
                out[alias.asname or alias.name] = ("def", base, alias.name)
            elif node.level:
                # An external name is still a binding: it shadows a def of the same name further out.
                out[alias.asname or alias.name] = None
            else:
                # ...and an absolute one keeps the module it came from: `c` after `from re import compile
                # as c` is how PyRegex knows the call builds a regex. Nothing here resolves through it.
                out[alias.asname or alias.name] = ("ext", module, alias.name)
        return out

    def resolve(self, callee: str) -> tuple:
        """(file, qualname) the callee runs, or ("", "") when the tree cannot say."""
        path, qual = self.bind(callee)
        if not path:
            return "", ""
        if path != self.rel:
            path, qual = follow(self.tree, path, qual)
        head, _, tail = qual.partition(".")
        binding = module_scope(self.tree, path).get(head)
        if binding and binding[0] == "var" and binding[1] == path and binding[2] == head:
            # AN INSTANCE HAS A CLASS WHEN A CALL MADE IT. `STORE.get()` - hundreds of calls of this shape on
            # one tree - reaches `Store.get` because store.py says `STORE = Store()`.
            # One member deep only, and only a member the class body writes: an assignment from a
            # function, or an inherited method, is not placed.
            owner = module_binder(self.tree, path)
            if owner is None:
                return "", ""
            # AN ALIAS IS THE NAME IT IS BOUND TO. `_strip = fold` in helpers.py left several `_strip()`
            # calls unbound although the import of `_strip` bound: the value had no call to type it by.
            if len(binding) > 4 and binding[4]:
                return owner.through(binding[4], tail)
            return owner.typed(binding[3], [tail] if tail else [])
        # THE HEAD MUST BE A DEF OF THAT FILE. `state.handler.run()` reached state.py, where
        # `handler` is an instance of a class state.py does not name - so `handler.run` is no
        # def at all, and binding to it made the real `Handler.run` read as called by nobody.
        if path != self.rel and not is_def(self.tree, path, qual):
            return "", ""
        return path, qual

    def through(self, alias: str, tail: str) -> tuple:
        """What `alias` (+ `.tail`) resolves to in this binder's scope, guarded like `typed` against a name
        that is, through a chain of aliases, itself (`a = b` then `b = a`)."""
        if alias in self.typing:
            return "", ""
        self.typing.add(alias)
        try:
            return self.resolve(alias + ("." + tail if tail else ""))
        finally:
            self.typing.discard(alias)

    def typed(self, call: str, rest: list) -> tuple:
        """`rest[0]` as a member of the class `call` names, or ("", "")."""
        # A VALUE MADE FROM ITSELF HAS NO CLASS TO FIND. `name = name.strip()` types `name` by `name.strip`,
        # which is `name` again - resolved blindly that recursed until the deep map hung on a real tree.
        if not call or len(rest) != 1 or call in self.typing:
            return "", ""
        self.typing.add(call)
        try:
            path, qual = self.resolve(call)
        finally:
            self.typing.discard(call)
        if path and "." not in qual and is_def(self.tree, path, qual + "." + rest[0]):
            return path, qual + "." + rest[0]
        return "", ""

    def resolve_read(self, name: str) -> tuple:
        """(file, qualname) of the SYMBOL a read names - a def, a class, a member, or a module-level
        variable such as `ROOT` - or ("", "").

        A NAME READ IS A NAME THIS FILE BINDS, not a word. Thousands of reads on one tree named something
        two or more files define - `ROOT`, `write`, `load`, `register` - and matched by name, a read of
        config.py's `ROOT` was a read of every `ROOT`.
        """
        path, qual = self.resolve(name)
        if path:
            return path, qual
        path, qual = self.bind(name)
        if not path or "." in qual:
            return "", ""
        if path != self.rel:
            path, qual = follow(self.tree, path, qual)
        binding = module_scope(self.tree, path).get(qual)
        if binding and binding[0] == "var" and binding[1] == path and binding[2] == qual:
            return path, qual
        return "", ""

    def module_of(self, dotted_name: str) -> str:
        """The FILE a dotted name is when it names a whole module, or "" - `getattr(plugins, name)` reaches
        into that file's namespace, and only a module has one this tree can list."""
        parts = dotted_name.split(".") if dotted_name else []
        binding = self.lookup(parts[0]) if parts and all(parts) else None
        if binding is None or binding[0] in ("def", "var", "ext"):
            return ""
        if binding[0] == "path":
            path, qual = self.tree.descend(binding[1], parts[1:])
            return "" if qual else path
        return self.tree.module(self.rel, ".".join(binding[1].split(".") + parts[1:]))

    def bind(self, callee: str) -> tuple:
        parts = callee.split(".") if callee else []
        if not parts or not all(parts):
            return "", ""
        if parts[0] in RECEIVERS and len(parts) == 2:
            owner = next((s for s in reversed(self.scopes) if s[0] == "class"), None)
            if owner is not None and parts[1] in owner[2]:
                return self.rel, owner[1] + "." + parts[1]
            return "", ""
        binding = self.lookup(parts[0])
        if binding is None or binding[0] == "ext":
            return "", ""
        rest = parts[1:]
        if binding[0] == "def":
            return binding[1], ".".join([binding[2]] + rest)
        if binding[0] == "var":
            # A module-level variable is a symbol of its file; a local one is only its value's class, or the
            # name it aliases.
            if binding[1]:
                return binding[1], ".".join([binding[2]] + rest)
            if len(binding) > 4 and binding[4]:
                return self.through(binding[4], ".".join(rest))
            return self.typed(binding[3], rest)
        if binding[0] == "path":
            path, qual = self.tree.descend(binding[1], rest)
            return (path, qual) if qual else ("", "")
        names = binding[1].split(".") + rest
        for cut in range(len(names) - 1, 0, -1):
            path = self.tree.module(self.rel, ".".join(names[:cut]))
            if path:
                return path, ".".join(names[cut:])
        return "", ""

    def lookup(self, name: str):
        """Innermost function outwards, then the module. A CLASS BODY IS NOT AN ENCLOSING SCOPE: a method
        cannot name a sibling method bare, which is exactly why it writes `self.` in front of it."""
        for kind, _, bound in reversed(self.scopes):
            if kind != "class" and name in bound:
                return bound[name]
        return None
