/**
 * THE WORKSPACE'S PROJECTS, and the tsconfig that is each project's REAL build program.
 *
 * Ported from the original Angular extractor. Angular never builds one program for
 * a monorepo: one program per project, its own tsconfig, its own module-federation boundary. So the
 * extraction is per project and then merged.
 *
 * THREE LAYOUTS, ASKED IN ORDER, because "an Angular project" is a different file depending on which
 * toolchain generated the workspace, and a tool that knows one of them works in one repository:
 *
 *   1. Nx - a `project.json` per project. Authoritative when present.
 *   2. The Angular CLI - a `projects` map in `angular.json` (or the older `workspace.json`).
 *   3. Neither - a single-project workspace whose root `tsconfig.json` names its own sources.
 *
 * The layers are NOT merged: the same project described twice becomes two programs over the same files,
 * and every declaration in it is extracted and attributed twice.
 */
import { existsSync, readFileSync, readdirSync } from 'node:fs';
import path from 'node:path';

import { relativeTo, slash } from './TsPaths.mjs';

/** TypeScript's own default config filename - the one entry point that cannot be derived, because `tsc`
 *  itself defines it. Everything past it is read FROM it (`references`), never guessed. */
const TS_DEFAULT_CONFIG = 'tsconfig.json';

/** The workspace files that carry a `projects` map, in the order the toolchain introduced them. */
const WORKSPACE_FILES = ['angular.json', 'workspace.json'];

/**
 * Read a tsconfig with THE COMPILER'S OWN parser.
 *
 * A tsconfig is JSONC and Angular's ship with `//` comments, so `JSON.parse` throws on them. That cost
 * twice in the tool this is ported from: treating "cannot tell" as "unusable" dropped a whole project of
 * hundreds of files silently, and failing open then hid a config's `references`, so several libs fell back to a
 * references-only root and came out with EMPTY programs.
 */
function readJsonc(ts, p) {
  try {
    const parsed = ts.parseConfigFileTextToJson(p, readFileSync(p, 'utf8'));
    return parsed.error ? null : parsed.config;
  } catch {
    return null;
  }
}

/** A tsconfig is usable only if it names sources itself. An Nx solution-style config with nothing but
 *  `references` yields an EMPTY program, which reads as a project with no code in it. */
function usableConfig(ts, p) {
  if (!existsSync(p)) return false;
  const cfg = readJsonc(ts, p);
  if (!cfg) return true;                       // unparseable: fail OPEN, see readJsonc
  if (cfg.files?.length || cfg.include?.length) return true;
  return !cfg.references?.length;              // no references and no include: extends-only, worth trying
}

/**
 * Configs a references-only config POINTS AT, followed transitively.
 *
 * This replaces a list of conventional basenames (`tsconfig.lib.json`, `tsconfig.app.json`, ...): an Nx
 * library root config declares its own alternatives in `references`, so the candidates are stated in the
 * file and a project that names its config something else still resolves.
 */
function referencedConfigs(ts, configPath, seen = new Set()) {
  const abs = path.resolve(configPath);
  if (seen.has(abs)) return [];
  seen.add(abs);
  const cfg = readJsonc(ts, abs);
  const out = [];
  for (const ref of cfg?.references ?? []) {
    if (!ref?.path) continue;
    let target = path.resolve(path.dirname(abs), ref.path);
    // A reference may name a directory (its tsconfig.json) or the file directly - TypeScript takes both.
    if (existsSync(target) && !target.endsWith('.json')) target = path.join(target, TS_DEFAULT_CONFIG);
    if (!existsSync(target)) continue;
    out.push(target);
    out.push(...referencedConfigs(ts, target, seen));
  }
  return out;
}

/** The tsConfig a build target declares, resolved against the workspace root. Both toolchains write it
 *  per target, so this is the project's REAL program definition rather than a filename convention. */
function declaredTsConfigs(meta, feRoot) {
  const out = [];
  for (const target of [...Object.values(meta.targets ?? {}), ...Object.values(meta.architect ?? {})]) {
    const v = target?.options?.tsConfig;
    for (const c of Array.isArray(v) ? v : v === undefined ? [] : [v]) {
      const abs = path.resolve(feRoot, c);
      if (existsSync(abs) && !out.includes(abs)) out.push(abs);
    }
  }
  return out;
}

/** Every target name a project declares, under whichever of the two keys its toolchain uses. */
const targetNames = (meta) => [...Object.keys(meta.targets ?? {}), ...Object.keys(meta.architect ?? {})]
  .filter((n, i, all) => all.indexOf(n) === i);

/**
 * One project from a directory and whatever metadata described it - or null when no tsconfig in it names
 * any sources.
 *
 * Shared by all three layers on purpose: the candidate ORDER (what the target declares, then what the root
 * config references, then the root config itself) and "first usable wins" are the part that was measured,
 * and a second layer re-deriving them differently would pick a different program for an Angular-CLI
 * workspace than for an Nx one.
 */
