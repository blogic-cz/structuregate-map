/**
 * TsPlain.mjs - the PLAIN TypeScript half of `structuregate --map-sqlite`: every `.ts .tsx .js .mjs` file of
 * a tree that is NOT an Angular workspace, as rows.
 *
 * WHY A SECOND TYPESCRIPT HALF. The deep TypeScript map (TsMap.mjs) is an Angular map: it needs
 * `angular.json` and `@angular/compiler`, and everywhere else it stops with MAP-SKIP. A React, Vue, node or
 * plain-script tree then had no expression rows at all - only the file graph, which carries no code - and
 * the question "is this body written somewhere else, and where is the helper it should have called" had
 * nothing to be asked of. This half asks the PARSER only: no program, no type checker, no framework.
 *
 * SHAPED LIKE THE PYTHON HALF, NOT THE ANGULAR ONE. Its rows are SYNTACTIC - a call binds through the
 * file's own imports, never through a checker - so a file whose bytes did not move keeps its rows, and the
 * store drops and rewrites by FILE (PyRows.py's contract). What a row names in ANOTHER file is recorded as
 * `deps`, so an importer is read again when the file it imports appears, moves or goes.
 *
 * THE TREE'S OWN COMPILER, OR NOTHING. `typescript` is borrowed from the tree (then --ts-node-modules), as
 * every other TypeScript half here does. A tree with none installed is a NOTE and not an error: a `.mjs`
 * build script in a C# repository is not a TypeScript project, and the deep map must not fail there. Its
 * rows are then DROPPED rather than kept - rows describing files nobody re-read look real.
 *
 * NODE PARSES, IT DOES NOT WRITE. The database is written by `rust/fbtcore`, linked into the exe.
 *
 * Contract with the caller (rust/fbtcore/src/mapper/deep/):
 *   in   --list-file <path>   UTF-8, one file per line: <relative-path><TAB><absolute-path>
 *        --root <dir>         the tree being mapped; `typescript` and tsconfig aliases resolve from here
 *        --state <path>       what the database already holds, as the store published it (JSON)
 *        --rows <path>        where the payload goes (JSON)
 *        --node-modules <dir> repeatable: where else `typescript` may be borrowed from
 *        --reset              the retry: drop every file this half records and write them again
 *   out  MAP-ERROR|rel|line|message   a file that does not parse - its rows come from a PARTIAL tree
 *        MAP-NOTE|message             the half did not run, and why
 *        MAP-FATAL|message            the state could not be read or the payload not written
 *        MAP-READ|<parsed>|<files>
 *        MAP-DONE|<files>             LAST line; its absence means this half died half way
 *
 * NO REGEX - the build refuses one over this folder.
 */
import { createHash } from 'node:crypto';
import { createRequire } from 'node:module';
import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { loadAliases } from './TsGate.Map.mjs';
import { resolveToolchain } from './TsResolve.mjs';
import { Rows, readFile, skippedFile } from './TsPlainRows.mjs';

/** What the `files` rows of this half are stamped with. NOT `typescript`: that is the Angular half's, and
 *  the store scopes "recorded", "gone" and "disagrees" by it - two halves sharing one would each read the
 *  other's files as their own. */
const LANG = 'ts';

/** WHAT A ROW MEANS, folded into every file's sha with the compiler's version. Bump it with every change to
 *  what TsPlainRows.mjs writes, or files that did not move keep the old rows - PyRows' ROWS_VERSION. */
const ROWS_VERSION = '5';

/** A file this large is a bundle or a generated table, not a source anybody edits; it gets its `files`
 *  row, saying so, and no other. One minified vendor file would otherwise outweigh the tree. */
const MAX_CHARS = 1_000_000;

const NEWLINE = String.fromCharCode(10);

function emit(...fields) {
    const parts = [];
    for (const field of fields) {
        parts.push(String(field).split('|').join('/').split(NEWLINE).join(' ').split('\r').join(' '));
    }
    console.log(parts.join('|'));
}

