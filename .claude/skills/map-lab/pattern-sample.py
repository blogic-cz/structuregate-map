"""A PATTERN SAMPLE of a real Angular workspace: the fewest files that still hold every kind of code the full map holds.

    python3 pattern-sample.py make  <full map> <workspace> <sample dir>   # pick, close over imports, copy
    python3 pattern-sample.py cover <full map> <sample map>               # every feature of the full map in the sample's?

A FEATURE is what a row says about the SHAPE of code, never its names: a low-cardinality column's value (`gates.gate_kind=
hidden`, `bindings.kind=event`, `key_reach.route=literal`) and, for every structure a row holds (its node table
`<table>__<column>`), each parent -> role -> child step of the tree (`expressions.ast: Binary.left>Read`, an operator's value
kept). Each is attributed to the file its row came from (`owner_file`; the closure's tables through their components).

`make` covers every feature with a greedy set cover over files, adds what those files need - each component's template and,
transitively, every file the map's own dependency graph (`ts_deps`) says they depend on - and copies the workspace without
`node_modules` (linked back) and without the `.ts`/`.html` files nobody needs; every other file stays. `cover` maps nothing:
it compares two maps and exits 1 while the sample's lacks a feature the full one has - the proof the sample stands for the
tree. Node tables are read in key order, one streaming pass each: no SQLite JSON function, no self-join.
"""

import collections
import os
import shutil
import sqlite3
import subprocess
import sys

SKIPPED = {"node_modules", ".git", "dist", ".angular", ".nx"}
# A column with more values than this is a name or an id, not a kind.
CATEGORICAL = 40
# Columns that say where a row is, never what it is.
PLACE = {"id", "file", "owner_file", "half", "line", "col", "end_line", "decorator_line", "path", "abs", "class", "component",
         "template", "member", "parent", "node", "binding", "expression", "content_hash", "sha", "chars", "bytes", "lines"}
# Leaf roles whose VALUE is the shape: an operator, a node's kind.
VALUED = {"op", "kind", "$kind", "gate_kind", "via", "route", "sense"}


def ts_tables(db):
    out = []
    for (t,) in db.execute("SELECT name FROM sqlite_master WHERE type = 'table'"):
        if "__" in t or t.startswith("sqlite") or t.startswith("_"):
            continue
        cols = [r[1] for r in db.execute(f'PRAGMA table_info("{t}")')]
        if "half" in cols and db.execute(f"SELECT 1 FROM \"{t}\" WHERE half = 'typescript' LIMIT 1").fetchone():
            out.append((t, cols))
    return out


def file_of_owner(db, t, cols, paths):
    """`owner` of a node row -> the workspace path of the file its row came from (None when the row says no file)."""
    out = {}
    if "owner_file" in cols and "id" in cols:
        for i, f in db.execute(f"SELECT id, owner_file FROM \"{t}\" WHERE half = 'typescript'"):
            if f in paths:
                out[str(i)] = paths[f]
    elif "component" in cols:
        comp = dict(db.execute("SELECT id, owner_file FROM components WHERE half = 'typescript'"))
        key = "id" if "id" in cols else "rowid"
        for i, c in db.execute(f"SELECT {key}, component FROM \"{t}\" WHERE half = 'typescript'"):
            f = comp.get(c)
            if f in paths:
                out[str(i) if key == "id" else f"#{i}"] = paths[f]
    return out


def features_of(path):
    """`features` of the map at `path`, cached beside it (`<map>.features`) while the map is unchanged - reading every node
    table of a large map is minutes."""
    import pickle
    cache = f"{path}.features"
    stamp = os.stat(path).st_mtime_ns
    try:
        with open(cache, "rb") as f:
            held, found = pickle.load(f)
        if held == stamp:
            return found
    except (OSError, EOFError, pickle.UnpicklingError, ValueError):
        pass
    found = features(sqlite3.connect(f"file:{path}?mode=ro", uri=True))
    with open(cache, "wb") as f:
        pickle.dump((stamp, found), f)
    return found


def features(db):
    """feature -> the workspace paths of the files that hold it (empty when no file can be named)."""
    paths = dict(db.execute("SELECT id, path FROM files WHERE half = 'typescript'"))
    found = collections.defaultdict(set)
    comp = dict(db.execute("SELECT id, owner_file FROM components WHERE half = 'typescript'"))
    for t, cols in ts_tables(db):
        owners = file_of_owner(db, t, cols, paths)
        # key_reach: its components' files.
        if t == "key_reach":
            for owner, c in db.execute("SELECT owner, value FROM key_reach__components WHERE parent = 0"):
                if comp.get(c) in paths:
                    owners.setdefault(owner, paths[comp[c]])
        key = "id" if "id" in cols else "rowid"
        for col in cols:
            if col in PLACE:
                continue
            n = db.execute(f"SELECT COUNT(DISTINCT \"{col}\") FROM \"{t}\" WHERE half = 'typescript'").fetchone()[0]
            if not 0 < n <= CATEGORICAL:
                continue
            for i, v in db.execute(f"SELECT {key}, \"{col}\" FROM \"{t}\" WHERE half = 'typescript' AND \"{col}\" IS NOT NULL"):
                owner = str(i) if key == "id" else f"#{i}"
                found[f"{t}.{col}={v}"].add(owners.get(owner))
        nodes = [r[0] for r in db.execute("SELECT name FROM sqlite_master WHERE type = 'table'") if r[0].startswith(t + "__")]
        for table in nodes:
            column = table[len(t) + 2:]
            current, rows = None, []
            def flush():
                kinds = {n: v for (n, p, role, node, v) in rows if role == "k"}
                roles = {n: role for (n, p, role, node, v) in rows}
                label = lambda n: kinds.get(n) or (roles.get(n) if roles.get(n) is not None else "#")
                where = owners.get(current)
                for (n, p, role, node, v) in rows:
                    if p is None or role == "k":
                        continue
                    child = kinds.get(n) or node
                    if role in VALUED and node == "string":
                        child = f"{node}:{v}"
                    found[f"{t}.{column}: {label(p)}.{role if role is not None else '[]'}>{child}"].add(where)
            for owner, n, p, role, node, v in db.execute(f'SELECT owner, n, parent, role, node, value FROM "{table}" ORDER BY owner, n'):
                if owner != current:
                    if rows:
                        flush()
                    current, rows = owner, []
                rows.append((n, p, role, node, v))
            if rows:
                flush()
    return {f: {p for p in where if p is not None} for f, where in found.items()}


