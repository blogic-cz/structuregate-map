/**
 * WHAT A FILE DECLARES AT ITS TOP LEVEL, other than types: the consts, and what each one is worth.
 *
 * Ported from the original Angular extractor. Imports are flat: see `TsTypeRef.mjs`.
 */
import { locationOf } from './TsNodes.mjs';

export function makeDeclRows({ ts, store, modifiersOf, localExports, evalCell, paramsOf, typeText,
  jsdocOf, collectBody, collectExpr, resolveRead, resolveMember, bindingFacts }) {
  /**
   * One row per declared name - `const a = 1, b = 2` is two consts, not one statement.
   *
   * A VARIABLE STATEMENT HAS NO NAME OF ITS OWN, so the `export { X }` form at the bottom of a file cannot
   * be answered for the statement: the names are one level down, and the same question is asked per
   * declaration.
   */
  function constRows(statement, fid) {
    const exported = modifiersOf(statement).exported;
    const named = localExports(statement.getSourceFile());
    for (const d of statement.declarationList.declarations) {
      // A DESTRUCTURING DECLARES EACH NAME IT BINDS (rows 76), not the pattern text: `export const { DemoActions, DemoSelectors } = makeSlice()` is one value-less row per bound name, at
      // the NAME's line and column, which is what a read's `target` anchors at (`TsReadRefs.mjs::at`). A row named by the pattern joined no read, and carrying the initializer's value
      // would let a gate reader take the whole initializer for the name's value. The initializer is walked once, below.
      if (!ts.isIdentifier(d.name)) {
        const names = [];
        const bound = (pattern) => {
          pattern.elements.forEach((el, at) => {
            if (ts.isOmittedExpression(el)) return;
            if (ts.isIdentifier(el.name)) names.push({ el, at: ts.isArrayBindingPattern(pattern) ? at : null }); else bound(el.name);
          });
        };
        bound(d.name);
        // `type`, `default` and `index` (rows 77, `TsExpr/TsBindings.mjs`) are what the NAME is - the pattern's initializer is still no value of it.
        for (const { el, at } of names) {
          const text = el.name.text;
          store.add('consts', 'k', {
            file: fid, name: text, exported: exported || named.has(text), destructured: true,
            value: undefined, ...bindingFacts(el, at), ...locationOf(el),
          });
        }
        if (d.initializer) collectExpr(d.initializer, null, null);
        continue;
      }
      const name = d.name.getText();
      store.add('consts', 'k', {
        file: fid,
        name,
        exported: exported || named.has(name),
        type: d.type ? d.type.getText() : null,
        value: d.initializer ? evalCell(d.initializer) : undefined,
        ...locationOf(d),
      });
      // A FUNCTION-VALUED CONST IS A FUNCTION. `export const initItems = (...) => {...}` is how many helpers
      // in a codebase are written, and recording it only as a const left its body unwalked: a caller could
      // resolve the call's target to the declaration and then find no `returns` row for it, so the value
      // chain dead-ended at exactly the shape it needed to follow.
      const init = d.initializer;
      if (init && (ts.isArrowFunction(init) || ts.isFunctionExpression(init))) {
        const fnId = store.add('functions', 'fn', {
          file: fid, name, exported,
          form: ts.isArrowFunction(init) ? 'arrow' : 'function-expression',
          params: paramsOf(init), returns: init.type ? init.type.getText() : typeText(init),
          jsdoc: jsdocOf(d), ...locationOf(d),
        });
        if (init.body) collectBody(init.body, fnId, null);
        for (const p of init.parameters ?? []) if (p.initializer) collectExpr(p.initializer, fnId, null);
      } else if (init) collectExpr(init, null, null);
    }
  }

  /** A top-level `function` declaration, and its body walked under it. */
  function functionRow(statement, fid) {
    const fnId = store.add('functions', 'fn', {
      file: fid, name: statement.name?.text ?? '(anonymous)',
      exported: modifiersOf(statement).exported,
      params: paramsOf(statement),
      returns: statement.type ? statement.type.getText() : typeText(statement),
      jsdoc: jsdocOf(statement), ...locationOf(statement),
    });
    if (statement.body) collectBody(statement.body, fnId, null);
    for (const p of statement.parameters ?? []) if (p.initializer) collectExpr(p.initializer, fnId, null);
    return fnId;
  }

  /**
   * AN AUGMENTATION'S BODY - `Array.prototype.overlaps = function (other) { ... }` - as a function row named by
   * what it assigns, with the `target` a CALL of the method resolves to: the checker gives the assigned name and
   * every `xs.overlaps(..)` the same declaration (the `declare global` signature), so that is the join, never the
   * name. Only a `function` expression: an arrow's `this` is not the receiver. The same assignment under
   * `if (!Array.prototype.overlaps)` (no `else`; the guard names what it assigns) defines it too, and so does
   * `Object.defineProperty(Array.prototype, 'overlaps', { value: function ... })`.
   */
  function prototypeRow(statement, fid) {
    const inner = guarded(statement);
    const found = definitionOf(inner);
    if (!found) return;
    const { name, target, fn } = found;
    const fnId = store.add('functions', 'fn', {
      file: fid, name, exported: false, form: 'prototype-method',
      params: paramsOf(fn), returns: fn.type ? fn.type.getText() : typeText(fn),
      ...(target ? { target } : {}), ...locationOf(inner),
    });
    // THE BODY IS WALKED WHERE THE STATEMENT'S WALK MEETS THE FUNCTION (rows 80, `TsBody.mjs::collectBody`'s `skip`), not here: under `if (!X.prototype.m)` the walk is inside the
    // `then` arm, so its calls, returns and locals carry that `branch` as any function's body does - walked from here they had none. The row stays the method's own (detached).
    let done = false;
    return { node: fn, walk() { if (done) return; done = true; if (fn.body) collectBody(fn.body, fnId, null); } };
  }

  const isPrototype = (n) => ts.isPropertyAccessExpression(n) && n.name.text === 'prototype';

  /** What one statement defines on a prototype: its name, the member it resolves to, and the function. */
  function definitionOf(statement) {
    if (!statement || !ts.isExpressionStatement(statement)) return null;
    const e = statement.expression;
    if (ts.isBinaryExpression(e) && e.operatorToken.kind === ts.SyntaxKind.EqualsToken) {
      const { left, right } = e;
      if (!ts.isPropertyAccessExpression(left) || !isPrototype(left.expression) || !ts.isFunctionExpression(right)) return null;
      return { name: left.getText(), target: resolveRead(left.name), fn: right };
    }
    if (!ts.isCallExpression(e) || e.expression.getText() !== 'Object.defineProperty') return null;
    const [on, key, descriptor] = e.arguments;
    if (!on || !isPrototype(on) || !key || !ts.isStringLiteral(key) || !descriptor
      || !ts.isObjectLiteralExpression(descriptor)) return null;
    const value = descriptor.properties.find((p) => ts.isPropertyAssignment(p) && p.name.getText() === 'value');
    if (!value || !ts.isFunctionExpression(value.initializer)) return null;
    return { name: `${on.getText()}.${key.text}`, target: resolveMember(on, key.text), fn: value.initializer };
  }

  /** `if (!X.prototype.m) { <one statement defining X.prototype.m> }` as that statement; any other `if` as nothing. */
  function guarded(statement) {
    if (!ts.isIfStatement(statement)) return statement;
    const { expression: cond, thenStatement: then } = statement;
    const inner = ts.isBlock(then) ? (then.statements.length === 1 ? then.statements[0] : null) : then;
    if (statement.elseStatement || !inner || !ts.isPrefixUnaryExpression(cond)
      || cond.operator !== ts.SyntaxKind.ExclamationToken) return null;
    return definitionOf(inner)?.name === cond.operand.getText() ? inner : null;
  }

  return { constRows, functionRow, prototypeRow };
}
