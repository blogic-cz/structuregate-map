/*
    TsGate.Rules.mjs - the twelve TypeScript rules, all of them read off the AST.

    WHAT EARNS A RULE. `tsc --strict` already answers every question about types that are WRITTEN. What it
    cannot answer is the shapes people use to get OUT of the type system, or the runtime bug classes that
    are legal TypeScript: an `any` that turns a whole call chain unchecked, a `!` that asserts what the
    compiler cannot see, a promise nobody awaited, an empty catch. Each rule below is one of those, and each
    fails the build - a finding that does not fail is a finding nobody fixes.
    `// tsgate-ok` on the line (or the line above) waives one, with the reason beside the code.

    ONE PASS, NOT ONE PASS PER RULE. The PowerShell half measured this: FindAll() per rule is O(nodes x
    rules) and took minutes over a few dozen files. Everything here is bucketed by kind in a single walk, and the rules
    read the buckets.
*/

/**
 * EVERY KIND NAME THESE RULES READ. Checked against the compiler before a single file is parsed, because
 * the two compilers do NOT agree on every name - 7.x renamed `EndOfFileToken` to `EndOfFile` - and
 * `SyntaxKind.Missing` is `undefined`, which no node's `kind` ever equals. A rule keyed on a name the
 * compiler dropped would not fail: it would go QUIET, which is the failure this gate exists to prevent.
 */
export const REQUIRED_KINDS = [
    'AnyKeyword', 'ArrowFunction', 'AsExpression', 'AsyncKeyword', 'BinaryExpression', 'CallExpression',
    'CatchClause', 'Constructor', 'EqualsEqualsToken', 'ExclamationEqualsToken', 'ExportKeyword',
    'ExpressionStatement', 'FunctionDeclaration', 'FunctionExpression', 'GetAccessor', 'Identifier',
    'ImportEqualsDeclaration', 'MethodDeclaration', 'ModuleDeclaration', 'NonNullExpression', 'NullKeyword',
    'Parameter', 'PropertyAccessExpression', 'SetAccessor', 'SourceFile', 'TypeAssertionExpression',
    'TypeLiteral', 'TypeReference', 'VariableDeclaration', 'VariableDeclarationList',
];

/** Every node the rules need, bucketed in ONE walk. */
function index(ctx) {
    const { ts, sourceFile } = ctx;
    const kind = ts.SyntaxKind;
    const ix = {
        any: [], assertion: [], nonNull: [], parameter: [], catches: [], binary: [], varList: [],
        callable: [], typeRef: [], typeLiteral: [], modules: [], importEquals: [], calls: [],
        statements: [], asyncNames: new Set(),
    };

    const walk = (node) => {
        switch (node.kind) {
            case kind.AnyKeyword: ix.any.push(node); break;
            case kind.AsExpression: case kind.TypeAssertionExpression: ix.assertion.push(node); break;
            case kind.NonNullExpression: ix.nonNull.push(node); break;
            case kind.Parameter: ix.parameter.push(node); break;
            case kind.CatchClause: ix.catches.push(node); break;
            case kind.BinaryExpression: ix.binary.push(node); break;
            case kind.VariableDeclarationList: ix.varList.push(node); break;
            case kind.TypeReference: ix.typeRef.push(node); break;
            case kind.TypeLiteral: ix.typeLiteral.push(node); break;
            case kind.ModuleDeclaration: ix.modules.push(node); break;
            case kind.ImportEqualsDeclaration: ix.importEquals.push(node); break;
            case kind.CallExpression: ix.calls.push(node); break;
            case kind.ExpressionStatement: ix.statements.push(node); break;
            case kind.FunctionDeclaration: case kind.MethodDeclaration: ix.callable.push(node); break;
            default: break;
        }
        collectAsyncName(ctx, ix, node);
        node.forEachChild(walk);
    };
    sourceFile.forEachChild(walk);
    return ix;
}

