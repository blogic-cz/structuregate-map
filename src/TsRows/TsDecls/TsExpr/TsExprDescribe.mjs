/**
 * ONE ROW PER EXPRESSION - and the rule for when an expression deserves a row at all.
 *
 * Ported from the original Angular extractor. Imports are flat: see `TsTypeRef.mjs`.
 *
 * MEMOIZED BY AST NODE IDENTITY. The evaluator is re-entered on the same syntactic node from several
 * contexts - the same value re-materializes under `assignments`, `calls`, `locals` and `returns` - and an
 * un-memoized sink wrote a fresh, byte-identical row each time: about a quarter of the rows duplicates.
 */
import { dedupeSummary, summarizeExpr } from './TsExprSummary.mjs';
import { makeTsExprNormalizer } from './TsExprNorm.mjs';

/**
 * THE SHAPE A FUNCTION RETURNS, as dotted paths - the write side of a read.
 *
 * A read resolves to the property it lands on; nothing said which code PRODUCES that property. Where state
 * is updated by returning a new object rather than by assigning to one, the producer is a callback whose
 * returned literal names exactly the paths it sets, and that literal is already in the tree.
 *
 * Keys only, never values: the VALUE at run time is not knowable and is not claimed. A spread contributes
 * nothing - it copies paths this function did not name - and is deliberately not invented as a wildcard.
 */
function writtenPaths(node, prefix, out) {
  if (node === null) return;
  if (node.k === 'Fn') {
    for (const r of node.returns ?? []) writtenPaths(r, prefix, out);
    return;
  }
  if (node.k !== 'Map') return;
  const keys = node.keys ?? [];
  const values = node.values ?? [];
  keys.forEach((k, i) => {
    if (k.key === '...') return;
    const path = prefix ? `${prefix}.${k.key}` : k.key;
    out.push(path);
    const value = values[i] ?? null;
    if (value && value.k === 'Map') writtenPaths(value, path, out);
  });
}

/** A method that changes the list, set or map it is called on - on a draft, a write to that property. */
const MUTATORS = new Set(['push', 'pop', 'shift', 'unshift', 'splice', 'sort', 'reverse', 'fill', 'copyWithin',
  'set', 'add', 'delete', 'clear']);

/**
 * THE PATHS A DRAFT HANDLER MUTATES (rows 79): `immerOn(Act, (s) => { s.a = 1; s.list.push(x); s.b.c++; })` returns nothing
 * and writes `a`, `list`, `b.c`, where `on(Act, (s) => ({ ...s, a: 1 }))` returns them (`writtenPaths`). The draft is the
 * first parameter. A path is the property names from it - an index is not a segment (`s.items[i].done = 1` is `items.done`) -
 * and a method call on the way (`s.items.find(..)`) is the element it hands back. A LOCAL initialised from such a path
 * (`const t = s.response.find(..)`) IS that path: `t.active = 1` writes `response.active`. Keys only, as `writtenPaths`.
 */
