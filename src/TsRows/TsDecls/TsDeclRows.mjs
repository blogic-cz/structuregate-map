/**
 * WHAT A FILE DECLARES AT ITS TOP LEVEL, other than types: the consts, and what each one is worth.
 *
 * Ported from the original Angular extractor. Imports are flat: see `TsTypeRef.mjs`.
 */
import { locationOf } from './TsNodes.mjs';

export function makeDeclRows({ ts, store, modifiersOf, localExports, evalNode, paramsOf, typeText,
  jsdocOf, collectBody }) {
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
      const name = d.name.getText();
      store.add('consts', 'k', {
        file: fid,
        name,
        exported: exported || named.has(name),
        type: d.type ? d.type.getText() : null,
        value: d.initializer ? evalNode(d.initializer) : undefined,
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
      } else if (init) collectBody(init, null, null);
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
    return fnId;
  }

  return { constRows, functionRow };
}
