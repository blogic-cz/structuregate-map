/**
 * TsPlainRows.mjs - every row ONE file contributes to the plain TypeScript half of the deep map.
 *
 * THE SAME TABLES AS THE PYTHON HALF wherever the meaning is the same - `files`, `functions`, `classes`,
 * `imports`, `calls`, `arguments`, `branches`, `assignments`, `consts`, `exports`, `returns`, `raises`,
 * `string_literals`, `expressions` - so one query reads both languages, told apart by `files.lang`. Three
 * tables are this language's own: `types` (interfaces, aliases, enums), `jsx` (every element a component
 * renders) and `regexes`.
 *
 * ONE FINGERPRINT WITH THE FILE MAP. `functions.body_shape` and `expressions.shape` are the digests
 * TsGate.Map.mjs prints as MAP-BODY and MAP-EXPR, computed by the same function over the same nodes, so a
 * `GROUP BY shape` here returns the groups buildmap.json lists as `duplicate_bodies` and
 * `duplicate_expressions` - with the source, the scope and the reads of every copy beside them.
 *
 * SCOPE IS RECORDED, NOT INFERRED. A row says which class and which NAMED function it sits in. A callback
 * passed inline is not a scope of its own: `items.map((x) => price(x))` is a call inside whatever function
 * wrote it, which is the answer "which calls happen inside `total`" wants.
 *
 * NO REGEX - the build refuses one over this folder.
 */
import { EXPRESSION_KINDS, MIN_BODY_STATEMENTS, MIN_EXPRESSION_LEAVES, alias, candidatesOf, fingerprint,
    isModuleSyntax, resolve, specifierOf, summary } from './TsGate.Map.mjs';
import { bindCall, bindingNames, dotted, facts, isFunctionLike, nameText } from './TsPlainFacts.mjs';
import { catchFacts } from './TsCatch.mjs';
import { literalUse, numberOf } from './TsLiterals.mjs';

/** The tables, filled as the walk goes. Ids CONTINUE from the counters the store handed over: an id given
 *  out twice is a join that silently pulls an unrelated row. */
export class Rows {
    constructor(counters) {
        this.counters = { ...(counters ?? {}) };
        this.tables = {};
    }

    add(table, prefix, row) {
        const n = (Number(this.counters[prefix]) || 0) + 1;
        this.counters[prefix] = n;
        row.id = `${prefix}:${n}`;
        if (!this.tables[table]) this.tables[table] = [];
        this.tables[table].push(row);
        return row.id;
    }
}

/** The module a file is imported as: its base name without the extension. */
function moduleOf(rel) {
    const base = rel.slice(rel.lastIndexOf('/') + 1);
    const dot = base.indexOf('.');
    return dot > 0 ? base.slice(0, dot) : base;
}

function hasModifier(ts, node, kind) {
    return (node.modifiers ?? []).some((m) => m.kind === kind);
}

function isExported(ts, node) {
    try { return (ts.getCombinedModifierFlags(node) & ts.ModifierFlags.Export) !== 0 ? 1 : 0; } catch { return 0; }
}

function docOf(ts, node) {
    try {
        for (const doc of ts.getJSDocCommentsAndTags(node)) {
            if (doc.kind !== ts.SyntaxKind.JSDoc) continue;
            const text = ts.getTextOfJSDocComment(doc.comment);
            if (text) return text;
        }
    } catch { /* a compiler without the helper has no doc to give */ }
    return '';
}

/** A function's own name, or its variable's or property's; '' for a callback nobody named. */
function functionName(ts, node, sourceFile) {
    const K = ts.SyntaxKind;
    const parent = node.parent;
    if ((node.kind === K.ArrowFunction || node.kind === K.FunctionExpression) && parent && parent.initializer === node
        && (parent.kind === K.VariableDeclaration || parent.kind === K.PropertyDeclaration
            || parent.kind === K.PropertyAssignment)) return nameText(ts, parent.name, sourceFile);
    if (node.kind === K.Constructor) return 'constructor';
    if (node.name) return nameText(ts, node.name, sourceFile);
    if (parent && parent.kind === K.ExportAssignment) return 'default';
    return '';
}

