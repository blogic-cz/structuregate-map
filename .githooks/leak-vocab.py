"""THE REPOSITORY'S VOCABULARY, FROM ITS OWN DEEP MAP, for `leak-scan.sh --map`. A file-by-file read sees a private
word once in a long file and may let it pass; the same word listed beside every other word the repository uses - its
count, one place it stands - is easier to tell from an invented one, and a cluster of one domain's words stands out.

Every identifier the map declares (classes, functions, members, parameters, locals, consts), every string literal,
comment and file path is cut into words (camelCase, snake_case, punctuation); each distinct word of three letters or
more is one line: `<word> <count> <path>:<line>`, numbered for the reviewer to cite.

    python3 leak-vocab.py <map.sqlite>
"""

import sqlite3
import sys

SOURCES = [
    ("SELECT k.name, f.path, k.line FROM {t} k JOIN files f ON f.id = k.file", t)
    for t in ("classes", "functions", "members", "parameters", "locals", "consts")
] + [
    ("SELECT k.value, f.path, k.line FROM {t} k JOIN files f ON f.id = k.file", "string_literals"),
    ("SELECT k.text, f.path, k.line FROM {t} k JOIN files f ON f.id = k.file", "comments"),
    ("SELECT path, path, 1 FROM {t}", "files"),
]


def words(text):
    """`fetchOrderList_v2` gives fetch, order, list; `HTTPServer` gives http, server."""
    out, word = [], ""
    chars = list(text) + [" "]
    for i, c in enumerate(chars[:-1]):
        if not c.isalpha():
            out.append(word)
            word = ""
            continue
        after = chars[i + 1]
        if word and c.isupper() and (word[-1].islower() or (after.isalpha() and after.islower() and word[-1].isupper())):
            out.append(word)
            word = ""
        word += c
    out.append(word)
    return [w.lower() for w in out if len(w) >= 3]


def main(path):
    conn = sqlite3.connect(f"file:{path}?mode=ro", uri=True)
    tables = {r[0] for r in conn.execute("SELECT name FROM sqlite_master WHERE type = 'table'")}
    seen = {}
    for sql, table in SOURCES:
        if table not in tables:
            continue
        for text, file, line in conn.execute(sql.format(t=table)):
            if not isinstance(text, str):
                continue
            for w in words(text):
                count, where = seen.get(w, (0, f"{file}:{line}"))
                seen[w] = (count + 1, where)
    print("=== vocabulary: every word of the repository, `<word> <count> <one path:line>` ===")
    for n, (w, (count, where)) in enumerate(sorted(seen.items()), 1):
        print(f"{n}: {w} {count} {where}")


if __name__ == "__main__":
    main(sys.argv[1])
