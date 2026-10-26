/**
 * ONE `ts.Program` PER PROJECT - the program `ng build` would type-check, minus emit.
 *
 * Ported from the original Angular extractor. The one deviation from a plain
 * `ts.createProgram` is that module resolution may fall back to a `node_modules` OUTSIDE the source tree,
 * because a worktree has none of its own. That affects third-party imports only; resolution inside the
 * project is unchanged.
 */
import { timed } from './TsMetrics.mjs';
import path from 'node:path';
import { readdirSync, statSync } from 'node:fs';
import { builtinModules } from 'node:module';

import { slash } from './TsPaths.mjs';

// Node's own registry, not a list to maintain: it cannot drift when Node adds a module.
const NODE_BUILTINS = new Set(builtinModules);

/**
 * WHETHER A RELATIVE SPECIFIER NAMES A FILE ONLY WHEN CASE IS IGNORED: `./Foo` beside `foo.ts`. Windows and macOS resolve it, Linux
 * does not, so the build that ships from there breaks on what the developers' machines never noticed. Published as
 * `unresolved_import.case_insensitive_match` so a consumer sees the Windows-only imports apart from the missing ones. Each path segment is
 * looked up in its folder's listing, and a hit that spells the path exactly (resolution failed for another reason) is not one.
 */
function caseInsensitiveMatch(ts, name, containingFile, options, host, nmDummy) {
  if (!name.startsWith('.')) return aliasMatch(ts, name, containingFile, options, host, nmDummy);
  const wanted = path.resolve(path.dirname(containingFile), name);
  // EACH FOLDER LISTED ONCE FOR THE RUN (`actualPath`'s listings): every unresolved import of every file listed every folder on its path
  // again, which was seconds of a partial run on a large workspace.
  for (const form of ['', '.ts', '.tsx', '.d.ts', '.js', '.mjs', '/index.ts', '/index.d.ts']) {
    const full = path.normalize(wanted + form);
    const at = actualPath(full, true);
    if (at === undefined || at === full) continue;
    if (statOf(at)?.isFile()) return true;
  }
  return false;
}

/**
 * A NON-RELATIVE SPECIFIER (a `paths` alias such as `@shared/x`) resolves only with case ignored (rows 80): the program's own resolution failed, so it is resolved AGAIN through a host
 * whose `fileExists` and `directoryExists` compare each path segment ignoring case - the `paths` mapping, `baseUrl` and `node_modules` lookups are the compiler's own, never re-implemented.
 * It is asked only of a specifier the exact resolution refused, so a hit is a case-only match by definition; a specifier nothing resolves (`version.json`) stays false.
 */
const LISTINGS = new Map();
const STATS = new Map();
/** `stat` of a path, once for the run; null when there is nothing there. */
function statOf(p) {
  if (!STATS.has(p)) {
    let st = null;
    try { st = statSync(p); } catch { /* nothing there */ }
    STATS.set(p, st);
  }
  return STATS.get(p);
}
/** The path as the disk spells it, each segment matched ignoring case; `caseFirst` takes a folder's first case-insensitive match even where the
 *  exact spelling is there too (`caseInsensitiveMatch` always did), else the exact one first. */
function actualPath(file, caseFirst = false) {
  const full = path.normalize(file);
  let at = path.parse(full).root;
  for (const part of full.slice(at.length).split(path.sep).filter((p) => p !== '')) {
    if (!LISTINGS.has(at)) {
      let entries;
      try { entries = readdirSync(at); } catch { entries = null; }
      LISTINGS.set(at, entries);
    }
    const entries = LISTINGS.get(at);
    if (entries === null) return undefined;
    const folded = () => entries.find((e) => e.toLowerCase() === part.toLowerCase());
    const hit = caseFirst ? folded() : entries.includes(part) ? part : folded();
    if (hit === undefined) return undefined;
    at = path.join(at, hit);
  }
  return at;
}
// ONE ANSWER PER SPECIFIER, FOLDER, EXTENSION AND PROGRAM: resolution reads only the importing file's folder (its `node_modules` up the tree, `paths`
// against `baseUrl`), so every file of a folder importing the same missing package was resolved again from scratch.
const ALIASED = new WeakMap();
function aliasMatch(ts, name, containingFile, options, host, nmDummy) {
  if (!ALIASED.has(options)) ALIASED.set(options, new Map());
  const memo = ALIASED.get(options);
  const key = `${path.dirname(containingFile)}\0${path.extname(containingFile)}\0${name}`;
  if (!memo.has(key)) memo.set(key, aliasResolved(ts, name, containingFile, options, host, nmDummy));
  return memo.get(key);
}
function aliasResolved(ts, name, containingFile, options, host, nmDummy) {
  const statFolded = (p) => { const real = actualPath(p); return real === undefined ? null : statOf(real); };
  const folded = {
    fileExists: (f) => statFolded(f)?.isFile() ?? false,
    directoryExists: (d) => statFolded(d)?.isDirectory() ?? false,
    readFile: (f) => host.readFile(actualPath(f) ?? f),
    realpath: (p) => p,
    getCurrentDirectory: () => host.getCurrentDirectory(),
    getDirectories: (d) => host.getDirectories?.(actualPath(d) ?? d) ?? [],
  };
  try {
    return !!(ts.resolveModuleName(name, containingFile, options, folded).resolvedModule
      ?? ts.resolveModuleName(name, nmDummy, options, folded).resolvedModule);
  } catch {
    return false;
  }
}

