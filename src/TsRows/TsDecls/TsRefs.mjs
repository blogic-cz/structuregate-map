/**
 * A REFERENCE BECOMES A ROW ID - `di.resolved`, `calls.target`, `assignments.target_ref` and
 * `imports.resolved` all name a declaration by file and name, and this is where that becomes a join.
 *
 * Ported from the original Angular extractor. Imports are flat: see `TsTypeRef.mjs`.
 *
 * KEYED BY FILE + NAME, never by name alone: two `SharedModule`s and duplicate component classes answer to
 * the same name in a monorepo, and a name-keyed index would bind a reference to whichever was extracted
 * last.
 */

const asRef = (v) => (v !== null && typeof v === 'object' && !Array.isArray(v) ? v : null);
const str = (v) => (typeof v === 'string' ? v : null);

export function rollupRefIds(store) {
  const idByPath = new Map();
  for (const f of store.table('files')) {
    // BOTH SPELLINGS: a row carries a workspace-relative `path` while a reference carries either that or
    // the absolute `abs`, and a reference should not have to know which it holds.
    idByPath.set(f.path, f.id);
    idByPath.set(f.abs, f.id);
  }

  // METHODS COUNT. Leaving `members` out of this index meant no method call could ever resolve - and a
  // method call is the dominant call shape. Measured: tens of thousands of calls whose file and name matched a real row
  // were skipped by construction.
  //
  // AMBIGUITY IS REFUSED, NOT GUESSED. Two members can share a file and a name - overloads, two classes in
  // one file - and a map keyed on file+name would bind a call to whichever was written last. A key that
  // resolves to more than one row resolves to NONE.
  //
  // A GETTER AND ITS SETTER ARE ONE PROPERTY. Both are `members` rows with the same file and name, so a
  // file+name index saw a collision and refused: most "ambiguous" keys were accessor pairs and hundreds of
  // assignments lost their target, including `this.disabled = x` whose own setter sits in the same class.
  // The SETTER is the declaration an assignment lands on, so it wins; a getter-only property still resolves.
  const declared = new Map();
  const ambiguous = new Set();
  const owner = new Map();
  const kindOf = new Map();
  for (const table of ['classes', 'functions', 'interfaces', 'enums', 'type_aliases', 'members']) {
    for (const row of store.table(table)) {
      const name = str(row.name);
      if (name === null) continue;
      const key = `${String(row.file)}#${name}`;
      const kind = str(row.kind) ?? '';
      const accessorPair = (kind === 'getter' || kind === 'setter')
        && (kindOf.get(key) === 'getter' || kindOf.get(key) === 'setter')
        && owner.get(key) === row.class;
      if (!declared.has(key)) {
        declared.set(key, row.id);
        owner.set(key, row.class);
        kindOf.set(key, kind);
      } else if (accessorPair) {
        if (kind === 'setter') {
          declared.set(key, row.id);
          kindOf.set(key, kind);
        }
      } else ambiguous.add(key);
    }
  }
  for (const key of ambiguous) declared.delete(key);

  const idOf = (ref) => {
    const file = str(ref?.file);
    const name = str(ref?.name);
    if (file === null || name === null) return null;
    const fid = idByPath.get(file);
    return fid === undefined ? null : declared.get(`${fid}#${name}`) ?? null;
  };

  for (const row of store.table('di')) {
    const id = idOf(asRef(row.resolved));
    if (id !== null) row.resolved_id = id;
  }
  for (const row of store.table('calls')) {
    const id = idOf(asRef(row.target));
    if (id !== null) row.target_id = id;
  }
  for (const row of store.table('imports')) {
    const file = str(row.resolved);
    const fid = file === null ? undefined : idByPath.get(file);
    if (fid !== undefined) row.resolved_file = fid;
  }
  for (const row of store.table('exports')) {
    const file = str(row.resolved);
    const fid = file === null ? undefined : idByPath.get(file);
    if (fid !== undefined) row.resolved_file = fid;
  }

  // WHAT AN IMPORTED NAME IS DECLARED AS - a TOP-LEVEL declaration only. The index above holds `members` too,
  // and would bind an imported const to a same-named member of some class in that file. A function-valued
  // const is written twice, as a `consts` row and a `functions` row: the function answers for it. Any other
  // collision is ambiguous, and refused.
  const topLevel = new Map();
  const refused = new Set();
  for (const table of ['functions', 'classes', 'interfaces', 'enums', 'type_aliases', 'consts']) {
    for (const row of store.table(table)) {
      const name = str(row.name);
      if (name === null || row.inline === true || (table === 'functions' && row.parent)) continue;
      const key = `${String(row.file)}#${name}`;
      const seen = topLevel.get(key);
      if (seen === undefined) topLevel.set(key, { id: row.id, table });
      else if (!(seen.table === 'functions' && table === 'consts')) refused.add(key);
    }
  }
  for (const row of store.table('import_names')) {
    const file = str(row.declared);
    const fid = file === null ? undefined : idByPath.get(file);
    if (fid === undefined) continue;
    row.declared_file = fid;
    const key = `${fid}#${str(row.declared_name) ?? ''}`;
    if (topLevel.has(key) && !refused.has(key)) row.declared_id = topLevel.get(key).id;
  }
  // An assignment's target property is the same kind of reference and was the only one left carrying a name
  // where the rest now carry a row.
  for (const row of store.table('assignments')) {
    const id = idOf(asRef(row.target_ref));
    if (id !== null) row.target_id = id;
  }

  // A LAZY ROUTE'S TARGET IS THE OTHER HALF OF THE ROUTE TREE. `loadChildren` is a function, so its value is
  // a `$fn` - and the declaration it imports, which the evaluator resolved onto `$module`, is where that
  // route's children, guards and component actually live. Without an id the two halves sit in different
  // files with nothing between them.
  const lazyRef = (v) => asRef(asRef(v)?.$module);
  for (const row of store.table('routes')) {
    const componentId = idOf(asRef(row.component));
    if (componentId !== null) row.component_id = componentId;
    const ids = (Array.isArray(row.can_activate) ? row.can_activate : [])
      .map((g) => idOf(asRef(g))).filter((x) => x !== null);
    if (ids.length) row.can_activate_ids = ids;
    const lazy = idOf(lazyRef(row.load_children));
    if (lazy !== null) {
      row.load_children_module = lazy;
      // A TARGET REACHED ONLY BY A REJECTION HANDLER IS NOT THE ROUTE'S DESTINATION. The evaluator marks it;
      // carrying the mark onto the row stops a consumer joining on the id alone from reporting a fallback
      // module as what this route loads.
      if (lazyRef(row.load_children)?.recovery === true) row.load_children_recovery = true;
    }
    const lazyOne = idOf(lazyRef(row.load_component));
    if (lazyOne !== null) row.load_component_id = lazyOne;
  }
}

