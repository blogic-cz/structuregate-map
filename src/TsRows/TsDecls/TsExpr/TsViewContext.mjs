/**
 * THE CONTEXT A DIRECTIVE HANDS THE VIEW IT RENDERS, when it builds it in CODE (rows 75): `this.vcRef.createEmbeddedView(this.templateRef, { rule: this.rule })`.
 *
 * `inject(TemplateRef)` and `TemplateRef<any>` declare no context type, yet the object the directive passes IS the context its template's `let-x` and `*dir="x as y"`
 * variables read - an object literal (a `shapes` row), a local typed by a literal, or a member. `TsTpl/TsTplContext.mjs` types the variables from it when the `TemplateRef`
 * says nothing: `di.context_ref`, the `type_ref` of that argument, on the `di` row of the `TemplateRef` the call renders.
 *
 * WHICH CALL IS ONE is the declaration the checker resolved - `ViewContainerRef.createEmbeddedView` in Angular's own package - and which `TemplateRef` it renders is
 * the declaration the first argument names (a property or a parameter property), never a name. Several calls of one class that hand different types name no one context.
 *
 * Imports are flat: see `TsTypeRef.mjs`.
 */

const ANGULAR_CORE = '@angular/core';

export function makeViewContexts(ts, checker, typeRefAt) {
  /** Whether a call is Angular's `createEmbeddedView`: its resolved signature is declared by that name in `@angular/core`. */
  const isEmbeddedView = (call) => {
    const callee = call.expression;
    if (!ts.isPropertyAccessExpression(callee) || callee.name.text !== 'createEmbeddedView') return false;
    let declaration;
    try { declaration = checker.getResolvedSignature(call)?.declaration; } catch { return false; }
    return declaration !== undefined && declaration.getSourceFile().fileName.includes(ANGULAR_CORE);
  };

  /** The declaration nodes of the property or parameter the first argument names (`this.tpl`, `tpl`). */
  const declarationsOf = (arg) => {
    const name = ts.isPropertyAccessExpression(arg) ? arg.name : ts.isIdentifier(arg) ? arg : null;
    return name === null ? [] : (checker.getSymbolAtLocation(name)?.declarations ?? []);
  };

  /** For a class: declaration node of a rendered `TemplateRef` -> the `type_ref` of the context its calls hand it (absent where they hand none, or differing ones). */
  return function viewContextsOf(classNode) {
    const found = new Map();
    const visit = (node) => {
      if (ts.isCallExpression(node) && node.arguments.length >= 2 && isEmbeddedView(node)) {
        const { type_ref: ref } = typeRefAt(node.arguments[1]);
        for (const d of declarationsOf(node.arguments[0])) {
          if (!found.has(d)) found.set(d, []);
          found.get(d).push(ref ?? null);
        }
      }
      ts.forEachChild(node, visit);
    };
    visit(classNode);
    const out = new Map();
    for (const [d, refs] of found) {
      const distinct = new Set(refs.map((r) => JSON.stringify(r)));
      if (distinct.size === 1 && refs[0] !== null) out.set(d, refs[0]);
    }
    return out;
  };
}
