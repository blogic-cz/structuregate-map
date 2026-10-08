/**
 * WHICH DECLARED INPUT A TEMPLATE BINDING ACTUALLY FILLS - and how many bindings each input has.
 *
 * Ported from the original Angular extractor. Imports are flat: see
 * `TsDecls/TsTypeRef.mjs`.
 */
import { slash } from './TsPaths.mjs';

const str = (v) => (typeof v === 'string' ? v : null);

export function rollupInputUsage(store) {
  const outOfScope = new Set();
  const byClassAndName = new Map();
  const declaredInputs = [];
  for (const row of store.table('io')) {
    if (row.kind !== 'input') continue;
    declaredInputs.push(row);
    const bindingName = str(row.binding_name) ?? str(row.alias) ?? str(row.member);
    if (bindingName !== null) byClassAndName.set(`${String(row.class)}#${bindingName}`, row);
  }

  /**
   * EVERY CLASS AN ELEMENT INSTANTIATES - the component AND every directive on it.
   *
   * Restricting this to components meant a directive's inputs could never be counted: about a fifth of the inputs
   * reported as "never bound" were declared on directive-only classes, and most of those are bound
   * in a template somewhere.
   *
   * SCOPE DECIDES WHICH CANDIDATE when a selector matches two classes. Overwriting by table order handed the
   * count to whichever row came last: hundreds of multi-candidate nodes attributed their bindings to the
   * out-of-scope duplicate, so one input read no bindings on the class the module imports and hundreds on the one
   * it does not. An in-scope candidate always wins; where every candidate is out of scope the binding is
   * left unattributed rather than guessed onto one of them.
   */
  const targetsOfNode = new Map();
  const inScopeNodes = new Set();
  for (const r of store.table('renders')) {
    if (r.node == null || r.to_class == null) continue;
    const inScope = r.scope === 'declared' || r.scope === 'imported';
    if (inScope) inScopeNodes.add(String(r.node));
    const hit = targetsOfNode.get(r.node);
    if (hit) hit.add(r.to_class);
    else targetsOfNode.set(r.node, new Set([r.to_class]));
    if (!inScope) outOfScope.add(`${String(r.node)}#${String(r.to_class)}`);
  }

  /**
   * AN INPUT DECLARED ON A BASE CLASS BELONGS TO EVERY SUBCLASS. Shared inputs live on abstract directive
   * bases while a template binds the CONCRETE class, so a lookup keyed on the concrete class alone never
   * found them: dozens of the never-bound rows are abstract-base inputs that are demonstrably live.
   *
   * `classes.extends` is ALREADY RESOLVED and carries the declaring FILE, so a duplicate class name cannot
   * attach inputs to an unrelated class of the same name.
   */
  const classRows = store.table('classes');
  const idByPath = new Map();
  for (const f of store.table('files')) {
    idByPath.set(String(f.path), f.id);
    idByPath.set(String(f.abs), f.id);
  }
  const classByFileName = new Map(classRows.map((c) => [`${String(c.file)}#${String(c.name)}`, c.id]));
  const baseOf = new Map();
  for (const c of classRows) {
    const name = str(c.extends?.name);
    const file = str(c.extends?.file);
    if (name === null || file === null) continue;
    const fid = idByPath.get(slash(file));
    const base = fid === undefined ? undefined : classByFileName.get(`${String(fid)}#${name}`);
    if (base !== undefined) baseOf.set(c.id, base);
  }

  /** A class and every base it inherits from, nearest first. Cycle-safe. */
  const chainOf = (classId) => {
    const out = [];
    let current = classId;
    const guard = new Set();
    while (current !== undefined && !guard.has(current)) {
      guard.add(current);
      out.push(current);
      current = baseOf.get(current);
    }
    return out;
  };

  const expressionOfBinding = new Map();
  for (const e of store.table('expressions')) {
    if (e.lang !== 'ts' && e.binding != null) expressionOfBinding.set(e.binding, e.id);
  }

  const boundCount = new Map();
  let bound = 0;
  for (const b of store.table('bindings')) {
    if (b.kind !== 'input' && b.kind !== 'template_attr') continue;
    const name = str(b.name);
    const candidates = targetsOfNode.get(b.node);
    if (name === null || candidates === undefined) continue;
    // The class that DECLARES this input, preferring an in-scope candidate. A name declared by two classes
    // on one element is real - a component and a directive can share an input name - so BOTH are counted
    // rather than one being chosen.
    const nodeHasInScope = inScopeNodes.has(String(b.node));
    for (const targetClass of candidates) {
      if (nodeHasInScope && outOfScope.has(`${String(b.node)}#${String(targetClass)}`)) continue;
      const io = chainOf(targetClass)
        .map((cls) => byClassAndName.get(`${String(cls)}#${name}`))
        .find((row) => row !== undefined);
      if (io === undefined) continue;
      store.add('input_usage', 'iu', {
        io: io.id, input: name, to_class: targetClass, component: io.component,
        from_component: b.component, template: b.template, node: b.node, binding: b.id,
        source: b.source ?? null, expression: expressionOfBinding.get(b.id) ?? null,
        line: b.line ?? null,
      });
      boundCount.set(io.id, (boundCount.get(io.id) ?? 0) + 1);
      bound += 1;
    }
  }

  let unused = 0;
  for (const row of declaredInputs) {
    const count = boundCount.get(row.id) ?? 0;
    row.bound_count = count;
    if (count === 0) unused += 1;
  }
  return { bound, unused, declared: declaredInputs.length };
}
