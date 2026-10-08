"""PyMap.py - the python half of `structuregate --map`.

WHY A SCRIPT AND NOT C#: every row here is read from the `ast` of the interpreter that will RUN the code, so
the map reads the grammar that repo actually executes - a match statement, a PEP 695 type parameter,
whatever the next release adds. A grammar approximated in C# would error-recover into a partial tree and the
map would quietly stop covering the newest files, which is the failure this whole tool exists to avoid.

It is EMBEDDED in structuregate.exe and written to a temp folder on demand, so a consumer still deploys two
files and cannot end up with a map whose python half is missing or stale.

NO PATTERN MATCHING OVER SOURCE TEXT, and the build refuses one (see BanRegex in StructureGate.csproj).

Contract with the caller (structuregate.exe):
  in   --list-file <path>  UTF-8, one file per line: <relative-path><TAB><absolute-path>
       --root <dir>        the tree being mapped; a dotted import is resolved against it
  out  the MAP-* protocol on stdout (see rust/fbtcore/src/mapper/protocol.rs), MAP-DONE last
  exit 0 even when findings exist - the caller owns the verdict.
"""
from __future__ import annotations

import ast
import hashlib
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from PyAst import (NEWLINE, block_span, char_column, dotted, line_table, looked_up, path_parts, sizes, span,  # noqa: E402,F401
                   span_end, span_start, walked)
from PyBind import Binder, Tree, declares, exists_exact, module_path, relative_path  # noqa: E402
from PyLaunch import entry_spec, launched  # noqa: E402
from PyInputs import options  # noqa: E402

# A body this short is not a duplication worth reporting: two one-line wrappers returning the same attribute
# are alike by coincidence. A repeated body starts carrying a DECISION someone must remember to change in
# both places at around three statements.
MIN_BODY_STATEMENTS = 3

# An expression smaller than this is shared vocabulary, not a copy - `x.get(y) or {}` says nothing about
# duplication. Counted in AST NODES so a long variable name cannot make a trivial expression look large.
MIN_EXPRESSION_NODES = 20

MAX_SUMMARY = 160

# The exceptions that mean "this module may legitimately be absent". An import written under one of them is
# OPTIONAL - the file works without it - and counting it as a dependency answers "can these two trees ship
# apart" with a no that is not true.
ABSENT_MODULE = ("ImportError", "ModuleNotFoundError")

# A string shorter than this is not worth testing against the names in the tree: `id`, `ok`, `db` collide
# with everything and would draw an edge out of a coincidence. A string longer than the second is prose.
MIN_LITERAL = 4
MAX_LITERAL = 120

# The calls that reach into a NAMESPACE by string. Not a list of frameworks - a list of the four ways python
# itself turns a string into a binding, which is what makes a string an indirection rather than a word.
INDIRECTION = ("getattr", "setattr", "hasattr", "__import__", "import_module")


def emit(*fields: object) -> None:
    """One protocol record. A field carrying `|` or a newline would split into the wrong slot on the other
    side, so both are folded here rather than parsed around there."""
    parts = []
    for field in fields:
        text = str(field)
        parts.append(text.replace("|", "/").replace("\r", " ").replace("\n", " "))
    print("|".join(parts))


def blanked(node: ast.AST, literals: bool) -> str:
    """`ast.dump` of `node` with every local NAME blanked to `_` - and, with `literals`, every constant to its
    type's name - so two spellings of one shape fingerprint alike.

    WITHOUT THE BLANKING THE DETECTOR MISSES WHAT IT IS FOR: the idiom copied most often in a tree usually appears
    under two or three different variable names. ATTRIBUTE AND KEYWORD NAMES ARE KEPT - `unicodedata.normalize` is
    what makes that expression that expression, while whether its input is called `s` or `word` is not. Literals
    are blanked only for the per-statement shingles (`retry(3)` and `retry(5)` are one statement shape), never for
    the body digest: two bodies that differ in one number are not identical.

    IN PLACE, AND PUT BACK: a deep copy of every body and expression to blank it was a quarter of the half's time,
    and the dump of the blanked tree is the same either way.
    """
    names, constants = [], []
    for n in ast.walk(node):
        if isinstance(n, ast.Name):
            names.append(n)
        elif literals and isinstance(n, ast.Constant):
            constants.append(n)
    kept_names = [n.id for n in names]
    kept_constants = [(c.value, c.kind) for c in constants]
    try:
        for n in names:
            n.id = "_"
        for c in constants:
            c.value, c.kind = type(c.value).__name__, None
        return ast.dump(node, include_attributes=False)
    finally:
        for n, kept in zip(names, kept_names):
            n.id = kept
        for c, (value, kind) in zip(constants, kept_constants):
            c.value, c.kind = value, kind


