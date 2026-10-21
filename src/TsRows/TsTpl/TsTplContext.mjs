/**
 * WHAT A DIRECTIVE TYPES IN THE TEMPLATE IT IS GIVEN (rows 74): the context of an `<ng-template>` or a `*dir` wrapper - what its `let-x` and `*dir="let x"` variables are -
 * and the instance of a generic component a `#ref` names. Both are the same question: which type parameters did the inputs bound on the element feed?
 *
 * Imports are flat: see `TsDecls/TsTypeRef.mjs`.
 */

const str = (v) => (typeof v === 'string' ? v : null);

export function makeContexts(store, owners, directivesAt, inputsAt) {
  const { propByOwner, resolveRef, elementRefOf, hopOf, instance, paramNames } = owners;

  const classOfDirective = new Map();
  for (const table of ['components', 'directives']) {
    for (const row of store.table(table)) if (row.class != null) classOfDirective.set(row.id, row.class);
  }
  // THE MEMBER AN INPUT NAME (its alias, as a template spells it) IS: `io` says which member, `members` how it is typed.
  const inputMember = new Map();
  for (const io of store.table('io')) {
    if (io.kind !== 'input' || io.class == null || str(io.binding_name) === null || str(io.member) === null) continue;
    const row = propByOwner.get(`class:${String(io.class)}#${io.member}`);
    if (row !== undefined) inputMember.set(`${String(io.class)}#${io.binding_name}`, row);
  }
  // THE CONTEXT A DIRECTIVE DECLARES, most specific first: `static ngTemplateContextGuard<T>(dir: X<T>, ctx: unknown): ctx is Ctx<T>` (its `guard_ref`), else the `TemplateRef<Ctx>`
  // it injects. Angular's own checker reads the guard alone; a directive that only types its `TemplateRef` still says what it hands the view.
  const guardOf = new Map();
  for (const m of store.table('members')) {
    if (m.name === 'ngTemplateContextGuard' && m.static && m.guard_ref != null && m.class != null) guardOf.set(m.class, m);
  }
  const injectedOf = new Map();
  for (const d of store.table('di')) {
    const ref = d.type_ref;
    if (d.class != null && ref?.name === 'TemplateRef' && String(ref.file ?? '').includes('@angular/core') && Array.isArray(ref.args) && ref.args[0]) injectedOf.set(d.class, ref.args[0]);
  }

  const sameDeclaration = (a, b) => a?.name === b?.name && a?.file === b?.file && a?.line === b?.line;

  /** WHAT EACH TYPE PARAMETER OF THE CLASS IS, from the inputs bound on the node: an input typed `T` takes the value's owner, `T[]` the element of its list (`wrapped` first: what an
   *  `async` alias holds), `Box<T>` the argument of the same declaration the value is. The first input to name a parameter wins; a value that types nothing feeds null. */
  const feedOf = (classId, nodeId, component, scope, valueOf) => {
    const fed = new Map();
    if (!paramNames({ kind: 'class', id: classId }).length) return fed;
    for (const [name, ast] of inputsAt.get(nodeId) ?? []) {
      const ref = inputMember.get(`${String(classId)}#${name}`)?.type_ref;
      if (!ref || typeof ref !== 'object') continue;
      const value = valueOf(ast, component, scope);
      if (ref.param === true) {
        if (!fed.has(ref.name)) fed.set(ref.name, value?.owner ?? null);
        continue;
      }
      const slot = elementRefOf(ref);
      if (slot?.param === true) {
        if (fed.has(slot.name)) continue;
        const element = elementRefOf(value?.wrapped ?? value?.ref);
        fed.set(slot.name, element === null ? null : resolveRef(element, value?.env ?? null));
        continue;
      }
      if (Array.isArray(ref.args) && sameDeclaration(ref, value?.ref) && Array.isArray(value.ref.args)) {
        ref.args.forEach((a, i) => {
          if (a?.param === true && !fed.has(a.name)) fed.set(a.name, resolveRef(value.ref.args[i] ?? null, value.env ?? null));
        });
      }
    }
    return fed;
  };

  /** The instance of the generic class a `#ref` names: its parameters bound by what the inputs on its element feed. A class with none is its owner as it was. */
  const instanceOf = (classId, nodeId, component, scope, valueOf) => {
    const owner = { kind: 'class', id: classId };
    return paramNames(owner).length ? instance(owner, feedOf(classId, nodeId, component, scope, valueOf)) : owner;
  };

  /** The owner of the context the directives matched on a node hand their view, `undefined` when none of them declares one (the node's variables are then read as before), null
   *  when it is declared and names no owner. `scope` is the node's OUTER scope: its inputs are evaluated before the names it binds exist. */
  const contextAt = (nodeId, component, scope, valueOf) => {
    for (const d of directivesAt.get(nodeId) ?? []) {
      const classId = classOfDirective.get(d.id);
      if (classId === undefined || (!guardOf.has(classId) && !injectedOf.has(classId))) continue;
      const owner = { kind: 'class', id: classId };
      const env = instance(owner, feedOf(classId, nodeId, component, scope, valueOf)).env ?? new Map();
      const guard = guardOf.get(classId);
      if (guard === undefined) return resolveRef(injectedOf.get(classId), env);
      // THE GUARD NAMES ITS OWN PARAMETERS (`<U>(dir: X<U>, ..)`): `dir`'s arguments say which of the class's each is, in order.
      const named = guard.params?.[0]?.type_ref?.args ?? [];
      const bound = new Map([...env].map(([p, v], i) => [named[i]?.param === true ? named[i].name : p, v]));
      return resolveRef(guard.guard_ref, bound);
    }
    return undefined;
  };

  /** A variable of a context (`let-x="prop"`, `*dir="let x"`): the value of the context's property of that name, `$implicit` for one with no value. */
  const variableOf = (context, value) => {
    if (context === null) return null;
    const hop = hopOf(context, value === '' ? '$implicit' : value);
    return hop === null ? null : { owner: hop.owner, ref: hop.member.type_ref ?? null, row: hop.member, env: hop.env };
  };

  return { contextAt, variableOf, instanceOf, classOfDirective };
}
