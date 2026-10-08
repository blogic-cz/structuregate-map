/**
 * TsPlainFacts.mjs - what an expression READS and CALLS, and what a name is bound to, for the plain
 * TypeScript half of the deep map (see TsPlain.mjs).
 *
 * THE FACTS ARE EXTRACTED, THE TEXT IS CARRIED - the rule PyRows.py states for the python half. A row holds
 * its `source` so a reader can see it, and `reads` and `calls` so a query never has to parse that text
 * again. A reader who pattern-matches over `source` has re-implemented, badly, what this file resolved.
 *
 * A BINDING IS NEVER A GUESS BY NAME. A call binds to a file only through this file's own imports and its
 * own top-level declarations, and only when no enclosing function declares the same name. Anything else is
 * left empty: an empty `target_path` is the honest answer, a wrong one is a join that pulls the wrong row.
 *
 * NO REGEX - the build refuses one over this folder. Every decision is a SyntaxKind or a node member.
 */

/** A dotted name - `a`, `this.b`, `a.b.c` - or '' when the chain holds anything but names. */
export function dotted(ts, node) {
    const K = ts.SyntaxKind;
    if (!node) return '';
    if (node.kind === K.Identifier || node.kind === K.PrivateIdentifier) return node.text;
    if (node.kind === K.ThisKeyword) return 'this';
    if (node.kind === K.SuperKeyword) return 'super';
    if (node.kind === K.PropertyAccessExpression) {
        const head = dotted(ts, node.expression);
        return head ? head + '.' + node.name.text : '';
    }
    return '';
}

/** A declaration's name as written: an identifier's text, a literal key's value, else its source. */
export function nameText(ts, name, sourceFile) {
    if (!name) return '';
    const K = ts.SyntaxKind;
    if (name.kind === K.Identifier || name.kind === K.PrivateIdentifier || name.kind === K.StringLiteral
        || name.kind === K.NumericLiteral) return name.text;
    return name.getText(sourceFile);
}

/** Every name a binding introduces - `a`, and each of `{ a, b: [c] }` - in source order. */
export function bindingNames(ts, name, out = []) {
    if (!name) return out;
    if (name.kind === ts.SyntaxKind.Identifier) { out.push(name.text); return out; }
    for (const element of name.elements ?? []) {
        if (element.name) bindingNames(ts, element.name, out);
    }
    return out;
}

/** The declarations whose `name` member is a DEFINITION, never a read of the same text. */
function definingKinds(ts) {
    const K = ts.SyntaxKind;
    return new Set([K.VariableDeclaration, K.Parameter, K.FunctionDeclaration, K.FunctionExpression,
        K.ClassDeclaration, K.ClassExpression, K.MethodDeclaration, K.PropertyDeclaration, K.PropertyAssignment,
        K.PropertySignature, K.MethodSignature, K.GetAccessor, K.SetAccessor, K.EnumMember, K.BindingElement,
        K.ImportSpecifier, K.ImportClause, K.NamespaceImport, K.ExportSpecifier, K.ImportEqualsDeclaration,
        K.InterfaceDeclaration, K.TypeAliasDeclaration, K.EnumDeclaration, K.TypeParameter, K.JsxAttribute,
        K.NamespaceExport, K.ModuleDeclaration]);
}

const DEFINING = new WeakMap();

/**
 * Is this identifier a READ? Not a member name after a dot (the dotted chain carries it), not the name a
 * declaration introduces, not a label, and not an INTRINSIC jsx tag: `<div>` names no binding, `<Price>`
 * reads the component it renders.
 */
function isRead(ts, id) {
    const K = ts.SyntaxKind;
    const parent = id.parent;
    if (!parent) return false;
    let defining = DEFINING.get(ts);
    if (!defining) { defining = definingKinds(ts); DEFINING.set(ts, defining); }
    if (defining.has(parent.kind) && parent.name === id) return false;
    if (parent.kind === K.BindingElement && parent.propertyName === id) return false;
    if (parent.kind === K.ImportSpecifier || parent.kind === K.ExportSpecifier) return false;
    if (parent.kind === K.LabeledStatement || parent.kind === K.BreakStatement
        || parent.kind === K.ContinueStatement) return false;
    if (parent.kind === K.JsxOpeningElement || parent.kind === K.JsxSelfClosingElement
        || parent.kind === K.JsxClosingElement) {
        const first = id.text.charAt(0);
        return first !== first.toLowerCase();
    }
    return true;
}