def digest(node: ast.AST) -> str:
    return hashlib.sha256(blanked(node, False).encode("utf-8")).hexdigest()[:16]


def shingles(body: list) -> str:
    """One digest per top-level STATEMENT of a body, names and literals blanked, as a sorted set. The whole-body
    digest sees a copy with one line added as a different function; rust compares these sets and reports the
    pair that shares most of them (`similar_bodies`)."""
    seen = set()
    for statement in body:
        seen.add(hashlib.sha256(blanked(statement, True).encode("utf-8")).hexdigest()[:8])
    return " ".join(sorted(seen))


def source_lines(text: str) -> int:
    """Non-blank, non-`#`-only lines - the same counter the gate's own python rule uses. DOCSTRINGS COUNT:
    they are content, and a 300-line docstring is still a file that is hard to navigate."""
    total = 0
    for raw in text.split("\n"):
        line = raw.strip()
        if line and not line.startswith("#"):
            total += 1
    return total


def summary(tree: ast.Module) -> str:
    doc = (ast.get_docstring(tree) or "").strip()
    if not doc:
        return ""
    first = doc.split("\n")[0].strip()
    return first[:MAX_SUMMARY] + " ..." if len(first) > MAX_SUMMARY else first


def is_entry(tree: ast.Module) -> bool:
    """A `__main__` guard, or a `main()` at module scope. An entry point is allowed to have no importer, so
    saying so here is what keeps the dead-code finding honest."""
    for node in tree.body:
        if isinstance(node, ast.If):
            for sub in ast.walk(node.test):
                if isinstance(sub, ast.Name) and sub.id == "__name__":
                    return True
        if (isinstance(node, ast.Expr) and isinstance(node.value, ast.Call)
                and isinstance(node.value.func, ast.Name) and node.value.func.id == "main"):
            return True
    return False


def guarded(tree: ast.Module) -> set:
    """The `id()` of every import node sitting under a handler that catches an absent module.

    Read off the TRY, not off the import: an import is optional because of what encloses it, and there is
    nothing on the import node itself that says so.
    """
    out = set()
    for node in walked(tree):
        if not isinstance(node, ast.Try):
            continue
        caught = []
        for handler in node.handlers:
            if isinstance(handler.type, ast.Name):
                caught.append(handler.type.id)
            elif isinstance(handler.type, ast.Tuple):
                caught += [e.id for e in handler.type.elts if isinstance(e, ast.Name)]
        if not any(name in ABSENT_MODULE for name in caught):
            continue
        for stmt in node.body:
            for sub in ast.walk(stmt):
                if isinstance(sub, (ast.Import, ast.ImportFrom)):
                    out.add(id(sub))
    return out


def suffixes(name: str) -> list:
    """The file spellings a dotted module can have, as path SUFFIXES - for a name this root cannot resolve.

    A TREE IS NOT ONE ROOT. `from client_pkg.importer import X`, written under `scripts/`, cannot resolve
    against `scripts/` because the package lives under another root; the fallback - the FIRST segment,
    as a name - matched the package's `__init__.py` and left `importer.py` reported as read by nobody,
    twice in one day. This half cannot fix that either, since it is handed one root at a time. So the
    SPELLINGS are emitted and the join is left to the graph, which is the one place that holds every root.
    """
    parts = name.split(".")
    if len(parts) < 2 or not all(parts):
        return []
    joined = "/".join(parts)
    return [joined + ".py", joined + "/__init__.py"]


