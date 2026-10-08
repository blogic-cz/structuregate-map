/**
 * ONE `ts.Program` PER PROJECT - the program `ng build` would type-check, minus emit.
 *
 * Ported from the original Angular extractor. The one deviation from a plain
 * `ts.createProgram` is that module resolution may fall back to a `node_modules` OUTSIDE the source tree,
 * because a worktree has none of its own. That affects third-party imports only; resolution inside the
 * project is unchanged.
 */
import path from 'node:path';
import { builtinModules } from 'node:module';

import { slash } from './TsPaths.mjs';

// Node's own registry, not a list to maintain: it cannot drift when Node adds a module.
const NODE_BUILTINS = new Set(builtinModules);

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
export const forgetParses = () => PARSED.clear();
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

export function createProgram(ts, tsconfigPath, nodeModules, diag, noted) {
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
  const cache = ts.createModuleResolutionCache(host.getCurrentDirectory(), host.getCanonicalFileName, options);
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
      diag.note(builtin ? 'unresolved_node_builtin' : 'unresolved_import',
        { module: name, from: containingFile });
    }
    return r;
  });

  const program = ts.createProgram({ rootNames: parsed.fileNames, options, host });
  return { program, checker: program.getTypeChecker(), options, rootNames: parsed.fileNames };
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
