"""PyDeps.py - what a python file's rows were BOUND THROUGH, so a file that did not change is re-read
when what it names did.

WHY. The deep map re-reads a file when its own bytes move, and a row can name ANOTHER file: a call's
`target_path` is the def it runs, found through the file's imports. `a.py` kept pointing at
`b/c.py` after that module moved to `d/`, so the def it really calls
read as dead until the database was deleted by hand. Nothing in `a.py` had changed.

WHAT IS RECORDED is everything a resolution LOOKED AT, found or not: every file whose module scope was
read (a re-export chain passes through each `__init__.py` on the way), every candidate path probed on
disk (a module that did not exist yet is a path that was probed and missed), and every name joined by
`Tree.unique` as `*/<spelling>`, since any file spelling it may change that answer. It is TRANSITIVE: what
a re-exporting file's own imports probed is part of the answer for every file that reads through it.

STORED INTERNED, never cut. Every file of a package probes the same few hundred paths, and written out per
file the record was a large share of the database - every key of it needed, since dropping a probe is a file
not re-read when that path appears. So each key is written ONCE, in `keys`, and a file names its keys by
position: the same record, a fraction of the bytes.

It is IMPORTED by `PyBind.py`, so it is staged and embedded wherever that one is. It also holds the PROBES themselves
(`exists_exact` and the two import-to-file helpers), which note every path they ask the disk about.
"""
from __future__ import annotations

import json
import os

# A stack, one set per resolution in progress: reading one file parses another, and what that one
# consulted belongs to both.
CONSULTED = []

# What each file's OWN module-level binding consulted, by the same key as `PyBind.MODULE_SCOPE`. A scope
# is parsed once per run, so a second reader of it gets this instead of the probes it did not repeat.
SCOPE_DEPS = {}

# A dependency on every file that spells this name, rather than on one path.
ANY = "*/"


def note(key: str) -> None:
    if CONSULTED:
        CONSULTED[-1].add(key)


def reading(path: str) -> set:
    """Open the record of `path`'s own module scope - see `closing`."""
    own = SCOPE_DEPS.setdefault(path, set())
    CONSULTED.append(own)
    return own


def closing() -> None:
    """Close the innermost record, and hand what it consulted to the one that asked for it."""
    own = CONSULTED.pop()
    if CONSULTED:
        CONSULTED[-1].update(own)


def cached(path: str) -> None:
    """A scope read earlier this run: its consultations count for this reader too."""
    if CONSULTED:
        CONSULTED[-1].update(SCOPE_DEPS.get(path, ()))


def consulting(tree, rel: str, work) -> list:
    """Run `work()` and return the rels (and `*/` spellings) it was bound through, `rel` itself left out."""
    CONSULTED.append(set())
    try:
        work()
    finally:
        seen = CONSULTED.pop()
    out = set()
    for key in seen:
        if key.startswith(ANY):
            out.add(key)
            continue
        other = tree.rel_of(key)
        if other and other != rel:
            out.add(other)
    return sorted(out)


# What marks the interned record - see `stored`. A record without it is the plain `{rel -> [deps]}` an
# earlier version wrote, read as it is.
INTERNED = "interned"


def recorded(state: dict) -> dict:
    """`{rel -> [deps]}` as the last run stored it, or {} when it stored none."""
    try:
        value = json.loads(state.get("deps") or "{}")
    except ValueError:
        return {}
    if not isinstance(value, dict):
        return {}
    if value.get(INTERNED) != 1:
        return value
    keys = value.get("keys") or []
    try:
        return {rel: [keys[i] for i in through] for rel, through in (value.get("files") or {}).items()}
    except (IndexError, TypeError):
        return {}


def rebound(deps: dict, moved: set) -> set:
    """The files whose rows were bound through a file that changed, appeared or went away."""
    if not moved:
        return set()
    spelled = set()
    for rel in moved:
        parts = rel.split("/")
        spelled.update(ANY + "/".join(parts[i:]) for i in range(len(parts)))
    touched = moved | spelled
    return {rel for rel, through in deps.items() if any(dep in touched for dep in through)}


def stored(deps: dict) -> str:
    keys = sorted({key for through in deps.values() for key in through})
    index = {key: i for i, key in enumerate(keys)}
    files = {rel: [index[key] for key in through] for rel, through in deps.items()}
    return json.dumps({INTERNED: 1, "keys": keys, "files": files}, separators=(",", ":"), sort_keys=True)


DIRECTORY_CACHE = {}


def exists_exact(candidate: str) -> bool:
    """Is there a file at this path with THIS EXACT NAME?

    `os.path.isfile` answers yes on a case-insensitive filesystem for a name that differs only in case, and
    that turns a NAME import into a MODULE one: `from registry import STORE` found `registry/store.py`
    on Windows and drew an edge to a file the code never imports - the real `STORE` is an object inside the
    package's `__init__.py`. The directory listing is the only authority on how a file is really spelled.
    """
    # A MISS IS RECORDED TOO: a module that does not exist yet is exactly the file that, once written,
    # changes what this import binds.
    note(candidate)
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