/** The names declared `async` IN THIS FILE - what the floating-promise rule resolves a call against. */
function collectAsyncName(ctx, ix, node) {
    const { ts } = ctx;
    const kind = ts.SyntaxKind;
    if (node.kind === kind.FunctionDeclaration || node.kind === kind.MethodDeclaration) {
        if (isAsync(ctx, node) && node.name) ix.asyncNames.add(node.name.getText(ctx.sourceFile));
        return;
    }
    if (node.kind !== kind.VariableDeclaration || !node.initializer || !node.name) return;
    const value = node.initializer;
    if (value.kind !== kind.ArrowFunction && value.kind !== kind.FunctionExpression) return;
    if (isAsync(ctx, value)) ix.asyncNames.add(node.name.getText(ctx.sourceFile));
}

function hasModifier(ctx, node, modifierKind) {
    for (const modifier of node.modifiers ?? []) {
        if (modifier.kind === modifierKind) return true;
    }
    return false;
}

function isAsync(ctx, node) {
    return hasModifier(ctx, node, ctx.ts.SyntaxKind.AsyncKeyword);
}

/** Exported here, or a member of something exported - the boundary other code calls across. */
function isExported(ctx, node) {
    const { ts } = ctx;
    let current = node;
    while (current) {
        if (hasModifier(ctx, current, ts.SyntaxKind.ExportKeyword)) return true;
        if (current.kind === ts.SyntaxKind.SourceFile) return false;
        current = current.parent;
    }
    return false;
}

// ---------------------------------------------------------------------------------------------------
// The rules
// ---------------------------------------------------------------------------------------------------

/**
 * 1. NO `any`. It is not a type, it is the type system switched off: one `any` parameter makes every call
 * through it unchecked, and `tsc --strict` says nothing because the annotation is legal. `unknown` is the
 * honest version - it forces a narrow before use.
 */
function noExplicitAny(ctx, ix) {
    for (const node of ix.any) {
        ctx.add(node, 'the `any` type - the type system switched OFF for everything that flows through it',
            'annotate the real type, or `unknown` and narrow it');
    }
}

/**
 * 2. NO DOUBLE ASSERTION. Banning `any` pushes people to `x as unknown as T`, which is the same hole with
 * two steps: TypeScript allows it precisely BECAUSE it refuses the single cast as unsound.
 */
function noDoubleAssertion(ctx, ix) {
    const { ts } = ctx;
    for (const node of ix.assertion) {
        const inner = node.expression;
        if (!inner) continue;
        if (inner.kind !== ts.SyntaxKind.AsExpression && inner.kind !== ts.SyntaxKind.TypeAssertionExpression) continue;
        ctx.add(node, 'a double assertion (`as unknown as T`) - it casts past the check that refused the single one',
            'validate the value and return the real type, or fix the type it comes from');
    }
}

/**
 * 3. NO `!`. A non-null assertion asserts what the compiler CANNOT see, so it is a claim nothing rechecks
 * when the code around it changes - and it fails as a TypeError at runtime, far from the assertion.
 */
function noNonNullAssertion(ctx, ix) {
    for (const node of ix.nonNull) {
        ctx.add(node, 'a non-null assertion (`!`) - it claims what the compiler cannot see and nothing rechecks it',
            'narrow it (`if (x)`), or make the type honest about being optional');
    }
}

/**
 * 4. NO `@ts-ignore`. It silences the NEXT LINE ENTIRELY, including errors nobody has seen yet, and it
 * stays after the underlying problem is fixed. `@ts-expect-error <reason>` fails once the error is gone.
 */
function noTsIgnore(ctx, comments) {
    for (const range of comments) {
        const text = ctx.text.slice(range.pos, range.end);
        if (!text.includes('@ts-ignore')) continue;
        ctx.report(ctx.line(range.pos), text, 'a `@ts-ignore` - it silences every error on the next line, '
            + 'including the ones written later, and it outlives the problem',
            'use `@ts-expect-error <reason>`, which fails when the error goes away');
    }
}

