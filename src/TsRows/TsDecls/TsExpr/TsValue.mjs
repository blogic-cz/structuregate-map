/**
 * WHAT A DECLARATION'S VALUE IS, worked out past the compiler.
 *
 * Ported from the original Angular extractor. The C# half of this repo states the same rule
 * for the same reason: a value that reaches the map as source text forces every consumer to pattern-match
 * it, which is the one thing this tool forbids in every language it maps. So a literal is its value, a
 * reference is followed to the declaration that carries one, an enum member keeps BOTH its name and its
 * number, and a value that cannot be known keeps its full text plus whatever structure it does have.
 *
 * NOTHING IS COLLAPSED TO A BARE VALUE where the value is conditional. `a && { id }` yields the object only
 * when `a` holds, so publishing the object alone would assert it is unconditionally there - the same
 * mistake as an empty guard list reading "unguarded" when it means "unknown".
 *
 * Imports are flat: see `TsTypeRef.mjs`.
 */
import { makeValueRefs } from './TsValueRefs.mjs';

/**
 * `describe` is the expressions writer, and it is OPTIONAL: given one, an unevaluable value also carries
 * the identifiers it reads and the id of its own `expressions` row, which is what makes it joinable.
 */
export function makeEvaluator(ts, checker, symbolAt, describe = null) {
  /** Declarations currently being followed, by file+position - the recursion PATH, not a visited set: a
   *  declaration reached twice by different routes is fine, one reached from inside itself is a cycle. */
  const following = new Set();
  const declKey = (decl) => `${decl.getSourceFile().fileName}:${String(decl.pos)}`;

  function follow(decl, initializer, text) {
    const key = declKey(decl);
    if (following.has(key)) return { $cycle: text };
    following.add(key);
    try {
      return evalNode(initializer);
    } finally {
      following.delete(key);
    }
  }

  /** Attach the flat summary, and the tree row id when one was written, to any marked value. */
  function describeInto(out, node) {
    const d = describe ? describe(node, 'expr') : null;
    if (d && d.summary) {
      for (const [field, values] of Object.entries(d.summary)) {
        if (values.length) out['$' + field] = values;
      }
    }
    if (d && d.writes && d.writes.length) out.$writes = d.writes;
    if (d && d.id !== null && d.id !== undefined) out.$expr_id = d.id;
  }

  /** An expression that cannot be evaluated keeps its FULL source text - truncating it would make the
   *  mirror lossy exactly where the consumer has to fall back to reading the code. */
  function unresolved(node) {
    if (!node) return { $expr: '', $kind: 'Unknown' };
    const out = { $expr: node.getText(), $kind: ts.SyntaxKind[node.kind] ?? String(node.kind) };
    describeInto(out, node);
    return out;
  }

  const condRef = (node, role) => {
    const id = describe ? describe(node, role).id : null;
    return id === null || id === undefined ? {} : { $cond_expr: id };
  };

  /**
   * A VALUE BEHIND A CONDITION IS STILL A VALUE - but it is never unconditional.
   *
   * Both operands are evaluated and the OPERATOR is kept rather than a guard/value pair: `a && b` yields b
   * when a is truthy, while `a || b` and `a ?? b` yield A, and those two are the majority. A shape that
   * modelled only `&&` would state the fallback is the value on most of them. Comparisons are deliberately
   * excluded - their operand is not a payload whose presence is conditional.
   */
  function logical(node, op) {
    return {
      $logic: op,
      $operands: [evalNode(node.left), evalNode(node.right)],
      ...condRef(node.left, 'logic'),
      ...unresolved(node),
    };
  }

  const refs = makeValueRefs({
    ts, checker, symbolAt,
    api: { evalNode: (n) => evalNode(n), follow, unresolved },
  });

  function evalArray(node) {
    // A SPREAD ELEMENT MUST BE FLATTENED, not stored as an opaque node: `declarations: [...COMPONENTS]` is
    // how a module lists what it declares, and leaving it unflattened made hundreds of declared components
    // invisible - every element in their templates then resolved to an unknown NgModule scope.
    const out = [];
    for (const e of node.elements) {
      if (ts.isSpreadElement(e)) {
        const value = evalNode(e.expression);
        if (Array.isArray(value)) out.push(...value);
        else out.push({ $spread: value });
        continue;
      }
      out.push(evalNode(e));
    }
    return out;
  }

  function evalObject(node) {
    const out = {};
    for (const p of node.properties) {
      if (ts.isPropertyAssignment(p)) out[propNameOf(p.name)] = evalNode(p.initializer);
      else if (ts.isShorthandPropertyAssignment(p)) out[p.name.text] = refs.evalIdentifier(p.name);
      else if (ts.isSpreadAssignment(p)) {
        const spread = evalNode(p.expression);
        if (spread && typeof spread === 'object' && !Array.isArray(spread) && !('$expr' in spread)) {
          Object.assign(out, spread);
        } else out.$spread = spread;
      } else if (ts.isMethodDeclaration(p) || ts.isGetAccessor(p) || ts.isSetAccessor(p)) {
        out[propNameOf(p.name)] = { $kind: ts.SyntaxKind[p.kind] ?? String(p.kind) };
      }
    }
    return out;
  }

  /**
   * A METHOD CALLED ON A LITERAL COLLECTION STILL HAS ITS COLLECTION, and a chain keeps every link.
   *
   * `[{...}].filter(x).map(f)` is a call, so an evaluator that gives up keeps only the raw source text of
   * the whole array - data physically present in the map and reachable only by pattern. ARRAYS ONLY,
   * deliberately: a `$ref`/`$expr` receiver would add a field to thousands of rows and recover nothing.
   * A receiver that is not a collection keeps the LINK to its own row instead, because evaluating it has
   * already minted that row and throwing the pointer away leaves it reachable by nothing.
   */
  function evalCall(node) {
    const callee = ts.isPropertyAccessExpression(node.expression) ? node.expression.name : node.expression;
    const target = refs.declTarget(symbolAt(callee)?.getName() ?? callee.getText(), callee) ?? undefined;
    let receiver;
    let receiverExpr;
    if (ts.isPropertyAccessExpression(node.expression)) {
      const evaluated = evalNode(node.expression.expression);
      const isChainLink = !!evaluated && typeof evaluated === 'object' && !Array.isArray(evaluated)
        && evaluated.$receiver !== undefined;
      if (Array.isArray(evaluated) || isChainLink) receiver = evaluated;
      else if (!!evaluated && typeof evaluated === 'object' && !Array.isArray(evaluated)) {
        const id = evaluated.$expr_id ?? evaluated.$cond_expr;
        if (typeof id === 'string') receiverExpr = id;
      }
    }
    return {
      $call: node.expression.getText(),
      $args: node.arguments.map((a) => evalNode(a)),
      ...(receiver !== undefined ? { $receiver: receiver, $method: node.expression.name.getText() } : {}),
      ...(receiverExpr !== undefined ? { $receiver_expr: receiverExpr } : {}),
      ...(target ? { $target: target } : {}),
    };
  }

  function evalNode(node) {
    if (!node) return unresolved(node);
    if (ts.isStringLiteral(node) || ts.isNoSubstitutionTemplateLiteral(node)) return node.text;
    if (ts.isNumericLiteral(node)) return Number(node.text);
    if (node.kind === ts.SyntaxKind.TrueKeyword) return true;
    if (node.kind === ts.SyntaxKind.FalseKeyword) return false;
    if (node.kind === ts.SyntaxKind.NullKeyword) return null;
    if (ts.isParenthesizedExpression(node) || ts.isAsExpression(node) || ts.isSatisfiesExpression(node)
      || ts.isNonNullExpression(node) || ts.isTypeAssertionExpression(node)) {
      return evalNode(node.expression);
    }
    if (ts.isPrefixUnaryExpression(node)) {
      const inner = evalNode(node.operand);
      if (typeof inner === 'number' && node.operator === ts.SyntaxKind.MinusToken) return -inner;
      if (typeof inner === 'boolean' && node.operator === ts.SyntaxKind.ExclamationToken) return !inner;
      return unresolved(node);
    }
    if (ts.isArrayLiteralExpression(node)) return evalArray(node);
    if (ts.isObjectLiteralExpression(node)) return evalObject(node);
    if (ts.isTemplateExpression(node)) {
      // A template literal WITH substitutions: the literal parts are kept and the holes are marked, so a
      // dynamically built translation key still yields its stable prefix.
      const parts = [node.head.text];
      const holes = [];
      for (const span of node.templateSpans) {
        holes.push(span.expression.getText());
        parts.push(span.literal.text);
      }
      // THE TREE, as a TERNARY's condition carries one: a hole written `Enum[x]` names the member the built
      // key spells, and only the tree says the hole is that lookup rather than any text.
      const tree = describe ? describe(node, 'template').id : null;
      return { $template: parts, $holes: holes, ...(tree === null || tree === undefined ? {} : { $expr_id: tree }) };
    }
    if (ts.isBinaryExpression(node)) {
      if (node.operatorToken.kind === ts.SyntaxKind.PlusToken) {
        const left = evalNode(node.left);
        const right = evalNode(node.right);
        if (typeof left === 'string' && typeof right === 'string') return left + right;
        if (typeof left === 'number' && typeof right === 'number') return left + right;
        return { $concat: [left, right] };
      }
      const op = node.operatorToken.getText();
      if (op === '&&' || op === '||' || op === '??') return logical(node, op);
      return unresolved(node);
    }
    if (ts.isPropertyAccessExpression(node) || ts.isElementAccessExpression(node)) {
      return refs.evalMemberAccess(node);
    }
    if (ts.isIdentifier(node)) return refs.evalIdentifier(node);
    // TYPE ARGUMENTS ARE NOT PART OF THE VALUE. `Modal<Person>` in a value position is an instantiation
    // whose value is the class itself; stopping at the outer node published the whole text as one blob, so
    // a component handed to a factory that way resolved to no class and read as rendered by nobody.
    if (ts.isExpressionWithTypeArguments(node)) return evalNode(node.expression);
    // AWAIT HID THE CALL UNDERNEATH IT: thousands of them reached a real map as source text, many naming a
    // feature flag. The awaited value IS what the expression evaluates to.
    if (ts.isAwaitExpression(node)) return { $await: evalNode(node.expression) };
    if (ts.isNewExpression(node)) {
      const name = symbolAt(node.expression)?.getName() ?? node.expression.getText();
      const target = refs.declTarget(name, node.expression);
      return {
        $new: node.expression.getText(),
        $args: (node.arguments ?? []).map((a) => evalNode(a)),
        ...(target ? { $target: target } : {}),
      };
    }
    // A FUNCTION IS UNEVALUABLE, NOT UNSTRUCTURED. The value stays a function - nothing here pretends to
    // know what it returns at run time - but the paths it reads are published inline, and a dynamic import
    // inside it is resolved to the declaration it loads.
    if (ts.isArrowFunction(node) || ts.isFunctionExpression(node)) {
      const out = { $fn: node.getText() };
      const lazy = refs.lazyTarget(node);
      if (lazy !== null) out.$module = lazy;
      describeInto(out, node);
      return out;
    }
    if (ts.isConditionalExpression(node)) {
      return {
        $cond: node.condition.getText(),
        $then: evalNode(node.whenTrue),
        $else: evalNode(node.whenFalse),
        ...condRef(node.condition, 'ternary'),
        ...unresolved(node),
      };
    }
    if (ts.isCallExpression(node)) return evalCall(node);
    return unresolved(node);
  }

  /** A resolved value as a KEY: a string or a number, or the `value` a resolved access carries. */
  function computedKey(v) {
    if (typeof v === 'string' || typeof v === 'number') return String(v);
    if (v !== null && typeof v === 'object' && !Array.isArray(v)) {
      const inner = v.value;
      if (typeof inner === 'string' || typeof inner === 'number') return String(inner);
    }
    return null;
  }

  /**
   * What a property name SAYS - and a COMPUTED KEY IS ITS VALUE.
   *
   * `[Flags.IsBetaEnabled]: Groups.Beta` builds the property `IsBetaEnabled`, and
   * publishing the source text makes every consumer resolve the constant for itself, which it cannot do
   * without a compiler. Resolved through the FULL evaluator rather than a narrow constant reader, because
   * the constants a real application computes keys from are static class fields and const objects as often
   * as they are enums. Where the value is not knowable the text is kept, because that is all there is.
   */
  function propNameOf(name) {
    if (!name) return '?';
    if (ts.isIdentifier(name) || ts.isStringLiteral(name) || ts.isNumericLiteral(name)) return name.text;
    if (ts.isComputedPropertyName(name)) return computedKey(evalNode(name.expression)) ?? name.getText();
    return name.getText();
  }

  return {
    evalNode, follow, unresolved, propName: propNameOf,
    declTarget: refs.declTarget, valueDecl: refs.valueDecl,
  };
}