def imports(root: str, rel: str, tree: ast.Module) -> None:
    """Every import, wherever it is written, and whether the file already handles its absence.

    A FUNCTION-LEVEL IMPORT COUNTS. In a flat-import tree those are the majority rather than the exception:
    a module-level import of a sibling step is a cycle there, so steps import each other inside the function
    that needs one. A pass reading only module-level imports would report most of such a tree as importing
    nothing, which is the same wrong answer as not asking.
    """
    optional = guarded(tree)
    for node in walked(tree):
        soft = id(node) in optional
        by_path = "MAP-SOFTPATH" if soft else "MAP-PATH"
        by_name = "MAP-SOFT" if soft else "MAP-USE"
        by_suffix = "MAP-SOFTSUFFIX" if soft else "MAP-SUFFIX"
        if isinstance(node, ast.Import):
            for alias in node.names:
                path = module_path(root, alias.name)
                if path:
                    emit(by_path, rel, path)
                else:
                    emit(by_name, rel, alias.name.split(".")[0])
                    for candidate in suffixes(alias.name):
                        emit(by_suffix, rel, candidate)
        elif isinstance(node, ast.ImportFrom):
            if node.level:
                path = relative_path(root, rel, node.level, node.module or "")
                if path:
                    emit(by_path, rel, path)
                continue
            if not node.module:
                continue
            path = module_path(root, node.module)
            if path:
                emit(by_path, rel, path)
            else:
                for candidate in suffixes(node.module):
                    emit(by_suffix, rel, candidate)
            # `from pkg import mod` imports BOTH: running it executes pkg/__init__.py and then binds
            # pkg/mod.py. Emitting only the package would miss the file the code actually calls into, and
            # emitting only the submodule would hide the __init__ that runs first.
            for alias in node.names:
                nested = module_path(root, node.module + "." + alias.name)
                if nested:
                    emit(by_path, rel, nested)
                elif not path:
                    # `from client_pkg import importer` names a submodule under ANOTHER root just as
                    # `from client_pkg.importer import X` does; only the spelling differs.
                    for candidate in suffixes(node.module + "." + alias.name):
                        emit(by_suffix, rel, candidate)
            if not path:
                emit(by_name, rel, node.module.split(".")[0])
                # WHAT THE IMPORT TAKES, so the graph can tell three `util.py` apart: only the one that
                # binds every name here is what this line reads - the others would raise ImportError.
                taken = [alias.name for alias in node.names if alias.name != "*"]
                if taken:
                    emit("MAP-NAMES", rel, node.module, " ".join(taken))


def dynamic(root: str, rel: str, tree: ast.Module) -> None:
    """`importlib.import_module(x)` and `__import__(x)`.

    A LITERAL ARGUMENT IS A REAL EDGE and is resolved like any other import. A COMPUTED one is not guessed
    at - it is REPORTED, because it is exactly the reason a module with no importers may still have one, and
    a blind spot nobody is told about gets acted on as if it were not there.
    """
    for node in walked(tree):
        if not isinstance(node, ast.Call) or not node.args:
            continue
        name = dotted(node.func)
        if name not in ("importlib.import_module", "import_module", "__import__"):
            continue
        first = node.args[0]
        if isinstance(first, ast.Constant) and isinstance(first.value, str):
            path = module_path(root, first.value)
            if path:
                emit("MAP-PATH", rel, path)
            else:
                emit("MAP-USE", rel, first.value.split(".")[0])
            continue
        emit("MAP-COMPUTED", rel, node.lineno, name + "() with a name built at run time")


def keyed(tree: ast.Module) -> list:
    """The string constants USED AS A KEY, and only those.

    WHERE THE STRING SITS IS THE WHOLE EVIDENCE. A word that merely appears somewhere is not a reference to
    anything: emitted from everywhere, `"tasks"` - an ordinary English word and also a package in the tree -
    drew dozens of optional edges out of dict keys and log messages, and a graph that says everything says nothing.
    Three positions carry an actual indirection, and the constants that read as dead sat in them:

      * an argument of a CALL - `__import__(name)`, `getattr(mod, "run")`, `attr="DEMO_PATH"`;
      * anywhere inside a MODULE-LEVEL assignment - a registry, a dispatch table, a step list.

    A DICT LOOKUP IS NOT ONE, even inside such an assignment: `_S["server"]` and `_S.get("worker")` read a
    settings section, and taken as names they made a shared config module look like it imported two apps.
    `literals` drops them - see `looked_up`.
    """
    found = []
    for node in walked(tree):
        if isinstance(node, ast.Call) and dotted(node.func).split(".")[-1] in INDIRECTION:
            found += list(node.args) + [word.value for word in node.keywords]
    for node in tree.body:
        if isinstance(node, (ast.Assign, ast.AnnAssign)) and node.value is not None:
            found.append(node.value)
    return found


