/*
    TsGate.mjs - the TypeScript half of structuregate (`--ts-discipline`).

    WHY A SCRIPT AND NOT C#: every rule here keys on the TypeScript AST, and the only authoritative parser
    for that is the one THE REPO ITSELF COMPILES WITH. `typescript` is resolved out of the checked tree, so
    the gate reads `satisfies`, `const` type parameters and whatever the next release adds exactly as `tsc`
    does. A grammar approximated in C# would error-recover into a partial answer instead of stopping - the
    same failure the PowerShell half exists to avoid - and a compiler BUNDLED here would be a second
    TypeScript version, silently disagreeing with the one that produces the build.

    TWO COMPILERS, ONE RULE SET. There are now two shapes of `typescript` in the wild and the gate speaks
    both:
      * 5.x - the in-process JS parser. `ts.createSourceFile` returns the tree directly.
      * 7.x - the native port. The JS package no longer parses anything: `.` exports the version only, and
        the AST arrives from the compiler SERVER through `typescript/unstable/sync`, with the kinds and
        trivia helpers in `typescript/unstable/ast`. Nodes carry the same members (`kind`, `forEachChild`,
        `modifiers`, `getText`), so only the ACQUISITION differs - the rules are untouched.
      A 7.x compiler answers per PROJECT (an inferred one when no tsconfig covers the file), and a file it
      reports no source for is named as UNCHECKED rather than quietly passed.

    It is EMBEDDED in structuregate.exe and written to a temp folder on demand, so a consumer still deploys
    two files and cannot end up with a gate whose second half is missing.

    NO REGEX, ANYWHERE - and the build refuses one (see the BanRegex target in StructureGate.csproj). Every
    decision is read off the AST, off a comment RANGE the parser handed back, or off a keyword kind. A
    pattern over source text would match `any` inside `company`, inside a string and inside a comment.

    Contract with the caller (structuregate.exe):
      in   --list-file <path>  UTF-8, one file per line: <relative-path><TAB><absolute-path>
           --root <dir>        the tree being checked; `typescript` is resolved from here
           --map               map instead of check: emit the MAP-* protocol (rust/fbtcore/src/mapper/protocol.rs) and run
                               no rule at all. A map is not a verdict, so the two never mix.
      out  TSGATE-LINES|<rel>|<n>                     source lines, by TOKEN, for every file parsed
           TSGATE|error|<rel>|<line>|<message>         one finding per line
           TSGATE-FATAL|<message>                     this half cannot run at all (no compiler, no host)
           TSGATE-DONE|<files parsed>                  LAST line; its absence means this half failed
      exit 0 even when findings exist - the caller owns the verdict.

    A line carrying `// tsgate-ok` (on it or on the line above) is waived. Every rule here is a bug class
    that survives `tsc` with `strict` on; the waiver is where the case that genuinely needs the shape keeps
    its reason next to the code.
*/