function parseArgs(argv) {
    const out = { listFile: '', root: '.', state: '', rows: '', hashes: '', nodeModules: [], reset: false };
    for (let i = 0; i < argv.length; i++) {
        const a = argv[i];
        if (a === '--list-file') out.listFile = argv[++i] ?? '';
        else if (a === '--root') out.root = argv[++i] ?? '.';
        else if (a === '--state') out.state = argv[++i] ?? '';
        else if (a === '--rows') out.rows = argv[++i] ?? '';
        else if (a === '--hashes') out.hashes = argv[++i] ?? '';
        else if (a === '--node-modules') out.nodeModules.push(argv[++i] ?? '');
        else if (a === '--reset') out.reset = true;
    }
    return out;
}

function listOf(file) {
    const out = [];
    for (const raw of readFileSync(file, 'utf8').split(NEWLINE)) {
        const tab = raw.indexOf('\t');
        if (tab <= 0) continue;
        out.push([raw.slice(0, tab).trim(), raw.slice(tab + 1).trim()]);
    }
    return out;
}

/** The `node_modules` beside every package folder the listed files sit in, under `root`: a tree whose TypeScript is
 *  installed in `sub/` and mapped from its parent found no compiler at the root or above it. Tried after them,
 *  so a compiler at the root still wins. */
function packageModules(list, root) {
    const out = new Set();
    const seen = new Set();
    for (const [, abs] of list) {
        let dir = path.dirname(path.resolve(abs));
        while (dir.startsWith(root) && dir !== root && !seen.has(dir)) {
            seen.add(dir);
            if (existsSync(path.join(dir, 'package.json'))) {
                out.add(path.join(dir, 'node_modules'));
                break;
            }
            dir = path.dirname(dir);
        }
    }
    return [...out];
}

function scriptKind(ts, rel) {
    const lower = rel.toLowerCase();
    const K = ts.ScriptKind;
    if (lower.endsWith('.tsx')) return K.TSX;
    if (lower.endsWith('.jsx')) return K.JSX;
    if (lower.endsWith('.js') || lower.endsWith('.mjs') || lower.endsWith('.cjs')) return K.JS;
    return K.TS;
}

/** The compiler, or the reason there is none. Only the in-process parser of 5.x will do: this half walks
 *  nodes with the helpers of the `typescript` module itself, which 7.x's native package does not export. */
function compiler(root, declared) {
    const found = resolveToolchain(root, declared, [['typescript']]);
    if (!found.nodeModules) {
        return { why: `no typescript to borrow (tried ${found.tried.join(', ')}) - install it in the tree, or `
            + 'pass --ts-node-modules <dir>' };
    }
    let ts;
    try { ts = createRequire(path.join(found.nodeModules, 'noop.js'))('typescript'); } catch (error) {
        return { why: `typescript in ${found.nodeModules} could not be loaded - ${error.message}` };
    }
    if (typeof ts.createSourceFile !== 'function') {
        return { why: `typescript ${ts.version} in ${found.nodeModules} has no in-process parser (the 7.x native `
            + 'compiler); this half reads the 5.x one' };
    }
    return { ts };
}

/** The files whose rows were bound through a file that changed, appeared or went away. */
function rebound(deps, moved) {
    const out = new Set();
    if (moved.size === 0) return out;
    for (const [rel, through] of Object.entries(deps)) {
        if (Array.isArray(through) && through.some((dep) => moved.has(dep))) out.add(rel);
    }
    return out;
}

function recordedDeps(state) {
    try {
        const value = JSON.parse(state.deps || '{}');
        return value && typeof value === 'object' && !Array.isArray(value) ? value : {};
    } catch { return {}; }
}

function sorted(object) {
    const out = {};
    for (const key of Object.keys(object).sort()) out[key] = object[key];
    return out;
}

function write(file, payload) {
    try {
        writeFileSync(file, JSON.stringify(payload), 'utf8');
        return true;
    } catch (error) {
        emit('MAP-FATAL', `the payload could not be written - ${error.message}`);
        return false;
    }
}

