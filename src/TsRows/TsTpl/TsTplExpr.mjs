/**
 * AN ANGULAR TEMPLATE EXPRESSION AS THE SAME NORMALIZED TREE THE TYPESCRIPT SIDE EMITS.
 *
 * Ported from the original Angular extractor. A component's template and its class say the same
 * kinds of thing about the same properties, so one consumer reads both - which is only true if both sides
 * publish one vocabulary. The flat summary of such a tree (`reads`, `identifiers`, `calls`, `pipes`,
 * `strings`) lives in `TsExprSummary.mjs` and is shared with the TypeScript side unchanged.
 *
 * KIND IS DECIDED BY `instanceof` AGAINST THE COMPILER'S OWN CLASSES. The bundle renames colliding classes
 * (`Element` ships as `Element$1`), so dispatching on `constructor.name` picks the wrong branch the moment a
 * version renames one. Imports are flat: see `TsDecls/TsTypeRef.mjs`.
 */

/** `Element$1` -> `Element`: the bundle's collision suffix is not part of the node's identity. */
function sanitizeName(name) {
  if (typeof name !== 'string') return 'Unknown';
  const i = name.indexOf('$');
  return i > 0 ? name.slice(0, i) : name;
}

export function makeExprNormalizer(ng) {
  // Order matters only in that the FIRST match wins; the narrower class is listed first because
  // `ThisReceiver` extends `ImplicitReceiver` and `SafePropertyRead` extends `PropertyRead`.
  const KINDS = [
    ['Source', ng.ASTWithSource], ['Interpolation', ng.Interpolation],
    ['Literal', ng.LiteralPrimitive], ['Array', ng.LiteralArray], ['Map', ng.LiteralMap],
    ['SafeRead', ng.SafePropertyRead], ['Read', ng.PropertyRead], ['Write', ng.PropertyWrite],
    ['This', ng.ThisReceiver], ['Implicit', ng.ImplicitReceiver],
    ['Binary', ng.Binary], ['Unary', ng.Unary], ['Cond', ng.Conditional], ['Pipe', ng.BindingPipe],
    ['SafeCall', ng.SafeCall], ['Call', ng.Call],
    ['SafeKeyedRead', ng.SafeKeyedRead], ['KeyedRead', ng.KeyedRead],
    ['Not', ng.PrefixNot], ['NonNull', ng.NonNullAssert], ['Chain', ng.Chain],
    ['Empty', ng.EmptyExpr],
  ].filter((e) => typeof e[1] === 'function');

  const kindOf = (ast) => {
    for (const [name, cls] of KINDS) if (ast instanceof cls) return name;
    return sanitizeName(ast?.constructor?.name);
  };

  const isAst = (v) => !!v && typeof v === 'object' && typeof v.visit === 'function';

  function normalizeExpr(ast, depth = 0) {
    if (ast === null || ast === undefined || depth > 60) return null;
    const k = kindOf(ast);
    const a = ast;
    switch (k) {
      case 'Source':
        return { k, src: a.source ?? null, ast: normalizeExpr(a.ast, depth + 1) };
      case 'Interpolation':
        return {
          k, strings: a.strings,
          expressions: (a.expressions ?? []).map((e) => normalizeExpr(e, depth + 1)),
        };
      case 'Literal':
        return { k, v: a.value };
      case 'Array':
        return { k, items: (a.expressions ?? []).map((e) => normalizeExpr(e, depth + 1)) };
      case 'Map':
        return {
          k,
          keys: (a.keys ?? []).map((x) => ({ key: x.key, quoted: x.quoted })),
          values: (a.values ?? []).map((v) => normalizeExpr(v, depth + 1)),
        };
      case 'Read':
      case 'SafeRead':
        return { k, name: a.name, receiver: normalizeExpr(a.receiver, depth + 1) };
      case 'Write':
        return {
          k, name: a.name, receiver: normalizeExpr(a.receiver, depth + 1),
          value: normalizeExpr(a.value, depth + 1),
        };
      case 'This':
      case 'Implicit':
      case 'Empty':
        return { k };
      case 'Binary':
        return {
          k, op: a.operation, left: normalizeExpr(a.left, depth + 1),
          right: normalizeExpr(a.right, depth + 1),
        };
      case 'Unary':
        return { k, op: a.operator, expr: normalizeExpr(a.expr, depth + 1) };
      case 'Cond':
        return {
          k, cond: normalizeExpr(a.condition, depth + 1), then: normalizeExpr(a.trueExp, depth + 1),
          else: normalizeExpr(a.falseExp, depth + 1),
        };
      case 'Pipe':
        return {
          k, name: a.name, exp: normalizeExpr(a.exp, depth + 1),
          args: (a.args ?? []).map((x) => normalizeExpr(x, depth + 1)),
        };
      case 'Call':
      case 'SafeCall':
        return {
          k, receiver: normalizeExpr(a.receiver, depth + 1),
          args: (a.args ?? []).map((x) => normalizeExpr(x, depth + 1)),
        };
      case 'KeyedRead':
      case 'SafeKeyedRead':
        return { k, receiver: normalizeExpr(a.receiver, depth + 1), key: normalizeExpr(a.key, depth + 1) };
      case 'Not':
      case 'NonNull':
        return { k, expr: normalizeExpr(a.expression, depth + 1) };
      case 'Chain':
        return { k, items: (a.expressions ?? []).map((e) => normalizeExpr(e, depth + 1)) };
      default: {
        // A node type this compiler version added: keep its kind and every child it exposes, so it is
        // VISIBLE in the map instead of vanishing.
        const out = { k };
        for (const [f, v] of Object.entries(a)) {
          if (f === 'span' || f === 'sourceSpan' || f === 'nameSpan') continue;
          if (Array.isArray(v)) out[f] = v.map((x) => (isAst(x) ? normalizeExpr(x, depth + 1) : x));
          else if (isAst(v)) out[f] = normalizeExpr(v, depth + 1);
          else if (v === null || ['string', 'number', 'boolean'].includes(typeof v)) out[f] = v;
        }
        return out;
      }
    }
  }

  return { normalizeExpr, kindOf };
}
