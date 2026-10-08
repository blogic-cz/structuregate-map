/**
 * TsLiterals.mjs - what a LITERAL is doing where it is written, for `string_literals` and `number_literals`, in
 * BOTH TypeScript halves (plain and Angular), and in the vocabulary the python, C# and rust halves share - so one
 * `--magic` query reads every language.
 *
 * `use` is `compare` (an operand of `===`/`<`, a `case`; `typeof` when the other side is a `typeof`), `arith`, `index` (an element access), `argument` (with
 * the `callee` it is passed to), `assign`, `return`, `default` (a parameter's), `declared` (the initializer of a
 * top-level `const`, an enum member or a `static readonly` field), `format` (a template hole) or `other`. `target`
 * is what an index or a method call applies to: `line.split(':')[2]` is a ':' passed to `split` on `line`, and a
 * 2 indexing `line.split(':')`.
 *
 * THE COLUMN IS `use`, NOT `context`: the Angular half's `string_literals.context` already means the parent's
 * SyntaxKind, and a column meaning two things depending on the half is a query that is wrong for one of them.
 *
 * Staged FLAT with each half's scripts, so it imports nothing by a relative folder.
 */

const OTHER = { use: 'other', callee: '', target: '' };

/** `{use, callee, target}` of one string or numeric literal. `text(node)` is the node's source as written. */
export function literalUse(ts, node, text) {
  let contained = false;
  const found = useOf(ts, node, text, () => { contained = true; });
  // AN ELEMENT IS NOT THE ARGUMENT: the strings of `[a, 'x'].join(',')` are not separators, nor are the values
  // of an object handed to a call. Only a constant's value is the container's - `const SIZES = [1, 2]`.
  return contained && found.use !== 'declared' ? OTHER : found;
}

function useOf(ts, node, text, enter) {
  const K = ts.SyntaxKind;
  let child = node;
  let parent = node.parent;
  // Through a sign, brackets, `as const` and the array or object a literal sits in: `const SIZES = [1, 2]`
  // declares both numbers, and `{ retries: 3 }` passed to a call is an argument of that call.
  while (parent && (ts.isPrefixUnaryExpression(parent) || ts.isParenthesizedExpression(parent)
      || ts.isAsExpression(parent) || ts.isArrayLiteralExpression(parent) || ts.isObjectLiteralExpression(parent)
      || (ts.isPropertyAssignment(parent) && parent.initializer === child)
      || (ts.isSatisfiesExpression && ts.isSatisfiesExpression(parent)))) {
    if (ts.isArrayLiteralExpression(parent) || ts.isObjectLiteralExpression(parent)) enter();
    child = parent;
    parent = parent.parent;
  }
  if (!parent) return OTHER;
  if (ts.isBinaryExpression(parent)) {
    const op = parent.operatorToken.kind;
    if ([K.EqualsEqualsToken, K.EqualsEqualsEqualsToken, K.ExclamationEqualsToken, K.ExclamationEqualsEqualsToken,
      K.LessThanToken, K.LessThanEqualsToken, K.GreaterThanToken, K.GreaterThanEqualsToken].includes(op)) {
      // `typeof x === 'string'` names a TYPE the language spells as a string - an idiom, not a magic value.
      const other = parent.left === child ? parent.right : parent.left;
      return { ...OTHER, use: ts.isTypeOfExpression(other) ? 'typeof' : 'compare' };
    }
    if (op === K.EqualsToken) return { ...OTHER, use: parent.right === child ? 'assign' : 'other' };
    return { ...OTHER, use: 'arith' };
  }
  if (ts.isCaseClause(parent)) return { ...OTHER, use: 'compare' };
  if (ts.isElementAccessExpression(parent) && parent.argumentExpression === child) {
    return { ...OTHER, use: 'index', target: text(parent.expression) };
  }
  if ((ts.isCallExpression(parent) || ts.isNewExpression(parent)) && (parent.arguments || []).includes(child)) {
    const callee = parent.expression;
    if (ts.isPropertyAccessExpression(callee)) return { use: 'argument', callee: callee.name.text, target: text(callee.expression) };
    return { use: 'argument', callee: ts.isIdentifier(callee) ? callee.text : '', target: '' };
  }
  if (ts.isVariableDeclaration(parent) && parent.initializer === child) {
    const list = parent.parent;
    const topLevel = list && list.parent && ts.isVariableStatement(list.parent) && ts.isSourceFile(list.parent.parent);
    return { ...OTHER, use: topLevel && (list.flags & ts.NodeFlags.Const) ? 'declared' : 'assign' };
  }
  if (ts.isEnumMember(parent)) return { ...OTHER, use: 'declared' };
  if (ts.isParameter(parent) && parent.initializer === child) return { ...OTHER, use: 'default' };
  if (ts.isPropertyDeclaration(parent) && parent.initializer === child) {
    const mods = (ts.getModifiers ? ts.getModifiers(parent) : parent.modifiers) || [];
    const has = (kind) => mods.some((m) => m.kind === kind);
    return { ...OTHER, use: has(K.StaticKeyword) && has(K.ReadonlyKeyword) ? 'declared' : 'assign' };
  }
  if (ts.isReturnStatement(parent) || (ts.isArrowFunction(parent) && parent.body === child)) return { ...OTHER, use: 'return' };
  if (ts.isTemplateSpan(parent)) return { ...OTHER, use: 'format' };
  return OTHER;
}

/** `{value, number}` of a numeric literal - the sign included, as written - or null for anything else. */
export function numberOf(ts, node, text) {
  if (!ts.isNumericLiteral(node)) return null;
  const parent = node.parent;
  const negative = parent && ts.isPrefixUnaryExpression(parent) && parent.operator === ts.SyntaxKind.MinusToken;
  const number = Number(node.text);
  return { value: (negative ? '-' : '') + text(node), number: negative ? -number : number };
}
