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
  const anchor = node.name && typeof node.name.getStart === 'function'
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
