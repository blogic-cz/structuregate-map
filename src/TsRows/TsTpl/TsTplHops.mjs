/**
 * HOPS A TEMPLATE CHAIN CAN GO THROUGH WITHOUT A DECLARATION OF ITS OWN (rows 80, split out of `TsTplReads.mjs`): the `key` and `value` of what a framework `keyvalue` pipe hands, and
 * a step of a UNION whose arms declare it in several places but all as ONE type. Neither names a row, so the hop carries no `target`; the chain goes on from the owner it
 * reaches, so the next name resolves. A hop that is none of these is `hopOf`'s (`TsTplOwners.mjs`).
 *
 * Imports are flat: see `TsDecls/TsTypeRef.mjs`.
 */

import { slash } from './TsPaths.mjs';

export function makeHops({ hopOf, indexByShape, scopes, fieldsOf }) {
  /**
   * THE LIBRARY PIPE a name is in the scope of a component's class, read off the typings the extraction recorded (`external_pipes`, the scope's `ext`): `{name, class, file}` or
   * null. A pipe is the framework's by WHERE ITS CLASS IS DECLARED (`@angular/common`'s `KeyValuePipe`), never by the name a template spells it with.
   */
  const libraryPipeOf = (classId, name) => {
    const all = scopes();
    const standalone = all.standaloneByClass.get(classId);
    const scope = standalone ? [all.standaloneScope(standalone)] : (([s]) => (s ? [s.declared, s.imported] : []))([all.scopeOf(all.moduleOfClass(classId))]);
    const found = scope.flatMap((s) => [...s.ext.values()]).filter((p) => String(p.name) === name);
    return found.length === 1 ? found[0] : null;
  };
  const isKeyValuePipe = (pipe) => pipe !== null && pipe.class === 'KeyValuePipe' && slash(pipe.file ?? '').includes('/@angular/common/');

  /**
   * WHAT `keyvalue` HANDS ON, as Angular types it: over `Record<K, V>`, `Map<K, V>` or an index signature `{ [k: string]: V }` a LIST of `{ key: K, value: V }` - a value
   * whose element ref is a PAIR (`pair`, resolved to the owner of that name by `TsTplOwners.mjs::resolveRef`). A value of any other type is refused: it must not borrow its input's.
   * (rows 86) What a framework pipe EMITS is read, not what it unwraps to (`wrapped`: `x$ | async | keyvalue` over `Observable<Record<K, V>>` is the record); an ARRAY is the
   * `Record<number, T>` the pipe's overload takes it as, its value the element; and a class or interface with no base is `Record<string, V>`, `V` the union of what its
   * members are typed as (`fieldsOf`) - refused when one is typed by nothing the map holds, as a union with an unknown arm would be.
   */
  const keyValueOf = (held) => {
    const ref = held?.wrapped ?? held?.ref ?? null;
    const two = Array.isArray(ref?.args) && ref.args.length === 2;
    let pair = null;
    if (ref?.index) pair = { key: two ? ref.args[0] : null, value: ref.index };
    else if (two && ['Record', 'Map', 'ReadonlyMap'].includes(ref.name)) pair = { key: ref.args[0], value: ref.args[1] };
    else if (held?.owner?.kind === 'shape' && indexByShape.get(held.owner.id)?.type_ref) pair = { key: null, value: indexByShape.get(held.owner.id).type_ref };
    else if (['Array', 'ReadonlyArray'].includes(ref?.name) && ref.element) pair = { key: null, value: ref.element };
    else if (held?.wrapped === undefined && (held?.owner?.kind === 'class' || held?.owner?.kind === 'type')) {
      const fields = fieldsOf(held.owner);
      const refs = fields?.flatMap((f) => (f.type_ref ? [f.type_ref] : Array.isArray(f.type_refs) ? f.type_refs : [null]));
      if (refs?.length && refs.every((r) => r !== null)) pair = { key: null, value: refs.length === 1 ? refs[0] : { type_refs: refs, type_refs_of: 'or' } };
    }
    return pair === null ? null : { owner: null, ref: { element: { pair: true, ...pair } }, row: null, env: held.env ?? null };
  };

  /** A hop of a `{ key, value }` owner: `key` and `value`, nothing else. */
  const pairHop = (cursor, name) => {
    if (name !== 'key' && name !== 'value') return null;
    return { name, owner: cursor[name], ref: cursor[`${name}Ref`] ?? null, member: undefined, env: null };
  };

  /**
   * A UNION STEP ITS ARMS AGREE ON THE TYPE OF: `model.Id.Value` over `A | B | C`, each declaring `Id` in its own place as the one class `Pin`. The step names no one place
   * (it keeps no `target`), but every arm leads to the same owner, so the chain goes on from it. One arm that does not declare the name, or types it differently, ends the chain.
   */
  const agreedHop = (cursor, name) => {
    if (cursor?.kind !== 'any' || cursor.op !== 'or') return null;
    const arms = cursor.of.map((one) => hopOf(one, name));
    const first = arms[0]?.owner;
    const agree = first && first.kind !== 'any' && arms.every((a) => a !== null && a.owner && a.owner.kind === first.kind && a.owner.id === first.id);
    return agree ? { name, owner: first, ref: arms[0].member.type_ref ?? null, member: undefined, env: arms[0].env } : null;
  };

  return { libraryPipeOf, isKeyValuePipe, keyValueOf, pairHop, agreedHop };
}
