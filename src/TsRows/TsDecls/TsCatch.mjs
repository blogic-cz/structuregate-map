/**
 * WHAT ONE `catch` CLAUSE DOES - the `handlers` row of both TypeScript halves, in the columns the python half
 * writes for an `except`. Each half spreads it into a row of its own shape: the plain half beside `where()`,
 * the Angular half beside its member, class and branch. Imports are flat: see `TsTypeRef.mjs`.
 *
 * TYPESCRIPT NAMES NO TYPE A CLAUSE CATCHES. `catch (e: unknown)` is an annotation, not a filter, so every
 * clause is `bare = 1` with `types = []`; the python-only columns (`star`, `exc_info`, `noqa`, `codes`) and
 * C#'s `guard` and `symbol` are written neutral, so a filter on them reads the same in every half.
 *
 * THE BODY IS THE CLAUSE'S OWN NODES. A function or a class declared in the block runs later, or never: a
 * `throw` there is not the handler's, as python leaves out a nested `def`.
 */

/** `a.b.c` for a callee written as a name chain, '' for anything computed. The plain half's `dotted`. */
export function chainOf(ts, node) {
  const K = ts.SyntaxKind;
  if (!node) return '';
  if (node.kind === K.Identifier || node.kind === K.PrivateIdentifier) return node.text;
  if (node.kind === K.ThisKeyword) return 'this';
  if (node.kind === K.SuperKeyword) return 'super';
  if (node.kind === K.PropertyAccessExpression) {
    const head = chainOf(ts, node.expression);
    return head ? head + '.' + node.name.text : '';
  }
  return '';
}

/** An identifier that READS a value: not the name half of `a.b`, nor a declared or assigned key. */
function isRead(ts, node) {
  const parent = node.parent;
  if (!parent) return true;
  if (ts.isPropertyAccessExpression(parent) && parent.name === node) return false;
  if ((ts.isPropertyAssignment(parent) || ts.isVariableDeclaration(parent) || ts.isParameter(parent)
    || ts.isBindingElement(parent)) && parent.name === node) return false;
  return true;
}

/** The first comment on the `catch` line - after `)` or the `catch` keyword, else after the `{`. */
function commentOn(ts, sf, block) {
  for (const at of [block.getFullStart(), block.getStart(sf) + 1]) {
    const ranges = ts.getTrailingCommentRanges(sf.text, at) ?? [];
    if (ranges.length > 0) return sf.text.slice(ranges[0].pos, ranges[0].end);
  }
  return '';
}

/** The `handlers` cells of `tryNode`'s catch clause, or null when it has none. */
export function catchFacts(ts, sf, tryNode) {
  const clause = tryNode.catchClause;
  if (!clause) return null;
  const block = clause.block;
  const bound = clause.variableDeclaration ? clause.variableDeclaration.name : null;
  const name = !bound ? '' : ts.isIdentifier(bound) ? bound.text : bound.getText(sf);
  const calls = new Set();
  let raises = 0;
  let reraises = 0;
  let nameRead = 0;
  const walk = (node) => {
    if (ts.isFunctionLike(node) || ts.isClassLike(node)) return;
    if (ts.isThrowStatement(node)) {
      raises += 1;
      // `throw e` of the name this clause bound: the exception goes on as it came.
      if (name && node.expression && ts.isIdentifier(node.expression) && node.expression.text === name) reraises = 1;
    } else if (ts.isIdentifier(node) && name && node.text === name && isRead(ts, node)) {
      nameRead = 1;
    } else if (ts.isCallExpression(node) || ts.isNewExpression(node)) {
      const callee = chainOf(ts, node.expression);
      if (callee) calls.add(callee);
    }
    ts.forEachChild(node, walk);
  };
  ts.forEachChild(block, walk);
  const at = (pos) => sf.getLineAndCharacterOfPosition(pos).line + 1;
  return {
    line: at(clause.getStart(sf)), end_line: at(clause.getEnd()), try_line: at(tryNode.getStart(sf)),
    types: [], bare: 1, star: 0, name, name_read: nameRead,
    // A COMMENT IS NO STATEMENT: a block holding only one still does nothing.
    passes: block.statements.every((s) => s.kind === ts.SyntaxKind.EmptyStatement) ? 1 : 0,
    raises, reraises, exc_info: 0, calls: [...calls].sort(), comment: commentOn(ts, sf, block),
    noqa: 0, codes: [], reads: [], guard: '', finally: tryNode.finallyBlock ? 1 : 0, symbol: '',
  };
}
