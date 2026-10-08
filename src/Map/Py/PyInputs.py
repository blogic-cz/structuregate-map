"""PyInputs.py - what the python half is handed: the file list, the roots, the tree map's hashes - and a
file's text, read only when the file is parsed - and the command line they arrive on."""
from __future__ import annotations

import json
import sys
from types import SimpleNamespace


def options(flags: dict, switches: tuple = ()) -> SimpleNamespace:
    """The command line, as `--name value` for each of `flags` (its default, None = required) and a bare `--name`
    for each of `switches`. NOT ARGPARSE: on python 3.14 importing and building one parser is ~35 ms of every
    run, paid by both python halves on every edit turn. The caller is the exe, never a person, so what this
    loses is only the usage text."""
    values = {name: default for name, default in flags.items()}
    values.update({name: False for name in switches})
    argv = sys.argv[1:]
    at = 0
    while at < len(argv):
        name = argv[at][2:] if argv[at].startswith("--") else ""
        if name in switches:
            values[name] = True
            at += 1
        elif name in flags and at + 1 < len(argv):
            values[name] = argv[at + 1]
            at += 2
        else:
            raise SystemExit("unknown or incomplete option: %s" % argv[at])
    missing = [name for name, value in values.items() if value is None]
    if missing:
        raise SystemExit("missing option: --%s" % missing[0])
    return SimpleNamespace(**{name.replace("-", "_"): value for name, value in values.items()})


def rows_of(list_file: str) -> list:
    out = []
    with open(list_file, "r", encoding="utf-8") as handle:
        for raw in handle:
            parts = raw.rstrip("\n").split("\t")
            if len(parts) >= 2:
                out.append((parts[0].strip(), parts[1].strip()))
    return out


def roots_of(path: str) -> list:
    """The `(prefix, dir)` of every root, from `--roots-file` as a JSON list of pairs, or [] with no file - one
    root, which `--root` names."""
    if not path:
        return []
    with open(path, "r", encoding="utf-8") as handle:
        return [tuple(pair) for pair in json.load(handle)]


def handed_of(path: str) -> dict:
    """`{rel -> the tree map's content hash}` from `--hashes`, or {} with no file - every file is then read."""
    if not path:
        return {}
    with open(path, "r", encoding="utf-8") as handle:
        return json.load(handle)


def text_of(abs_path: str):
    """The file's text, or the OSError that stopped the read - the caller says it."""
    try:
        with open(abs_path, "r", encoding="utf-8", errors="replace") as handle:
            return handle.read()
    except OSError as error:
        return error