/**
 * Each call argument gains the PARAMETER it fills, by position.
 *
 * Only where the callee's declaration is in the map and carries parameters. A REST parameter absorbs every
 * argument from its position onward, which is what the language does and is stated rather than
 * approximated - and it is found by its own `rest` flag, not by being last. An argument past a fixed
 * parameter list gets nothing: that is a call the compiler would reject, or an overload this rollup cannot
 * tell apart, and either way inventing a name would be worse than leaving the position unlabelled.
 */
export function rollupCallParams(store) {
  const params = new Map();
  const collect = (rows) => {
    for (const row of rows) {
      const p = row.params;
      if (!Array.isArray(p) || !p.length) continue;
      params.set(row.id, p.map((x) => ({
        name: String(x.name ?? ''), type: x.type ?? null, rest: x.rest === true,
      })));
    }
  };
  collect(store.table('functions'));
  collect(store.table('members'));

  for (const row of store.table('calls')) {
    const list = row.target_id === undefined ? undefined : params.get(row.target_id);
    const args = row.args;
    if (!list || !Array.isArray(args) || !args.length) continue;
    const rest = list.find((p) => p.rest);
    const names = args.map((_, i) => list[i]?.name ?? rest?.name ?? null);
    if (!names.some((n) => n !== null)) continue;
    row.arg_params = names;
  }
}
