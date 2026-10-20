"""A PATTERN SAMPLE of a real Angular workspace: the fewest files that still hold every kind of code the full map holds.

    python3 pattern-sample.py make  <full map> <workspace> <sample dir> [cover report]   # pick, close over imports, copy
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
         "template", "member", "parent", "node", "binding", "expression", "content_hash", "sha", "chars", "bytes", "lines",
         # names, not shapes: an element's tag, a path a diagnostic quotes
         "tag", "source"}
# A node table whose objects without a `k` use more member names than this holds DATA (an object literal's keys, a
# `{name: count}` map, ids as keys): such a name is no shape, and is read as `*`. A `$`-prefixed key stays: it is structure.
DATA_ROLES = 200
# What `features` computes; a cache written by another version is read again.
VERSION = 6
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
    """`owner` of a node row -> the workspace path of the file its row came from (None when the row says no file): its
    `owner_file`, else its component's, else a path it quotes (`source`, `trace_file`); a `files` row is its own file."""
    out = {}
    by_path = set(paths.values())
    comp = dict(db.execute("SELECT id, owner_file FROM components WHERE half = 'typescript'"))
    key = "id" if "id" in cols else "rowid"
    wanted = [c for c in ("owner_file", "component", "source", "trace_file", "file") if c in cols]
    if t == "files":
        for i, p in paths.items():
            out[str(i)] = p
        return out
    if wanted:
        for row in db.execute(f"SELECT {key}, {', '.join(chr(34) + c + chr(34) for c in wanted)} FROM \"{t}\" WHERE half = 'typescript'"):
            i, cells = row[0], dict(zip(wanted, row[1:]))
            owner = str(i) if key == "id" else f"#{i}"
            f = cells.get("owner_file")
            if f in paths:
                out[owner] = paths[f]
            elif comp.get(cells.get("component")) in paths:
                out[owner] = paths[comp[cells["component"]]]
            else:
                for c in ("source", "trace_file", "file"):
                    v = cells.get(c)
                    if isinstance(v, str) and v in by_path:
                        out[owner] = v
                        break
                    if v in paths:
                        out[owner] = paths[v]
                        break
        return out
    if "component" in cols:
        comp = dict(db.execute("SELECT id, owner_file FROM components WHERE half = 'typescript'"))
        key = "id" if "id" in cols else "rowid"
        for i, c in db.execute(f"SELECT {key}, component FROM \"{t}\" WHERE half = 'typescript'"):
            f = comp.get(c)
            if f in paths:
                out[str(i) if key == "id" else f"#{i}"] = paths[f]
    return out


def is_shape(v):
    """A column value that says what code IS: a word, a flag - never a count (`root_count=33`) or an id (`c:4021`), which say
    how big the tree is or which row, and no smaller sample can hold."""
    if isinstance(v, (int, float)):
        return v in (0, 1)
    text = str(v)
    head, _, tail = text.partition(":")
    return not (tail.isdigit() and head.replace("_", "").isalpha())


def data_tables(db):
    """The node tables whose objects without a `k` hold DATA names - decided ONCE, on the full map, and used for every map
    compared with it: decided per map, a table a sample holds fewer of read as schema and its names as features."""
    out = set()
    for (t,) in db.execute("SELECT name FROM sqlite_master WHERE type = 'table'"):
        if "__" in t and db.execute(f'SELECT COUNT(DISTINCT role) FROM "{t}"').fetchone()[0] > DATA_ROLES:
            out.add(t)
    return out


def features_of(path, data=None):
    """`features` of the map at `path`, cached beside it (`<map>.features`) while the map is unchanged - reading every node
    table of a large map is minutes."""
    import pickle
    cache = f"{path}.features"
    stamp = (os.stat(path).st_mtime_ns, tuple(sorted(data)) if data is not None else None)
    try:
        with open(cache, "rb") as f:
            held, found = pickle.load(f)
        if held == (stamp, VERSION):
            return found
    except (OSError, EOFError, pickle.UnpicklingError, ValueError):
        pass
    db = sqlite3.connect(f"file:{path}?mode=ro", uri=True)
    found = features(db, data if data is not None else data_tables(db))
    with open(cache, "wb") as f:
        pickle.dump(((stamp, VERSION), found), f)
    return found


