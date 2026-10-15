#!/bin/sh
# A FULL deep map of a tree, then a PARTIAL run after one comment appended to one file - the per-turn hook's case.
#   bench.sh <structuregate.dll> <tree> <out dir> <file to touch, relative to the tree> [extra gate arguments...]
# Writes <out>/full.log, <out>/partial.log (each ending with GNU time's wall clock and peak memory) and <out>/size.
# The touched file is restored byte for byte afterwards. Run ONE at a time on a large tree.
set -u
DLL=$1; TREE=$2; OUT=$3; LEAF=$4; shift 4
mkdir -p "$OUT/tmp"; rm -f "$OUT"/map.sqlite*
cp "$TREE/$LEAF" "$OUT/leaf.orig" || exit 1
run() { (cd "$TREE" && TMPDIR="$OUT/tmp" /usr/bin/time -f "%e s wall, %M KB peak" \
    dotnet "$DLL" --root "$TREE" --map-sqlite "$OUT/map.sqlite" "$@" > "$OUT/$PHASE.log" 2>&1); }
PHASE=full run "$@"
printf '\n// bench: one comment\n' >> "$TREE/$LEAF"
PHASE=partial run "$@"
cp "$OUT/leaf.orig" "$TREE/$LEAF"
stat -c %s "$OUT/map.sqlite" > "$OUT/size"
for p in full partial; do echo "$p: $(tail -1 "$OUT/$p.log")"; grep -E 'spent|error' "$OUT/$p.log" | cut -c1-200; done