/**
 * What a sub-tree reads and calls, each once, in source order. A TYPE is not a read - `x as Price` reads
 * `x` and nothing else - so every type node is left unwalked. A callee is both: `STORE.load(k)` reads
 * `STORE.load` and calls it, which is how "who reads STORE" finds the call.
 */
export function facts(ts, node) {
    const K = ts.SyntaxKind;
    const reads = [];
    const calls = [];
    const seenReads = new Set();
    const seenCalls = new Set();
    const read = (name) => { if (name && !seenReads.has(name)) { seenReads.add(name); reads.push(name); } };
    const visit = (n) => {
        if (ts.isTypeNode(n)) return;
        if (n.kind === K.CallExpression || n.kind === K.NewExpression) {
            const callee = dotted(ts, n.expression);
            if (callee && !seenCalls.has(callee)) { seenCalls.add(callee); calls.push(callee); }
        }
        if (n.kind === K.PropertyAccessExpression) {
            const chain = dotted(ts, n);
            if (chain) { read(chain); return; }
        }
        if (n.kind === K.Identifier && isRead(ts, n)) read(n.text);
        if (n.kind === K.ShorthandPropertyAssignment) read(n.name.text);
        n.forEachChild(visit);
    };
    if (node) visit(node);
    return { reads, calls };
}

/** Is this node a function of any spelling? */
export function isFunctionLike(ts, node) {
    const K = ts.SyntaxKind;
    return node.kind === K.FunctionDeclaration || node.kind === K.FunctionExpression
        || node.kind === K.ArrowFunction || node.kind === K.MethodDeclaration || node.kind === K.Constructor
        || node.kind === K.GetAccessor || node.kind === K.SetAccessor;
}

/**
 * Every name a function declares INSIDE itself - its parameters, its variables, its inner function and
 * class names - without crossing into a nested function. Block scope is ignored ON PURPOSE: a `let` in an
 * inner block counts for the whole function, which can only leave a call unbound, never bind it wrongly.
 */
function declaredIn(ts, fn, cache) {
    let names = cache.get(fn);
    if (names) return names;
    names = new Set();
    for (const parameter of fn.parameters ?? []) for (const n of bindingNames(ts, parameter.name)) names.add(n);
    const K = ts.SyntaxKind;
    const visit = (n) => {
        if (n.kind === K.VariableDeclaration) for (const name of bindingNames(ts, n.name)) names.add(name);
        if ((n.kind === K.FunctionDeclaration || n.kind === K.ClassDeclaration) && n.name) {
            names.add(n.name.text);
            return;
        }
        if (isFunctionLike(ts, n) || n.kind === K.ClassExpression) return;
        n.forEachChild(visit);
    };
    if (fn.body) fn.body.forEachChild(visit);
    cache.set(fn, names);
    return names;
}

/** Whether a function around `node` declares `name` itself, so the file-level binding does not apply. */
export function shadowed(ts, node, name, cache) {
    for (let at = node.parent; at; at = at.parent) {
        if (isFunctionLike(ts, at) && declaredIn(ts, at, cache).has(name)) return true;
        if (at.kind === ts.SyntaxKind.CatchClause && at.variableDeclaration
            && bindingNames(ts, at.variableDeclaration.name).includes(name)) return true;
    }
    return false;
}

/**
 * The def a call RUNS, as `[path, name]`, bound through this file's imports and top-level declarations -
 * or `['', '']`. `imported` maps a local name to `{ path, name }` (a namespace import has `name: '*'`);
 * `local` is the set of this file's own top-level declarations.
 */
export function bindCall(ts, call, callee, rel, imported, local, cache) {
    if (!callee) return ['', ''];
    const dot = callee.indexOf('.');
    const head = dot === -1 ? callee : callee.slice(0, dot);
    const rest = dot === -1 ? '' : callee.slice(dot + 1);
    if (head === 'this' || head === 'super' || shadowed(ts, call, head, cache)) return ['', ''];
    const taken = imported.get(head);
    if (taken) {
        if (!taken.path) return ['', ''];
        if (taken.name === '*') return rest ? [taken.path, rest] : ['', ''];
        return [taken.path, rest ? taken.name + '.' + rest : taken.name];
    }
    if (local.has(head)) return [rel, callee];
    return ['', ''];
}