def literals(rel: str, tree: ast.Module) -> None:
    """String constants that SPELL A NAME - the only trace a string-keyed lookup leaves in a parse tree.

    `attr="DEMO_PATH"`, `__import__(_name)`, a dispatch table keyed by `"main_" + word`:
    none of those is an import, and several constants read as dead in one session because of it. The literal
    is emitted and NEVER resolved here - matching it against what the tree declares is a join, and the
    graph is the one place that holds every root's declarations.
    """
    seen = set()
    # A LOOKUP KEY AND A PATH SEGMENT ARE DATA: `_S["server"]` reads a section, `join(ROOT, "build")` names a folder.
    data = set(looked_up(tree)) | path_parts(tree)
    for held in keyed(tree):
        for node in ast.walk(held):
            if not isinstance(node, ast.Constant) or not isinstance(node.value, str) or id(node) in data:
                continue
            value = node.value
            if len(value) < MIN_LITERAL or len(value) > MAX_LITERAL or value in seen:
                continue
            if all(part.isidentifier() for part in value.split(".")):
                seen.add(value)
                emit("MAP-LITERAL", rel, value)


def prose(tree: ast.Module) -> set:
    """The `id()` of every string that is a STATEMENT by itself - a docstring, a bare string. It names files
    in sentences ("see PyAst.py"), and an edge drawn from a docstring is an edge drawn from a comment."""
    return {id(node.value) for node in walked(tree)
            if isinstance(node, ast.Expr) and isinstance(node.value, ast.Constant)}


def script_path(value: str) -> str:
    """`tools/build.py`, `.\\app.py` -> the path as a SUFFIX, or "". A launcher names a script by PATH -
    PyInstaller's `Analysis(['app.py'])`, `subprocess.run([python, 'tools/x.py'])` - and the path is the
    edge. Leading `./` and `../` are dropped: the suffix join finds the file under whichever root holds it."""
    path = value.strip().replace("\\", "/")
    if not path.endswith(".py") or " " in path or ":" in path:
        return ""
    parts = [part for part in path.split("/") if part not in ("", ".", "..")]
    return "/".join(parts) if parts and parts[-1] != ".py" else ""


def named_files(root: str, rel: str, tree: ast.Module) -> None:
    """Strings that name a FILE to be launched, as OPTIONAL edges: an entry point or a script path.

    `MAP-LITERAL` cannot carry these - it is joined against module NAMES, and `cli:main_x` is not one, nor is
    `tools/x.py`. A string is evidence, not proof, so both land on the optional side, as a literal does.
    """
    skipped = prose(tree)
    seen = set()
    for node in walked(tree):
        if not isinstance(node, ast.Constant) or not isinstance(node.value, str) or id(node) in skipped:
            continue
        if len(node.value) > MAX_LITERAL:
            continue
        module, attr = entry_spec(node.value)
        script = "" if module else script_path(node.value)
        # AN EXACT PATH FIRST, a suffix only when this root cannot say: `app.py` as a suffix matches every
        # `app.py` in every root, while the one beside the root or beside this file is what a launcher reads.
        if module:
            exact = module_path(root, module)
            # `session:token` has the shape and is a cache key. A module that does not bind the attribute
            # is not what the string names; one this root cannot resolve is left to the suffix join.
            if exact and not declares(Tree([("", root)]), exact, attr.split(".")[0]):
                continue
            joined = module.replace(".", "/")
            spellings = [joined + ".py", joined + "/__init__.py"]
            if not exact:
                # `a:b` with two `a.py` in the tree: the one binding the attribute.
                emit("MAP-NAMES", rel, module, attr.split(".")[0])
        elif script:
            exact = next((os.path.relpath(c, root).replace(os.sep, "/") for c in (
                os.path.join(root, script), os.path.join(root, os.path.dirname(rel), script))
                if exists_exact(c)), "")
            spellings = [script]
        else:
            continue
        for record, spelling in ([("MAP-SOFTPATH", exact)] if exact else
                                 [("MAP-SOFTSUFFIX", s) for s in spellings]):
            if spelling not in seen:
                seen.add(spelling)
                emit(record, rel, spelling)