def features(db, data):
    """feature -> the workspace paths of the files that hold it (empty when no file can be named)."""
    paths = dict(db.execute("SELECT id, path FROM files WHERE half = 'typescript'"))
    found = collections.defaultdict(set)
    comp = dict(db.execute("SELECT id, owner_file FROM components WHERE half = 'typescript'"))
    for t, cols in ts_tables(db):
        owners = file_of_owner(db, t, cols, paths)
        # ONLY THE ANGULAR HALF'S ROWS: a plain-half row (a `.ts` outside the workspace) is no code a workspace sample holds.
        key0 = "id" if "id" in cols else "rowid"
        ours = {str(i) if key0 == "id" else f"#{i}" for (i,) in db.execute(f"SELECT {key0} FROM \"{t}\" WHERE half = 'typescript'")}
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
                if not is_shape(v):
                    continue
                owner = str(i) if key == "id" else f"#{i}"
                found[f"{t}.{col}={v}"].add(owners.get(owner))
        nodes = [r[0] for r in db.execute("SELECT name FROM sqlite_master WHERE type = 'table'") if r[0].startswith(t + "__")]
        for table in nodes:
            column = table[len(t) + 2:]
            is_data = table in data
            current, rows = None, []
            def flush():
                kinds = {n: v for (n, p, role, node, v) in rows if role == "k"}
                parents = {n: p for (n, p, role, node, v) in rows}
                def named(n, role):
                    # A member of an object WITHOUT a kind, in a table of data, is a data name.
                    if role is None or not is_data or role.startswith("$") or kinds.get(parents.get(n)) is not None:
                        return role
                    return "*"
                roles = {n: named(n, role) for (n, p, role, node, v) in rows}
                label = lambda n: kinds.get(n) or (roles.get(n) if roles.get(n) is not None else "#")
                where = owners.get(current)
                for (n, p, role, node, v) in rows:
                    if p is None or role == "k":
                        continue
                    child = kinds.get(n) or node
                    if role in VALUED and node == "string":
                        child = f"{node}:{v}"
                    shown = roles.get(n)
                    found[f"{t}.{column}: {label(p)}.{shown if shown is not None else '[]'}>{child}"].add(where)
            for owner, n, p, role, node, v in db.execute(f'SELECT owner, n, parent, role, node, value FROM "{table}" ORDER BY owner, n'):
                if str(owner) not in ours:
                    continue
                if owner != current:
                    if rows:
                        flush()
                    current, rows = owner, []
                rows.append((n, p, role, node, v))
            if rows:
                flush()
    return {f: {p for p in where if p is not None} for f, where in found.items()}


