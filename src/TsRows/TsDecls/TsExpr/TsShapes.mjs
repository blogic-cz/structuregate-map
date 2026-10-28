/**
 * EVERY ANONYMOUS OBJECT TYPE IS A ROW (rows 71): a `shapes` row per TYPE LITERAL (`{ a: string }` in a type) and per OBJECT LITERAL
 * (`{ a: 1 }` in code), wherever it sits - a member, a parameter, a local, a return, an argument, a decorator, nested in another - and a
 * `shape_members` row per property, method, accessor, spread, index, call or construct signature in it.
 *
 * A `type_ref` to such a type has no name (`__type`, `__object`), so a template read chained through `labels.nested.deep` stopped at
 * the first literal and a source with thousands of them had no row for the declarations. A ref to a literal is `{shape: true, file, line,
 * col}` (`TsTypeRef.mjs::siteOf`), the place of its `{`, which is what a `shapes` row is keyed by: `TsTpl/TsTplReads.mjs` joins them.
 *
 * ONE SCAN PER FILE, over the whole tree of the file, so a literal no body walk reaches (a type, a top-level const) still has its row.
 * What the walk KNOWS about an object literal - its member, class, branch, case, loop and choices, the context a `calls` row carries -
 * is `remember`ed by the body walk and read here; a literal the walk never reached has none. A `.json` source holds data and a
 * declaration file holds types of a library: neither is read.
 */
import { locationOf } from './TsNodes.mjs';