def registrars(root: str, tree: ast.Module) -> set:
    """Names that stand for something else in this tree - an app, a router, a registry, a CLI.

    Two ways a name gets in: it is BOUND BY AN IMPORT that resolved to a file in the tree, or it is assigned
    at module level FROM A CALL (`app = FastAPI()`), which is how a registry object is made in the first
    place.
    """
    names = set()
    for node in tree.body:
        if isinstance(node, ast.Assign) and isinstance(node.value, ast.Call):
            names.update(t.id for t in node.targets if isinstance(t, ast.Name))
    for node in walked(tree):
        if isinstance(node, ast.Import):
            for alias in node.names:
                if module_path(root, alias.name):
                    names.add(alias.asname or alias.name.split(".")[0])
        elif isinstance(node, ast.ImportFrom):
            inside = node.level > 0 or bool(module_path(root, node.module or ""))
            for alias in node.names:
                if inside or module_path(root, (node.module or "") + "." + alias.name):
                    names.add(alias.asname or alias.name)
    return names


def registrations(root: str, rel: str, tree: ast.Module) -> None:
    """A def HANDED TO an object at import time - `@app.get("/")`, `@router.post`, `@cli.command()`.

    A file of these has no importer BY DESIGN: it is entered over HTTP, or by a CLI dispatch, so reporting
    it as unread says the opposite of what is true. THE TEST IS STRUCTURAL, not a list of framework names:
    the decorator is an ATTRIBUTE OF AN OBJECT this tree makes or imports, which is what handing yourself
    to something looks like. `@functools.lru_cache` is not one - nothing in the tree owns `functools` - and
    neither is a bare `@memoize`, which decorates a function without registering it anywhere.
    """
    objects = registrars(root, tree)
    if not objects:
        return
    for node in walked(tree):
        if not isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef)):
            continue
        for decorator in node.decorator_list:
            call = decorator.func if isinstance(decorator, ast.Call) else decorator
            if not isinstance(call, ast.Attribute):
                continue
            name = dotted(call)
            if name and name.split(".")[0] in objects:
                emit("MAP-REGISTERED", rel, decorator.lineno, name)


def bodies(rel: str, tree: ast.Module, table: dict) -> None:
    """Every function whose body is long enough to pair, with the DOCSTRING REMOVED - so two functions match
    when they DO the same thing however differently they are described."""
    for node in walked(tree):
        if not isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
            continue
        body = node.body
        if (body and isinstance(body[0], ast.Expr) and isinstance(body[0].value, ast.Constant)
                and isinstance(body[0].value.value, str)):
            body = body[1:]
        if len(body) < MIN_BODY_STATEMENTS:
            continue
        block = ast.Module(body=body, type_ignores=[])
        emit("MAP-BODY", rel, node.lineno, node.name, digest(block), block_span(table, body), shingles(body))


def expressions(rel: str, tree: ast.Module, table: dict) -> None:
    """A SECOND UNIT ON PURPOSE, beside the per-function bodies. A body fingerprint only sees a whole
    function, so the most-copied thing in a codebase - an idiom pasted INSIDE larger functions, or a helper
    too short to clear the body threshold - is invisible to it."""
    counted = sizes(tree)
    for node in walked(tree):
        # A slice is not an expression anyone copies as a unit, and it cannot be dumped alone.
        if not isinstance(node, ast.expr) or isinstance(node, ast.Slice):
            continue
        if counted[id(node)] < MIN_EXPRESSION_NODES:
            continue
        emit("MAP-EXPR", rel, node.lineno, digest(node), span(table, node))


