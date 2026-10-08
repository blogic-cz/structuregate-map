# fbt — fast source tree map

Stores a source tree in SQLite: one row per node with its content hash, plus a
merkle hash per directory. A later run compares the tree against the stored map,
reports what moved, and maps the changed paths onto the build targets that have
to rebuild.

```
fbt snapshot .          # map the tree, store it
fbt status .            # what changed, store nothing
fbt update .            # what changed, store the new map
fbt dirty .             # which targets must rebuild
```

## How change detection works

Three layers, from cheapest to most exact.

**1. The stat cache.** A file whose size and mtime match the stored row keeps its
stored hash and is never read. This is what makes a rescan cheap: only the files
that moved are opened.

**2. The content hash.** Every file that fails the stat check is read and hashed
with BLAKE3. A change is therefore a change of bytes, not of timestamps: `touch`,
a rebuild that rewrites identical output, and a checkout that restores a file all
report nothing.

**3. The directory merkle.** A directory hash covers each child's name, kind and
hash, in path order. So one root hash answers "did anything at all change" and
two trees with the same content always hash the same, wherever they sit. This is
what makes comparing two stored snapshots — local against CI, branch against
branch — an O(1) question before it is a list.

### The racy timestamp

A file written in the same clock tick as the snapshot can keep its size and mtime
while its bytes differ. So a stored hash is trusted only when the file's mtime is
strictly older than the snapshot, and every file at or after that mark is read
again. The snapshot's own timestamp is taken **before** the walk, never after,
for the same reason.

## The NTFS journal fast path (`--usn`)

NTFS records every change on a volume in a numbered ring buffer. A snapshot
stores the position it was taken at; `--usn` reads only the entries after it, so
the cost follows the number of changes rather than the size of the tree. It falls
back to a full walk, and says why, when the journal was recreated, when the ring
buffer wrapped past the stored position, or when it cannot be opened — reading it
needs administrator rights.

**It is off by default, and that is deliberate.** NTFS coalesces journal reasons
per open file: once a reason has been recorded for a file some process still
holds open, further changes of that same reason add no new record until every
handle closes. A log a server appends to all day produces one record, not
thousands. Measured on a real tree, `--usn` missed files that a process was
actively writing, and a full walk caught them all.

For ordinary edit-save-close work the two agree exactly — `usn_matches_full_scan`
in the test suite asserts the journal path reaches the same root hash and the
same change list as a full walk. For anything holding a file open it can lag. So
the journal chain is bounded: after 20 journal runs in a row, the next run walks
the tree and resettles the map.

`fbt journal <root>` prints the raw records, to see what the volume actually
reported.

## Measured

On Windows, on an NVMe disk, over trees of very different sizes:

- The first snapshot is the only expensive run: it reads and hashes every file
  once, and a cold OS cache makes it several times slower than a warm one.
- `status` with nothing changed answers in a small fraction of the first
  snapshot's time - a stat cache hit throughout, no file read.
- `dirty` costs more than `status` because it writes every row as a new
  snapshot. Add `--dry-run` when only the answer is wanted.
- A walk with a few files changed reads only those files, and costs little more
  than one with nothing changed.

The journal path is **not** the win it sounds like: loading the stored rows
out of SQLite and rolling up the merkle costs about as much as the walk itself,
so both paths land close together. The journal only pulls ahead on much larger
trees. The walk stays the default because it is exact.

The first scan is disk bound and unavoidable — every byte has to be read once.

## Skipping

Pruning a directory is the single biggest saving, because the subtree is never
enumerated. Skipped by default: `.fbt`, `.git`, `.hg`, `.svn`, `.jj`,
`node_modules`, `target`, `bin`, `obj`, `.venv`, `venv`, `__pycache__`,
`.gradle`, `.idea`, `.vs`, `.next`, `dist`, `build`, `.cargo`, `.mypy_cache`,
`.pytest_cache`. The root `.gitignore` is applied as well.

`--skip NAME` adds a directory name, `--no-default-skip` drops the built-in list,
`--no-gitignore` ignores the ignore file. The database file is always pruned from
the map it describes.

## Targets

A change list does not say what to do. A target names a unit the build can
rebuild, its input patterns say which paths belong to it, and the dependency
edges carry a change outward.

```json
{
  "targets": [
    { "name": "core",   "inputs": ["crates/core/**"], "deps": [] },
    { "name": "api",    "inputs": ["crates/api/**"],  "deps": ["core"] },
    { "name": "web",    "inputs": ["web/**"],         "deps": ["api"] }
  ]
}
```

```
fbt targets import targets.json
fbt dirty .                     # prints every target that must rebuild
fbt dirty . --show-unclaimed    # and every changed path no target owns
```

`dirty` is the transitive closure: a change under `crates/core` rebuilds `core`,
`api` and `web`. `--show-unclaimed` is worth watching in CI — a changed path no
pattern matches is a change the build would silently skip.

Wire it into a build script:

```bash
for target in $(fbt dirty . --json | jq -r '.dirty_targets[]'); do
  ./build.sh "$target"
done
```

## Schema

```sql
snapshot(id, root, root_key, created_ns, usn, journal_id, volume,
         usn_chain, entry_count, root_hash)

entry(snapshot_id, path, path_key, parent_key, kind,
      size, mtime_ns, file_id, hash)          -- PK (snapshot_id, path_key)

target(id, name)
target_input(target_id, pattern)
target_dep(target_id, depends_on)
```

`path_key` is the lowercased path and carries every comparison, because Windows
paths are case insensitive; `path` keeps the real spelling. Paths are stored flat
rather than as an adjacency list, so a diff is one join instead of a recursive
query, while `parent_key` still serves the merkle rollup and subtree deletes.

`file_id` is the NTFS file reference. NTFS keeps it across a move inside a
volume, so it turns an add plus a delete into one rename. It costs nothing to
collect: `GetFileInformationByHandleEx(FileIdBothDirectoryInfo)` returns name,
size, timestamps and file id for a whole directory in one call, which is also why
the walk needs no per-file `stat`. Without a file id — the portable walker —
rename detection falls back to matching content hash and size.

Several snapshots per root are kept, so `fbt diff <old> <new>` can compare any
two. `fbt prune --keep N` drops the rest.

## Layout

| File | Holds |
|---|---|
| `entry.rs` | the node type, path normalisation, time conversion |
| `db.rs` | schema, snapshot rows, bulk writes |
| `walk/win.rs` | NTFS directory enumeration with file ids |
| `walk/portable.rs` | `read_dir` fallback |
| `hash.rs` | BLAKE3 hashing, merkle rollup |
| `scan.rs` | walk, hash what moved, roll up |
| `usn.rs` | journal reader |
| `incremental.rs` | journal replay onto the stored map |
| `diff.rs` | comparison and rename pairing |
| `targets.rs` | target graph and dirty closure |
| `pipeline.rs` | the flow every command shares |

## Limits

* The journal fast path can lag behind a file another process holds open. See
  above; this is why it is opt-in and bounded.
* A symlink or junction is recorded by its target text, never followed. Following
  would risk cycles and would hide the link being repointed.
* A file that cannot be read — locked, permission denied — gets a hash folded
  from its size and mtime, so a later change is still seen and the scan does not
  fail. Its content is not covered.
* Volumes with two second timestamp granularity (FAT, some network shares) need
  `--rehash-all`, which reads every file.
* `file_id` is unique per volume. A tree spanning volumes loses rename detection
  across the boundary, falling back to content matching.