/** The name a function row carries when nothing names it. */
const ANONYMOUS = '(anonymous)';

/**
 * What an exported statement declares. Spelled out rather than read back from `ts.SyntaxKind[kind]`: that
 * enum maps several names to one number, so a `VariableStatement` reads back as `FirstStatement`.
 */
function exportKind(ts, statement) {
    const K = ts.SyntaxKind;
    if (statement.kind === K.VariableStatement) {
        const flags = statement.declarationList.flags;
        return (flags & ts.NodeFlags.Const) ? 'const' : (flags & ts.NodeFlags.Let) ? 'let' : 'var';
    }
    const names = [[K.FunctionDeclaration, 'function'], [K.ClassDeclaration, 'class'],
        [K.InterfaceDeclaration, 'interface'], [K.TypeAliasDeclaration, 'type'], [K.EnumDeclaration, 'enum'],
        [K.ModuleDeclaration, 'namespace']];
    const found = names.find(([kind]) => kind === statement.kind);
    return found ? found[1] : 'other';
}

const FUNCTION_KIND = ['FunctionDeclaration', 'function', 'MethodDeclaration', 'method', 'Constructor',
    'constructor', 'GetAccessor', 'get', 'SetAccessor', 'set', 'ArrowFunction', 'arrow', 'FunctionExpression',
    'function-expression'];

const BRANCH_KIND = ['IfStatement', 'if', 'WhileStatement', 'while', 'DoStatement', 'do', 'ForStatement', 'for',
    'ForOfStatement', 'for-of', 'ForInStatement', 'for-in', 'SwitchStatement', 'switch', 'CaseClause', 'case',
    'TryStatement', 'try', 'ConditionalExpression', 'ternary'];

function pairs(ts, flat) {
    const out = new Map();
    for (let i = 0; i < flat.length; i += 2) out.set(ts.SyntaxKind[flat[i]], flat[i + 1]);
    return out;
}

/** What a branch TESTS - the condition, the iterated value, the switched value; nothing for a `try`, whose
 *  caught name is `handlers.name`. */
function tested(ts, node) {
    const K = ts.SyntaxKind;
    if (node.kind === K.ForStatement) return node.condition;
    if (node.kind === K.TryStatement) return null;
    if (node.kind === K.ConditionalExpression) return node.condition;
    return node.expression ?? null;
}

/**
 * The rows of one parsed file. Returns what the rows were BOUND THROUGH - every path an import resolved to,
 * and every candidate an unresolved relative import tried - so the file is read again when one of those
 * appears, moves or goes.
 */