function projectAt(ts, feRoot, dir, meta, discoveredBy) {
  const root = path.join(dir, TS_DEFAULT_CONFIG);
  const candidates = [...declaredTsConfigs(meta, feRoot), ...referencedConfigs(ts, root), root]
    .filter((c, i, all) => all.indexOf(c) === i)
    .filter((c) => usableConfig(ts, c));
  const tsconfig = candidates[0];
  if (!tsconfig) return null;
  return {
    name: meta.name ?? path.basename(dir),
    dir,
    rel: relativeTo(feRoot, dir),
    projectType: meta.projectType ?? null,
    sourceRoot: meta.sourceRoot ?? null,
    tsconfig,
    tsconfigCandidates: candidates,
    targets: targetNames(meta),
    discoveredBy,
  };
}

function findProjectJsons(root, depth = 3) {
  const out = [];
  const walk = (dir, d) => {
    if (d < 0) return;
    let entries;
    try {
      entries = readdirSync(dir, { withFileTypes: true });
    } catch {
      return;
    }
    for (const e of entries) {
      if (e.name === 'node_modules' || e.name.startsWith('.')) continue;
      const p = path.join(dir, e.name);
      if (e.isDirectory()) walk(p, d - 1);
      else if (e.name === 'project.json') out.push(p);
    }
  };
  walk(root, depth);
  return out;
}

/** Layer 1: an Nx-style `project.json` per project. */
function fromProjectJsons(ts, feRoot) {
  const out = [];
  for (const pj of findProjectJsons(feRoot)) {
    let meta = {};
    try {
      meta = JSON.parse(readFileSync(pj, 'utf8'));
    } catch { /* an unreadable project.json still yields a project, named after its dir */ }
    const p = projectAt(ts, feRoot, path.dirname(pj), meta, 'project.json');
    if (p) out.push(p);
  }
  return out;
}

/**
 * Layer 2: the `projects` map in `angular.json` / `workspace.json`.
 *
 * A project's directory is its declared `root`, and the workspace root itself is the legitimate answer for
 * the default app of a single-project workspace (`"root": ""`). An entry that is a STRING is the old
 * indirection - `"my-app": "apps/my-app"` - and it is followed rather than skipped, because layer 1 only
 * sees it if it lies within the search depth.
 */
function fromWorkspaceFile(ts, feRoot) {
  const out = [];
  for (const name of WORKSPACE_FILES) {
    const file = path.join(feRoot, name);
    if (!existsSync(file)) continue;
    const cfg = readJsonc(ts, file);
    for (const [projectName, entry] of Object.entries(cfg?.projects ?? {})) {
      let meta = {};
      let dir;
      if (typeof entry === 'string') {
        dir = path.resolve(feRoot, entry);
        const pj = path.join(dir, 'project.json');
        if (existsSync(pj)) {
          try {
            meta = JSON.parse(readFileSync(pj, 'utf8'));
          } catch { /* named by its dir */ }
        }
      } else if (entry && typeof entry === 'object') {
        meta = entry;
        dir = path.resolve(feRoot, meta.root ?? '');
      } else continue;
      meta.name ??= projectName;
      const p = projectAt(ts, feRoot, dir, meta, 'workspace');
      if (p) out.push(p);
    }
    if (out.length) return out;
  }
  return out;
}

/** Layer 3: no workspace metadata at all - the root tsconfig, if it names its own sources. */
function fromRootTsconfig(ts, feRoot) {
  const p = projectAt(ts, feRoot, feRoot, {}, 'root-tsconfig');
  return p ? [p] : [];
}

export function discoverProjects(ts, feRoot) {
  // FIRST LAYER THAT ANSWERS WINS. Not a merge - see the header.
  const projects = [fromProjectJsons, fromWorkspaceFile, fromRootTsconfig]
    .reduce((found, layer) => (found.length ? found : layer(ts, feRoot)), []);
  // A workspace file can name two entries with the same `root` (an app and its e2e suite often share
  // one) and two `project.json` files cannot - so the de-duplication is on the DIRECTORY, which is what
  // decides the program, and never on the name.
  const seen = new Set();
  const unique = projects.filter((p) => {
    if (seen.has(p.dir)) return false;
    seen.add(p.dir);
    return true;
  });
  unique.sort((a, b) => a.name.localeCompare(b.name));
  return unique;
}

/** The workspace root a map is taken of: the directory holding `angular.json` / `nx.json` /
 *  `workspace.json`, at or under the root this gate was pointed at. */
export function findWorkspaceRoot(root) {
  const marks = ['angular.json', 'nx.json', 'workspace.json'];
  if (marks.some((m) => existsSync(path.join(root, m)))) return slash(path.resolve(root));
  const walk = (dir, depth) => {
    if (depth < 0) return '';
    let entries;
    try {
      entries = readdirSync(dir, { withFileTypes: true });
    } catch {
      return '';
    }
    for (const e of entries) {
      if (!e.isDirectory() || e.name === 'node_modules' || e.name.startsWith('.')) continue;
      const p = path.join(dir, e.name);
      if (marks.some((m) => existsSync(path.join(p, m)))) return slash(p);
      const deeper = walk(p, depth - 1);
      if (deeper) return deeper;
    }
    return '';
  };
  return walk(path.resolve(root), 3);
}
