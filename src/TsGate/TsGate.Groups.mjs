/**
 * WHICH COMPILER READS WHICH FILE. A tree whose TypeScript lives in `sub/` - with `sub/package.json` and
 * `sub/node_modules/typescript` - and is gated from its parent had every `.ts` file UNMAPPED: the compiler was
 * resolved from `--root` alone, and node looks UP from a folder, never down into one.
 *
 * So a file is read by the compiler resolved from the nearest folder at or under `--root` that holds a
 * `package.json` - the folder `npm install` put its compiler beside. Node still looks up from there, so a compiler
 * installed at the root reaches every folder as it always did. Folders that resolve to the SAME compiler share one
 * parser: that is the whole tree in the common case, and one parser per distinct install otherwise.
 */
import { existsSync } from 'node:fs';
import { createRequire } from 'node:module';
import path from 'node:path';
import { pathToFileURL } from 'node:url';

/** The folder whose `package.json` decides `abs`'s compiler - `root` when nothing nearer has one. */
function packageFolder(abs, root, seen) {
    const top = path.resolve(root);
    let dir = path.dirname(path.resolve(abs));
    const walked = [];
    let found = top;
    while (dir.startsWith(top) && dir !== top) {
        if (seen.has(dir)) { found = seen.get(dir); break; }
        walked.push(dir);
        if (existsSync(path.join(dir, 'package.json'))) { found = dir; break; }
        const up = path.dirname(dir);
        if (up === dir) break;
        dir = up;
    }
    for (const folder of walked) seen.set(folder, found);
    return found;
}

/** Which `typescript` a folder resolves to, as a key - or the folder itself when it resolves to none. */
function compilerOf(folder, script) {
    for (const origin of [pathToFileURL(path.join(folder, 'package.json')), script]) {
        try { return createRequire(origin).resolve('typescript/package.json'); } catch { /* the next origin */ }
    }
    return `none:${folder}`;
}

/**
 * The listed files as groups, one per compiler, in the order the list first reaches each: `[{ origin, files,
 * folders }]`. `origin` is the folder the compiler is opened from; `folders` maps a file to its package folder,
 * which is where its own tsconfig `paths` are read.
 */
export function groupByCompiler(files, root, script) {
    const seen = new Map();
    const keys = new Map();
    const groups = new Map();
    for (const file of files) {
        const folder = packageFolder(file.abs, root, seen);
        if (!keys.has(folder)) keys.set(folder, compilerOf(folder, script));
        const key = keys.get(folder);
        if (!groups.has(key)) groups.set(key, { origin: folder, files: [], folders: new Map() });
        const group = groups.get(key);
        group.files.push(file);
        group.folders.set(file.rel, folder);
    }
    return [...groups.values()];
}
