#!/bin/sh
# ONE FULL DEEP MAP of a tree, its memory sampled twice a second: the gate process (total, ANONYMOUS - what the
# OOM killer counts - and file-backed, which a mapped payload inflates) and its node children.
#   memprof.sh <structuregate.dll> <tree> <out dir> [extra gate arguments...]
# Writes <out>/rss.tsv, <out>/run.log and the map <out>/map.sqlite (with its .last-run.jsonl trace, which
# memreport.py reads for the phases). TMPDIR is <out>/tmp - never /tmp, whose quota is small.
set -u
DLL=$1; TREE=$2; OUT=$3; shift 3
rm -rf "$OUT"; mkdir -p "$OUT/tmp"
cd "$TREE" || exit 1
TMPDIR="$OUT/tmp" dotnet "$DLL" --root "$TREE" --map-sqlite "$OUT/map.sqlite" "$@" > "$OUT/run.log" 2>&1 &
PID=$!
printf 'epoch_ns\tgate_kb\tnode_kb\tgate_anon_kb\tgate_file_kb\n' > "$OUT/rss.tsv"
field() { awk -v k="$1" '$1 == k":" {print $2}' "/proc/$2/status" 2>/dev/null; }
while kill -0 $PID 2>/dev/null; do
  N=0
  for c in $(pgrep -P $PID); do v=$(field VmRSS "$c"); N=$((N + ${v:-0})); done
  printf '%s\t%s\t%s\t%s\t%s\n' "$(date +%s%N)" "$(field VmRSS $PID)" "$N" "$(field RssAnon $PID)" "$(field RssFile $PID)" >> "$OUT/rss.tsv"
  sleep 0.5
done
wait $PID; echo "exit=$?" >> "$OUT/run.log"
