"""PyLaunch.py - the python files a LAUNCHER enters, which no import in the tree ever names.

An installer runs `cli:main` because `pyproject.toml` says so; PyInstaller freezes `app.py` because a `.spec`
lists it; `setup(entry_points=...)` does the same as the first. None of the three is an import, so each
target read as a file nothing reads and a def nothing calls - an entry point reported as dead code.

EVERY ONE OF THEM IS PARSED, never scanned: `pyproject.toml` by `tomllib` (python 3.11+; an older host maps
without it, and says so once), a `.spec` and a `setup.py` by `ast`, since both are python.

It is IMPORTED, never launched, and staged and embedded with the two halves that use it - see PyBind.py.
"""
from __future__ import annotations

import ast
import os

from PyAst import dotted
from PyBind import Tree, listing


def entry_spec(value: str) -> tuple:
    """`cli:main_x` -> (`cli`, `main_x`), `x = pkg.cli:main [extra]` -> (`pkg.cli`, `main`), else ("", "").

    THE SHAPE IS PYTHON'S OWN ENTRY-POINT SYNTAX - `console_scripts`, `[project.scripts]`, uvicorn's
    `app:app`. BOTH SIDES MUST BE DOTTED IDENTIFIERS, which is what keeps `localhost:8080` and `http://x` out.
    """
    spec = value.split("=")[-1].strip().split(" ")[0].split("[")[0]
    module, colon, attr = spec.partition(":")
    if not colon or not module or not attr:
        return "", ""
    for dotted_name in (module, attr):
        if not all(part.isidentifier() for part in dotted_name.split(".")):
            return "", ""
    return module, attr


def folders(rels: list) -> list:
    """The root and every folder above a listed file - where a `pyproject.toml` or a `.spec` can sit."""
    seen = {""}
    for rel in rels:
        folder = os.path.dirname(rel)
        while folder and folder not in seen:
            seen.add(folder)
            folder = os.path.dirname(folder)
    return sorted(seen)


def target(tree: Tree, folder: str, module: str, listed: set) -> str:
    """The listed file an entry's module is, from the folder declaring it or its `src/` layout."""
    home = tree.abs_of(folder)
    for base in (home, os.path.join(home, "src")) if home else ():
        rel = tree.found(os.path.join(base, *module.split(".")))
        if rel in listed:
            return rel
    return ""


def pyproject_specs(home: str, folder: str, notes: list) -> list:
    try:
        import tomllib
    except ImportError:
        notes.append("pyproject.toml is not read: this python has no tomllib (3.11+)")
        return []
    try:
        with open(os.path.join(home, "pyproject.toml"), "rb") as handle:
            data = tomllib.load(handle)
    except (OSError, ValueError) as error:
        notes.append("%s/pyproject.toml cannot be read - %s" % (folder or ".", error))
        return []
    project = data.get("project") or {}
    tables = [project.get("scripts") or {}, project.get("gui-scripts") or {}]
    tables += [group for group in (project.get("entry-points") or {}).values() if isinstance(group, dict)]
    tables.append(((data.get("tool") or {}).get("poetry") or {}).get("scripts") or {})
    specs = []
    for table in tables:
        for value in table.values():
            if isinstance(value, dict):
                value = value.get("callable") or value.get("reference") or ""
            if isinstance(value, str):
                specs.append(value)
    return specs


def setup_specs(tree: ast.Module) -> list:
    """The strings under `setup(entry_points=...)` - whatever shape the mapping is written in."""
    specs = []
    for node in ast.walk(tree):
        if not isinstance(node, ast.Call) or dotted(node.func).split(".")[-1] != "setup":
            continue
        for word in node.keywords:
            if word.arg == "entry_points":
                specs += [c.value for c in ast.walk(word.value)
                          if isinstance(c, ast.Constant) and isinstance(c.value, str)]
    return specs


def spec_scripts(tree: ast.Module) -> list:
    """The scripts a PyInstaller `.spec` freezes: the first argument of `Analysis(...)`."""
    scripts = []
    for node in ast.walk(tree):
        if (isinstance(node, ast.Call) and dotted(node.func).split(".")[-1] == "Analysis" and node.args
                and isinstance(node.args[0], (ast.List, ast.Tuple))):
            scripts += [e.value for e in node.args[0].elts
                        if isinstance(e, ast.Constant) and isinstance(e.value, str)]
    return scripts


def parse(path: str):
    try:
        with open(path, "r", encoding="utf-8", errors="replace") as handle:
            return ast.parse(handle.read())
    except (OSError, SyntaxError, ValueError):
        return None


def launched(tree: Tree, rels: list, notes: list) -> dict:
    """{listed rel -> the qualnames a launcher calls in it}. An empty set is a file run as a SCRIPT."""
    listed = set(rels)
    out: dict = {}

    def entries(folder: str, specs: list) -> None:
        for value in specs:
            module, attr = entry_spec(value)
            rel = target(tree, folder, module, listed) if module else ""
            if rel:
                out.setdefault(rel, set()).add(attr)

    for folder in folders(rels):
        # A folder no root owns - the parent of several roots - is on no one's disk as a mapped folder.
        home = tree.abs_of(folder)
        names = listing(home) if home else set()
        if "pyproject.toml" in names:
            entries(folder, pyproject_specs(home, folder, notes))
        for name in sorted(n for n in names if n.endswith(".spec")):
            parsed = parse(os.path.join(home, name))
            for script in spec_scripts(parsed) if parsed else []:
                rel = tree.rel_of(os.path.normpath(os.path.join(home, script)))
                if rel in listed:
                    out.setdefault(rel, set())
    for rel in rels:
        if os.path.basename(rel) == "setup.py":
            parsed = parse(tree.abs_of(rel))
            if parsed:
                entries(os.path.dirname(rel), setup_specs(parsed))
    return out