export function draftWrites(ts, fn) {
  const first = fn.parameters?.[0]?.name;
  if (first === undefined || !ts.isIdentifier(first)) return [];
  // BY KIND RANGE: `ts.SyntaxKind[kind]` answers an ALIAS name for the first and last of a range (`FirstAssignment` for `=`).
  const isAssignment = (k) => k >= ts.SyntaxKind.FirstAssignment && k <= ts.SyntaxKind.LastAssignment;
  const aliases = new Map();
  const out = [];
  const pathOf = (e) => {
    if (ts.isParenthesizedExpression(e) || ts.isNonNullExpression(e) || ts.isAsExpression(e)
      || (ts.isSatisfiesExpression !== undefined && ts.isSatisfiesExpression(e))) return pathOf(e.expression);
    if (ts.isIdentifier(e)) return e.text === first.text ? [] : aliases.get(e.text) ?? null;
    if (ts.isPropertyAccessExpression(e)) {
      const base = pathOf(e.expression);
      return base === null ? null : [...base, e.name.text];
    }
    if (ts.isElementAccessExpression(e)) return pathOf(e.expression);
    if (ts.isCallExpression(e) && ts.isPropertyAccessExpression(e.expression)) return pathOf(e.expression.expression);
    return null;
  };
  const note = (e) => {
    const path = pathOf(e);
    if (path !== null && path.length) out.push(path.join('.'));
  };
  const visit = (n) => {
    if (ts.isVariableDeclaration(n) && ts.isIdentifier(n.name) && n.initializer !== undefined) {
      const path = pathOf(n.initializer);
      if (path !== null) aliases.set(n.name.text, path);
    } else if (ts.isBinaryExpression(n) && isAssignment(n.operatorToken.kind)) {
      note(n.left);
    } else if ((ts.isPrefixUnaryExpression(n) || ts.isPostfixUnaryExpression(n))
      && (n.operator === ts.SyntaxKind.PlusPlusToken || n.operator === ts.SyntaxKind.MinusMinusToken)) {
      note(n.operand);
    } else if (ts.isDeleteExpression(n)) {
      note(n.expression);
    } else if (ts.isCallExpression(n) && ts.isPropertyAccessExpression(n.expression) && MUTATORS.has(n.expression.name.text)) {
      note(n.expression.expression);
    }
    ts.forEachChild(n, visit);
  };
  visit(fn.body);
  return [...new Set(out)];
}

export function makeExprDescriber(ts, addRow, resolveRead, resolveName = null, log = null, resolveLocal = null, resolveParam = null) {
  const norm = makeTsExprNormalizer(ts, resolveRead, resolveName, resolveLocal, resolveParam);
  // Keyed by node AND role - see the note in the returned function.
  const cache = new Map();

  const pureReadChain = (e) => {
    if (e === null) return true;
    if (e.k === 'Read' || e.k === 'SafeRead') return pureReadChain(e.receiver ?? null);
    return e.k === 'This' || e.k === 'Implicit' || e.k === 'Literal';
  };

  return (node, role) => {
    let perRole = cache.get(node);
    if (perRole === undefined) {
      perRole = new Map();
      cache.set(node, perRole);
    }
    // THROUGH THE READ LOG when there is one, so a hit still records what the description read.
    if (log) return log.cached(perRole, role, () => described(node, role, perRole));
    const hit = perRole.get(role);
    if (hit !== undefined) return hit;
    return described(node, role, perRole);
  };

  function described(node, role, perRole) {

    const ast = norm(node);
    if (ast === null) {
      const miss = { id: null, summary: null, writes: [] };
      perRole.set(role, miss);
      return miss;
    }
    // PER NODE **AND** ROLE, because `role` is a claim about how the expression is USED and has to stay
    // true. The same node is described twice - once as a plain unevaluable value, where a pure read chain
    // needs no row, and once as a CONDITION, which always needs one. Caching on the node alone returned the
    // first answer to the second caller and dropped the row for every guard that is a bare identifier:
    // condition rows fell to a fraction of their count, so the join target vanished for exactly the
    // guards that matter most.
    //
    // A TREE ROW IS WRITTEN ONLY WHEN THE TREE SAYS MORE THAN THE SUMMARY DOES. A bare identifier or a pure
    // member chain is fully described by `identifiers` + `reads`, so a row for it would double this table
    // to restate what is already inline. The test is STRUCTURAL, never a size or a depth number.
    const needsRow = role !== 'expr' || !pureReadChain(ast);
    const summary = dedupeSummary(summarizeExpr(ast));
    const paths = [];
    writtenPaths(ast, '', paths);
    const writes = [...new Set(paths)];
    const out = { id: needsRow ? addRow(node, role, ast, summary, writes) : null, summary, writes };
    perRole.set(role, out);
    return out;
  };
}
