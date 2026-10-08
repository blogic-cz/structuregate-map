/*
    TsGate.Map.mjs - the TypeScript/JavaScript half of `structuregate --map` (see rust/fbtcore/src/mapper/protocol.rs).

    THE SAME PARSER AS THE RULES, asked a different question: not "is this shape a bug" but "what does this
    file import, and is this body written somewhere else too". It runs under `--map` and the rules do not
    run at all - a map is not a verdict.

    AN IMPORT HERE NAMES A FILE, not a name. That is the one thing TypeScript gives the map for free and C#
    does not: a relative specifier resolves against the filesystem, so the edge is EXACT and a specifier
    resolving to nothing is a genuinely broken import rather than an unknown package. Resolution is done
    here, where the file list and the disk both are; the caller only joins what it is handed.

    NO REGEX - the build refuses one over this folder. Every decision is a kind, a node member, or a
    filesystem question.
*/

import { createHash } from 'node:crypto';
import { existsSync, readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import path from 'node:path';
import { pathToFileURL } from 'node:url';

/** The suffixes a specifier may be missing, in the order a bundler would try them. */
const SUFFIXES = ['', '.ts', '.tsx', '.mts', '.cts', '.d.ts', '.js', '.mjs', '.cjs', '.jsx',
    '/index.ts', '/index.tsx', '/index.mts', '/index.js', '/index.mjs', '/index.jsx'];

/** A body this short is not a duplication worth reporting - see the same constant in the other two halves. */
export const MIN_BODY_STATEMENTS = 3;

/**
 * An expression smaller than this is shared vocabulary, not a copy. Counted in LEAVES, so a long variable
 * name cannot make a trivial expression look substantial - and a leaf here is a NAME or a LITERAL, never
 * punctuation: `forEachChild` walks nodes, so the braces, colons and commas of an object literal are not
 * among them. That makes a leaf worth about two of the tokens the C# half counts, and the threshold is set
 * against that rather than copied across from it.
 */
export const MIN_EXPRESSION_LEAVES = 12;

const MAX_SUMMARY = 160;

/**
 * The expression kinds a duplicate is worth reporting for. An ALLOW-LIST and not "everything that is not a
 * statement": there is no stable predicate for "is an expression" across both compilers, and a wrong guess
 * here reports a duplication several times over its own fragments. These are the shapes people actually
 * paste.
 */
export const EXPRESSION_KINDS = ['CallExpression', 'NewExpression', 'BinaryExpression', 'ConditionalExpression',
    'ObjectLiteralExpression', 'ArrayLiteralExpression', 'ArrowFunction', 'TemplateExpression',
    'TaggedTemplateExpression'];

/**
 * EVERY KIND THIS HALF IS KEYED ON, checked before a file is read. A kind name the compiler does not have
 * reads back as `undefined`, and no node's `kind` equals that - so the rule keyed on it does not fail, it
 * goes QUIET, and the map reports a clean tree for a question it stopped asking.
 */
export const REQUIRED_MAP_KINDS = ['ImportDeclaration', 'ExportDeclaration', 'ExportAssignment',
    'ImportEqualsDeclaration', 'ExportKeyword', 'ImportKeyword', 'PropertyAccessExpression', 'Identifier',
    'StringLiteral', 'NoSubstitutionTemplateLiteral',
    ...EXPRESSION_KINDS];

/** One protocol record. A field carrying `|` would split into the wrong slot on the other side. */
function emit(...fields) {
    const parts = [];
    for (const field of fields) parts.push(String(field).split('|').join('/').split('\n').join(' '));
    console.log(parts.join('|'));
}

/**
 * ESM-IN-TYPESCRIPT WRITES `./x.js` AND MEANS `./x.ts`. Following only the literal suffix would report
 * every such import as broken in exactly the repos that got their module config right.
 */
function swaps(target) {
    if (target.endsWith('.js')) return [target.slice(0, -3) + '.ts', target.slice(0, -3) + '.tsx'];
    if (target.endsWith('.mjs')) return [target.slice(0, -4) + '.mts'];
    if (target.endsWith('.cjs')) return [target.slice(0, -4) + '.cts'];
    return [];
}

/**
 * Every path a relative specifier may name, in the order they are tried. EXPORTED for the deep map, which
 * records the ones an unresolved import tried: a file added at one of them later changes the answer.
 */
export function candidatesOf(rel, specifier) {
    const joined = path.posix.normalize(path.posix.join(path.posix.dirname(rel), specifier));
    const out = [];
    for (const suffix of SUFFIXES) out.push(joined + suffix);
    for (const swapped of swaps(joined)) out.push(swapped);
    return out;
}

/**
 * A specifier, as the file it names. Three answers, and the difference matters:
 *   a path in the MAPPED set   -> an edge
 *   a path on DISK but not mapped (a .json, a .css, a tree left out of --ext) -> nothing; it is not broken
 *   neither                    -> reported as the path it tried, and the caller calls that broken
 */
export function resolve(rel, specifier, known, root) {
    const joined = path.posix.normalize(path.posix.join(path.posix.dirname(rel), specifier));
    const candidates = candidatesOf(rel, specifier);
    for (const candidate of candidates) {
        if (known.has(candidate)) return { edge: candidate };
    }
    for (const candidate of candidates) {
        if (existsSync(path.join(root, candidate))) return { edge: '' };
    }
    return { edge: '', broken: joined };
}


const BACKSLASH = String.fromCharCode(92);
const NEWLINE = String.fromCharCode(10);

/**
 * A tsconfig, read WITHOUT a JSON parser that trips over comments. Every `tsc --init` writes a jsonc file -
 * `//` lines, block comments, trailing commas - and JSON.parse rejects all three, so a strict parse means
 * no alias is ever resolved in exactly the repos that configured one. Scanned character by character with
 * the string state tracked, because a `//` inside a path string is not a comment.
 */
function readJsonc(text) {
    const out = [];
    let inString = false;
    let escaped = false;
    for (let i = 0; i < text.length; i++) {
        const c = text[i];
        if (inString) {
            out.push(c);
            if (escaped) { escaped = false; continue; }
            if (c === BACKSLASH) { escaped = true; continue; }
            if (c === '"') inString = false;
            continue;
        }
        if (c === '"') { inString = true; out.push(c); continue; }
        if (c === '/' && text[i + 1] === '/') {
            while (i < text.length && text[i] !== NEWLINE) i++;
            out.push(NEWLINE);
            continue;
        }
        if (c === '/' && text[i + 1] === '*') {
            i += 2;
            while (i < text.length && !(text[i] === '*' && text[i + 1] === '/')) i++;
            i++;
            out.push(' ');
            continue;
        }
        out.push(c);
    }
    // A trailing comma before a closer is legal in a tsconfig and not in JSON.
    const cleaned = [];
    for (let i = 0; i < out.length; i++) {
        if (out[i] === ',') {
            let j = i + 1;
            while (j < out.length && out[j].trim() === '') j++;
            if (out[j] === '}' || out[j] === ']') continue;
        }
        cleaned.push(out[i]);
    }
    try { return JSON.parse(cleaned.join('')); } catch { return null; }
}

/** How deep an `extends` chain is followed before it is treated as a loop. */
const MAX_EXTENDS = 5;

/**
 * An `extends` value, as the file it names. TypeScript resolves it two ways and so does this:
 *   `./base.json`      relative to the tsconfig that names it (a missing `.json` is added)
 *   `@tsconfig/node20` as a NODE MODULE, resolved from the tsconfig's own folder - which is the only way a
 *                      shared base config is ever published, and skipping it left every repo using one with
 *                      no aliases at all.
 * The package form is asked of node's own resolver rather than guessed at: a hand-rolled node_modules walk
 * is the same class of approximation this tool refuses everywhere else.
 */
function resolveExtends(specifier, directory) {
    if (specifier.startsWith('.')) {
        const target = path.resolve(directory, specifier);
        return target.endsWith('.json') ? target : target + '.json';
    }
    const require = createRequire(pathToFileURL(path.join(directory, 'package.json')));
    // A package may name the file itself (`pkg/tsconfig.json`) or rely on the default TypeScript appends.
    for (const candidate of [specifier, specifier + '/tsconfig.json']) {
        try { return require.resolve(candidate); } catch { /* try the next form */ }
    }
    return '';
}

/**
 * The first `compilerOptions` in the extends chain that carries a `paths` or a `baseUrl`, and the folder to
 * resolve it against. Depth-first through `extends`, and an ARRAY of them is read in REVERSE: TypeScript
 * lets a later entry override an earlier one, so the last is the one that wins.
 *
 * THE FOLDER IS THE FILE THAT WROTE THE OPTIONS, not the tsconfig the chain started at - which is
 * TypeScript's own rule: a `baseUrl` inside a shared base config is relative to THAT file, so a published
 * one has to write its way back out (`"baseUrl": "../../.."`). Resolving it against the consuming repo
 * instead would silently point every alias at the wrong tree.
 */
function findOptions(file, depth) {
    if (depth > MAX_EXTENDS || !existsSync(file)) return null;
    let parsed;
    try { parsed = readJsonc(readFileSync(file, 'utf8')); } catch { return null; }
    if (!parsed) return null;
    const here = parsed.compilerOptions ?? {};
    if (here.paths || here.baseUrl) return { options: here, base: path.dirname(file) };

    const chain = Array.isArray(parsed.extends) ? [...parsed.extends].reverse()
        : typeof parsed.extends === 'string' ? [parsed.extends] : [];
    for (const specifier of chain) {
        const next = resolveExtends(specifier, path.dirname(file));
        if (!next) continue;
        const found = findOptions(next, depth + 1);
        if (found) return found;
    }
    return null;
}

/**
 * A `references` entry, as the tsconfig it names. TypeScript accepts either a directory or a file, so a
 * bare folder gets `/tsconfig.json` appended exactly as the compiler does.
 */
function resolveReference(target, directory) {
    const full = path.resolve(directory, target);
    return full.endsWith('.json') ? full : path.join(full, 'tsconfig.json');
}

/**
 * `compilerOptions.paths` and `baseUrl`, as something a specifier can be matched against. An aliased import
 * (`~/core/log`, `@app/thing`) is not a package, and reporting it as one hides a real edge - which is what
 * this half did until an aliased tree made it obvious.
 *
 * A GROUP PER CONFIG, not one merged set: each config's `baseUrl` is relative to the file that wrote it, so
 * two configs' patterns cannot share a base. A composite build reaches its siblings through `references`,
 * and the alias a referenced project defines is the one its own files are imported by - so those are
 * collected too, each keeping its own base.
 */
export function loadAliases(root) {
    const groups = [];
    collectAliases(path.join(root, 'tsconfig.json'), groups, new Set(), 0);
    return groups.length > 0 ? groups : null;
}

function collectAliases(file, groups, seen, depth) {
    if (depth > MAX_EXTENDS || seen.has(file) || !existsSync(file)) return;
    seen.add(file);

    const found = findOptions(file, 0);
    if (found !== null) {
        const { options, base } = found;
        const baseUrl = options.baseUrl ? path.resolve(base, options.baseUrl) : base;
        const patterns = [];
        for (const [key, targets] of Object.entries(options.paths ?? {})) {
            const star = key.indexOf('*');
            patterns.push({
                prefix: star === -1 ? key : key.slice(0, star),
                suffix: star === -1 ? '' : key.slice(star + 1),
                exact: star === -1,
                targets: Array.isArray(targets) ? targets : [],
            });
        }
        groups.push({ baseUrl, patterns });
    }

    let parsed;
    try { parsed = readJsonc(readFileSync(file, 'utf8')); } catch { return; }
    if (!parsed || !Array.isArray(parsed.references)) return;
    for (const reference of parsed.references) {
        if (!reference || typeof reference.path !== 'string') continue;
        collectAliases(resolveReference(reference.path, path.dirname(file)), groups, seen, depth + 1);
    }
}

/** The paths an aliased specifier could mean, most specific first, across every config that defines one. */
function aliasTargets(groups, specifier) {
    if (groups === null) return [];
    const out = [];
    for (const group of groups) {
        for (const pattern of group.patterns) {
            let middle = null;
            if (pattern.exact) { if (specifier === pattern.prefix) middle = ''; }
            else if (specifier.startsWith(pattern.prefix) && specifier.endsWith(pattern.suffix)
                && specifier.length >= pattern.prefix.length + pattern.suffix.length) {
                middle = specifier.slice(pattern.prefix.length, specifier.length - pattern.suffix.length);
            }
            if (middle === null) continue;
            for (const target of pattern.targets) {
                const star = target.indexOf('*');
                const filled = star === -1 ? target : target.slice(0, star) + middle + target.slice(star + 1);
                out.push(path.resolve(group.baseUrl, filled));
            }
        }
        // `baseUrl` alone makes a bare specifier resolvable from it, with no `paths` entry at all.
        out.push(path.resolve(group.baseUrl, specifier));
    }
    return out;
}

/**
 * Is this node import or export syntax? A file with NONE is not a module at all — a script a host
 * loads, a classic browser script — and such a file cannot be imported by anything, so reporting that
 * nothing imports it is noise on many files at once. It is loaded rather than imported, which is what an
 * entry point is.
 */
export function isModuleSyntax(ts, node) {
    const kinds = [ts.SyntaxKind.ImportDeclaration, ts.SyntaxKind.ExportDeclaration,
        ts.SyntaxKind.ExportAssignment, ts.SyntaxKind.ImportEqualsDeclaration];
    if (kinds.includes(node.kind)) return true;
    for (const modifier of node.modifiers ?? []) {
        if (modifier.kind === ts.SyntaxKind.ExportKeyword) return true;
    }
    return false;
}

/** The module specifier of any statement that carries one, whatever shape it is written in. */
export function specifierOf(ts, node) {
    if (node.moduleSpecifier && typeof node.moduleSpecifier.text === 'string') return node.moduleSpecifier.text;
    // `import x = require('y')` and a bare `require('y')` / `import('y')` call.
    if (node.kind === ts.SyntaxKind.ImportEqualsDeclaration
        && node.moduleReference && node.moduleReference.expression
        && typeof node.moduleReference.expression.text === 'string') {
        return node.moduleReference.expression.text;
    }
    if (node.kind === ts.SyntaxKind.CallExpression && node.arguments && node.arguments.length > 0) {
        const callee = node.expression;
        const name = callee && callee.escapedText ? String(callee.escapedText) : '';
        const dynamic = callee && callee.kind === ts.SyntaxKind.ImportKeyword;
        if (dynamic || name === 'require') {
            // THE KIND, not just `.text`. An Identifier carries a `.text` of its own, so testing for a
            // string was true for `import(name)` and handed back the VARIABLE's name as if it were a
            // module specifier - an invented package on every dynamic import in the tree.
            const literal = node.arguments[0].kind === ts.SyntaxKind.StringLiteral
                || node.arguments[0].kind === ts.SyntaxKind.NoSubstitutionTemplateLiteral;
            if (literal && typeof node.arguments[0].text === 'string') return node.arguments[0].text;
            // AN `import(expr)` IS STILL AN IMPORT, and this pass cannot say of what. Not guessed at and not
            // dropped either: it is what the dead-file finding has to be qualified by.
            return { computed: (dynamic ? 'import' : 'require') + '() with a specifier built at run time' };
        }
    }
    return '';
}

/** The leaves of a subtree - the tokens, in source order. */
function leaves(node, sourceFile, visit, parent) {
    let children = 0;
    node.forEachChild((child) => { children++; leaves(child, sourceFile, visit, node); });
    if (children === 0) visit(node, parent);
}

/**
 * The shape of a body, with LOCAL NAMES BLANKED so two spellings of one function fingerprint alike, and
 * MEMBER NAMES KEPT because `x.normalize` is what makes that code that code. Comments never enter: this
 * walks leaves, and a comment is trivia.
 */
export function fingerprint(ts, node, sourceFile) {
    const hash = createHash('sha256');
    let count = 0;
    leaves(node, sourceFile, (leaf, parent) => {
        count++;
        const isName = parent !== undefined && parent !== null
            && parent.kind === ts.SyntaxKind.PropertyAccessExpression && parent.name === leaf;
        const identifier = leaf.kind === ts.SyntaxKind.Identifier
            || leaf.kind === ts.SyntaxKind.PrivateIdentifier;
        const text = identifier && !isName ? '_' : leaf.getText(sourceFile);
        hash.update(`${leaf.kind}:${text}`);
    });
    // LEAVES DECIDE WHETHER TO REPORT IT, CHARACTERS SAY HOW BIG IT IS. A leaf count is the right filter -
    // a long name cannot inflate it - but a leaf here is a name or a literal and never punctuation, so it
    // is not comparable with what the other three halves count, and the groups are ranked in ONE list. A
    // source span is the same unit in every language.
    return { digest: hash.digest('hex').slice(0, 16), leaves: count,
             span: node.getEnd() - node.getStart(sourceFile) };
}

/** The first line of the file's leading block comment or doc comment - its headline, if it has one. */
export function summary(ts, sourceFile, text) {
    for (const range of ts.getLeadingCommentRanges(text, 0) ?? []) {
        for (const raw of text.slice(range.pos, range.end).split('\n')) {
            let line = raw.trim();
            for (const lead of ['/**', '/*', '//', '*/', '*']) {
                if (line.startsWith(lead)) { line = line.slice(lead.length).trim(); break; }
            }
            // A ONE-LINE `/** x */` closes on the line it opens, and its `*/` is not part of the headline.
            if (line.endsWith('*/')) line = line.slice(0, -2).trim();
            if (line.length === 0) continue;
            return line.length > MAX_SUMMARY ? line.slice(0, MAX_SUMMARY) + ' ...' : line;
        }
    }
    return '';
}

/**
 * An ALIASED specifier, as the file it names. `~/core/log` and `@app/thing` are not packages, and calling
 * them one hides a real edge — on a tree written with aliases that was every import. Tried only after the
 * relative form, and before the specifier is written off as third-party.
 */
export function alias(rel, specifier, aliases, known, root) {
    for (const target of aliasTargets(aliases, specifier)) {
        const asRel = path.relative(root, target).split(path.sep).join('/');
        if (asRel.startsWith('..')) continue;
        const found = resolve(rel, './' + path.posix.relative(path.posix.dirname(rel), asRel), known, root);
        if (found.edge) return found.edge;
    }
    return '';
}

/**
 * One file's rows. `known` is every relative path in this map, so an import can be told from a package
 * without asking the disk about paths the caller already listed.
 */
export function runMap(ts, rel, text, sourceFile, known, root, aliases) {
    // A TS file is reached by its PATH, so that is what it declares. The caller joins names and paths the
    // same way, and this is what makes "nothing imports this file" answerable for TypeScript at all.
    emit('MAP-DECL', rel, rel);
    const headline = summary(ts, sourceFile, text);
    if (headline) emit('MAP-SUMMARY', rel, headline);

    const expressionKinds = new Set(EXPRESSION_KINDS.map((name) => ts.SyntaxKind[name]));
    let isModule = false;
    const walk = (node) => {
        if (!isModule && isModuleSyntax(ts, node)) isModule = true;
        const line = () => sourceFile.getLineAndCharacterOfPosition(node.getStart(sourceFile)).line + 1;
        const specifier = specifierOf(ts, node);
        if (specifier && specifier.computed) {
            emit('MAP-COMPUTED', rel, line(), specifier.computed);
        } else if (specifier) {
            if (specifier.startsWith('.')) {
                const found = resolve(rel, specifier, known, root);
                if (found.edge) emit('MAP-PATH', rel, found.edge);
                else if (found.broken) emit('MAP-PATH', rel, found.broken);
            } else {
                const aliased = alias(rel, specifier, aliases, known, root);
                // A bare specifier that no alias claims is a package. The caller reports the ones no file in
                // the tree declares, which is the file's real third-party dependency list.
                if (aliased) emit('MAP-PATH', rel, aliased);
                else emit('MAP-USE', rel, specifier);
            }
        }
        if (node.body && node.body.statements && node.body.statements.length >= MIN_BODY_STATEMENTS) {
            const named = node.name && node.name.getText ? node.name.getText(sourceFile) : 'anonymous';
            const shape = fingerprint(ts, node.body, sourceFile);
            emit('MAP-BODY', rel, line(), named, shape.digest, shape.span);
        } else if (expressionKinds.has(node.kind)) {
            // A SECOND UNIT, beside the bodies. A body fingerprint only sees whole functions, so the
            // most-copied thing in a codebase - an idiom pasted INSIDE larger ones - is invisible to it.
            const shape = fingerprint(ts, node, sourceFile);
            if (shape.leaves >= MIN_EXPRESSION_LEAVES) emit('MAP-EXPR', rel, line(), shape.digest, shape.span);
        }
        node.forEachChild(walk);
    };
    sourceFile.forEachChild(walk);
    if (!isModule) emit('MAP-ENTRY', rel);
}