export function makeShapeRows({ ts, store, typeRefAt, typeText, propName, returnRefOf, fieldTarget }) {
  const contexts = new WeakMap();

  /** WHERE THE BODY WALK STANDS at an object literal: the columns a `calls` row there would carry. */
  const remember = (node, context) => contexts.set(node, context);

  /** A property's name as the map spells every other (`members.name`): a computed key is its value, else its text. */
  const nameOf = (name) => {
    const value = propName(name);
    return typeof value === 'string' ? value : name.getText();
  };

  /** A type as its ANNOTATION when written, else what the checker infers - `members.type` for an un-annotated property. */
  const typed = (m, annotation) => ({
    type: annotation ? annotation.getText() : typeText(m, null),
    ...(annotation ? typeRefAt(annotation) : writtenElements(m) ?? typeRefAt(m)),
  });

  /** AN UN-ANNOTATED ARRAY LITERAL'S ELEMENT IS WHAT ITS ELEMENTS NAME (rows 123), each as written, one per place and sorted by it: the checker's
   *  element type is a union it REDUCES - two structurally identical classes (`typeof AModule | typeof BModule`) keep whichever it interned
   *  first, which a full and a partial run do not agree on - so the `element` it named moved with no source change. Null when the literal
   *  is no array of an array type, and the checker's answer stands. */
  const writtenElements = (m) => {
    if (!ts.isPropertyAssignment(m) || !ts.isArrayLiteralExpression(m.initializer)) return null;
    const base = typeRefAt(m);
    if (!base?.type_ref || base.type_ref.element === undefined) return null;
    const seen = new Map();
    for (const e of m.initializer.elements) {
      // AN ELEMENT WRITTEN IN PLACE (`{ .. }`, `[ .. ]`) is typed by the literal the checker merges, which a loop reads: left to it.
      if (ts.isSpreadElement(e) || ts.isOmittedExpression(e) || ts.isObjectLiteralExpression(e) || ts.isArrayLiteralExpression(e)) return null;
      const ref = typeRefAt(e)?.type_ref;
      if (!ref) continue;
      seen.set(`${ref.file ?? ''}\0${String(ref.line ?? '').padStart(9, '0')}\0${ref.name ?? ''}`, ref);
    }
    const refs = [...seen.keys()].sort().map((k) => seen.get(k));
    const element = refs.length === 1 ? refs[0] : refs.length > 1 ? { type_refs: refs, type_refs_of: 'or' } : null;
    // `unwrapped` is the same element for an array (`TsTypeRef.mjs::unwrappedOf`): both are the written elements, or neither is published.
    const { element: _, unwrapped: had, ...rest } = base.type_ref;
    return { ...base, type_ref: element === null ? rest : { ...rest, ...(had === undefined ? {} : { unwrapped: element }), element } };
  };
  /** THE DECLARED MEMBER A LITERAL'S PROPERTY FILLS (rows 128, `TsReadRefs.mjs::fieldTarget`): `{ list: [..] }` typed `State` names `State`'s `list` signature, the
   *  place a template read of `state.list` lands on - so the closure reads the value written there (`shape_select.rs`). None without a contextual type. */
  const fills = (m) => {
    try { const target = fieldTarget?.(m.initializer); return target ? { target } : {}; } catch { return {}; }
  };
  /** A signature's type is only its annotation: an un-annotated one is `any` and names nothing. */
  const written = (annotation) => ({ type: annotation ? annotation.getText() : null, ...(annotation ? typeRefAt(annotation) : { type_ref: null }) });

  /** One member of a literal, as `{kind, name, ...}`, or null for a node that is none (never: every kind of member is listed). */
  function memberOf(m) {
    if (ts.isSpreadAssignment(m)) return { kind: 'spread', name: m.expression.getText(), type: typeText(m.expression, null), ...typeRefAt(m.expression) };
    if (ts.isIndexSignatureDeclaration(m)) {
      const key = m.parameters[0];
      return { kind: 'index', name: `[${key ? key.name.getText() : ''}]`, key_type: key?.type ? key.type.getText() : null, ...written(m.type) };
    }
    if (ts.isCallSignatureDeclaration(m)) return { kind: 'call', name: '()', ...written(m.type), ...returnRefOf(m, 'method') };
    if (ts.isConstructSignatureDeclaration(m)) return { kind: 'construct', name: 'new', ...written(m.type), ...returnRefOf(m, 'method') };
    if (!m.name) return null;
    const name = ts.isShorthandPropertyAssignment(m) ? m.name.text : nameOf(m.name);
    if (ts.isMethodSignature(m) || ts.isMethodDeclaration(m)) return { kind: 'method', name, ...written(m.type), ...returnRefOf(m, 'method') };
    if (ts.isGetAccessor(m)) return { kind: 'getter', name, ...typed(m, m.type) };
    if (ts.isSetAccessor(m)) return { kind: 'setter', name, ...typed(m, m.type) };
    if (ts.isShorthandPropertyAssignment(m)) return { kind: 'shorthand', name, ...typed(m, null), ...returnRefOf(m, 'property') };
    if (ts.isPropertySignature(m)) return { kind: 'signature', name, ...written(m.type), ...returnRefOf(m, 'property') };
    if (ts.isPropertyAssignment(m)) return { kind: 'property', name, ...typed(m, null), ...returnRefOf(m, 'property'), ...fills(m) };
    return null;
  }

  function shapeRows(node) {
    const isType = ts.isTypeLiteralNode(node);
    const id = store.add('shapes', 'shp', { kind: isType ? 'type' : 'object', ...(contexts.get(node) ?? {}), ...locationOf(node) });
    for (const m of isType ? node.members : node.properties) {
      let row = null;
      try { row = memberOf(m); } catch { row = null; }
      if (row) store.add('shape_members', 'sm', { shape: id, optional: !!m.questionToken, ...row, ...locationOf(m) });
    }
  }

  /** Every literal of one source file, in source order (a literal before the ones it holds). */
  function scan(sf) {
    if (sf.isDeclarationFile || sf.fileName.toLowerCase().endsWith('.json')) return;
    const visit = (node) => {
      if (ts.isTypeLiteralNode(node) || ts.isObjectLiteralExpression(node)) shapeRows(node);
      ts.forEachChild(node, visit);
    };
    visit(sf);
  }

  return { remember, scan };
}
