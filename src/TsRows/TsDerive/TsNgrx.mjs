/**
 * THE STATE GRAPH: the actions an application declares, the reducer handlers that answer them, and the
 * selectors that read what those handlers write.
 *
 * Ported from the original Angular extractor. Every API is recognised by its RESOLVED
 * declaration inside the state library, never by its name. Imports are flat: see `TsDecls/TsTypeRef.mjs`.
 */

const str = (v) => (typeof v === 'string' ? v : null);
const arr = (v) => (Array.isArray(v) ? v : []);
const asCall = (v) => (v !== null && typeof v === 'object' && v.$call !== undefined ? v : null);

/** Is this call the named export of the state library, by RESOLVED declaration rather than by name? */
const isLibraryCall = (call, name) => {
  if (call === null) return false;
  return str(call.$target?.name) === name && String(call.$target?.file ?? '').includes('/@ngrx/');
};

export function rollupStateGraph(store) {
  const consts = store.table('consts');

  // A STRING BUILT FROM A SAME-FILE CONST IS STILL KNOWABLE. A codebase may write
  // `const actionsName = '[Cart]'; createAction(`${actionsName} addItem`)`, and the evaluator keeps
  // that honestly as a template with holes rather than folding it. Most action types were therefore
  // null. The hole names a const in the same file whose value the map already holds, so the fold is a JOIN.
  const stringConstByFileName = new Map();
  for (const c of consts) {
    if (typeof c.value === 'string') stringConstByFileName.set(`${String(c.file)}#${String(c.name)}`, c.value);
  }
  const foldString = (v, file) => {
    if (typeof v === 'string') return v;
    const parts = v?.$template;
    const holes = v?.$holes;
    if (!Array.isArray(parts) || !Array.isArray(holes)) return null;
    let out = '';
    for (const [i, part] of parts.entries()) {
      out += String(part ?? '');
      if (i < holes.length) {
        const filled = stringConstByFileName.get(`${String(file)}#${String(holes[i])}`);
        if (filled === undefined) return null;
        out += filled;
      }
    }
    return out;
  };

  /** An action reference under either shape the evaluator produces for one. */
  const refText = (v) => str(v?.$expr) ?? str(v?.$member);

  let actions = 0;
  let handlers = 0;
  let selectors = 0;

  for (const c of consts) {
    const call = asCall(c.value);
    if (isLibraryCall(call, 'createAction')) {
      // An action's own `type` string is its identity across the application - what a reducer refers to and
      // what appears in a devtools trace - so it is published beside the const that declares it.
      const [type] = arr(call?.$args);
      store.add('state_actions', 'sa', {
        const: c.id, file: c.file, name: c.name, type: foldString(type, c.file), line: c.line, group: null,
      });
      actions += 1;
      continue;
    }
    // A GROUP DECLARES MANY ACTIONS AT ONCE, and it is how nearly half of a typical application's action declarations
    // are written - reading only `createAction` would publish a table missing about half of them with
    // nothing to show it was incomplete.
    if (!isLibraryCall(call, 'createActionGroup')) continue;
    const [config] = arr(call?.$args);
    const source = foldString(config?.source, c.file);
    const events = config?.events;
    if (source === null || events === null || typeof events !== 'object') continue;
    for (const [event] of Object.entries(events)) {
      // THE DISPATCHED NAME IS NOT THE DECLARED KEY. The library lowercases the event key to build the
      // property the application actually calls - `openPanel` is dispatched as `openpanel` - so publishing the
      // raw key made `name` wrong for many of the rows and broke the join from the handlers, which see the
      // dispatched spelling. Both are published; the TYPE keeps the declared casing, as the library does.
      store.add('state_actions', 'sa', {
        const: c.id, file: c.file, name: `${String(c.name)}.${event.toLowerCase()}`,
        event, type: `[${source}] ${event}`, line: c.line, group: source,
      });
      actions += 1;
    }
  }

  // A REDUCER NEED NOT BE ITS OWN CONST: a slice can be written inline as a property of the object passed
  // to `combineReducers`, so inspecting only consts whose whole value is a `createReducer` missed it and its
  // handlers. The value tree is searched instead.
  //
  // A DIRECT DECLARATION WINS. A reducer that has its own const AND is referenced inside an outer
  // `combineReducers` is found by both scans, because the evaluator RESOLVES that reference and inlines the
  // whole call - emitting on both paths filed over half the rows a second time, under the OUTER const's name
  // and line.
  const reducerCalls = [];
  const direct = new Set();
  for (const c of consts) {
    const call = asCall(c.value);
    if (isLibraryCall(call, 'createReducer') && call !== null) {
      reducerCalls.push({ row: c, call });
      direct.add(c.value);
    }
  }
  const findReducers = (value, row, seen) => {
    if (value === null || typeof value !== 'object' || seen.has(value)) return;
    seen.add(value);
    const call = asCall(value);
    if (isLibraryCall(call, 'createReducer') && call !== null && !direct.has(value)) {
      reducerCalls.push({ row, call });
    }
    for (const v of Object.values(value)) findReducers(v, row, seen);
  };
  for (const c of consts) findReducers(c.value, c, new Set());

  const emittedHandlers = new Set();
  for (const { row: c, call } of reducerCalls) {
    for (const entry of arr(call?.$args)) {
      const on = asCall(entry);
      if (!isLibraryCall(on, 'on')) continue;
      const [actionRef, handler] = arr(on?.$args);
      // THE ACTION IS PART OF THE HANDLER'S IDENTITY, not just the function. Expression rows are memoized by
      // AST node, so two entries sharing ONE named handler would share an expression id and the second would
      // be dropped - a real handler lost to the dedup meant for a duplicated reducer.
      const identity = `${String(refText(actionRef) ?? '')}|${str(handler?.$expr_id)
        ?? JSON.stringify(handler?.$writes ?? null)}`;
      if (emittedHandlers.has(identity)) continue;
      emittedHandlers.add(identity);
      store.add('state_handlers', 'sh', {
        reducer: c.id, reducer_name: c.name, file: c.file, line: c.line,
        // The action is referenced as `cartActions.resetState`, so the READ is the identity a consumer
        // joins on - the map does not resolve a namespace import to the const behind it.
        action_source: refText(actionRef),
        action_reads: actionRef?.$reads ?? null,
        // The handler's writes are the paths it produces - the whole point of the join - and its reads say
        // what it consumed to produce them. Both are already computed and are copied, never recomputed.
        writes: handler?.$writes ?? null,
        reads: handler?.$reads ?? null,
        expression: handler?.$expr_id ?? null,
      });
      handlers += 1;
    }
  }

  for (const c of consts) {
    const call = asCall(c.value);
    if (!isLibraryCall(call, 'createSelector')) continue;
    const args = arr(call?.$args);
    // Every argument but the last is an INPUT selector; the last is the projector. Stated rather than
    // assumed: a selector with one argument is a projector over the whole state.
    const projector = args.length > 1 ? args[args.length - 1] : args[0];
    const inputs = args.length > 1 ? args.slice(0, args.length - 1) : [];
    /**
     * What an input selector READS - through an INLINED selector, not just off the top of it.
     *
     * The evaluator resolves a same-file selector reference by inlining the whole `createSelector` object it
     * points at, and that object has no reads of its own - so most inputs reported null where the
     * table implied a state path. A cross-file reference stays a name, because that is all the evaluator
     * resolved it to.
     */
    const readsOf = (v, seen = new Set()) => {
      if (v === null || typeof v !== 'object' || seen.has(v)) return null;
      seen.add(v);
      if (v.$reads !== undefined) return v.$reads;
      const inner = asCall(v);
      if (!isLibraryCall(inner, 'createSelector')) return null;
      const nested = arr(inner?.$args).map((a) => readsOf(a, seen)).filter((x) => x !== null);
      return nested.length ? nested.flat() : null;
    };
    store.add('state_selectors', 'ss', {
      const: c.id, file: c.file, name: c.name, line: c.line,
      input_count: inputs.length,
      // What the inputs read IS the state path this selector depends on - the join back to the type members
      // and to a handler's writes.
      state_reads: inputs.map((i) => readsOf(i)),
      projector_reads: readsOf(projector),
      expression: projector?.$expr_id ?? null,
    });
    selectors += 1;
  }

  return { actions, handlers, selectors };
}
