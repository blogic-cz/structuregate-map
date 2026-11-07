/**
 * THE QUESTIONS EVERY COLLECTOR ASKS OF A NODE - in one place, because several of them were answered
 * differently by several callers in the tool this is ported from before they were pulled together.
 *
 * Where it is, which symbol it is, what documentation it carries, and what it SITS IN. None of them is
 * decided from source text: a comment that mentions a property, a string containing `{`, an identifier
 * inside a template literal all look like the real thing to a pattern and like nothing at all to a tree.
 *
 * WHAT A PROPERTY NAME SAYS is deliberately NOT here. A computed key is its VALUE, and only the value
 * evaluator can work that out - see `TsDecls/TsValue.mjs`.
 */

/**
 * Where a declaration IS - and `line` points at its NAME, not at `node.getStart()`.
 *
 * In the TypeScript AST a node starts at its first DECORATOR, so `getStart()` on a `@Component`-annotated
 * class lands on `@Component({` and the class name is several lines below. Measured in the tool this is
 * ported from: nearly every class row and a large share of member rows anchored to a decorator instead of
 * the thing they name, which breaks every consumer that opens `file:line` expecting to see the
 * declaration. `decorator_line` keeps the node's real start, so nothing is lost.
 */
export function locationOf(node) {
  const sf = node.getSourceFile();
  const start = sf.getLineAndCharacterOfPosition(node.getStart());
  const end = sf.getLineAndCharacterOfPosition(node.getEnd());
  // AN EXPRESSION IS ANCHORED AT ITS START, never at a name inside it: a property access (`o.name`) has a `name` and an
  // `expression`, and a concise arrow's implicit return anchored at the LAST name of its chain (a wrong column, or a
  // bogus `decorator_line` for a chain over several lines).
  const isExpression = node.expression !== undefined && node.name !== undefined;
  const anchor = !isExpression && node.name && typeof node.name.getStart === 'function'
    ? sf.getLineAndCharacterOfPosition(node.name.getStart())
    : start;
  const out = { line: anchor.line + 1, col: anchor.character + 1, end_line: end.line + 1 };
  if (anchor.line !== start.line) out.decorator_line = start.line + 1;
  return out;
}

/**
 * A node's symbol, memoised and with an alias followed to what it aliases.
 *
 * MEMOISED because the same nodes are asked about repeatedly: in the tool this is ported from, resolving
 * targets for every call inside every evaluated value made a single scope several times slower. The cache is
 * per program, so it dies with the checker it belongs to.
 */
export function makeSymbolAt(ts, checker, log = null) {
  const cache = new Map();
  const find = (node) => {
    let sym = checker.getSymbolAtLocation(node);
    if (sym && (sym.flags & ts.SymbolFlags.Alias) !== 0) sym = checker.getAliasedSymbol(sym);
    return sym;
  };
  // THROUGH THE READ LOG when there is one, so a hit still records what finding the symbol read.
  if (log) return (node) => log.cached(cache, node, () => find(node));
  return (node) => {
    if (cache.has(node)) return cache.get(node);
    const sym = find(node);
    cache.set(node, sym);
    return sym;
  };
}

/** JSDoc attached to a declaration, from the AST (`getJSDocCommentsAndTags`) and never scraped out of the
 *  source text. */
export function jsdocOf(ts, node) {
  const docs = ts.getJSDocCommentsAndTags(node);
  if (!docs.length) return undefined;
  const text = docs
    .map((d) => (typeof d.comment === 'string' ? d.comment : d.getText()))
    .filter((s) => !!s)
    .join('\n');
  return text || undefined;
}

/** The kind of node this one sits in - `PropertyAssignment`, `CallExpression`, and so on. */
export function parentKind(ts, node) {
  return (node.parent ? ts.SyntaxKind[node.parent.kind] : undefined) ?? 'Unknown';
}

/**
 * If the node sits in `{ name: <node> }` or `foo(<node>)`, WHICH property or callee it belongs to.
 *
 * Structural, read off the parent node. It is what makes a bare string literal joinable: `'en'` says
 * nothing, `'en'` as the `locale` property of a call to `setLanguage` says what it is.
 */
export function propertyContext(ts, node, propName) {
  const parent = node.parent;
  if (!parent) return null;
  if (ts.isPropertyAssignment(parent)) return propName(parent.name);
  if (ts.isCallExpression(parent)) {
    const callee = parent.expression;
    return ts.isPropertyAccessExpression(callee) ? callee.name.text
      : (ts.isIdentifier(callee) ? callee.text : null);
  }
  if (ts.isVariableDeclaration(parent) || ts.isPropertyDeclaration(parent)) return parent.name.getText();
  if (ts.isDecorator(parent)) return parent.expression.getText();
  return null;
}

