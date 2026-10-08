/**
 * THE FLAT SUMMARY OF A NORMALIZED EXPRESSION - what it READS, CALLS, pipes through, and which strings.
 *
 * Ported from the original Angular extractor (the half the TypeScript side uses). A NAME IS
 * JOINABLE, A STRING IS NOT: `this.model.Order.Lines.Count` cannot be VALUED - it is runtime
 * state and the map must not pretend otherwise - but it is fully STRUCTURED, and the difference between
 * publishing the text and publishing `reads` + `identifiers` is the difference between a consumer
 * pattern-matching a string and a consumer doing a join.
 *
 * Imports are flat: see `TsTypeRef.mjs`.
 */

/**
 * A NON-NULL ASSERTION IS NOT A LINK IN THE CHAIN. `x!.items` is a read of `x`, exactly as `x.field`
 * is - the `!` is the author telling the compiler something, not another hop. Stopping on it made the root
 * report `items` as an identifier, so `x!.a.b` summarized as two different variables being read.
 * Transparent here rather than by unwrapping the node, so the tree still records the assertion.
 */
const throughNonNull = (n) => {
  let c = n;
  while (c && c.k === 'NonNull') c = c.expr ?? null;
  return c;
};

export function dottedPath(norm) {
  const parts = [];
  let n = throughNonNull(norm);
  while (n && (n.k === 'Read' || n.k === 'SafeRead')) {
    parts.unshift(String(n.name));
    n = throughNonNull(n.receiver ?? null);
  }
  if (n && n.k === 'This') parts.unshift('this');
  return parts.join('.');
}

function rootName(norm) {
  let n = throughNonNull(norm) ?? norm;
  for (;;) {
    const receiver = throughNonNull(n.receiver ?? null);
    if ((n.k === 'Read' || n.k === 'SafeRead') && receiver
      && (receiver.k === 'Read' || receiver.k === 'SafeRead')) {
      n = receiver;
      continue;
    }
    break;
  }
  return typeof n.name === 'string' ? n.name : null;
}

export function summarizeExpr(norm, acc = { identifiers: [], strings: [], pipes: [], calls: [], reads: [] }) {
  if (!norm || typeof norm !== 'object') return acc;
  switch (norm.k) {
    case 'Read':
    case 'SafeRead': {
      acc.reads.push(dottedPath(norm));
      const root = rootName(norm);
      if (root) acc.identifiers.push(root);
      break;
    }
    case 'Literal':
      if (typeof norm.v === 'string') acc.strings.push(norm.v);
      break;
    case 'Pipe':
      if (typeof norm.name === 'string') acc.pipes.push(norm.name);
      break;
    case 'Call':
    case 'SafeCall':
      if (norm.receiver) acc.calls.push(dottedPath(norm.receiver));
      break;
    default: break;
  }
  for (const [key, v] of Object.entries(norm)) {
    if (key === 'k') continue;
    if (Array.isArray(v)) v.forEach((x) => summarizeExpr(x, acc));
    else if (v && typeof v === 'object') summarizeExpr(v, acc);
  }
  return acc;
}

export function dedupeSummary(s) {
  const uniq = (a) => [...new Set(a.filter((x) => x !== null && x !== undefined && x !== ''))];
  return {
    identifiers: uniq(s.identifiers), reads: uniq(s.reads), calls: uniq(s.calls),
    pipes: uniq(s.pipes), strings: uniq(s.strings),
  };
}
