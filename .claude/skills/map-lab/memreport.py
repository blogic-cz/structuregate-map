"""WHERE A MAP'S MEMORY WENT: the samples memprof.sh took, against the run's own trace spans.

    python3 memreport.py <out dir>             # peak per phase (anonymous and file-backed), longest phases only
    python3 memreport.py <out dir> --timeline  # anonymous memory every ~10 s, with each phase's start and end

The phases come from `<out>/map.sqlite.last-run.jsonl` (OTLP spans); a sample's time is nanoseconds, a span's too.
"""

import json
import sys


def samples(out):
    rows = []
    for line in list(open(f"{out}/rss.tsv"))[1:]:
        cells = line.rstrip("\n").split("\t")
        if not cells[0]:
            continue
        number = lambda i: int(cells[i]) if len(cells) > i and cells[i] else 0
        rows.append((int(cells[0]) // 10**6, number(1), number(2), number(3), number(4)))
    return rows


def spans(out):
    found = []

    def walk(x):
        if isinstance(x, dict):
            if "startTimeUnixNano" in x:
                found.append((x["name"], int(x["startTimeUnixNano"]) // 10**6, int(x["endTimeUnixNano"]) // 10**6))
            for v in x.values():
                walk(v)
        elif isinstance(x, list):
            for v in x:
                walk(v)

    lines = open(f"{out}/map.sqlite.last-run.jsonl").read().splitlines()
    walk(json.loads(lines[-1]))
    return sorted(found, key=lambda s: s[1])


def gb(kb):
    return f"{kb / 1e6:5.2f}"


def phases(out):
    rows, found = samples(out), spans(out)
    print(f"peak total {gb(max(r[1] for r in rows))} GB, anonymous {gb(max(r[3] for r in rows))} GB, "
          f"file {gb(max(r[4] for r in rows))} GB, node {gb(max(r[2] for r in rows))} GB")
    for name, a, b in found:
        inside = [r for r in rows if a <= r[0] <= b]
        if b - a < 2000 or not inside:
            continue
        print(f"{(b - a) / 1000:7.1f} s  anon {gb(max(r[3] for r in inside))}  file {gb(max(r[4] for r in inside))}  "
              f"node {gb(max(r[2] for r in inside))}  {name}")


def timeline(out):
    rows, found = samples(out), spans(out)
    t0 = rows[0][0]
    marks = sorted([(a - t0, "start " + n) for n, a, b in found if b - a > 5000]
                   + [(b - t0, "end   " + n) for n, a, b in found if b - a > 5000])
    at = 0
    for i in range(0, len(rows), 20):
        t = rows[i][0] - t0
        while at < len(marks) and marks[at][0] <= t:
            print(f"           -- {marks[at][0] / 1000:7.1f} s {marks[at][1]}")
            at += 1
        print(f"{t / 1000:7.1f} s  anon {gb(rows[i][3])} GB  node {gb(rows[i][2])} GB")


if __name__ == "__main__":
    if len(sys.argv) < 2:
        sys.exit(__doc__)
    (timeline if "--timeline" in sys.argv[2:] else phases)(sys.argv[1])