/**
 * 5. NO IMPLICIT `any` PARAMETER on a declared function. `noImplicitAny` catches this only when it is on;
 * a repo migrating from JavaScript usually has it off, and then a bare parameter is an `any` nobody wrote.
 * A callback parameter is NOT flagged: there the type comes from the call site, which is real inference.
 */
function noImplicitAnyParameter(ctx, ix) {
    const { ts } = ctx;
    const declared = [ts.SyntaxKind.FunctionDeclaration, ts.SyntaxKind.MethodDeclaration,
                      ts.SyntaxKind.Constructor, ts.SyntaxKind.GetAccessor, ts.SyntaxKind.SetAccessor];
    for (const node of ix.parameter) {
        if (node.type || node.initializer) continue;
        if (!node.parent || !declared.includes(node.parent.kind)) continue;
        if (node.name && node.name.kind === ts.SyntaxKind.Identifier
            && node.name.getText(ctx.sourceFile) === 'this') continue;
        ctx.add(node, 'a parameter with NO type - an implicit `any` that `noImplicitAny` reports only when it is on',
            'annotate it');
    }
}

/**
 * 6. NO EMPTY CATCH. The error is gone: no log, no rethrow, no state change - and the code after the try
 * runs as if nothing failed. This is the single hardest bug class to find in production, because the only
 * evidence was the exception that was discarded.
 */
function noEmptyCatch(ctx, ix) {
    for (const node of ix.catches) {
        if (!node.block || node.block.statements.length > 0) continue;
        ctx.add(node, 'an empty catch - the error is discarded and the code after the try runs as if nothing failed',
            'log it, rethrow it, or handle it; `catch { return fallback }` is a handler, `catch {}` is not');
    }
}

/**
 * 7. NO `==`. It coerces: `0 == ''`, `'1' == 1` and `[] == false` are all true, so a comparison passes on a
 * value of the wrong type. `== null` is the exception the whole ecosystem uses for "null or undefined".
 */
function noLooseEquality(ctx, ix) {
    const { ts } = ctx;
    for (const node of ix.binary) {
        const operator = node.operatorToken.kind;
        if (operator !== ts.SyntaxKind.EqualsEqualsToken && operator !== ts.SyntaxKind.ExclamationEqualsToken) continue;
        if (isNullish(ctx, node.left) || isNullish(ctx, node.right)) continue;
        ctx.add(node, 'a coercing comparison (`==`) - `0 == \'\'` and `[] == false` are both true',
            'use `===`; keep `== null` when the point is "null or undefined"');
    }
}

function isNullish(ctx, node) {
    const { ts } = ctx;
    if (node.kind === ts.SyntaxKind.NullKeyword) return true;
    return node.kind === ts.SyntaxKind.Identifier && node.getText(ctx.sourceFile) === 'undefined';
}

/**
 * 8. NO `var`. It is function-scoped and hoisted, so the name exists before its line and leaks out of the
 * block it was written in - which is how a loop variable ends up shared by every closure in the loop.
 */
function noVar(ctx, ix) {
    const { ts } = ctx;
    for (const node of ix.varList) {
        if (node.flags & ts.NodeFlags.BlockScoped) continue;
        ctx.add(node, '`var` - function-scoped and hoisted, so it leaks out of its block and every closure in a loop shares it',
            'use `const`, or `let` when it is reassigned');
    }
}

/**
 * 9. AN EXPORTED FUNCTION DECLARES ITS RETURN TYPE. Inferred at the boundary, the signature changes
 * silently: an edit inside the body becomes a breaking change to every caller, and the error surfaces in
 * THEIR file. Only declarations are checked - not callbacks, where inference is the point.
 */
function explicitBoundaryReturn(ctx, ix) {
    for (const node of ix.callable) {
        if (node.type || !isExported(ctx, node)) continue;
        const name = node.name ? node.name.getText(ctx.sourceFile) : '(anonymous)';
        ctx.add(node.name ?? node, `the exported \`${name}\` has NO return type - `
            + 'an edit inside the body silently changes the signature every caller compiled against',
            'annotate the return type');
    }
}

