/**
 * THE FILES NO BUILD PROGRAM REACHES ARE STILL CODE (#120): a DTO that lost its last importer, a script beside an app, a
 * selector nothing imports any more. They were listed (`files`, `diagnostics` `ts_not_in_any_program`) and never parsed,
 * so a reader could not tell "declared, no users" from "not declared" - and the dead code a review looks for is exactly
 * that. After every real program is walked, the ones left are walked through ONE MORE PROGRAM PER PROJECT: the project
 * whose folder holds them most closely (the first project for a file under none), its own tsconfig's options, these
 * files as its only roots. Its rows are a real program's - checker and all - and only its membership is not: a file it
 * walks stays out of `inAnyProgram`, so `reachable` is 0 and the diagnostic stays (`TsWalk.mjs::markReachable`); it adds no
 * `projects` row - its files belong to the project whose config it borrowed.
 *
 * WHICH FILES: `.ts`, never `.d.ts`, never under `node_modules`, under the workspace - read off the disk as the
 * inventory does, which runs after the walk. A file an outside program imports and a real one already walked keeps
 * that one's rows (the walk's `extracted`).
 */
import path from 'node:path';
import { relativeTo } from './TsPaths.mjs';
import { walkTree } from './TsInventory.mjs';

/** The `.ts` sources under `fe` the walk could be asked about, as canonical paths. */
export function treeSources(fe, skipDirs, skipFiles) {
  return walkTree(fe, new Set([...skipDirs, 'node_modules']), skipFiles)
    .filter((f) => f.ext === 'ts' && !f.abs.toLowerCase().endsWith('.d.ts'))
    .map((f) => f.abs);
}

/** One project per project folder holding files no program reached, each with those files as `roots`, in the order
 *  the projects were walked. */
export function outsideProjects(projects, left, fe) {
  if (!projects.length || !left.length) return [];
  const within = (file, project) => {
    const rel = relativeTo(path.resolve(fe, project.rel), file);
    return !rel.startsWith('..') && !path.isAbsolute(rel);
  };
  const byProject = new Map();
  for (const file of [...left].sort()) {
    const home = projects.filter((p) => within(file, p))
      .sort((a, b) => b.rel.length - a.rel.length)[0] ?? projects[0];
    if (!byProject.has(home)) byProject.set(home, []);
    byProject.get(home).push(file);
  }
  return projects.filter((p) => byProject.has(p)).map((p) => ({ ...p, roots: byProject.get(p) }));
}