def make(full, workspace, sample):
    db = sqlite3.connect(f"file:{full}?mode=ro", uri=True)
    feats = features_of(full)
    by_file = collections.defaultdict(set)
    for f, where in feats.items():
        for p in where:
            by_file[p].add(f)
    todo = {f for f, where in feats.items() if where}
    keep = set()
    while todo:
        best = max(by_file, key=lambda p: len(by_file[p] & todo))
        gain = by_file[best] & todo
        if not gain:
            break
        keep.add(best)
        todo -= gain
    picked = len(keep)
    # WHAT THE KEPT FILES NEED TO TYPE AS THEY DO: each component's template; every file declaring a name a kept file imports,
    # and THAT file's needs in turn; the barrels on the way and each module imported, but not what a barrel re-exports besides -
    # following a barrel's own imports pulled in most of the tree for the few names each importer uses.
    paths = dict(db.execute("SELECT id, path FROM files WHERE half = 'typescript'"))
    templates = collections.defaultdict(set)
    # A TEMPLATE'S `owner_file` IS THE TEMPLATE: the component's own file is its `components` row's.
    for owner, f in db.execute("SELECT c.owner_file, t.file FROM templates t JOIN components c ON c.id = t.component "
                               "WHERE t.half = 'typescript'"):
        if f in paths:
            templates[owner].add(paths[f])
    declares = collections.defaultdict(set)
    beside = collections.defaultdict(set)
    for owner, f in db.execute("SELECT owner_file, declared_file FROM import_names WHERE half = 'typescript' AND declared_file IS NOT NULL"):
        if f in paths:
            declares[owner].add(paths[f])
    by_id = dict(db.execute("SELECT id, owner_file FROM import_names WHERE half = 'typescript'"))
    for owner, barrel in db.execute("SELECT owner, value FROM import_names__via WHERE node = 'string'"):
        if by_id.get(owner) is not None:
            beside[by_id[owner]].add(barrel)
    for owner, f in db.execute("SELECT owner_file, resolved_file FROM imports WHERE half = 'typescript' AND resolved_file IS NOT NULL"):
        if f in paths:
            beside[owner].add(paths[f])
    ids = {p: i for i, p in paths.items()}
    stack = list(keep)
    while stack:
        f = ids.get(stack.pop())
        for p in templates.get(f, set()) | beside.get(f, set()):
            keep.add(p)
        for p in declares.get(f, ()):
            if p not in keep:
                keep.add(p)
                stack.append(p)
    every = set(paths.values())
    if os.path.exists(sample):
        shutil.rmtree(sample)
    for here, dirs, files in os.walk(workspace):
        dirs[:] = [d for d in dirs if d not in SKIPPED]
        rel = os.path.relpath(here, workspace)
        for name in files:
            path = name if rel == "." else f"{rel}/{name}".replace(os.sep, "/")
            if path in every and path not in keep and (name.endswith(".ts") or name.endswith(".html")):
                continue
            os.makedirs(os.path.join(sample, rel), exist_ok=True)
            shutil.copy2(os.path.join(here, name), os.path.join(sample, rel, name))
    os.symlink(os.path.join(workspace, "node_modules"), os.path.join(sample, "node_modules"))
    # A PROJECT ROOTED IN `files` (`src/main.ts`) WOULD BE EMPTY: its entry's imports no longer reach the kept files. Its `include`
    # of declaration files is widened to every `.ts` - only kept files are left, each one the full map parsed. Text, not JSON:
    # a tsconfig holds comments.
    widened = []
    for here, dirs, files in os.walk(sample):
        dirs[:] = [d for d in dirs if d not in SKIPPED]
        for name in files:
            if not (name.startswith("tsconfig") and name.endswith(".json")):
                continue
            at = os.path.join(here, name)
            text = open(at, encoding="utf-8").read()
            if '"files"' in text and '*.d.ts"' in text:
                open(at, "w", encoding="utf-8").write(text.replace('*.d.ts"', '*.ts"'))
                widened.append(os.path.relpath(at, sample))
    print("widened", widened)
    print(f"{len(feats)} features; {picked} file(s) cover them, {len(keep)} with templates and dependencies, of {len(every)}")
    uncoverable = [f for f, where in feats.items() if not where]
    print(f"{len(uncoverable)} feature(s) no file can be named for (checked by `cover`, not chosen for)")


def cover(full, sample):
    a = features_of(full)
    b = features_of(sample)
    missing = sorted(set(a) - set(b))
    print(f"{len(a)} features in the full map, {len(b)} in the sample's; {len(missing)} missing")
    for f in missing[:200]:
        print("  MISSING", f)
    return 1 if missing else 0


if __name__ == "__main__":
    if len(sys.argv) == 5 and sys.argv[1] == "make":
        make(*sys.argv[2:])
    elif len(sys.argv) == 4 and sys.argv[1] == "cover":
        sys.exit(cover(*sys.argv[2:]))
    else:
        sys.exit(__doc__)