def make(full, workspace, sample, report=None):
    db = sqlite3.connect(f"file:{full}?mode=ro", uri=True)
    feats = features_of(full, data_tables(db))
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
    # A FEATURE A SAMPLE LOST IN ITS CONTEXT - a template two components share, an ambiguous pipe, a target its declaration
    # resolves - is kept with EVERY file holding it, as the last `cover` report names them: the greedy pick keeps one.
    if report:
        # EVERY REPORT SO FAR, not the last: a feature found once in its context is lost again when the next pick drops it.
        lost = sorted({line.split("MISSING ", 1)[1].rstrip("\n") for r in report.split(",") if os.path.exists(r)
                       for line in open(r, encoding="utf-8") if "MISSING " in line})
        for f in lost:
            keep |= feats.get(f, set())
        print(f"{len(lost)} feature(s) a sample lost: every file holding them kept")
    # WHAT A TEMPLATE RESOLVES THROUGH IS NO IMPORT: a directive, a pipe, the NgModule declaring them reach a template by its
    # selector and its module's scope. Every one of them is kept - a few hundred classes - or a directive's config object
    # read in a template (`gate_values.listed_json`) resolved to nothing.
    for table in ("directives", "pipes", "ng_modules"):
        for (f,) in db.execute(f"SELECT DISTINCT p.path FROM {table} r JOIN files p ON p.id = r.owner_file WHERE r.half = 'typescript'"):
            keep.add(f)
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
    # AND EVERY FILE A KEPT ROW POINTS AT - a `file` the extractor resolved anywhere in a row's structures: a type_ref, a
    # `$target`, a `$read`'s declaring class. A member read through `this.viewModel.Input.StartDate` names classes reached
    # through TYPES, not imports, and without them the sample resolved it to nothing.
    by_abs = {a: p for p, a in db.execute("SELECT path, abs FROM files WHERE half = 'typescript'")}
    points = collections.defaultdict(set)
    for (t,) in db.execute("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE '%sqlite%'"):
        if "__" not in t:
            continue
        base = t.split("__", 1)[0]
        cols = [r[1] for r in db.execute(f'PRAGMA table_info("{base}")')]
        if "owner_file" not in cols or "id" not in cols:
            continue
        for f, v in db.execute(f'SELECT b.owner_file, n.value FROM "{t}" n JOIN "{base}" b ON b.id = n.owner WHERE n.role = \'file\''):
            p = by_abs.get(v) or (v if v in set(paths.values()) else None)
            if p is not None:
                points[f].add(p)
    # A TEMPLATE TWO COMPONENTS SHARE is kept with both.
    users = collections.defaultdict(set)
    for owner, f in db.execute("SELECT c.owner_file, t.file FROM templates t JOIN components c ON c.id = t.component WHERE t.half = 'typescript'"):
        if owner in paths:
            users[f].add(paths[owner])
    ids = {p: i for i, p in paths.items()}
    stack = list(keep)
    while stack:
        f = ids.get(stack.pop())
        for p in templates.get(f, set()) | beside.get(f, set()):
            keep.add(p)
        for p in declares.get(f, set()) | points.get(f, set()) | users.get(f, set()):
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
    # `files` gains exactly the kept files under it that the full map PARSED - a file the full map left outside every program stays
    # outside (a key traced into one reads `unparsed_file` in both maps). Text, not JSON: a tsconfig holds comments.
    parsed = {p for (p,) in db.execute("SELECT path FROM files WHERE half = 'typescript' AND parsed = 1")}
    widened = []
    for here, dirs, files in os.walk(sample):
        dirs[:] = [d for d in dirs if d not in SKIPPED]
        for name in files:
            if not (name.startswith("tsconfig") and name.endswith(".app.json")):
                continue
            at = os.path.join(here, name)
            text = open(at, encoding="utf-8").read()
            if '"files": [' not in text:
                continue
            base = os.path.relpath(here, sample).replace(os.sep, "/")
            under = sorted(p[len(base) + 1:] for p in keep & parsed if base == "." or p.startswith(base + "/"))
            listed = ", ".join(f'"{p}"' for p in under if p.endswith(".ts"))
            # `files`, not `include`: the compiler turns every `include` pattern into one regular expression, and thousands of
            # paths made one it refused - the program was never built.
            open(at, "w", encoding="utf-8").write(text.replace('"files": [', f'"files": [{listed}, ', 1))
            widened.append((os.path.relpath(at, sample), len(under)))
    print("widened", widened)
    print(f"{len(feats)} features; {picked} file(s) cover them, {len(keep)} with templates and dependencies, of {len(every)}")
    uncoverable = [f for f, where in feats.items() if not where]
    print(f"{len(uncoverable)} feature(s) no file can be named for (checked by `cover`, not chosen for)")


def cover(full, sample):
    data = data_tables(sqlite3.connect(f"file:{full}?mode=ro", uri=True))
    a = features_of(full, data)
    b = features_of(sample, data)
    missing = sorted(set(a) - set(b))
    print(f"{len(a)} features in the full map, {len(b)} in the sample's; {len(missing)} missing")
    for f in missing[:200]:
        print("  MISSING", f)
    return 1 if missing else 0


if __name__ == "__main__":
    if len(sys.argv) in (5, 6) and sys.argv[1] == "make":
        make(*sys.argv[2:])
    elif len(sys.argv) == 4 and sys.argv[1] == "cover":
        sys.exit(cover(*sys.argv[2:]))
    else:
        sys.exit(__doc__)
