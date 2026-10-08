/**
 * THE PROPERTY A KEY OF A RETURNED OBJECT LITERAL SETS, resolved through the literal's CONTEXTUAL type.
 *
 * Ported from the original Angular extractor. The write side of the map published bare
 * names while the read side resolved to a declaration, so "who sets this property" stayed a name match
 * inside whichever file happened to hold both ends. The checker knows what the literal is being assigned
 * or returned INTO, and that type declares the property - so a write can point at exactly the row a read
 * points at.
 *
 * Only the keys of the literal itself are resolved, one level at a time, mirroring how the written paths
 * are built: a nested literal is resolved against the property type of the key that contains it.
 *
 * Imports are flat: see `TsTypeRef.mjs`.
 */
import { canonicalPath } from './TsPaths.mjs';

function declSite(d) {
  const sf = d.getSourceFile();
  const pos = d.name && typeof d.name.getStart === 'function' ? d.name.getStart() : d.getStart();
  return { file: canonicalPath(sf.fileName), line: sf.getLineAndCharacterOfPosition(pos).line + 1 };
}

export function makeWriteResolver(ts, checker) {
  /** `({...} as Foo)` and `({...})` wrap the literal; the literal is what carries the keys. */
  const unwrap = (n) => {
    let current = n;
    for (;;) {
      if (ts.isParenthesizedExpression(current)) { current = current.expression; continue; }
      if (ts.isAsExpression(current)) { current = current.expression; continue; }
      return current;
    }
  };

  /** The object literals a node PRODUCES: itself, or what its `return`s produce. Nested functions are left
   *  to their own resolution, exactly as the written paths are built. */
  const produced = (node) => {
    if (ts.isObjectLiteralExpression(node)) return [node];
    if (!ts.isArrowFunction(node) && !ts.isFunctionExpression(node)) return [];
    const body = node.body;
    if (body !== undefined && !ts.isBlock(body)) {
      const inner = unwrap(body);
      return ts.isObjectLiteralExpression(inner) ? [inner] : [];
    }
    const out = [];
    const visit = (n) => {
      if (ts.isArrowFunction(n) || ts.isFunctionExpression(n) || ts.isFunctionDeclaration(n)) return;
      if (ts.isReturnStatement(n) && n.expression) {
        const e = unwrap(n.expression);
        if (ts.isObjectLiteralExpression(e)) out.push(e);
      }
      ts.forEachChild(n, visit);
    };
    if (body !== undefined) ts.forEachChild(body, visit);
    return out;
  };

  const resolveLiteral = (literal, out) => {
    let target;
    try {
      target = checker.getContextualType(literal);
      // A TYPE ASSERTION IS A DECLARED TYPE, and contextual typing does not see through one: for
      // `({...}) as Foo` the checker reports no contextual type at all, which left the most common way a
      // codebase names a literal's shape unresolved - about half of the literals that produced no refs.
      if (!target) {
        const parent = literal.parent;
        const assertion = parent !== undefined && ts.isAsExpression(parent) ? parent
          : parent !== undefined && ts.isParenthesizedExpression(parent)
            && parent.parent !== undefined && ts.isAsExpression(parent.parent) ? parent.parent : null;
        if (assertion) target = checker.getTypeFromTypeNode(assertion.type);
      }
    } catch {
      return;
    }
    if (!target) return;

    const walk = (node, type, prefix) => {
      for (const p of node.properties) {
        if (!p.name) continue;
        const key = ts.isStringLiteral(p.name) || ts.isIdentifier(p.name) ? p.name.text : p.name.getText();
        const prop = type.getProperty(key);
        const decls = prop?.declarations ?? [];
        const path = prefix ? `${prefix}.${key}` : key;
        // A declaration INSIDE the literal being resolved is the literal's OWN anonymous type describing
        // itself - the checker's honest answer when nothing constrains it, and a useless one to publish:
        // an audit found about half of write refs pointing at the write's own line. A write ref must
        // name a declaration that exists independently of the write.
        const external = decls.filter((d) => !(d.getSourceFile() === literal.getSourceFile()
          && d.getStart() >= literal.getStart() && d.getEnd() <= literal.getEnd()));
        if (prop && external.length) {
          // The checker's order is kept - see the note in `TsMap`'s describer.
          const places = external.map(declSite);
          const [first, ...rest] = places;
          out.set(path, { name: key, ...first, ...(rest.length ? { also: rest } : {}) });
        }
        // A nested literal is resolved against the property's OWN type, so a nested path resolves to the
        // declaration that actually holds it rather than to its parent.
        if (prop && ts.isPropertyAssignment(p) && ts.isObjectLiteralExpression(p.initializer)) {
          const nested = checker.getTypeOfSymbolAtLocation(prop, p.initializer);
          if (nested) walk(p.initializer, nested, path);
        }
      }
    };
    walk(literal, target, '');
  };

  return (node) => {
    const out = new Map();
    for (const literal of produced(node)) resolveLiteral(literal, out);
    return out;
  };
}
