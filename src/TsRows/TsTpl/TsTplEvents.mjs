/**
 * WHAT `$event` IS IN AN OUTPUT BINDING (rows 80, split out of `TsTplReads.mjs`): `(picked)="go($event.row.RowID)"` over an in-tree component or directive whose output
 * `picked` is `@Output() picked = new EventEmitter<Picked>()` (or `output<Picked>()`) is a `Picked` - the first type argument of the type the output's own member declares,
 * bound by the generic instance the element makes (`EventEmitter<T & { cell: Cell }>` over `<demo-grid [rows]="items">`). A DOM event (`(click)` no matched directive
 * outputs) and an output declared with no argument type nothing: `$event` then stays what it was, the component's own `$event` member, which is none.
 *
 * Imports are flat: see `TsDecls/TsTypeRef.mjs`.
 */

/** THE TYPE ARGUMENT an emitter was declared with: the first of its `args`, else its `element` - the one argument of a single-argument generic, kept when the argument is a type parameter
 *  or an unnamed intersection (`EventEmitter<T & { cell: Cell }>`), which `args` leave out. */
const argumentOf = (member) => member?.type_ref?.args?.[0] ?? member?.type_ref?.element ?? null;

export function makeEvents({ store, propByOwner, directivesAt, declaredBy, contexts, resolveRef, scopes, judgeScope }) {
  // THE OUTPUTS OF A CLASS by the name a template spells them: the `io` row's member, whose `type_ref` carries the emitter's arguments.
  const outputsOf = new Map();
  for (const io of store.table('io')) {
    // `io.member` is the member's NAME: its row is the class's property of that name.
    if (io.kind !== 'output' || typeof io.binding_name !== 'string' || io.member == null) continue;
    outputsOf.set(`${String(io.class)}#${io.binding_name}`, propByOwner.get(`class:${String(io.class)}#${String(io.member)}`) ?? null);
  }

  /** `{ owner, ref, row, env }` for `$event` of the output expression `row` (kind `output`), or null when no matched directive declares an output of that name with a type argument. */
  return (row, component, scopeFor, valueOf) => {
    const matched = (directivesAt.get(row.node) ?? []).filter((d) => {
      const klass = declaredBy.get(d.id)?.class;
      const member = klass == null ? null : outputsOf.get(`${String(klass)}#${String(row.name)}`);
      return argumentOf(member) !== null;
    });
    // TWO MATCHED CANDIDATES: the one the host's SCOPE renders, as `renders` chooses (`judgeScope`).
    const seen = matched.length > 1
      ? matched.filter((d) => judgeScope(scopes(), { from_class: component.id, to_class: declaredBy.get(d.id).class, to_name: d.name }).scope !== 'out_of_scope')
      : matched;
    const hit = seen[0] ?? matched[0];
    if (hit === undefined) return null;
    const classId = declaredBy.get(hit.id).class;
    const ref = argumentOf(outputsOf.get(`${String(classId)}#${String(row.name)}`));
    const instance = contexts.instanceOf(classId, row.node, component, scopeFor(), valueOf);
    const env = instance?.env ?? null;
    return { owner: resolveRef(ref, env), ref, row: null, env };
  };
}
