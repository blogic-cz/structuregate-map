/**
 * FILE INVENTORY - every file in scope gets a row, parsed or not.
 *
 * Ported from the original Angular extractor. A program contains only what TypeScript
 * reaches: `.ts` plus the templates a decorator names. A `.scss`, a fixture `.json`, an `.svg`, an `.html`
 * nobody references - none of them appear, and a consumer asking "is that all of it?" cannot tell an
 * absent file from an unparsed one. So the walk records a row for every file and marks `parsed: false`
 * until a pass claims it.
 */
import { createReadStream, readdirSync, readFileSync, statSync } from 'node:fs';
import path from 'node:path';
import { availableParallelism } from 'node:os';
import { createHash } from 'node:crypto';

import { canonicalPath, relativeTo } from './TsPaths.mjs';

/** The one algorithm. SHA-256, hex, in full - a shortened digest trades a real collision probability for
 *  bytes nobody is counting. */
export const HASH_ALGORITHM = 'sha256';

/** The newline byte, asked of the language rather than written as 10. */
const NEWLINE = '\n'.charCodeAt(0);

/**
 * Line count AND content digest, from ONE streamed read, with no size constant of any kind.
 *
 * The hash rides the same pass: every byte is already flowing through here, so digesting it costs no
 * extra I/O, and a second walk over thousands of files to compute one field would be the expensive way
 * to the same answer.
 *
 * Text-vs-binary is decided from the FIRST chunk: a NUL byte means binary. `lines: null` means "no line
 * count" (binary, or unreadable) and never 0, which would read as an empty file. A BINARY FILE IS STILL
 * HASHED - "its lines cannot be counted" is no reason to leave an .svg, a font or a .png without content
 * identity, which would be a hole exactly where a consumer comparing two runs would not think to look.
 * `hash: null` means the read FAILED, which is a different fact.
 */
async function scanFile(file) {
  const digest = createHash(HASH_ALGORITHM);
  let newlines = 0;
  let sawBytes = false;
  let binary = false;
  let first = true;
  try {
    // No `highWaterMark`: Node's own default applies, so there is no chunk-size constant here at all.
    for await (const chunk of createReadStream(file)) {
      if (first && chunk.includes(0)) binary = true;
      first = false;
      sawBytes = true;
      digest.update(chunk);
      if (!binary) for (const byte of chunk) if (byte === NEWLINE) newlines += 1;
    }
  } catch {
    return { lines: null, hash: null };
  }
  // Matches what a whole-file split on newlines reports, so counts stay comparable: a file with content
  // has one more line than its newline count; an empty file counts as 1.
  return { lines: binary ? null : sawBytes ? newlines + 1 : 1, hash: digest.digest('hex') };
}

/**
 * EVERY FILE OF THE TREE, as `{abs, p, ext, size}` - the one walk both the inventory and the fingerprint
 * use. Two walks that skipped a directory differently would let the fingerprint say "unchanged" about a
 * tree it had not looked at, which is the only way an early exit can be wrong.
 */
export function walkTree(root, skipDirs = new Set(['node_modules']), skipFiles = new Set()) {
  const found = [];
  const walk = (dir) => {
    let entries;
    try {
      entries = readdirSync(dir, { withFileTypes: true });
    } catch {
      return;
    }
    for (const e of entries) {
      const p = path.join(dir, e.name);
      if (e.isDirectory()) {
        if (!skipDirs.has(e.name)) walk(p);
        continue;
      }
      if (!e.isFile()) continue;
      // THE MAP IS NOT ITS OWN SOURCE. A consumer may write the database inside the tree it maps, and
      // then the file's hash changes every run BECAUSE of the run - the tree never reads as unchanged and
      // the `files` row holds a hash of a file the row itself is inside.
      if (skipFiles.has(canonicalPath(p))) continue;
      let size = 0;
      try {
        size = statSync(p).size;
      } catch { /* row still written, size 0 */ }
      found.push({ abs: canonicalPath(p), p, ext: path.extname(e.name).slice(1).toLowerCase(), size });
    }
  };
  walk(root);
  return found;
}

/** Every file's content hash, keyed the way `files.path` is - what "has this tree changed" is asked of. */
export async function fingerprint({ root, feRoot, skipDirs = new Set(['node_modules']),
  skipFiles = new Set() }) {
  const found = walkTree(root, skipDirs, skipFiles);
  const hashes = new Array(found.length).fill(null);
  let next = 0;
  const worker = async () => {
    for (;;) {
      const i = next++;
      if (i >= found.length) return;
      hashes[i] = (await scanFile(found[i].p)).hash;
    }
  };
  await Promise.all(Array.from({ length: Math.min(availableParallelism(), found.length || 1) }, worker));
  const out = new Map();
  for (const [i, f] of found.entries()) if (hashes[i] !== null) out.set(relativeTo(feRoot, f.abs), hashes[i]);
  return out;
}