/**
 * 10. NO `{}`, `Object` OR `Function` AS A TYPE. `{}` accepts everything except null and undefined,
 * `Object` accepts every object shape, and `Function` accepts any callable with any arguments - so all
 * three type-check a call the runtime will reject.
 */
function noWeakType(ctx, ix) {
    for (const node of ix.typeRef) {
        const name = node.typeName.getText(ctx.sourceFile);
        if (name !== 'Object' && name !== 'Function') continue;
        ctx.add(node, `\`${name}\` as a type - it accepts almost anything, including the shape that breaks at runtime`,
            name === 'Function' ? 'declare the signature, e.g. `(id: string) => void`' : 'declare the real shape, or `Record<string, unknown>`');
    }
    for (const node of ix.typeLiteral) {
        if (node.members.length > 0) continue;
        ctx.add(node, '`{}` as a type - it means "anything but null", not "an empty object"',
            'declare the real shape, or `Record<string, never>` for genuinely empty');
    }
}

/**
 * 11. NO `namespace` AND NO `require`. Both predate ES modules and neither participates in them: a
 * namespace merges across files with no import to show it, and `require` is not statically analysable, so
 * a bundler cannot tree-shake it and TypeScript cannot check what it returns.
 */
function noLegacyModule(ctx, ix) {
    const { ts } = ctx;
    for (const node of ix.modules) {
        ctx.add(node, '`namespace`/`module` - it merges across files with no import to show it, outside the ES module graph',
            'use ES modules: `export` what is public, `import` what is needed');
    }
    for (const node of ix.importEquals) {
        ctx.add(node, '`import x = require(...)` - the pre-ES-module form, invisible to the module graph',
            'use `import x from ...`');
    }
    for (const node of ix.calls) {
        if (node.expression.kind !== ts.SyntaxKind.Identifier) continue;
        if (node.expression.getText(ctx.sourceFile) !== 'require') continue;
        ctx.add(node, '`require(...)` in TypeScript - not statically analysable, so its result is unchecked and it cannot be tree-shaken',
            'use `import`, or `await import(...)` when it must be lazy');
    }
}

/**
 * 12. NO FLOATING PROMISE from a call this file can PROVE is async. The statement returns immediately, so
 * the work runs unordered, the error becomes an unhandled rejection (a process exit on Node), and a test
 * that passes did not wait for what it was testing. Only names declared `async` HERE are resolved -
 * without a type checker that is what can be known, and it is the case that bites in practice.
 * `void doWork()` is not flagged: that is the written-down "deliberately not awaited".
 */
function noFloatingPromise(ctx, ix) {
    const { ts } = ctx;
    for (const statement of ix.statements) {
        const call = statement.expression;
        if (!call || call.kind !== ts.SyntaxKind.CallExpression) continue;
        const name = calleeName(ctx, call);
        if (name === null || !ix.asyncNames.has(name)) continue;
        ctx.add(statement, `the async \`${name}\` is called and NOT awaited - the work runs unordered and a `
            + 'rejection becomes an unhandled one',
            'await it, return it, or write `void ' + name + '(...)` with the reason why');
    }
}

function calleeName(ctx, call) {
    const { ts } = ctx;
    const target = call.expression;
    if (target.kind === ts.SyntaxKind.Identifier) return target.getText(ctx.sourceFile);
    if (target.kind === ts.SyntaxKind.PropertyAccessExpression) return target.name.getText(ctx.sourceFile);
    return null;
}

export function runRules(ctx, comments) {
    const ix = index(ctx);
    noExplicitAny(ctx, ix);
    noDoubleAssertion(ctx, ix);
    noNonNullAssertion(ctx, ix);
    noTsIgnore(ctx, comments);
    noImplicitAnyParameter(ctx, ix);
    noEmptyCatch(ctx, ix);
    noLooseEquality(ctx, ix);
    noVar(ctx, ix);
    explicitBoundaryReturn(ctx, ix);
    noWeakType(ctx, ix);
    noLegacyModule(ctx, ix);
    noFloatingPromise(ctx, ix);
}
