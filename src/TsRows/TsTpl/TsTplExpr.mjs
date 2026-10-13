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
  // `ThisReceiver` extends `ImplicitReceiver`, `SafePropertyRead` extends `PropertyRead` and `Unary` extends
  // `Binary` - matched as a `Binary`, `-a` kept neither operand and its read was lost.
  // The rest of the list is what later compilers added, in the TypeScript side's vocabulary (`TsExprNorm.mjs`).
  const KINDS = [
    ['Source', ng.ASTWithSource], ['Interpolation', ng.Interpolation],
    ['Literal', ng.LiteralPrimitive], ['Array', ng.LiteralArray], ['Map', ng.LiteralMap],
    ['SafeRead', ng.SafePropertyRead], ['Read', ng.PropertyRead], ['Write', ng.PropertyWrite],
    ['This', ng.ThisReceiver], ['Implicit', ng.ImplicitReceiver],
    ['Unary', ng.Unary], ['Binary', ng.Binary], ['Cond', ng.Conditional], ['Pipe', ng.BindingPipe],
    ['SafeCall', ng.SafeCall], ['Call', ng.Call],
    ['SafeKeyedRead', ng.SafeKeyedRead], ['KeyedRead', ng.KeyedRead],
    ['Not', ng.PrefixNot], ['NonNull', ng.NonNullAssert], ['Chain', ng.Chain],
    ['Empty', ng.EmptyExpr], ['Paren', ng.ParenthesizedExpression],
    ['TemplateLiteral', ng.TemplateLiteral], ['Tagged', ng.TaggedTemplateLiteral],
    ['Typeof', ng.TypeofExpression], ['Void', ng.VoidExpression], ['Spread', ng.SpreadElement],
    ['Regex', ng.RegularExpressionLiteral], ['Arrow', ng.ArrowFunction],
  ].filter((e) => typeof e[1] === 'function');

  const kindOf = (ast) => {
    for (const [name, cls] of KINDS) if (ast instanceof cls) return name;
    return sanitizeName(ast?.constructor?.name);
  };

  const isAst = (v) => !!v && typeof v === 'object' && typeof v.visit === 'function';

  /** `isAssignmentOperation` is static on `Binary` from v20, when assignment stopped being its own node. */
  const isAssignment = (op) => typeof ng.Binary?.isAssignmentOperation === 'function'
    && ng.Binary.isAssignmentOperation(op);

  /**
   * `params`: the arrow-function parameters in scope. A bare read of one is the parameter, not the component
   * member of that name, and is marked `param` so the summary leaves it out of `reads` (`TsExprSummary.mjs`).
   */
  function normalizeExpr(ast, depth = 0, params = null) {
    if (ast === null || ast === undefined || depth > 60) return null;
    const k = kindOf(ast);
    const a = ast;
    switch (k) {
      case 'Source':
        return { k, src: a.source ?? null, ast: normalizeExpr(a.ast, depth + 1, params) };
      case 'Interpolation':
        return {
          k, strings: a.strings,
          expressions: (a.expressions ?? []).map((e) => normalizeExpr(e, depth + 1, params)),
        };
      case 'Literal':
        return { k, v: a.value };
      case 'Array':
        return { k, items: (a.expressions ?? []).map((e) => normalizeExpr(e, depth + 1, params)) };
      case 'Map':
        return {
          k,
          keys: (a.keys ?? []).map((x) => ({ key: x.key, quoted: x.quoted })),
          values: (a.values ?? []).map((v) => normalizeExpr(v, depth + 1, params)),
        };
      case 'Read':
      case 'SafeRead': {
        const receiver = normalizeExpr(a.receiver, depth + 1, params);
        const param = params !== null && receiver?.k === 'Implicit' && params.has(a.name);
        return { k, name: a.name, receiver, ...(param ? { param: true } : {}) };
      }
      case 'Write':
        return {
          k, name: a.name, receiver: normalizeExpr(a.receiver, depth + 1, params),
          value: normalizeExpr(a.value, depth + 1, params),
        };
      case 'This':
      case 'Implicit':
      case 'Empty':
        return { k };
      case 'Binary':
        // FROM v20 AN ASSIGNMENT IS A `Binary` (`=`, `+=`, `??=`, ...), where it was `PropertyWrite` /
        // `KeyedWrite` before. Published as before, so the target is a write and not a read of the target.
        if (isAssignment(a.operation) && (a.left instanceof ng.PropertyRead || a.left instanceof ng.KeyedRead)) {
          const value = normalizeExpr(a.right, depth + 1, params);
          const op = a.operation === '=' ? {} : { op: a.operation };
          return a.left instanceof ng.PropertyRead
            ? { k: 'Write', name: a.left.name, receiver: normalizeExpr(a.left.receiver, depth + 1, params), value, ...op }
            : {
              k: 'KeyedWrite', receiver: normalizeExpr(a.left.receiver, depth + 1, params),
              key: normalizeExpr(a.left.key, depth + 1, params), value, ...op,
            };
        }
        return {
          k, op: a.operation, left: normalizeExpr(a.left, depth + 1, params),
          right: normalizeExpr(a.right, depth + 1, params),
        };
      case 'Unary':
        return { k, op: a.operator, expr: normalizeExpr(a.expr, depth + 1, params) };
      case 'Cond':
        return {
          k, cond: normalizeExpr(a.condition, depth + 1, params), then: normalizeExpr(a.trueExp, depth + 1, params),
          else: normalizeExpr(a.falseExp, depth + 1, params),
        };
      case 'Pipe':
        return {
          k, name: a.name, exp: normalizeExpr(a.exp, depth + 1, params),
          args: (a.args ?? []).map((x) => normalizeExpr(x, depth + 1, params)),
        };
      case 'Call':
      case 'SafeCall':
        return {
          k, receiver: normalizeExpr(a.receiver, depth + 1, params),
          args: (a.args ?? []).map((x) => normalizeExpr(x, depth + 1, params)),
        };
      case 'KeyedRead':
      case 'SafeKeyedRead':
        return { k, receiver: normalizeExpr(a.receiver, depth + 1, params), key: normalizeExpr(a.key, depth + 1, params) };
      case 'Not':
      case 'NonNull':
        return { k, expr: normalizeExpr(a.expression, depth + 1, params) };
      case 'Chain':
        return { k, items: (a.expressions ?? []).map((e) => normalizeExpr(e, depth + 1, params)) };
      // Parentheses carry no meaning once the tree is explicit.
      case 'Paren':
        return normalizeExpr(a.expression, depth + 1, params);
      case 'TemplateLiteral':
        return {
          k: 'Interpolation', strings: (a.elements ?? []).map((e) => e.text),
          expressions: (a.expressions ?? []).map((e) => normalizeExpr(e, depth + 1, params)),
        };
      case 'Tagged':
        return { k, receiver: normalizeExpr(a.tag, depth + 1, params), expr: normalizeExpr(a.template, depth + 1, params) };
      case 'Typeof':
      case 'Void':
        return { k: 'Unary', op: k === 'Typeof' ? 'typeof' : 'void', expr: normalizeExpr(a.expression, depth + 1, params) };
      case 'Spread':
        return { k, expr: normalizeExpr(a.expression, depth + 1, params) };
      case 'Regex':
        return { k: 'Other', kind: 'RegularExpressionLiteral', src: `/${a.body}/${a.flags ?? ''}` };
      case 'Arrow': {
        const names = (a.parameters ?? []).map((p) => p.name);
        const inner = new Set([...(params ?? []), ...names]);
        const body = normalizeExpr(a.body, depth + 1, inner);
        return { k: 'Fn', params: names, ...(body ? { returns: [body] } : {}) };
      }
      default: {
        // A node type this compiler version added: keep its kind and every child it exposes, so it is
        // VISIBLE in the map instead of vanishing.
        const out = { k };
        for (const [f, v] of Object.entries(a)) {
          if (f === 'span' || f === 'sourceSpan' || f === 'nameSpan') continue;
          if (Array.isArray(v)) out[f] = v.map((x) => (isAst(x) ? normalizeExpr(x, depth + 1, params) : x));
          else if (isAst(v)) out[f] = normalizeExpr(v, depth + 1, params);
          else if (v === null || ['string', 'number', 'boolean'].includes(typeof v)) out[f] = v;
        }
        return out;
      }
    }
  }

  return { normalizeExpr, kindOf };
}