function main() {
    const args = parseArgs(process.argv.slice(2));
    let state;
    try { state = JSON.parse(readFileSync(args.state, 'utf8')); } catch (error) {
        emit('MAP-FATAL', `the database state could not be read - ${error.message}`);
        return;
    }
    const list = listOf(args.listFile);
    const root = path.resolve(args.root);

    const found = compiler(root, [...args.nodeModules.filter((d) => d), ...packageModules(list, root)]);
    if (!found.ts) {
        emit('MAP-NOTE', `the plain typescript half did not run: ${found.why}`);
        // NOTHING RECORDED IS KEPT: every file of this half is gone from the payload's point of view.
        const empty = { all: true, first: true, final: true, reset: false, shas: {}, read: [], counters: {},
            tables: {}, lang: LANG };
        if (write(args.rows, empty)) emit('MAP-DONE', list.length);
        return;
    }
    const ts = found.ts;

    // HASH FIRST, DECIDE SECOND - the compiler's version is in the hash, because a different parser is a
    // different map over the same bytes. A file the tree map already hashed (`--hashes`) is not READ unless
    // it is parsed: its sha is that hash under this half's version.
    const handed = new Map(args.hashes ? Object.entries(JSON.parse(readFileSync(args.hashes, 'utf8'))) : []);
    const digest = (content) => createHash('sha256').update(`${ROWS_VERSION}|${ts.version}${NEWLINE}${content}`).digest('hex').slice(0, 16);
    const files = [];
    const shas = {};
    for (const [rel, abs] of list) {
        if (handed.has(rel)) {
            files.push([rel, abs, null]);
            shas[rel] = digest(`tree ${handed.get(rel)}`);
            continue;
        }
        let text;
        try { text = readFileSync(abs, 'utf8'); } catch (error) {
            emit('MAP-ERROR', rel, 1, `cannot be read - ${error.message}`);
            continue;
        }
        files.push([rel, abs, text]);
        shas[rel] = digest(text);
    }

    const full = Boolean(state.rebuild);
    const before = full || args.reset ? {} : (state.shas ?? {});
    const stale = new Set(Object.keys(shas).filter((rel) => before[rel] !== shas[rel]));
    const moved = new Set([...stale, ...Object.keys(before).filter((rel) => !(rel in shas))]);
    const deps = {};
    if (!full && !args.reset) {
        for (const [rel, through] of Object.entries(recordedDeps(state))) if (rel in shas) deps[rel] = through;
    }
    for (const rel of rebound(deps, moved)) stale.add(rel);

    // IDS CONTINUE unless the database is being rebuilt: a reset drops only THIS half's rows, and the other
    // halves share several prefixes (`f`, `c`, `fn`) - restarting would hand out an id they already hold.
    const rows = new Rows(full ? {} : (state.counters ?? {}));
    const known = new Set(Object.keys(shas));
    const aliases = loadAliases(root);
    const read = [];
    for (const [rel, abs, given] of files) {
        if (!(full || args.reset || stale.has(rel))) continue;
        let text = given;
        if (text === null) {
            try { text = readFileSync(abs, 'utf8'); } catch (error) {
                emit('MAP-ERROR', rel, 1, `cannot be read - ${error.message}`);
                delete shas[rel];
                continue;
            }
        }
        delete deps[rel];
        read.push([rel, abs]);
        if (text.length > MAX_CHARS) {
            skippedFile(rows, rel, shas[rel], text, `larger than ${MAX_CHARS} characters - a bundle, not a source`);
            continue;
        }
        const sourceFile = ts.createSourceFile(abs, text, ts.ScriptTarget.Latest, true, scriptKind(ts, rel));
        const errors = Array.isArray(sourceFile.parseDiagnostics) ? sourceFile.parseDiagnostics : [];
        // A PARSE ERROR IS REPORTED, and the rows still land: the parser error-recovers, so a file with no
        // rows would look exactly like an empty one. The count rides on the `files` row.
        if (errors.length > 0) {
            const at = sourceFile.getLineAndCharacterOfPosition(errors[0].start ?? 0).line + 1;
            emit('MAP-ERROR', rel, at, `does not parse as TypeScript: ${ts.flattenDiagnosticMessageText(errors[0].messageText, ' ')}`);
        }
        const through = readFile(ts, rows, { rel, text, sourceFile, known, root, aliases, sha: shas[rel],
            errors: errors.length });
        if (through.length > 0) deps[rel] = through;
    }

    const payload = { all: true, first: true, final: true, reset: args.reset, shas, read,
        counters: rows.counters, tables: rows.tables, lang: LANG, deps: JSON.stringify(sorted(deps)) };
    if (!write(args.rows, payload)) return;
    emit('MAP-READ', read.length, Object.keys(shas).length);
    emit('MAP-DONE', Object.keys(shas).length);
}

main();
