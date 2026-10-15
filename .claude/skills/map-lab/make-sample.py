"""A SMALL SAMPLE of a real workspace, to iterate on in seconds instead of minutes.

    python3 make-sample.py <workspace>                      # where the bulk is: .ts/.html per folder, biggest first
    python3 make-sample.py <workspace> <sample> --drop a/b --drop c/d

The copy leaves `node_modules` out and links it back, so the sample borrows the same compiler; each `--drop` is a
folder (relative to the workspace) removed from the copy. Imports into a dropped folder just stay unresolved, which
the map records as such. The workspace itself is never written.
"""

import os
import shutil
import subprocess
import sys

SKIPPED = {"node_modules", ".git", "dist", ".angular", ".nx"}


def counts(root):
    """`.ts` and `.html` files under every folder, keyed by its path relative to `root`."""
    held = {}
    for here, dirs, files in os.walk(root):
        dirs[:] = [d for d in dirs if d not in SKIPPED]
        ts = sum(1 for f in files if f.endswith(".ts"))
        html = sum(1 for f in files if f.endswith(".html"))
        rel = os.path.relpath(here, root)
        parts = [] if rel == "." else rel.split(os.sep)
        for depth in range(len(parts) + 1):
            key = "/".join(parts[:depth]) or "."
            t, h = held.get(key, (0, 0))
            held[key] = (t + ts, h + html)
    return held


def report(root):
    """The folders worth dropping: down to four levels, the ones holding at least 2% of the code."""
    held = counts(root)
    total = held["."][0] + held["."][1] or 1
    rows = [(t + h, t, h, k) for k, (t, h) in held.items() if k != "." and k.count("/") < 4 and (t + h) * 50 >= total]
    print(f"{held['.'][0]} .ts and {held['.'][1]} .html in {root}")
    for both, t, h, k in sorted(rows, reverse=True)[:40]:
        print(f"{both * 100 / total:5.1f}%  {t:6} ts  {h:6} html  {k}")


def make(root, sample, drops):
    if os.path.exists(sample):
        sys.exit(f"{sample} exists - remove it first")
    excludes = [f"--exclude={name}" for name in sorted(SKIPPED)]
    subprocess.run(["rsync", "-a", *excludes, root.rstrip("/") + "/", sample + "/"], check=True)
    modules = os.path.join(root, "node_modules")
    if os.path.isdir(modules):
        os.symlink(os.path.abspath(modules), os.path.join(sample, "node_modules"))
    for drop in drops:
        target = os.path.join(sample, drop)
        if not os.path.isdir(target):
            sys.exit(f"no folder {drop} in the copy")
        shutil.rmtree(target)
    left = counts(sample)["."]
    print(f"sample {sample}: {left[0]} .ts and {left[1]} .html left")


def main(argv):
    if len(argv) == 1:
        report(argv[0])
        return
    if len(argv) < 2:
        sys.exit(__doc__)
    drops, rest = [], argv[2:]
    while rest:
        if rest[0] != "--drop" or len(rest) < 2:
            sys.exit(__doc__)
        drops.append(rest[1])
        rest = rest[2:]
    make(argv[0], argv[1], drops)


if __name__ == "__main__":
    main(sys.argv[1:])
