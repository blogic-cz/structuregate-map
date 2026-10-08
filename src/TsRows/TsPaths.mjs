/**
 * ONE PHYSICAL FILE, ONE ROW - canonical on-disk casing.
 *
 * Ported from the original Angular extractor, and the comment there is the reason it
 * has to be ported rather than re-derived: the extractor discovers files two ways, and on Windows they
 * disagree about case. `ts.Program` keeps the spelling from the IMPORT SPECIFIER, while a directory walk
 * reads the real entries. Windows and git treat those as one file; a map that does not gets TWO rows for
 * each such file, one `core, reachable` and one `inventory, dead`, and then reports live files as dead code.
 *
 * `realpathSync.native` asks the filesystem for the true entry, so both spellings resolve to one key, and
 * it is the honest answer on a case-sensitive filesystem too, where the two really are different files.
 */
import { realpathSync } from 'node:fs';
import path from 'node:path';

const cache = new Map();

/** Backslashes to forward slashes. Split and join rather than a pattern - the build's own ban over this
 *  folder rejects the pattern forms, for the reason the rest of the tool gives: a pattern matches inside a
 *  string and inside a comment, where a tree does not. (Naming the banned call here would fail the build
 *  on this very line: the ban is a literal `findstr`.) */
export const slash = (p) => String(p).split('\\').join('/');

// BY THE SPELLING ASKED, before `path.resolve`: the extractor asks for the same few thousand compiler file names
// millions of times, and resolving each one first cost more than the lookup it guards.
const asked = new Map();

export function canonicalPath(p) {
  const known = asked.get(p);
  if (known !== undefined) return known;
  const canon = canonicalOf(p);
  if (typeof p === 'string') asked.set(p, canon);
  return canon;
}

function canonicalOf(p) {
  const resolved = path.resolve(p);
  const hit = cache.get(resolved);
  if (hit !== undefined) return hit;
  let canon;
  try {
    canon = realpathSync.native(resolved);
  } catch {
    // A path that cannot be resolved (deleted between the walk and the read) keeps its own spelling:
    // better a row with the name we saw than no row at all.
    canon = resolved;
  }
  canon = slash(canon);
  cache.set(resolved, canon);
  return canon;
}

/** A path as the map spells it: relative to the workspace root, forward slashes, `.` for the root itself. */
export function relativeTo(root, p) {
  return slash(path.relative(root, p)) || '.';
}