/**
 * A FILE WHOSE HASH DID NOT MOVE IS NOT READ AGAIN for its line count. `now` is the plan's hashes of the tree,
 * `known` what the database recorded (`{path: [sha, lines]}`): where the two agree, the recorded count is the
 * count. Every other file - new, changed, never counted, or a run with no plan - is read as before.
 */
export async function inventory({ store, root, feRoot, projectId = null,
  skipDirs = new Set(['node_modules']), skipFiles = new Set(), now = new Map(), known = new Map() }) {
  // BACKFILL, don't just intern. A template file row is interned the moment a `templateUrl` resolves -
  // with only {path, abs, ext, project} - and `intern` returns that existing row WITHOUT running the
  // builder. So every .html that is a templateUrl target silently kept no lines, bytes or tier. Patch the
  // row that is already there instead of relying on insertion order.
  const existing = new Map(store.table('files').map((r) => [r.abs, r]));
  const found = walkTree(root, skipDirs, skipFiles);

  const lines = new Array(found.length).fill(null);
  const hashes = new Array(found.length).fill(null);
  let next = 0;
  const worker = async () => {
    for (;;) {
      const i = next++;
      if (i >= found.length) return;
      const rel = relativeTo(feRoot, found[i].abs);
      const was = known.get(rel);
      if (was && now.get(rel) === was[0]) {
        lines[i] = was[1];
        hashes[i] = was[0];
        continue;
      }
      const scanned = await scanFile(found[i].p);
      lines[i] = scanned.lines;
      hashes[i] = scanned.hash;
    }
  };
  await Promise.all(Array.from({ length: Math.min(availableParallelism(), found.length || 1) }, worker));

  let bytes = 0;
  for (const [i, f] of found.entries()) {
    const row = existing.get(f.abs);
    if (row) {
      row.bytes ??= f.size;
      if (row.lines === undefined || row.lines === null) row.lines = lines[i] ?? null;
      // The digest is this walk's to give: a row interned by the declarations pass was created from a
      // REFERENCE, which never read the bytes. Assigned rather than defaulted - only one place computes it.
      if (hashes[i] !== null) row.content_hash = hashes[i];
      row.tier ??= 'inventory';
    } else {
      const id = store.intern('files', 'f', f.abs, () => ({
        path: relativeTo(feRoot, f.abs), abs: f.abs, ext: f.ext, project: projectId,
        bytes: f.size, lines: lines[i] ?? null, content_hash: hashes[i], tier: 'inventory', parsed: false,
      }));
      // Backfill through the store rather than trusting insertion order: this key can already exist, and
      // on a repeat `intern` hands back the id without running the builder above.
      store.patch(id, { bytes: f.size, lines: lines[i] ?? null, content_hash: hashes[i] });
    }
    bytes += f.size;
  }
  return { files: found.length, bytes };
}

/**
 * Which rows a pass actually parsed, so `parsed: false` is a real, countable gap.
 *
 * EVERY file row gets the flag, not only the inventory ones. A row interned during the declarations pass
 * started with no `parsed` field at all, so a query for `parsed === false` skipped it and the map looked
 * fully parsed while templates beyond the depth limit had never been read.
 */
export function markParsed(store, absPaths) {
  const wanted = new Set([...absPaths]);
  let n = 0;
  for (const row of store.table('files')) {
    if (wanted.has(row.abs)) {
      row.parsed = true;
      n += 1;
    } else if (row.parsed !== true) row.parsed = false;
  }
  return n;
}

/**
 * WHICH FILES A CHANGE REACHES - the transitive closure over the graph the database recorded.
 *
 * `deps` is `{path -> [the paths that must be re-extracted with it]}`, derived on the python side from
 * every id reference in the finished map (`rust/fbtcore/src/rows/partial/deps.rs`) and handed here in the state, because
 * node cannot read that database. It is keyed by PATH: node has no id for a file until it extracts one.
 *
 * THE GRAPH DESCRIBES THE MAP AS IT IS, so it answers only while the changed files keep resolving the same
 * way. A file that changes a component SELECTOR, or an NgModule's declarations, changes which components a
 * template resolves - an edge that does not exist yet and therefore cannot be in this graph. That is
 * checked separately, and the run falls back to extracting everything.
 */
export function affectedFiles(deps, changed) {
  const seen = new Set(changed);
  const stack = [...seen];
  while (stack.length) {
    for (const dependent of (deps && deps[stack.pop()]) || []) {
      if (!seen.has(dependent)) {
        seen.add(dependent);
        stack.push(dependent);
      }
    }
  }
  return seen;
}

/** `{path: [sha, lines]}` from `--lines`, or an empty map with no file. */
export function knownLines(file) {
  if (!file) return new Map();
  try {
    return new Map(Object.entries(JSON.parse(readFileSync(file, 'utf8'))));
  } catch {
    return new Map();
  }
}

/** The hashes a plan was decided from, or null when there is no plan to read them from. */
export function plannedHashes(file) {
  if (!file) return null;
  try {
    const hashes = JSON.parse(readFileSync(file, 'utf8')).hashes;
    return hashes ? new Map(Object.entries(hashes)) : null;
  } catch {
    return null;
  }
}
