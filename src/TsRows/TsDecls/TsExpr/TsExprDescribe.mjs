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

export function makeExprDescriber(ts, addRow, resolveRead, resolveName = null, log = null) {
  const norm = makeTsExprNormalizer(ts, resolveRead, resolveName);
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
