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

It is IMPORTED by `PyBind.py`, so it is staged and embedded wherever that one is.
"""
from __future__ import annotations

import json

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