/**
 * WHAT ONLY TYPES OR GROUPS AN EXPRESSION: `(x)`, `x as T`, `<T>x`, `x!`, `x satisfies T` - each is its `expression` at run
 * time. One set for every caller: the copies this replaced had drifted to different subsets, so `{..} satisfies T` was a
 * literal at one site and nothing at the next. `satisfies` is guarded for a TypeScript older than 4.9.
 */
export function isOuterWrapper(ts, node) {
  return ts.isParenthesizedExpression(node) || ts.isAsExpression(node) || ts.isTypeAssertionExpression(node)
    || ts.isNonNullExpression(node) || (ts.isSatisfiesExpression !== undefined && ts.isSatisfiesExpression(node));
}

/** The expression under every `isOuterWrapper` around `node` (`node` itself when none; `undefined` stays `undefined`). */
export function unwrapOuter(ts, node) {
  let at = node;
  while (at !== undefined && at !== null && isOuterWrapper(ts, at)) at = at.expression;
  return at;
}

/** A row cell that is an OBJECT (a ref, an options bag) - not null, not a list - or null. */
export const asObject = (v) => (v !== null && typeof v === 'object' && !Array.isArray(v) ? v : null);

/** THE JOIN KEY of two row cells - `file#name`, `class#member`, `file#line` - spelled once: a site that spelled it
 *  differently (a cell not stringified, another separator) missed every lookup without a word. */
export const joinKey = (a, b) => `${String(a)}#${String(b)}`;
/** The same key of three cells - `file#line#name`, `file#line#col`. */
export const joinKey3 = (a, b, c) => `${String(a)}#${String(b)}#${String(c)}`;

/** `=` and every compound or logical assignment, BY KIND RANGE: `ts.SyntaxKind[kind]` answers an alias name for the first
 *  and last kind of a range (`FirstAssignment` for `=`), so a name test misses them. */
export const isAssignmentKind = (ts, kind) => kind >= ts.SyntaxKind.FirstAssignment && kind <= ts.SyntaxKind.LastAssignment;

/** The call `n` is an ARGUMENT of - not its callee - or null. */
export const argCall = (ts, n) => {
  const call = n?.parent;
  return call !== undefined && ts.isCallExpression(call) && call.arguments.includes(n) ? call : null;
};

/** The property assignment `n` is the VALUE of, inside an object literal - or null. */
export const propertyOf = (ts, n) => {
  const prop = n.parent;
  return prop !== undefined && ts.isPropertyAssignment(prop) && prop.initializer === n && ts.isObjectLiteralExpression(prop.parent) ? prop : null;
};

/** A string written out whole - a string literal or a template with no substitution - never one built at run time. */
export const isPlainString = (ts, n) => n !== undefined && n !== null && (ts.isStringLiteral(n) || ts.isNoSubstitutionTemplateLiteral(n));
/** That string's text, or null for any other node. */
export const literalText = (ts, n) => (isPlainString(ts, n) ? n.text : null);

/** A declaration's generic parameters as the row cell `type_params`, or nothing when it has none - spread into the row. */
export const typeParamsOf = (node) => (node.typeParameters?.length ? { type_params: node.typeParameters.map((p) => p.name.text) } : {});

/** A property NAME that is written out - an identifier, a string or a number - never a computed one. */
export const isPlainKeyName = (ts, name) => ts.isIdentifier(name) || ts.isStringLiteral(name) || ts.isNumericLiteral(name);

/** What a type's numeric index gives - an array's element - as a list: one type, or none. */
export const elementTypes = (ts, checker, type) => {
  const element = checker.getIndexTypeOfType(type, ts.IndexKind.Number);
  return element === undefined ? [] : [element];
};

/** The variable declaration of a symbol that HAS an initializer - what a `const x = ...` the symbol names was set to - or undefined. */
export const initializedVar = (ts, symbol) => (symbol?.declarations ?? []).find((d) => ts.isVariableDeclaration(d) && d.initializer);

/** The symbol an import or re-export ALIAS stands for, followed through the checker; any other symbol - and none - as given. */
export const aliased = (ts, checker, symbol) => (symbol && (symbol.flags & ts.SymbolFlags.Alias) !== 0 ? checker.getAliasedSymbol(symbol) : symbol);