import { createRequire } from 'node:module';
import { existsSync, readFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';
import path from 'node:path';
import { runRules, REQUIRED_KINDS } from './TsGate.Rules.mjs';
import { runMap, loadAliases, REQUIRED_MAP_KINDS } from './TsGate.Map.mjs';
import { groupByCompiler } from './TsGate.Groups.mjs';

const WAIVER = 'tsgate-ok';

/** `.d.ts` is DECLARATIONS: counted, never rule-checked. It is usually generated, and a rule about empty
 *  catch blocks and floating promises has nothing to say about a signature file. */
function isDeclaration(rel) {
    return rel.toLowerCase().endsWith('.d.ts');
}

function args(argv) {
    const parsed = { listFile: '', knownFile: '', root: '.', map: false };
    for (let i = 0; i < argv.length; i++) {
        if (argv[i] === '--list-file') { parsed.listFile = argv[++i] ?? ''; continue; }
        // EVERY FILE OF THE MAP, when --list-file is only the files to parse: the rest are answered from the caller's
        // cache, and an import still resolves to them.
        if (argv[i] === '--known-file') { parsed.knownFile = argv[++i] ?? ''; continue; }
        if (argv[i] === '--root') { parsed.root = argv[++i] ?? '.'; continue; }
        if (argv[i] === '--map') { parsed.map = true; continue; }
    }
    return parsed;
}

/**
 * THE REPO'S OWN COMPILER, or nothing. Resolution starts at the checked tree, so a monorepo package and a
 * pinned version both get the parser that will actually compile them; the fallback is a `typescript`
 * reachable from this script (a NODE_PATH entry, a global install), which is still a real compiler. A MISS
 * IS FATAL, never a skip: a gate that passes because it could not find a parser is reporting on checks it
 * never ran.
 */
function resolveFrom(root, id) {
    const origins = [
        pathToFileURL(path.join(path.resolve(root), 'package.json')),
        import.meta.url,
    ];
    for (const origin of origins) {
        try { return createRequire(origin)(id); } catch { /* try the next origin */ }
    }
    return null;
}

// The SAME answer TsPlain.mjs gives. Parsed as TS, a `.jsx` file fails on its first `<tag>`: every element
// is an UNPARSED error and none of the file's imports become edges.
function scriptKind(ts, rel) {
    const lower = rel.toLowerCase();
    if (lower.endsWith('.tsx')) return ts.ScriptKind.TSX;
    if (lower.endsWith('.jsx')) return ts.ScriptKind.JSX;
    if (lower.endsWith('.mts')) return ts.ScriptKind.MTS;
    if (lower.endsWith('.cts')) return ts.ScriptKind.CTS;
    if (lower.endsWith('.js') || lower.endsWith('.mjs') || lower.endsWith('.cjs')) return ts.ScriptKind.JS;
    return ts.ScriptKind.TS;
}

/** TypeScript 5.x: the parser is in this process, and one call per file is the whole of it. */
function inProcessParser(ts) {
    return {
        syntax: ts,
        label: `typescript ${ts.version} (in-process parser)`,
        begin() { /* nothing to start */ },
        open(rel, abs, text) {
            const sourceFile = ts.createSourceFile(abs, text, ts.ScriptTarget.Latest, true, scriptKind(ts, rel));
            // `parseDiagnostics` is not on the public surface, so it is read defensively rather than
            // assumed: a build that stopped exposing it would turn "this file does not compile" into
            // silence, which is the one failure this gate refuses to have.
            const raw = sourceFile.parseDiagnostics;
            if (!Array.isArray(raw)) {
                return { fatal: `typescript ${ts.version} does not expose sourceFile.parseDiagnostics, so a `
                    + 'file that does not PARSE could not be reported - pin a typescript that does' };
            }
            return {
                sourceFile,
                errors: raw.map((d) => ({
                    position: d.start ?? 0,
                    message: ts.flattenDiagnosticMessageText(d.messageText, ' '),
                })),
            };
        },
        close() { /* nothing to stop */ },
    };
}

/**
 * TypeScript 7.x: the parser is the native compiler, reached over its own protocol. ONE server and ONE
 * snapshot for the whole file set - a snapshot per file would pay for loading the project's libs and
 * dependencies again on every file.
 */
function nativeParser(sync, ast, root) {
    const api = new sync.API({ cwd: path.resolve(root) });
    let snapshot = null;
    return {
        syntax: ast,
        label: 'typescript 7 (native compiler, unstable API)',
        begin(files) { snapshot = api.updateSnapshot({ openFiles: files }); },
        open(rel, abs) {
            const project = snapshot === null ? null : snapshot.getDefaultProjectForFile(abs);
            if (!project) {
                return { missing: 'no tsconfig project covers this file, so the compiler never parsed it - '
                    + 'add it to a tsconfig `include`, or exclude it from the gate with --skip' };
            }
            const program = project.program;
            const sourceFile = program.getSourceFile(abs);
            if (!sourceFile) {
                return { missing: `the project ${project.configFileName} does not contain this file, so it was `
                    + 'never parsed - add it to that tsconfig, or exclude it from the gate with --skip' };
            }
            return {
                sourceFile,
                errors: program.getSyntacticDiagnostics(abs).map((d) => ({ position: d.pos ?? 0, message: d.text })),
            };
        },
        close() { api.close(); },
    };
}

/**
 * THE NATIVE COMPILER'S OWN FILES, checked before it is started. 7.x keeps its executable and its `lib.*.d.ts`
 * in a per-platform package beside `typescript`; with one file of it gone the compiler PANICS on start, and
 * node reports that as a bare `exit 1` whose last lines are node's own stack - the Go panic naming the missing
 * file never reached the caller. Measured: a cache that had lost `lib.d.ts` alone failed every TypeScript case
 * in this repo's suite with "node did not finish" and nothing else.
 */
function nativeCompilerProblem(root) {
    const id = `@typescript/typescript-${process.platform}-${process.arch}`;
    for (const origin of [pathToFileURL(path.join(path.resolve(root), 'package.json')), import.meta.url]) {
        let manifest;
        try { manifest = createRequire(origin).resolve('typescript/package.json'); } catch { continue; }
        let platform;
        try { platform = createRequire(manifest).resolve(`${id}/package.json`); } catch {
            return `the native compiler package ${id} is not installed beside ${path.dirname(manifest)} - `
                + 'reinstall typescript so its platform package comes with it';
        }
        const lib = path.join(path.dirname(platform), 'lib', 'lib.d.ts');
        if (!existsSync(lib)) {
            return `the native compiler package ${id} is damaged: ${lib} does not exist, and the compiler `
                + 'cannot start without it - delete node_modules/@typescript and reinstall typescript';
        }
        return null;
    }
    return null;
}

/** Whichever of the two the checked tree has. The in-process parser wins when it exists: it is one call. */
function openParser(root) {
    const ts = resolveFrom(root, 'typescript');
    if (ts !== null && typeof ts.createSourceFile === 'function') return inProcessParser(ts);

    const sync = resolveFrom(root, 'typescript/unstable/sync');
    const ast = resolveFrom(root, 'typescript/unstable/ast');
    if (sync !== null && ast !== null) {
        const problem = nativeCompilerProblem(root);
        if (problem !== null) return { fatal: problem };
        return nativeParser(sync, ast, root);
    }

    if (ts === null) {
        return { fatal: `typescript could not be resolved from ${root} - install it in the repo being checked `
            + '(`npm i -D typescript`), or drop TypeScript from this run (--ts-discipline for the rules, '
            + '--ext for the map). Both key on the compiler THIS repo builds with; there is no second-best '
            + 'parser to fall back on' };
    }
    return { fatal: `the typescript resolved from ${root} has neither the in-process parser `
        + '(`createSourceFile`) nor the native one (`typescript/unstable/sync`), so nothing here can read a '
        + 'TypeScript file - check the install' };
}

/**
 * Source lines, BY TOKEN - exactly the C# rule ("a line a token sits on"), read off the same parse. Leaf
 * nodes are the tokens; comments and blank lines produce none, so a measured-reason header costs nothing
 * and nobody is pushed to delete the comments the repo exists to keep. A token that SPANS lines (a
 * template literal, a multi-line JSX text) counts every line it covers, because that is code.
 *
 * Counted off the TREE and not with a bare scanner: a scanner has no parser context, so it cannot tell `/`
 * (divide) from `/` (the start of a regex literal), and one wrong guess moves every token after it.
 */
function countTokenLines(syntax, sourceFile) {
    const lines = new Set();
    const walk = (node) => {
        let children = 0;
        node.forEachChild((child) => { children++; walk(child); });
        if (children > 0) return;
        // The end-of-file token sits one line PAST the last real one when the file ends with a newline, so
        // counting it added a phantom source line to every file that ends the way every file should. The
        // two compilers spell it differently: 5.x `EndOfFileToken`, 7.x `EndOfFile`.
        if (node.kind === endOfFile(syntax)) return;
        const start = sourceFile.getLineAndCharacterOfPosition(node.getStart(sourceFile)).line;
        const end = sourceFile.getLineAndCharacterOfPosition(node.getEnd()).line;
        for (let line = start; line <= end; line++) lines.add(line);
    };
    sourceFile.forEachChild(walk);
    return lines.size;
}

/** The end-of-file token's kind, under whichever name this compiler gives it. */
function endOfFile(syntax) {
    return syntax.SyntaxKind.EndOfFileToken ?? syntax.SyntaxKind.EndOfFile;
}

/**
 * THE RULES' VOCABULARY, CHECKED BEFORE ANYTHING IS PARSED. A kind name the compiler does not have reads
 * back as `undefined`, and no node's `kind` equals that - so the rule keyed on it does not fail, it goes
 * QUIET, and the gate reports OK on a check it stopped making. Verified once per run, and fatal.
 */
function missingKinds(syntax, map) {
    const wanted = map ? REQUIRED_MAP_KINDS : REQUIRED_KINDS;
    const missing = wanted.filter((name) => syntax.SyntaxKind[name] === undefined);
    if (endOfFile(syntax) === undefined) missing.push('EndOfFileToken/EndOfFile');
    return missing;
}

/**
 * Every comment in the file, from the PARSER's own trivia ranges. A trailing `// ...` is the leading
 * trivia of the next token, so walking token full-starts sees both placements; the Map drops the
 * duplicates that come from nested nodes sharing a start.
 */
function comments(syntax, sourceFile, text) {
    const found = new Map();
    const collect = (node) => {
        for (const range of syntax.getLeadingCommentRanges(text, node.pos) ?? []) found.set(range.pos, range);
        node.forEachChild(collect);
    };
    collect(sourceFile);
    for (const range of syntax.getLeadingCommentRanges(text, sourceFile.end) ?? []) found.set(range.pos, range);
    return [...found.values()];
}

/**
 * A file's findings, waivers applied. EVERY FINDING IS AN ERROR - the PowerShell half learned that a
 * severity below "fails the build" is a line that scrolls past in a log ending in OK. The severity field
 * stays in the protocol so the caller's parse is the same for both halves.
 */
function context(syntax, rel, text, sourceFile) {
    const lines = text.split('\n');
    const out = [];
    const waived = (line) => {
        for (const candidate of [line - 1, line]) {
            if (candidate >= 1 && candidate <= lines.length && lines[candidate - 1].includes(WAIVER)) return true;
        }
        return false;
    };
    const at = (position) => sourceFile.getLineAndCharacterOfPosition(position).line + 1;
    return {
        // The KINDS namespace, not the whole compiler: `SyntaxKind`, `NodeFlags` and the trivia helpers.
        // On 5.x that is `typescript` itself; on 7.x it is `typescript/unstable/ast`.
        ts: syntax,
        rel, text, sourceFile, findings: out,
        line: at,
        /** A finding on a NODE: the line it starts on, and the first line of it quoted back. */
        add(node, what, remedy) {
            this.report(at(node.getStart(sourceFile)), node.getText(sourceFile), what, remedy);
        },
        /** A finding on a line the tree does not own a node for - a comment range. */
        report(line, snippetSource, what, remedy) {
            if (waived(line)) return;
            let snippet = snippetSource.split('\n')[0].trim();
            if (snippet.length > 60) snippet = snippet.slice(0, 60) + ' ...';
            out.push(`TSGATE|error|${rel}|${line}|${what} - ${remedy} (\`${snippet}\`)`);
        },
    };
}

function rows(listFile) {
    const out = [];
    for (const raw of readFileSync(listFile, 'utf8').split('\n')) {
        if (!raw.trim()) continue;
        const parts = raw.split('\t');
        if (parts.length < 2) continue;
        out.push({ rel: parts[0].trim(), abs: parts[1].trim() });
    }
    return out;
}

function main() {
    const options = args(process.argv.slice(2));
    // The two modes speak two protocols, and a FATAL has to arrive in the one the caller is reading -
    // a fatal printed under the wrong prefix reaches the caller as silence.
    const fatal = (message) => console.log(`${options.map ? 'MAP' : 'TSGATE'}-FATAL|${message}`);
    if (!options.listFile) {
        fatal('--list-file was not passed');
        return;
    }

    const files = rows(options.listFile);
    const known = new Set((options.knownFile ? rows(options.knownFile) : files).map((file) => file.rel));
    const root = path.resolve(options.root);
    // ONE PARSER PER COMPILER, opened from the package folder that resolves it (`TsGate.Groups.mjs`): a tree whose
    // TypeScript is installed in `sub/` and gated from its parent read none of it.
    const groups = groupByCompiler(files, root, import.meta.url);
    const opened = groups.map((group) => ({ group, parser: openParser(group.origin) }));
    // NO COMPILER ANYWHERE is the one fatal it always was; a folder without one, beside folders with one, names
    // its own files - a hole in the verdict, never a skip.
    if (opened.length > 0 && opened.every(({ parser }) => parser.fatal)) {
        fatal(opened[0].parser.fatal);
        return;
    }
    // EACH MODE HAS ITS OWN VOCABULARY, and each is verified before a file is read. A kind the rules key on is not
    // read in map mode and vice versa, so checking the wrong list would either block the map for a reason that does
    // not apply to it or let the map go quiet on a kind it does need. Every parser, before any file.
    const ready = opened.filter(({ parser }) => !parser.fatal);
    for (const { parser } of ready) {
        const missing = missingKinds(parser.syntax, options.map);
        if (missing.length === 0) continue;
        fatal(`${parser.label} has no SyntaxKind for ${missing.join(', ')}, so what is keyed on them `
            + 'would never fire - a check that goes quiet is worse than one that never existed. '
            + 'Pin a typescript this gate knows, or update TsGate.Rules.mjs for this compiler');
        for (const other of ready) other.parser.close();
        return;
    }
    const findings = [];
    let parsed = 0;
    for (const { group, parser } of opened) {
        if (parser.fatal) {
            for (const { rel } of group.files) {
                if (options.map) console.log(`MAP-UNMAPPED|${rel}|${parser.fatal}`);
                else findings.push(`TSGATE|error|${rel}|1|${parser.fatal}`);
            }
            continue;
        }
        const read = readGroup(parser, group, { options, known, root, findings, fatal });
        if (read === null) return;
        parsed += read;
    }

    for (const finding of findings) console.log(finding);
    console.log(`${options.map ? 'MAP' : 'TSGATE'}-DONE|${parsed}`);
}

/** One compiler's files, read and judged - how many parsed, or null after a fatal. */
function readGroup(parser, group, { options, known, root, findings, fatal }) {
    // Read ONCE per package folder, not per file: it is the same tsconfig for every file under it, and re-reading
    // it hundreds of times is the cost that makes a map not worth running. A folder's own `paths`, else the root's.
    const aliases = new Map();
    const aliasesOf = (rel) => {
        const folder = group.folders.get(rel) ?? root;
        if (!aliases.has(folder)) aliases.set(folder, loadAliases(folder) ?? loadAliases(root));
        return aliases.get(folder);
    };
    let parsed = 0;
    try {
        parser.begin(group.files.map((file) => file.abs));
        for (const { rel, abs } of group.files) {
            let text;
            try {
                text = readFileSync(abs, 'utf8');
            } catch (error) {
                if (options.map) console.log(`MAP-ERROR|${rel}|1|cannot be read - ${error.message}`);
                else findings.push(`TSGATE|error|${rel}|1|cannot be read - ${error.message}`);
                continue;
            }

            const opened = parser.open(rel, abs, text);
            if (opened.fatal) {
                fatal(opened.fatal);
                return null;
            }
            // NOT a skip. A file the compiler never parsed is a hole in the verdict, and the reason belongs
            // next to the path it is about. In a MAP that is a file with no edges, which looks exactly like
            // a file that imports nothing - so it is named as unmapped rather than left to be misread.
            if (opened.missing) {
                if (options.map) console.log(`MAP-UNMAPPED|${rel}|${opened.missing}`);
                else findings.push(`TSGATE|error|${rel}|1|${opened.missing}`);
                continue;
            }

            const { sourceFile, errors } = opened;
            parsed++;
            const lines = countTokenLines(parser.syntax, sourceFile);
            console.log(options.map ? `MAP-LINES|${rel}|${lines}` : `TSGATE-LINES|${rel}|${lines}`);

            // A PARSE ERROR IS A VIOLATION, never a skip. The parser error-recovers and hands back a
            // PARTIAL tree, so every rule below would quietly stop covering the rest of the file - and
            // every edge the map reads out of it would be from that partial tree.
            if (errors.length > 0) {
                for (const error of errors.slice(0, 3)) {
                    const at = sourceFile.getLineAndCharacterOfPosition(error.position).line + 1;
                    if (options.map) {
                        console.log(`MAP-ERROR|${rel}|${at}|does not parse as TypeScript: ${error.message}`);
                    } else {
                        findings.push(`TSGATE|error|${rel}|${at}|does not parse as TypeScript: ${error.message}`
                            + ' - fix the syntax; every rule below reads a PARTIAL tree until it is fixed');
                    }
                }
                continue;
            }

            if (options.map) {
                runMap(parser.syntax, rel, text, sourceFile, known, root, options.map ? aliasesOf(rel) : null);
                continue;
            }
            if (isDeclaration(rel)) continue;

            const ctx = context(parser.syntax, rel, text, sourceFile);
            runRules(ctx, comments(parser.syntax, sourceFile, text));
            findings.push(...ctx.findings);
        }
    } finally {
        parser.close();
    }
    return parsed;
}

main();
