/**
 * THE PIPES A LIBRARY ENTRY OF AN `imports` OR `exports` LIST MAKES VISIBLE - read off its `.d.ts`, as Angular's own
 * compiler reads it (#86).
 *
 * A pipe in `node_modules` has no `pipes` row: its typings carry no decorator, only the metadata the library was
 * compiled to - `static ɵpipe: ɵɵPipeDeclaration<DatePipe, "date", true>` names the pipe, and an NgModule's
 * `static ɵmod: ɵɵNgModuleDeclaration<M, Decls, Imports, Exports>` lists what it exports as `typeof X` tuple elements.
 * So `CommonModule` in a module's `imports` makes `date` visible to that module's templates, and the template pipe
 * rollup (`TsTpl/TsPipeRefs.mjs`) can say a usage resolves to a LIBRARY pipe instead of to nothing.
 *
 * Read from the TYPE NODES of the declaration, every symbol through the checker: an exported module re-exporting another
 * is followed to its end, a class already stood on stops the walk. A list entry is followed as the compiler does - an
 * array, a spread, a `const` holding a list, and a `forRoot()`-style call by the `ModuleWithProviders<M>` it returns.
 * Only a library class is answered here; an in-tree one is the scope rollup's, from its own rows.
 *
 * Imports are flat: see `TsTypeRef.mjs`.
 */
import { canonicalPath, relativeTo, slash } from './TsPaths.mjs';
import { locationOf } from './TsNodes.mjs';

export function makeLibPipes({ ts, checker, symbolAt, feRoot, propName }) {
  const inLibrary = (decl) => decl.getSourceFile().isDeclarationFile
    && slash(decl.getSourceFile().fileName).includes('/node_modules/');
  const classOf = (sym) => (sym?.declarations ?? []).find((d) => ts.isClassDeclaration(d)) ?? null;

  /** The type arguments of a class's `static <name>: X<...>` member, or null. */
  const staticArgs = (cls, name) => {
    for (const m of cls.members) {
      if (!ts.isPropertyDeclaration(m) || !m.name || propName(m.name) !== name) continue;
      if (!(ts.getCombinedModifierFlags(m) & ts.ModifierFlags.Static)) continue;
      return m.type && ts.isTypeReferenceNode(m.type) ? [...(m.type.typeArguments ?? [])] : null;
    }
    return null;
  };

  // ONE ANSWER PER CLASS: `CommonModule` sits in most modules' `imports`.
  const memo = new Map();
  /** Every pipe a library class IS (`ɵpipe`) or EXPORTS (`ɵmod`, through re-exported modules to their end). */
  const pipesOfClass = (cls, seen = new Set()) => {
    if (memo.has(cls)) return memo.get(cls);
    if (seen.has(cls)) return [];
    seen.add(cls);
    const out = [];
    const pipe = staticArgs(cls, 'ɵpipe');
    const named = pipe?.[1];
    if (named && ts.isLiteralTypeNode(named) && ts.isStringLiteral(named.literal) && cls.name) {
      out.push({ name: named.literal.text, class: cls.name.text,
        file: relativeTo(feRoot, canonicalPath(cls.getSourceFile().fileName)), line: locationOf(cls).line });
    }
    const exported = staticArgs(cls, 'ɵmod')?.[3];
    if (exported && ts.isTupleTypeNode(exported)) {
      for (const el of exported.elements) {
        if (!ts.isTypeQueryNode(el)) continue;
        const at = ts.isQualifiedName(el.exprName) ? el.exprName.right : el.exprName;
        const next = classOf(symbolAt(at));
        if (next && inLibrary(next)) out.push(...pipesOfClass(next, seen));
      }
    }
    memo.set(cls, out);
    return out;
  };

  /** The class a `forRoot()`-style call hands over: `M` of the `ModuleWithProviders<M>` it returns, or null. */
  const providedModule = (call) => {
    try {
      const type = checker.getTypeAtLocation(call);
      const sym = type?.aliasSymbol ?? type?.getSymbol();
      if (sym?.getName() !== 'ModuleWithProviders') return null;
      const [first] = type.aliasTypeArguments ?? checker.getTypeArguments(type) ?? [];
      return classOf(first?.getSymbol());
    } catch {
      return null;
    }
  };

  /** Every class an `imports`/`exports` value names, followed as the compiler flattens it. */
  const classesIn = (expr, seen, out) => {
    let e = expr;
    while (e && (ts.isParenthesizedExpression(e) || ts.isAsExpression(e) || ts.isNonNullExpression(e)
      || (ts.isSatisfiesExpression && ts.isSatisfiesExpression(e)))) e = e.expression;
    if (!e || seen.has(e)) return;
    seen.add(e);
    if (ts.isArrayLiteralExpression(e)) {
      for (const el of e.elements) classesIn(ts.isSpreadElement(el) ? el.expression : el, seen, out);
    } else if (ts.isCallExpression(e)) {
      const cls = providedModule(e);
      if (cls) out.push(cls);
    } else if (ts.isIdentifier(e) || ts.isPropertyAccessExpression(e)) {
      const sym = symbolAt(ts.isPropertyAccessExpression(e) ? e.name : e);
      const cls = classOf(sym);
      if (cls) out.push(cls);
      else {
        const held = (sym?.declarations ?? []).find((d) => ts.isVariableDeclaration(d) && d.initializer);
        if (held) classesIn(held.initializer, seen, out);
      }
    }
  };

  /**
   * `external_pipes` of a `@Component`/`@NgModule` class: `{name, class, file, line, from}` per pipe a LIBRARY entry of
   * its `imports` (`from: 'imports'`) or `exports` (`from: 'exports'`) makes visible, in list order; undefined when none.
   */
  return (node, decorators) => {
    const out = [];
    const seenPipe = new Set();
    const list = ts.canHaveDecorators(node) ? (ts.getDecorators(node) ?? []) : [];
    for (const [at, d] of list.entries()) {
      const name = decorators[at]?.name;
      if ((name !== 'Component' && name !== 'NgModule') || !decorators[at]?.fromAngular) continue;
      const arg = ts.isCallExpression(d.expression) ? d.expression.arguments[0] : undefined;
      if (!arg || !ts.isObjectLiteralExpression(arg)) continue;
      for (const p of arg.properties) {
        if (!ts.isPropertyAssignment(p) || !p.name) continue;
        const from = propName(p.name);
        if (from !== 'imports' && (from !== 'exports' || name !== 'NgModule')) continue;
        const classes = [];
        classesIn(p.initializer, new Set(), classes);
        for (const cls of classes) {
          if (!inLibrary(cls)) continue;
          for (const pipe of pipesOfClass(cls)) {
            const key = `${from}\u0000${pipe.file}\u0000${pipe.line}\u0000${pipe.name}`;
            if (seenPipe.has(key)) continue;
            seenPipe.add(key);
            out.push({ ...pipe, from });
          }
        }
      }
    }
    return out.length ? out : undefined;
  };
}
