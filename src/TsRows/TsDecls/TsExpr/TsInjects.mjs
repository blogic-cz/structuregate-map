/**
 * EVERY `inject()` IS A DEPENDENCY OF WHAT CALLS IT, and a `di` row says WHICH (rows 78).
 *
 * A FIELD THAT IS THE CALL (`x = inject(T)`, through `as`, `<T>`, parentheses and `!`) is a dependency NAMED `x`: its row has `param: x`.
 * A call NESTED deeper (`x = (inject(T) as IConfig).x`, `inject(T).y`, `inject(T) ?? z`) makes `x` a COPY of something the injected object
 * holds - not a dependency called `x` - so the class has ONE row per distinct token it injects that way, `param` null and `via` the first field
 * that did, and none at all where a field already injects that token (the same dependency listed twice says nothing more).
 * An `inject()` in a FUNCTION that is no class member (a functional interceptor or guard, a provider's factory) is a row with `function`, the
 * `functions` row it sits in, and `param` the local's name when the call IS the local's initializer. A METHOD body is no injection context: no row.
 *
 * THE INJECTED VALUE IS TYPED AS WRITTEN: the field's (or the local's) annotation, else the cast around the call, else the call's own type.
 * `y: ITranslate = inject(TOKEN) as ITranslate` is an `ITranslate` - the token's `InjectionToken<unknown>` says nothing about it, as a constructor's
 * `@Inject(TOKEN) y: ITranslate` does not. `token` and `token_ref` stay the call's argument.
 *
 * Imports are flat: see `TsTypeRef.mjs`.
 */
import { locationOf } from './TsNodes.mjs';

export function makeInjectRows({ ts, checker, store, typeRefAt, typeText, declFileOf, declFileOfSymbol, resolvedOf, propName, fromAngularCore }) {
  /** Whether a node is a call of Angular's `inject` - by the declaration it resolves to, never the name alone. */
  const isInject = (n) => ts.isCallExpression(n) && ts.isIdentifier(n.expression) && n.expression.text === 'inject' && fromAngularCore(n.expression);

  /** What only TYPES an expression or groups it: `(x as T)`, `<T>x`, `x!`, `x satisfies T`. */
  const isWrapper = (n) => ts.isParenthesizedExpression(n) || ts.isAsExpression(n) || ts.isTypeAssertionExpression(n)
    || ts.isNonNullExpression(n) || (ts.isSatisfiesExpression !== undefined && ts.isSatisfiesExpression(n));

  const unwrap = (n) => {
    let at = n;
    while (isWrapper(at)) at = at.expression;
    return at;
  };

  /** EVERY `inject(...)` OF A PROPERTY INITIALIZER, in source order. A function-valued initializer is a deferred body, run in whatever context calls it,
   *  so it is not walked: `lazy = () => inject(X)` is not a dependency of the class. */
  const callsOf = (init) => {
    const out = [];
    const visit = (n) => {
      if (ts.isArrowFunction(n) || ts.isFunctionExpression(n) || ts.isClassExpression(n)) return;
      if (isInject(n)) out.push(n);
      ts.forEachChild(n, visit);
    };
    visit(init);
    return out;
  };

  /** The type a cast around the call WRITES - the outermost one (`inject(T) as unknown as I` is an `I`) - or null. `as const` writes none. */
  const castOf = (call) => {
    let cast = null;
    for (let at = call; at.parent && isWrapper(at.parent) && at.parent.expression === at; at = at.parent) {
      const type = ts.isAsExpression(at.parent) || ts.isTypeAssertionExpression(at.parent) ? at.parent.type : null;
      if (type && !(ts.isTypeReferenceNode(type) && type.typeName.getText() === 'const')) cast = type;
    }
    return cast;
  };

  /** `type`, `resolved` and `type_ref` of the injected value: the written type node when there is one, else what the checker says of the call. */
  const factsOf = (call, written) => {
    if (written) return { type: written.getText(), resolved: resolvedOf(written), type_ref: null, ...typeRefAt(written) };
    const t = checker.getTypeAtLocation(call);
    return { type: typeText(call, null), resolved: declFileOfSymbol(t.aliasSymbol ?? t.getSymbol()), type_ref: typeRefAt(call).type_ref ?? null };
  };

  const optionalOf = (call) => {
    const opts = call.arguments[1];
    return !!opts && ts.isObjectLiteralExpression(opts) && opts.properties.some((pr) =>
      ts.isPropertyAssignment(pr) && propName(pr.name) === 'optional' && pr.initializer.kind === ts.SyntaxKind.TrueKeyword);
  };

  const rowOf = (call, written, owner) => {
    const tokenArg = call.arguments[0];
    return {
      ...owner, inject: true, ...factsOf(call, written), decorators: [], optional: optionalOf(call),
      ...(tokenArg ? { token: tokenArg.getText(), token_ref: declFileOf(tokenArg) } : {}),
    };
  };

  /** One class's `inject()` fields: `field` per member with an initializer, `done` once the members are all seen (it writes the nested ones). */
  function forClass(classId, fid, contextOf) {
    const direct = new Set();
    const nested = new Map();
    let templateRef = false;
    return {
      field(m, init, memberId) {
        for (const call of callsOf(init)) {
          const token = call.arguments[0]?.getText() ?? '';
          if (call !== unwrap(init)) {
            if (!nested.has(token)) nested.set(token, { call, via: propName(m.name), member: memberId });
            continue;
          }
          direct.add(token);
          const row = rowOf(call, m.type ?? castOf(call), { class: classId, file: fid, param: propName(m.name), member: memberId });
          templateRef = templateRef || row.type_ref?.name === 'TemplateRef' && String(row.type_ref.file ?? '').includes('@angular/core');
          store.add('di', 'd', { ...row, ...contextOf(m) });
        }
      },
      /** Whether any of the class's `di` rows is Angular's `TemplateRef` (what makes a directive structural). */
      done() {
        for (const [token, { call, via, member }] of nested) {
          if (direct.has(token)) continue;
          const row = rowOf(call, castOf(call), { class: classId, file: fid, param: null, via, member });
          templateRef = templateRef || row.type_ref?.name === 'TemplateRef' && String(row.type_ref.file ?? '').includes('@angular/core');
          store.add('di', 'd', row);
        }
        return templateRef;
      },
    };
  }

  /** An `inject()` call met in a body outside any class (`ownerId` the `functions` row it sits in): a `di` row of that function. */
  function atFunction(call, ownerId, fid) {
    if (!isInject(call) || ownerId === null || ownerId === undefined) return;
    let at = call;
    while (at.parent && isWrapper(at.parent) && at.parent.expression === at) at = at.parent;
    const decl = at.parent && ts.isVariableDeclaration(at.parent) && at.parent.initializer === at && ts.isIdentifier(at.parent.name) ? at.parent : null;
    const row = rowOf(call, decl?.type ?? castOf(call), { class: null, file: fid, function: ownerId, param: decl ? decl.name.text : null });
    store.add('di', 'd', { ...row, ...locationOf(call) });
  }

  return { forClass, atFunction };
}