// WHAT THE DISK SAID, remembered for the process. The tree cannot change under one run, and the programs of
// one workspace ask the same questions over and over: on a large Angular workspace module resolution alone took many seconds, most of
// it `stat`, `open` and `realpath` on paths an earlier program had already probed.
const PROBED = { fileExists: new Map(), directoryExists: new Map(), realpath: new Map() };
function remembered(host, name) {
  const ask = host[name]?.bind(host);
  if (!ask) return;
  const seen = PROBED[name];
  host[name] = (p) => {
    if (seen.has(p)) return seen.get(p);
    const answer = ask(p);
    seen.set(p, answer);
    return answer;
  };
}

// ONE PARSE OF A FILE FOR EVERY PROGRAM THAT CONTAINS IT, as tsserver's document registry shares them: the
// projects of one workspace overlap in their libraries and in every `.d.ts` of `node_modules`, and each program
// read and parsed them all again. A parse depends on the file and on how it is asked for - the language
// version, the module format, the JSDoc mode - and on the options that decide whether it is a module, so
// those are the key; a program asking differently gets its own parse. WHICH options is the compiler's own list
// (`sourceFileAffectingCompilerOptions`, the one tsserver keys its registry by - `alwaysStrict` is on it,
// because binding reads it); a compiler without that list shares only between identical option sets.
const PARSED = new Map();
/** Every shared parse released - once the walk is done, no program asks again. */
export const forgetParses = () => { PARSED.clear(); RESOLVED.clear(); };

// ONE MODULE RESOLUTION CACHE PER CONFIG, as `tsc` keeps one per program: a config built ahead (`prewarm`) and then by
// the walk resolves its imports once. A cache holds what `resolveModuleName` answers - the host is still asked through
// `resolveModuleNames` for every import, so every note is made as before - and is asked with that config's options only.
const RESOLVED = new Map();
function shareSourceFiles(ts, host, options) {
  const parse = host.getSourceFile.bind(host);
  const affecting = Array.isArray(ts.sourceFileAffectingCompilerOptions)
    ? ts.sourceFileAffectingCompilerOptions.map((o) => o.name).sort()
    : Object.keys(options).sort();
  const shape = JSON.stringify(affecting.map((name) => [name, options[name] ?? null]));
  host.getSourceFile = (fileName, how, onError, shouldCreate) => {
    const asked = typeof how === 'object' && how !== null
      ? `${how.languageVersion}|${how.impliedNodeFormat ?? ''}|${how.jsDocParsingMode ?? ''}`
      : String(how);
    const key = `${fileName}\u0000${asked}\u0000${shape}`;
    if (!shouldCreate && PARSED.has(key)) return PARSED.get(key);
    const sf = parse(fileName, how, onError, shouldCreate);
    if (sf) PARSED.set(key, sf);
    return sf;
  };
}

/**
 * EVERY PROJECT'S PROGRAM BUILT AHEAD OF THE WALK, without a checker - while a partial run waits for the rows it is
 * handed back (`rows/partial/carry.rs` writes them as node starts). The parses land in the shared set the walk's own
 * programs take theirs from, in the order the walk would have made them, and no program is kept.
 *
 * AND THEY GO BACK AS THEY WERE PARSED: a compiler before 5.3 keeps what a program resolved a file's imports to ON the
 * parse (`resolvedModules`), and a later program holding a file that declares ambient modules takes that answer without
 * asking the host - so the walk's programs resolved, and noted, less than a run that built nothing ahead. Forgotten,
 * the walk resolves every import exactly as it would have. No checker, no note: a config that cannot be read is read
 * again, and reported, by the walk.
 */
export function prewarm(ts, projects, nodeModules) {
  for (const project of projects) {
    try {
      const { program } = timed('typescript: build a program ahead', project.tsconfig,
        () => createProgram(ts, project.tsconfig, nodeModules, null, new Set(), false));
      for (const sf of program.getSourceFiles()) {
        sf.resolvedModules = undefined;
        sf.resolvedTypeReferenceDirectiveNames = undefined;
      }
    } catch {
      // the walk builds it again, and says why it could not
    }
  }
}