export function readFile(ts, rows, file) {
    const { rel, text, sourceFile, known, root, aliases, sha, errors } = file;
    const K = ts.SyntaxKind;
    const sf = sourceFile;
    const kindOf = { fn: pairs(ts, FUNCTION_KIND), branch: pairs(ts, BRANCH_KIND) };
    const expressionKinds = new Map(EXPRESSION_KINDS.map((name) => [K[name], name]));
    const line = (node) => sf.getLineAndCharacterOfPosition(node.getStart(sf)).line + 1;
    const endLine = (node) => sf.getLineAndCharacterOfPosition(node.getEnd()).line + 1;
    const src = (node) => (node ? node.getText(sf) : '');
    const deps = new Set();
    const cache = new WeakMap();

    const isModule = sf.statements.some((s) => isModuleSyntax(ts, s));
    const fileId = rows.add('files', 'f', {
        path: rel, module: moduleOf(rel), sha, lines: text.split('\n').length,
        dialect: rel.endsWith('.ts') || rel.endsWith('.tsx') || rel.endsWith('.mts') || rel.endsWith('.cts')
            ? 'typescript' : 'javascript',
        entry: isModule ? 0 : 1, doc: summary(ts, sf, text), errors, skipped: '',
    });

    // WHERE A SPECIFIER LEADS, and what that answer depended on.
    const target = (spec) => {
        if (spec.startsWith('.')) {
            const found = resolve(rel, spec, known, root);
            if (found.edge) deps.add(found.edge);
            else for (const candidate of candidatesOf(rel, spec)) deps.add(candidate);
            return found.edge ?? '';
        }
        const aliased = alias(rel, spec, aliases, known, root);
        if (aliased) deps.add(aliased);
        return aliased;
    };

    // THE FILE'S OWN BINDINGS, read before the walk: a call above the declaration it runs still runs it.
    const imported = new Map();
    const local = new Set();
    for (const statement of sf.statements) {
        if (statement.kind === K.FunctionDeclaration || statement.kind === K.ClassDeclaration
            || statement.kind === K.EnumDeclaration) {
            if (statement.name) local.add(statement.name.text);
        }
        if (statement.kind === K.VariableStatement) {
            for (const declaration of statement.declarationList.declarations) {
                for (const name of bindingNames(ts, declaration.name)) local.add(name);
            }
        }
    }

    // What the file reads ANYWHERE, types included - an import's `used`.
    const used = new Set();
    const collectUsed = (n) => {
        if (n.kind === K.ImportDeclaration || n.kind === K.ImportEqualsDeclaration) return;
        if (n.kind === K.Identifier) used.add(n.text);
        n.forEachChild(collectUsed);
    };
    sf.forEachChild(collectUsed);

    const stack = [];
    const where = () => {
        const cls = stack.find((s) => s.kind === 'class');
        const fn = [...stack].reverse().find((s) => s.kind === 'func');
        return { file: fileId, cls: cls ? cls.name : '', func: fn ? fn.name : '' };
    };
    const qual = (name) => [...stack.map((s) => s.name), name].join('.');

    const importRows = (node) => {
        const spec = node.moduleSpecifier.text;
        const from = target(spec);
        const base = { ...where(), line: line(node), module: spec, from_path: from };
        if (node.kind === K.ExportDeclaration) {
            const elements = node.exportClause && node.exportClause.elements ? node.exportClause.elements : null;
            if (!elements) rows.add('imports', 'i', { ...base, name: '*', alias: '', kind: 're-export', type_only: node.isTypeOnly ? 1 : 0, used: 1 });
            for (const el of elements ?? []) {
                rows.add('imports', 'i', { ...base, name: (el.propertyName ?? el.name).text, alias: el.name.text,
                    kind: 're-export', type_only: node.isTypeOnly || el.isTypeOnly ? 1 : 0, used: 1 });
            }
            return;
        }
        const clause = node.importClause;
        if (!clause) { rows.add('imports', 'i', { ...base, name: '', alias: '', kind: 'side-effect', type_only: 0, used: 1 }); return; }
        const typeOnly = clause.isTypeOnly ? 1 : 0;
        const add = (name, localName, kind, only) => {
            imported.set(localName, { path: from, name });
            rows.add('imports', 'i', { ...base, name, alias: localName, kind, type_only: only,
                used: used.has(localName) ? 1 : 0 });
        };
        if (clause.name) add('default', clause.name.text, 'default', typeOnly);
        const bindings = clause.namedBindings;
        if (bindings && bindings.kind === K.NamespaceImport) add('*', bindings.name.text, 'namespace', typeOnly);
        for (const el of bindings && bindings.elements ? bindings.elements : []) {
            add((el.propertyName ?? el.name).text, el.name.text, 'named', typeOnly || (el.isTypeOnly ? 1 : 0));
        }
    };
    for (const statement of sf.statements) {
        if ((statement.kind === K.ImportDeclaration || statement.kind === K.ExportDeclaration)
            && statement.moduleSpecifier && typeof statement.moduleSpecifier.text === 'string') importRows(statement);
    }

    const functionRow = (node, name, anonymous) => {
        const holder = node.parent && node.parent.initializer === node ? node.parent : node;
        const body = node.body && node.body.statements ? node.body : null;
        const shape = body && body.statements.length >= MIN_BODY_STATEMENTS ? fingerprint(ts, body, sf) : null;
        rows.add('functions', 'fn', {
            ...where(), line: line(node), end_line: endLine(node), name, qualname: qual(name),
            kind: kindOf.fn.get(node.kind) ?? 'function',
            args: node.parameters.map((p) => (p.dotDotDotToken ? '...' : '') + src(p.name)),
            returns: src(node.type), is_async: hasModifier(ts, node, K.AsyncKeyword) ? 1 : 0,
            exported: isExported(ts, holder), doc: docOf(ts, holder),
            statements: body ? body.statements.length : 0,
            body_shape: shape ? shape.digest : '', body_size: shape ? shape.span : 0,
        });
        for (const [position, p] of anonymous ? [] : node.parameters.entries()) {
            rows.add('parameters', 'p', {
                ...where(), func: name, qualname: qual(name), line: line(p), position, name: src(p.name),
                kind: p.dotDotDotToken ? 'rest' : 'positional', annotation: src(p.type),
                default_expr: src(p.initializer), reads: facts(ts, p.initializer).reads,
            });
        }
    };

    const classRow = (node, name) => {
        const heritage = (token) => (node.heritageClauses ?? []).filter((h) => h.token === token)
            .flatMap((h) => h.types.map((t) => src(t)));
        rows.add('classes', 'c', {
            ...where(), line: line(node), end_line: endLine(node), name, qualname: qual(name),
            bases: heritage(K.ExtendsKeyword), implements: heritage(K.ImplementsKeyword),
            exported: isExported(ts, node.kind === K.ClassExpression && node.parent ? node.parent : node),
            doc: docOf(ts, node), members: node.members.length,
        });
    };

    const exportRows = (statement) => {
        const at = { ...where(), line: line(statement) };
        if (statement.kind === K.ExportAssignment) { rows.add('exports', 'e', { ...at, name: 'default', local: src(statement.expression), kind: 'default' }); return; }
        if (statement.kind === K.ExportDeclaration && !statement.moduleSpecifier && statement.exportClause
            && statement.exportClause.elements) {
            for (const el of statement.exportClause.elements) {
                rows.add('exports', 'e', { ...at, name: el.name.text, local: (el.propertyName ?? el.name).text, kind: 'list' });
            }
            return;
        }
        if (!hasModifier(ts, statement, K.ExportKeyword)) return;
        const isDefault = hasModifier(ts, statement, K.DefaultKeyword);
        const names = statement.kind === K.VariableStatement
            ? statement.declarationList.declarations.flatMap((d) => bindingNames(ts, d.name))
            : [statement.name ? statement.name.text : 'default'];
        for (const name of names) {
            rows.add('exports', 'e', { ...at, name: isDefault ? 'default' : name, local: name,
                kind: exportKind(ts, statement) });
        }
    };
    for (const statement of sf.statements) exportRows(statement);

    const touched = (node) => { const f = facts(ts, node); return { reads: f.reads, calls: f.calls }; };

    const callRow = (node) => {
        const callee = dotted(ts, node.expression) || src(node.expression);
        const [targetPath, targetName] = bindCall(ts, node, dotted(ts, node.expression), rel, imported, local, cache);
        const call = rows.add('calls', 'call', {
            ...where(), line: line(node), callee, target_path: targetPath, target_name: targetName,
            args: (node.arguments ?? []).length, is_new: node.kind === K.NewExpression ? 1 : 0,
            optional: node.questionDotToken ? 1 : 0, source: src(node),
        });
        // A `RegExp` CONSTRUCTION IS A REGEX ROW too, beside its call row - the global only: a name the file
        // binds (a function of its own, an import) is not it. A literal first argument is its own row already.
        const first = (node.arguments ?? [])[0];
        if (callee === 'RegExp' && targetPath === '' && targetName === ''
            && !(first && first.kind === K.RegularExpressionLiteral)) regexCall(node);
        for (const [position, value] of (node.arguments ?? []).entries()) {
            rows.add('arguments', 'arg', { ...where(), call, line: line(value), position, source: src(value),
                spread: value.kind === K.SpreadElement ? 1 : 0, reads: facts(ts, value).reads });
        }
        const spec = specifierOf(ts, node);
        if (typeof spec === 'string' && spec) {
            rows.add('imports', 'i', { ...where(), line: line(node), module: spec, from_path: target(spec),
                name: '', alias: '', kind: node.expression.kind === K.ImportKeyword ? 'dynamic' : 'require',
                type_only: 0, used: 1 });
        }
    };

    const assignment = (node, targetNode, value, kind) => {
        if (value && (isFunctionLike(ts, value) || value.kind === K.ClassExpression)) return;
        const rhs = touched(value);
        rows.add('assignments', 'a', { ...where(), line: line(node), target: src(targetNode), kind,
            source: src(value), reads: rhs.reads, calls: rhs.calls });
        const statement = node.parent && node.parent.parent;
        if (kind === 'const' && stack.length === 0 && statement && statement.parent === sf
            && targetNode.kind === K.Identifier && value) {
            rows.add('consts', 'k', { ...where(), line: line(node), name: targetNode.text, source: src(value),
                exported: isExported(ts, node), reads: rhs.reads, calls: rhs.calls });
        }
    };

    // WHAT RECEIVES THE REGEX: the method it is passed to, the variable it initialises, or ''.
    const usedBy = (node) => {
        const parent = node.parent;
        // THE METHOD, when the receiver is no name: `String(x).split(/\s+/)` is used by `.split`.
        if (parent && (parent.kind === K.CallExpression || parent.kind === K.NewExpression)) {
            return dotted(ts, parent.expression)
                || (parent.expression.kind === K.PropertyAccessExpression ? '.' + parent.expression.name.text : src(parent.expression));
        }
        if (parent && parent.kind === K.PropertyAccessExpression) return '.' + parent.name.text;
        if (parent && parent.kind === K.VariableDeclaration) return '= ' + src(parent.name);
        return '';
    };

    const regexRow = (node) => {
        const raw = node.text;
        const last = raw.lastIndexOf('/');
        rows.add('regexes', 'rx', { ...where(), line: line(node), kind: 'literal', api: '', pattern: raw.slice(1, last),
            pattern_kind: 'literal', flags: raw.slice(last + 1), used_by: usedBy(node), source: raw });
    };

    // A PATTERN IS KNOWN ONLY FROM A LITERAL: anything built at run time is '' - never a guess.
    const literalText = (arg) => (arg && (arg.kind === K.StringLiteral || arg.kind === K.NoSubstitutionTemplateLiteral)
        ? arg.text : null);

    const regexCall = (node) => {
        const [pattern, flags] = [0, 1].map((i) => literalText((node.arguments ?? [])[i]));
        rows.add('regexes', 'rx', { ...where(), line: line(node), kind: 'call', api: 'RegExp', pattern: pattern ?? '',
            pattern_kind: pattern === null ? '' : 'literal', flags: flags ?? '', used_by: usedBy(node), source: src(node) });
    };

    const isSpecifier = (node) => {
        const parent = node.parent;
        if (!parent) return false;
        if (parent.moduleSpecifier === node) return true;
        // NON-EMPTY: `specifierOf` answers '' for a call that imports nothing, and taking that for a specifier
        // dropped the first string of EVERY call - `split('|')`, `t('key')`, `fetch('/api')` - from the table.
        const specifier = parent.kind === K.CallExpression && parent.arguments[0] === node ? specifierOf(ts, parent) : '';
        return typeof specifier === 'string' && specifier !== '';
    };

    const visit = (node) => {
        const kind = node.kind;
        let pushed = null;

        if (isFunctionLike(ts, node)) {
            // A CALLBACK NOBODY NAMED IS STILL A ROW - the file map fingerprints its body too, and a
            // duplicate group naming a body this table does not hold could not be joined back. It is not a
            // SCOPE: what it holds belongs to the function that wrote it.
            const name = functionName(ts, node, sf);
            functionRow(node, name || ANONYMOUS, !name);
            if (name) pushed = { kind: 'func', name };
        } else if (kind === K.ClassDeclaration || kind === K.ClassExpression) {
            const name = node.name ? node.name.text
                : (node.parent && node.parent.kind === K.VariableDeclaration ? src(node.parent.name) : 'anonymous');
            classRow(node, name);
            pushed = { kind: 'class', name };
        } else if (kind === K.InterfaceDeclaration || kind === K.TypeAliasDeclaration || kind === K.EnumDeclaration) {
            rows.add('types', 't', { ...where(), line: line(node), end_line: endLine(node), name: node.name.text,
                kind: kind === K.InterfaceDeclaration ? 'interface' : kind === K.EnumDeclaration ? 'enum' : 'alias',
                exported: isExported(ts, node),
                members: (node.members ?? []).map((m) => (m.name ? nameText(ts, m.name, sf) : src(m))),
                source: kind === K.TypeAliasDeclaration ? src(node.type) : '' });
        } else if (kind === K.CallExpression || kind === K.NewExpression) {
            callRow(node);
        } else if (kind === K.VariableDeclaration && node.initializer) {
            const flags = node.parent ? node.parent.flags : 0;
            const declared = (flags & ts.NodeFlags.Const) ? 'const' : (flags & ts.NodeFlags.Let) ? 'let' : 'var';
            assignment(node, node.name, node.initializer, declared);
        } else if (kind === K.BinaryExpression && node.operatorToken.kind >= K.FirstAssignment
            && node.operatorToken.kind <= K.LastAssignment) {
            assignment(node, node.left, node.right, node.operatorToken.kind === K.EqualsToken ? 'assign' : 'augmented');
        } else if (kind === K.ReturnStatement) {
            rows.add('returns', 'r', { ...where(), line: line(node), source: src(node.expression), ...touched(node.expression) });
        } else if (kind === K.ThrowStatement) {
            const thrown = node.expression;
            const name = thrown && thrown.kind === K.NewExpression ? dotted(ts, thrown.expression) : dotted(ts, thrown);
            rows.add('raises', 'rs', { ...where(), line: line(node), name, source: src(thrown), ...touched(thrown) });
        } else if ((kind === K.StringLiteral || kind === K.NoSubstitutionTemplateLiteral) && node.text.trim()
            && !isSpecifier(node)) {
            rows.add('string_literals', 's', { ...where(), line: line(node), value: node.text, length: node.text.length,
                ...literalUse(ts, node, src) });
        } else if (kind === K.NumericLiteral) {
            rows.add('number_literals', 'n', { ...where(), line: line(node), ...numberOf(ts, node, src), ...literalUse(ts, node, src) });
        } else if (kind === K.RegularExpressionLiteral) {
            regexRow(node);
        } else if (kind === K.JsxOpeningElement || kind === K.JsxSelfClosingElement) {
            rows.add('jsx', 'jx', { ...where(), line: line(node), tag: src(node.tagName),
                attrs: node.attributes.properties.map((p) => (p.name ? src(p.name) : '...')) });
        }

        if (kindOf.branch.has(kind)) {
            const test = tested(ts, node);
            rows.add('branches', 'br', { ...where(), line: line(node), end_line: endLine(node),
                kind: kindOf.branch.get(kind), test: test ? src(test) : '', ...touched(test) });
            const handler = kind === K.TryStatement ? catchFacts(ts, sf, node) : null;
            if (handler) rows.add('handlers', 'h', { ...where(), ...handler });
        }

        // THE FILE MAP'S RULE, EXACTLY: a node with a body of its own is a BODY, never an expression row -
        // see TsGate.Map.mjs `runMap`. Diverging here would make this table's groups disagree with its.
        const hasBody = node.body && node.body.statements && node.body.statements.length >= MIN_BODY_STATEMENTS;
        if (!hasBody && expressionKinds.has(kind)) {
            const shape = fingerprint(ts, node, sf);
            if (shape.leaves >= MIN_EXPRESSION_LEAVES) {
                rows.add('expressions', 'x', { ...where(), line: line(node), role: expressionKinds.get(kind),
                    source: src(node), size: shape.leaves, span: shape.span, shape: shape.digest, ...touched(node) });
            }
        }

        if (pushed) stack.push(pushed);
        node.forEachChild(visit);
        if (pushed) stack.pop();
    };
    sf.forEachChild(visit);
    return [...deps].filter((dep) => dep !== rel).sort();
}

/** A file too large to walk: its `files` row, saying so, and no other row. */
export function skippedFile(rows, rel, sha, text, reason) {
    rows.add('files', 'f', { path: rel, module: moduleOf(rel), sha, lines: text.split('\n').length,
        dialect: '', entry: 0, doc: '', errors: 0, skipped: reason });
}
