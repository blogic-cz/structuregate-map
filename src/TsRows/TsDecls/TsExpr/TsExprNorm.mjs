/**
 * A TYPESCRIPT EXPRESSION AS THE SAME NORMALIZED TREE THE TEMPLATE SIDE EMITS, so ONE consumer reads both.
 *
 * Ported from the original Angular extractor. A component's template and its class say the
 * same kinds of thing about the same properties, and a consumer that had to learn two AST vocabularies to
 * follow one read across the boundary would learn neither well.
 *
 * KINDS WITH CHILDREN MUST NOT FALL TO THE `Other` LEAF. `Other` keeps text and does NOT recurse, so
 * `format(new Date(v), 'yyyy')` summarized to `[index, format]` and lost `v` entirely: the value was still
 * recorded in its own nested blob, but the flat summary a consumer joins on undercounted. Any node that
 * HOLDS an expression has to expose it as a field.
 *
 * Imports are flat: see `TsTypeRef.mjs`.
 */

export function makeTsExprNormalizer(ts, resolveRead, resolveName = null) {
  const outermost = (node) => !(node.parent !== undefined && ts.isPropertyAccessExpression(node.parent)
    && node.parent.expression === node);

  const norm = (node) => {
    if (node === undefined) return null;
    // Parentheses carry no meaning once the tree is explicit.
    if (ts.isParenthesizedExpression(node)) return norm(node.expression);
    if (ts.isNonNullExpression(node)) return { k: 'NonNull', expr: norm(node.expression) };
    if (ts.isAsExpression(node)) return norm(node.expression);
    // Old-style `<T>value` and `Foo<T>` are the same story as `as`: a type annotation wrapping a real
    // expression. Left on the `Other` leaf they hid their operand from the summary.
    if (ts.isTypeAssertionExpression(node)) return norm(node.expression);
    if (ts.isExpressionWithTypeArguments(node)) return norm(node.expression);
    if (ts.isIdentifier(node)) {
      // A BARE NAME THAT IS A MODULE'S DECLARATION IS RESOLVED LIKE A MEMBER. `isSpecial(id)` and
      // `SPECIAL_IDS.includes(id)` call a function and read a const that no `this.` reaches, so without
      // a target the condition they sit in names nothing a consumer can join. A local or a parameter gets
      // none: `resolveName` answers for module-level declarations only.
      const target = resolveName ? resolveName(node) : null;
      return { k: 'Read', name: node.text, receiver: { k: 'Implicit' }, ...(target ? { target } : {}) };
    }
    if (node.kind === ts.SyntaxKind.ThisKeyword) return { k: 'This' };
    if (ts.isStringLiteral(node) || ts.isNoSubstitutionTemplateLiteral(node)) {
      return { k: 'Literal', v: node.text };
    }
    if (ts.isNumericLiteral(node)) return { k: 'Literal', v: Number(node.text) };
    if (node.kind === ts.SyntaxKind.TrueKeyword) return { k: 'Literal', v: true };
    if (node.kind === ts.SyntaxKind.FalseKeyword) return { k: 'Literal', v: false };
    if (node.kind === ts.SyntaxKind.NullKeyword) return { k: 'Literal', v: null };
    if (ts.isPropertyAccessExpression(node)) {
      // `a?.b` is a distinct kind on the template side too - keep the distinction rather than flattening
      // it, because a safe read says something real about what the author expected to be absent.
      const safe = node.questionDotToken !== undefined;
      const target = resolveRead && outermost(node) ? resolveRead(node.name) : null;
      return {
        k: safe ? 'SafeRead' : 'Read', name: node.name.getText(), receiver: norm(node.expression),
        ...(target ? { target } : {}),
      };
    }
    if (ts.isElementAccessExpression(node)) {
      return { k: 'KeyedRead', receiver: norm(node.expression), key: norm(node.argumentExpression) };
    }
    if (ts.isCallExpression(node)) {
      return {
        k: node.questionDotToken !== undefined ? 'SafeCall' : 'Call',
        receiver: norm(node.expression),
        args: node.arguments.map((a) => norm(a)),
      };
    }
    if (ts.isBinaryExpression(node)) {
      return {
        k: 'Binary', op: node.operatorToken.getText(), left: norm(node.left), right: norm(node.right),
      };
    }
    if (ts.isConditionalExpression(node)) {
      return { k: 'Cond', cond: norm(node.condition), then: norm(node.whenTrue), else: norm(node.whenFalse) };
    }
    if (ts.isPrefixUnaryExpression(node)) {
      // `!x` is `Not` on the template side; every other prefix operator keeps its token.
      if (node.operator === ts.SyntaxKind.ExclamationToken) return { k: 'Not', expr: norm(node.operand) };
      return { k: 'Unary', op: ts.tokenToString(node.operator) ?? '', expr: norm(node.operand) };
    }
    if (ts.isArrayLiteralExpression(node)) return { k: 'Array', items: node.elements.map((e) => norm(e)) };
    if (ts.isObjectLiteralExpression(node)) return normObject(node);
    if (ts.isTemplateExpression(node)) {
      return {
        k: 'Interpolation',
        strings: [node.head.text, ...node.templateSpans.map((s) => s.literal.text)],
        expressions: node.templateSpans.map((s) => norm(s.expression)),
      };
    }
    if (ts.isNewExpression(node)) {
      return { k: 'New', receiver: norm(node.expression), args: (node.arguments ?? []).map((a) => norm(a)) };
    }
    if (ts.isAwaitExpression(node)) return { k: 'Await', expr: norm(node.expression) };
    if (ts.isTypeOfExpression(node)) return { k: 'Unary', op: 'typeof', expr: norm(node.expression) };
    if (ts.isDeleteExpression(node)) return { k: 'Unary', op: 'delete', expr: norm(node.expression) };
    if (ts.isVoidExpression(node)) return { k: 'Unary', op: 'void', expr: norm(node.expression) };
    if (ts.isPostfixUnaryExpression(node)) {
      return { k: 'Unary', op: ts.tokenToString(node.operator) ?? '', expr: norm(node.operand) };
    }
    if (ts.isTaggedTemplateExpression(node)) {
      return { k: 'Tagged', receiver: norm(node.tag), expr: norm(node.template) };
    }
    if (ts.isSpreadElement(node)) return { k: 'Spread', expr: norm(node.expression) };
    if (ts.isArrowFunction(node) || ts.isFunctionExpression(node)) return normFunction(node);
    // Anything with no mapping keeps its TS kind name and its text, so nothing is silently dropped.
    return { k: 'Other', kind: ts.SyntaxKind[node.kind], src: node.getText() };
  };

  /**
   * EVERY PROPERTY FORM IS A READ. Keeping only `PropertyAssignment` dropped the two that carry an
   * identifier without an explicit initializer: `{ createItem, itemType }` (shorthand - the value IS the
   * identifier) and `{ ...action.payload }` (spread), so a reducer's returned object did not summarize the
   * name its value depends on.
   */
  function normObject(node) {
    const keys = [];
    const values = [];
    for (const p of node.properties) {
      if (ts.isPropertyAssignment(p) && p.name !== undefined) {
        keys.push({ key: p.name.getText(), quoted: ts.isStringLiteral(p.name) });
        values.push(norm(p.initializer));
      } else if (ts.isShorthandPropertyAssignment(p)) {
        keys.push({ key: p.name.getText(), quoted: false });
        values.push({ k: 'Read', name: p.name.getText(), receiver: { k: 'Implicit' } });
      } else if (ts.isSpreadAssignment(p)) {
        keys.push({ key: '...', quoted: false });
        values.push(norm(p.expression));
      }
    }
    return { k: 'Map', keys, values };
  }

  /**
   * A CALLBACK'S BODY IS NOT A LEAF. Keeping only its source made every projector opaque: a selector whose
   * whole subject is the paths it reads published them ONLY inside a string, so `reads` was empty and the
   * content was reachable by matching text.
   *
   * WHAT IT RETURNS is the honest projection of a body - a concise arrow IS its expression, a block is
   * summarized by its `return`s. Nested functions are not walked here: each normalizes to its own `Fn` node
   * and carries its own returns, so nothing is lost and nothing is counted twice.
   */
  function normFunction(node) {
    const returns = [];
    const body = node.body;
    if (body !== undefined && !ts.isBlock(body)) {
      const one = norm(body);
      if (one) returns.push(one);
    } else if (body !== undefined) {
      const visit = (n) => {
        if (ts.isArrowFunction(n) || ts.isFunctionExpression(n) || ts.isFunctionDeclaration(n)) return;
        if (ts.isReturnStatement(n)) {
          const r = norm(n.expression);
          if (r) returns.push(r);
        }
        ts.forEachChild(n, visit);
      };
      ts.forEachChild(body, visit);
    }
    return returns.length ? { k: 'Fn', src: node.getText(), returns } : { k: 'Fn', src: node.getText() };
  }

  return norm;
}
