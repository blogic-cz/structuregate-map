/**
 * THE WORKSPACE'S OWN COMPILER, borrowed - and that is the whole point of this file.
 *
 * Ported from the original Angular extractor. Angular's template grammar changes
 * between majors (`@if`/`@for` in 17, `@let` in 18) and an enum member carries the value a real
 * type-checked `ts.Program` computed. A tool-pinned compiler parsing a newer workspace does not fail: it
 * error-recovers into a WRONG map, where a control-flow block reads as text and a gate that exists is
 * simply absent. So the map is produced by the compiler the project builds with, found through
 * `createRequire` and never declared as a dependency of this exe.
 *
 * THERE IS NO LAST-RESORT COPY HERE, and that is the one deliberate difference from the tool this is
 * ported from. It ships `typescript` and `@angular/compiler` in its own `node_modules` as a fallback;
 * structuregate is two files in a consumer and has no `node_modules` of its own to fall back to. A
 * workspace with nothing installed is therefore a named reason, not a quietly worse map.
 */
import { createRequire } from 'node:module';
import { pathToFileURL } from 'node:url';
import { existsSync, readFileSync } from 'node:fs';
import path from 'node:path';

/** The packages both passes need. Checked TOGETHER: a root satisfying only `typescript` would let the
 *  declarations pass succeed and the template pass fail one step later. */
const NEEDED = [['typescript'], ['@angular', 'compiler']];

const hasAll = (nm, needed) => needed.every((pkg) => existsSync(path.join(nm, ...pkg, 'package.json')));

/**
 * Candidate roots, in order: the workspace's own and each ancestor's, then whatever the caller declared.
 *
 * `declared` sits AFTER the ancestors because it exists for the case where neither has anything - a git
 * worktree borrowing from the primary checkout - and putting it first would let a stale sibling override
 * the tree actually being parsed.
 */
function candidates(feRoot, declared) {
  const out = [];
  let dir = path.resolve(feRoot);
  for (;;) {
    out.push([path.join(dir, 'node_modules'), 'workspace']);
    const up = path.dirname(dir);
    if (up === dir) break;
    dir = up;
  }
  for (const d of declared) out.push([path.resolve(d), 'declared']);
  return out.filter(([p], i, all) => all.findIndex(([q]) => q === p) === i);
}

/** `needed` is the Angular pair by default; the plain TypeScript half asks for `typescript` alone. */
export function resolveToolchain(feRoot, declared = [], needed = NEEDED) {
  const tried = [];
  for (const [nm, tier] of candidates(feRoot, declared)) {
    tried.push(nm);
    if (hasAll(nm, needed)) return { nodeModules: nm, tier, tried };
  }
  return { nodeModules: '', tier: '', tried };
}

/** A package's installed version, or null. */
function installedVersion(nodeModules, pkg) {
  try {
    const j = JSON.parse(readFileSync(path.join(nodeModules, ...pkg, 'package.json'), 'utf8'));
    return j.version ?? null;
  } catch {
    return null;
  }
}

const isDigit = (c) => c >= '0' && c <= '9';

/**
 * The major of a version or a range: `^18.2.0` -> `18`, `>=18 <19` -> `18`. By hand and not by pattern,
 * and the FIRST RUN OF DIGITS is all it needs.
 *
 * The leading junk is skipped by asking "is this a digit" rather than by comparing against `'0'`: `^` and
 * `~` sort ABOVE `'0'`, so a `< '0'` test skipped nothing and every ranged version returned "" - which the
 * mismatch check reads as "nothing to compare" and stays silent.
 */
function major(v) {
  const s = String(v);
  let i = 0;
  while (i < s.length && !isDigit(s[i])) i++;
  let out = '';
  while (i < s.length && isDigit(s[i])) out += s[i++];
  return out;
}

/**
 * The workspace's own `@angular/core` major against the `@angular/compiler` major actually used - or null
 * when they agree, or when the workspace declares nothing to compare with.
 *
 * `@angular/core`, not `@angular/compiler`: the compiler is a build dependency an application need not
 * declare, while `core` is the version the application IS.
 */
export function versionMismatch(feRoot, ngVersion) {
  let want = installedVersion(path.join(feRoot, 'node_modules'), ['@angular', 'core']);
  if (!want) {
    try {
      const pkg = JSON.parse(readFileSync(path.join(feRoot, 'package.json'), 'utf8'));
      want = pkg.dependencies?.['@angular/core'] ?? pkg.devDependencies?.['@angular/core'] ?? null;
    } catch {
      want = null;
    }
  }
  if (!want) return null;
  const a = major(want);
  const b = major(ngVersion);
  if (!a || !b || a === b) return null;
  return `parsed with @angular/compiler ${ngVersion}, but this workspace declares @angular/core ${want} `
    + `- Angular's template grammar differs between majors, so anything introduced after ${b}.x is `
    + 'recorded wrongly or not at all';
}

export async function loadToolchain(nodeModules) {
  const req = createRequire(path.join(nodeModules, 'noop.js'));
  const ts = req('typescript');
  // @angular/compiler is ESM: imported by RESOLVED PATH and not by bare specifier, because this script's
  // own node resolution has no node_modules to find it in - it is staged into a temporary folder.
  const compiler = await import(pathToFileURL(req.resolve('@angular/compiler')).href);
  const pkg = JSON.parse(readFileSync(path.join(nodeModules, '@angular', 'compiler', 'package.json'), 'utf8'));
  return { ts, ng: compiler.default ?? compiler, tsVersion: ts.version, ngVersion: pkg.version, req };
}