def read(root: str, rel: str, abs_path: str) -> None:
    try:
        with open(abs_path, "r", encoding="utf-8", errors="replace") as handle:
            text = handle.read()
    except OSError as error:
        emit("MAP-ERROR", rel, 1, "cannot be read - %s" % error)
        return

    emit("MAP-LINES", rel, source_lines(text))
    try:
        tree = ast.parse(text)
    except SyntaxError as error:
        # NOT a skip. `ast.parse` gives nothing at all on a syntax error, so every edge this file has is
        # missing from the map - and a file with no edges looks exactly like a file with no imports.
        emit("MAP-ERROR", rel, error.lineno or 1,
             "does not parse as python: %s - nothing in this file is in the map until it does" % error.msg)
        return

    # THE MODULE NAME, so a flat-import tree resolves: every tree is on sys.path there and siblings are
    # imported by bare name. A package import is resolved by PATH instead, in `imports` above.
    #
    # A `__init__.py` IS ITS FOLDER. Declaring the literal name `__init__` would give every package in the
    # tree the same one, and then report each of them as read by nobody - the file is reached by the
    # PACKAGE name, which is the directory it sits in.
    stem = os.path.splitext(os.path.basename(rel))[0]
    if stem == "__init__":
        stem = os.path.basename(os.path.dirname(rel))
    if stem:
        emit("MAP-DECL", rel, stem)
    # EVERYTHING THIS FILE BINDS AT MODULE LEVEL - defs, classes, imports, assignments - which is what a
    # `from <this> import name` can take from it. The graph matches MAP-NAMES against it.
    bound = sorted(Binder(Tree([("", root)]), rel, tree).scopes[0][2])
    if bound:
        emit("MAP-BINDS", rel, " ".join(bound))
    text_summary = summary(tree)
    if text_summary:
        emit("MAP-SUMMARY", rel, text_summary)
    if is_entry(tree):
        emit("MAP-ENTRY", rel)
    table = line_table(text)
    imports(root, rel, tree)
    dynamic(root, rel, tree)
    literals(rel, tree)
    named_files(root, rel, tree)
    registrations(root, rel, tree)
    bodies(rel, tree, table)
    expressions(rel, tree, table)


def rows(list_file: str) -> list:
    out = []
    with open(list_file, "r", encoding="utf-8") as handle:
        for raw in handle:
            if not raw.strip():
                continue
            parts = raw.rstrip("\n").split("\t")
            if len(parts) < 2:
                continue
            out.append((parts[0].strip(), parts[1].strip()))
    return out


def main() -> int:
    # --known-file is EVERY FILE OF THE MAP, when --list-file is only the files to parse: the rest are answered
    # from the caller's cache, and an entry point a launcher names is still found among them.
    args = options({"list-file": None, "root": ".", "known-file": ""})

    if hasattr(sys.stdout, "reconfigure"):
        # The caller reads UTF-8. Without this a console codepage mangles every non-ASCII summary.
        sys.stdout.reconfigure(encoding="utf-8", newline="\n")

    root = os.path.abspath(args.root)
    try:
        files = rows(args.list_file)
        known = rows(args.known_file) if args.known_file else files
    except OSError as error:
        emit("MAP-FATAL", "the file list could not be read - %s" % error)
        return 0

    for rel, abs_path in files:
        read(root, rel, abs_path)
    # AFTER every file: a launcher names a file this half was given, never one it was not.
    notes = []
    # ONE ROOT PER RUN HERE, so the tree is that root and nothing else - the join across roots is the
    # graph's, in rust/fbtcore/src/graph/.
    # `launched` MARKS THE ENTRY AS THE RUN'S, not the file's: it comes from a launcher (`pyproject.toml`, a `.spec`)
    # over every file, so the caller keeps it apart from what a file says of itself (a script's own MAP-ENTRY).
    for rel in sorted(launched(Tree([("", root)]), [rel for rel, _ in known], notes)):
        emit("MAP-ENTRY", rel, "launched")
    for note in notes:
        emit("MAP-NOTE", note)
    emit("MAP-DONE", len(files))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
