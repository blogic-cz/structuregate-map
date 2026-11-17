#!/usr/bin/env bash
# THE WHOLE REPOSITORY, READ FOR LEAKS - what the pre-commit hook never saw. The hook reads only the lines a commit
# ADDS, so whatever was committed before it existed (or past it with --no-verify) is never read again. This reads
# every tracked text file, and with --history every commit message and tag name, through the same instructions
# (`.githooks/leak-review.md`) and the same deny list, in chunks of about --chunk bytes, --jobs agents at a time.
#
#   .githooks/leak-scan.sh [--jobs N] [--chunk BYTES] [--model NAME] [--out DIR] [--history] [--map DB] [--only PATHSPEC...]
#
# --map DB adds the repository's VOCABULARY from its own deep map (`structuregate --root . --ext .rs,.cs,.mjs,.py,.ps1
# --map-sqlite DB`, read by `leak-vocab.py`): every distinct word of its identifiers, strings, comments and paths, with a
# count and one place - a private word that a file-by-file read passes stands out beside all the others.
#
# It edits nothing. Each chunk's answer is <out>/chunk-<n>.txt; the findings of all of them are <out>/findings.txt.
# Exit 0 = no finding, 1 = findings (a person decides each), 2 = a chunk gave no verdict (rerun it).
set -u
root=$(git rev-parse --show-toplevel) || exit 2
deny="$root/leak-deny.txt"
jobs=4 chunk=200000 model=sonnet history=0 map="" out="" only=()
while [ $# -gt 0 ]; do
    case "$1" in
        --jobs) jobs=$2; shift 2 ;;
        --chunk) chunk=$2; shift 2 ;;
        --model) model=$2; shift 2 ;;
        --out) out=$2; shift 2 ;;
        --history) history=1; shift ;;
        --map) map=$2; shift 2 ;;
        --only) shift; only=("$@"); break ;;
        *) echo "leak-scan: unknown argument $1" >&2; exit 2 ;;
    esac
done
command -v claude >/dev/null 2>&1 || { echo "leak-scan: no claude on PATH" >&2; exit 2; }
[ -n "$out" ] || out=$(mktemp -d "${TMPDIR:-/tmp}/leak-scan.XXXXXX")
mkdir -p "$out/in"
rm -f "$out"/in/* "$out"/chunk-*.txt "$out/findings.txt"

# 1. THE DENY LIST, exact, over the whole tree (what LeakGuard checks at build time).
if [ -s "$deny" ]; then
    git -C "$root" grep -n -I -F -f "$deny" -- . ':(exclude)rust/fbtcore/vendor' > "$out/deny-hits.txt"
    [ -s "$out/deny-hits.txt" ] && echo "leak-scan: deny-list terms in the tree - see $out/deny-hits.txt" >&2
fi

# 2. THE INPUT, cut into chunks: each file headed by its path, every line numbered; a file larger than a chunk is
#    a chunk alone. Vendored upstream code, lock files and binaries are not ours to leak.
spec=(. ':(exclude)rust/fbtcore/vendor' ':(exclude)src/MapView/vendor' ':(exclude)*.lock' ':(exclude)package-lock.json')
[ ${#only[@]} -gt 0 ] && spec=("${only[@]}")
n=0 size=0
next() { n=$((n + 1)); size=0; }
next
while IFS= read -r f; do
    [ -f "$root/$f" ] || continue
    grep -Iq . "$root/$f" 2>/dev/null || continue
    bytes=$(wc -c < "$root/$f")
    if [ $size -gt 0 ] && [ $((size + bytes)) -gt "$chunk" ]; then next; fi
    { printf '=== %s ===\n' "$f"; awk '{ printf "%d: %s\n", NR, $0 }' "$root/$f"; } >> "$out/in/$n.txt"
    size=$((size + bytes))
done < <(git -C "$root" ls-files -- "${spec[@]}")
if [ "$history" = 1 ]; then
    next
    git -C "$root" log --format='=== commit %h ===%n%B' | awk '{ printf "%d: %s\n", NR, $0 }' > "$out/in/$n.txt"
    if [ "$(wc -c < "$out/in/$n.txt")" -gt "$chunk" ]; then
        split -C "$chunk" -d -a 3 "$out/in/$n.txt" "$out/in/$n-" && rm "$out/in/$n.txt"
    fi
    next
    { echo "=== tag names ==="; git -C "$root" tag; echo "=== branch names ==="; git -C "$root" branch -a --format='%(refname:short)'; } \
        | awk '{ printf "%d: %s\n", NR, $0 }' > "$out/in/$n.txt"
fi
if [ -n "$map" ]; then
    next
    python3 -I "$root/.githooks/leak-vocab.py" "$map" > "$out/in/$n.txt" || { echo "leak-scan: no vocabulary from $map" >&2; exit 2; }
fi
find "$out/in" -empty -delete

# 3. THE AGENTS. The review instructions are written for a diff; this input is whole files.
prompt="THIS RUN READS WHOLE FILES, NOT A DIFF. The input is files, each headed \`=== <path> ===\` (or \`=== commit <hash> ===\`
for a commit message, \`=== tag names ===\` for refs, \`=== vocabulary ...\` for a list of words), every line prefixed with its line number and \`: \`. Treat EVERY line as
added. Report \`<path>:<line>\` with the line number printed before the text (for a commit message: \`commit <hash>\`; for a vocabulary word:
\`vocab <word> <the path:line it lists>\`). In the vocabulary, flag a word that is a private name or one private domain's
business word; ordinary English and programming words are not leaks.

$(cat "$root/.githooks/leak-review.md")"
[ -s "$deny" ] && prompt="$prompt

DENY LIST (one term per line, private - never repeat it outside a finding):
$(cat "$deny")"
export prompt model out
review() {
    name=$(basename "$1" .txt)
    timeout 1800 claude -p "$prompt" --model "$model" --no-session-persistence \
        --disallowedTools "Bash Edit Write NotebookEdit WebFetch WebSearch Read Grep Glob" < "$1" > "$out/chunk-$name.txt" 2>&1
    echo "leak-scan: chunk $name done: $(grep -F 'LEAK-REVIEW:' "$out/chunk-$name.txt" | tail -n 1)" >&2
}
export -f review
total=$(ls "$out/in" | wc -l)
echo "leak-scan: $total chunk(s), $jobs at a time, model $model, into $out" >&2
ls "$out"/in/*.txt "$out"/in/*-[0-9][0-9][0-9] 2>/dev/null | xargs -P "$jobs" -I{} bash -c 'review "$@"' _ {}

# 4. THE VERDICTS.
status=0
for answer in "$out"/chunk-*.txt; do
    verdict=$(grep -F 'LEAK-REVIEW:' "$answer" | tail -n 1)
    case "$verdict" in
        *"LEAK-REVIEW: PASS"*) ;;
        *"LEAK-REVIEW: FAIL"*) grep -v -F 'LEAK-REVIEW:' "$answer" | grep -v '^[[:space:]]*$' >> "$out/findings.txt"; [ $status = 0 ] && status=1 ;;
        *) echo "leak-scan: no verdict in $answer" >&2; status=2 ;;
    esac
done
[ -s "$out/deny-hits.txt" ] && [ $status = 0 ] && status=1
echo "leak-scan: $( [ -s "$out/findings.txt" ] && wc -l < "$out/findings.txt" || echo 0) finding line(s) in $out/findings.txt" >&2
exit $status