// `roots` IN PLACE OF THE CONFIG'S OWN FILES: the files no program reached, under a project's options (`TsOutside.mjs`).
export function createProgram(ts, tsconfigPath, nodeModules, diag, noted, checked = true, roots = null) {
  const configHost = {
    ...ts.sys,
    onUnRecoverableConfigFileDiagnostic: (d) => {
      throw new Error('tsconfig unreadable: ' + ts.flattenDiagnosticMessageText(d.messageText, '\n'));
    },
  };
  const parsed = ts.getParsedCommandLineOfConfigFile(tsconfigPath, {}, configHost);
  if (!parsed) throw new Error('could not parse tsconfig: ' + tsconfigPath);

  const options = {
    ...parsed.options,
    noEmit: true,
    skipLibCheck: true,
    skipDefaultLibCheck: true,
    // A missing @angular/* type does not stop AST extraction; it is recorded as a diagnostic instead.
    noResolve: false,
    typeRoots: [path.join(nodeModules, '@types')],
  };
  const host = ts.createCompilerHost(options, true);
  for (const name of Object.keys(PROBED)) remembered(host, name);
  shareSourceFiles(ts, host, options);
  // THE COMPILER'S OWN RESOLUTION CACHE, as `tsc` passes it: without one every import of every file is resolved
  // from the disk again. Used only when asked with this program's own options - a redirect through a project
  // reference resolves as before.
  if (!RESOLVED.has(tsconfigPath)) {
    RESOLVED.set(tsconfigPath, ts.createModuleResolutionCache(host.getCurrentDirectory(), host.getCanonicalFileName, options));
  }
  const cache = RESOLVED.get(tsconfigPath);
  const nmDummy = path.join(nodeModules, '__resolve__.ts');
  // Resolve against the real containing file first; for a bare specifier that fails, retry as if the
  // importer lived next to the node_modules that is available.
  host.resolveModuleNames = (names, containingFile, _reused, _redirect, opts) => names.map((name) => {
    const cached = opts === options ? cache : undefined;
    let r = ts.resolveModuleName(name, containingFile, opts, host, cached).resolvedModule;
    if (!r && !name.startsWith('.')) {
      r = ts.resolveModuleName(name, nmDummy, opts, host, cached).resolvedModule;
    }
    if (!r && diag && !noted.has(`${name}|${containingFile}`)) {
      // ONE NOTE PER UNRESOLVED IMPORT, not one per program that re-resolves it: projects overlap, so the
      // same specifier in the same file is re-resolved by every program containing that file, and the
      // count would then move with this tool's iteration order rather than with the frontend.
      //
      // `node:fs` and friends come from a dependency's typings in a browser app - absent by design, not a
      // gap in the map - so they are counted under their own kind.
      noted.add(`${name}|${containingFile}`);
      const builtin = name.startsWith('node:') || NODE_BUILTINS.has(name);
      // WHICH PROGRAM NOTED IT (`program`, rows 125): a partial run that leaves a project's program unbuilt says its notes again from the
      // map, in that project's place (`TsDecls/TsWalk.mjs`), so the table is what a full run writes.
      const program = diag.program ?? null;
      diag.note(builtin ? 'unresolved_node_builtin' : 'unresolved_import',
        builtin ? { module: name, from: containingFile, program }
          : { module: name, from: containingFile, case_insensitive_match: caseInsensitiveMatch(ts, name, containingFile, opts, host, nmDummy), program });
    }
    return r;
  });

  const rootNames = roots ?? parsed.fileNames;
  const program = ts.createProgram({ rootNames, options, host });
  return { program, checker: checked ? program.getTypeChecker() : null, options, rootNames };
}

/**
 * EVERY file of the FE tree the program CONTAINS, `.d.ts` included - which files are in the BUILD, as
 * opposed to which files the extractor walks.
 *
 * The two are not the same, and conflating them mislabelled every `.d.ts` of a real frontend as
 * "dead code the build never ships": every project pulls its typings in with an `include: ["**\/*.d.ts"]`
 * glob, so they are ROOT FILES of the real program.
 */
export function programFilePaths(program, feRoot) {
  const root = slash(path.resolve(feRoot)).toLowerCase();
  const out = [];
  for (const sf of program.getSourceFiles()) {
    const f = slash(path.resolve(sf.fileName)).toLowerCase();
    if (f.startsWith(root) && !f.includes('/node_modules/')) out.push(sf.fileName);
  }
  return out;
}

/** Project source files only: no `.d.ts`, nothing outside the FE tree (lib/@types noise). */
export function projectSourceFiles(program, feRoot) {
  const root = slash(path.resolve(feRoot)).toLowerCase();
  return program.getSourceFiles().filter((sf) => {
    if (sf.isDeclarationFile) return false;
    const f = slash(path.resolve(sf.fileName)).toLowerCase();
    return f.startsWith(root) && !f.includes('/node_modules/');
  });
}
