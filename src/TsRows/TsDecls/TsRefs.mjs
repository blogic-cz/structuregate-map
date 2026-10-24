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
      // AN OVERLOAD SET IS ONE METHOD. `error(a): void; error(a, b): void; error(..) { .. }` is one `members` row per
      // signature and one for the implementation, all of one class, and TypeScript puts the implementation LAST: a call
      // lands on it, never on a signature and never on none (a refused key left every call of an overloaded method with no
      // `target_id`). Two rows of DIFFERENT classes stay ambiguous.
      const overload = table === 'members' && kind === 'method' && kindOf.get(key) === 'method'
        && owner.get(key) === row.class && !ambiguous.has(key);
      if (overload) declared.set(key, row.id);
      else if (!declared.has(key)) {
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

  // `export { A as B }` NAMES A DECLARATION BY ITS PUBLIC NAME. The checker reports the symbol as `B` where
  // the file declares `A`, so a file+name key finds nothing and a route's component, a module's declaration
  // and a `di` type all lost their class. Only a statement WITHOUT a source is a local alias: `from` is the
  // other file's own naming, and its declaration is reached through that file.
  const aliasOf = new Map();
  for (const row of store.table('exports')) {
    if (row.from !== null && row.from !== undefined) continue;
    const names = Array.isArray(row.names) ? row.names : [];
    const outer = Array.isArray(row.exported_as) ? row.exported_as : [];
    for (let i = 0; i < outer.length; i++) {
      if (typeof names[i] === 'string' && names[i] !== outer[i]) aliasOf.set(`${String(row.file)}#${outer[i]}`, names[i]);
    }
  }

  const idOf = (ref) => {
    const file = str(ref?.file);
    const name = str(ref?.name);
    if (file === null || name === null) return null;
    const fid = idByPath.get(file);
    if (fid === undefined) return null;
    const hit = declared.get(`${fid}#${name}`);
    if (hit !== undefined) return hit;
    const local = aliasOf.get(`${fid}#${name}`);
    return local === undefined ? null : declared.get(`${fid}#${local}`) ?? null;
  };

  for (const row of store.table('di')) {
    const id = idOf(asRef(row.resolved));
    if (id !== null) row.resolved_id = id;
  }
  // A CALL'S TARGET IS FOUND BY ITS PLACE, never by its name (rows 80, 81): `target.line` is the declaration the checker named (`TsExpr/TsReadRefs.mjs::targetLine`), and the row at that file, line and
  // name is `target_id` - a function, a method, a class, the member of a type or an object literal, a local, a module const. The name key above bound a bare `getAmounts(..)` to a same-named METHOD of the
  // class around a local `const getAmounts`, and `Alpha.getAge(..)`, a property, to a `function getAge` of the file. A place with no row has NO id: falling back to the name was the bug.
  // `callee_id` is the callable that RUNS - the function, method or class row itself, or for a property that only names a value (`{ x }`, `{ y: x }`: `target.follow`) the callable it names - which is what a
  // reader following the call into the callee's `returns` and parameters needs, and a `shape_members` row has neither.
  const placed = new Map();
  const callable = new Set();
  // A LOCAL OR CONST HOLDING A FUNCTION runs that function (rows 84): its value names the `functions` row (`$fn_id`, `TsWalk.mjs::linkFunctions`), which is the callee of a call of it.
  const holds = new Map();
  for (const table of ['locals', 'consts']) {
    for (const row of store.table(table)) {
      const fn = asRef(row.value)?.$fn_id;
      if (typeof fn === 'string') holds.set(row.id, fn);
    }
  }
  for (const table of ['functions', 'members', 'classes', 'type_members', 'shape_members', 'locals', 'consts']) { // a NAMED declaration first: an anonymous literal has only the shape's, and a function-valued const is a `functions` row and a `consts` row
    for (const row of store.table(table)) {
      const name = str(row.name);
      // ONLY A ROW THAT IS THE DECLARATION ITSELF: one a file's extraction made for a declaration in ANOTHER file (`owner_file` is not `file` - a computed type's `via: 'computed'` copy of a library member) is not it,
      // and binding to it made the reference depend on a file it never read, chosen by file order. A library member keeps its `target` and has no id.
      if (name === null || typeof row.line !== 'number' || (row.owner_file !== undefined && row.owner_file !== row.file)) continue;
      const key = `${String(row.file)}#${row.line}#${name}`;
      if (placed.has(key)) continue;
      placed.set(key, row.id);
      if (table === 'functions' || table === 'members' || table === 'classes') callable.add(row.id);
    }
  }
  const placeKey = (ref) => {
    const fid = idByPath.get(str(ref?.file) ?? '');
    return fid === undefined || typeof ref?.line !== 'number' ? null : `${String(fid)}#${ref.line}#${String(ref.name)}`;
  };
  for (const row of store.table('calls')) {
    const ref = asRef(row.target);
    const key = placeKey(ref);
    const hit = key === null ? undefined : placed.get(key);
    if (hit === undefined) continue;
    row.target_id = hit;
    const followKey = callable.has(hit) ? null : placeKey(asRef(ref.follow));
    const callee = callable.has(hit) ? hit : followKey === null ? holds.get(hit) : placed.get(followKey);
    if (callee !== undefined && (callable.has(callee) || holds.get(hit) === callee)) row.callee_id = callee;
  }
  // A LITERAL IN AN OBJECT LITERAL NAMES THE MEMBER IT FILLS (rows 81, #104): `target` is the declaration the checker's contextual type reached (`TsExpr/TsReadRefs.mjs::fieldTarget`), and the row AT its file,
  // line and name - a `type_members`, `members` or `shape_members` row - is `target_id`, by the same place key a call's is. A library's member has no row, so no id.
  // An INLINE function in such a property names it the same way (rows 83, #105); a `prototype-method` row's `target` is what a call of it resolves to, and gets no id here.
  for (const table of ['string_literals', 'number_literals', 'template_literals', 'functions']) {
    for (const row of store.table(table)) {
      if (table === 'functions' && row.inline !== true) continue;
      const ref = asRef(row.target);
      const key = ref === null ? null : placeKey(ref);
      const hit = key === null ? undefined : placed.get(key);
      if (hit !== undefined) row.target_id = hit;
    }
  }
  // A PROPERTY PATH'S SEGMENTS NAME THEIR ROWS (rows 98, #109): each `path_refs` entry is the declaration at its place - `id`, the `members` or `type_members` row.
  for (const row of store.table('string_literals')) {
    if (!Array.isArray(row.path_refs)) continue;
    for (const ref of row.path_refs) {
      const key = ref?.element ? null : placeKey(ref);
      const hit = key === null ? undefined : placed.get(key);
      if (hit !== undefined) ref.id = hit;
    }
  }
  // `external` IS THE SPELLING (a specifier not starting with `.`); whether the import lands IN THE TREE is
  // `in_tree`, which most consumers meant by it: a tsconfig alias is external and in the tree.
  for (const row of store.table('imports')) {
    const file = str(row.resolved);
    const fid = file === null ? undefined : idByPath.get(file);
    if (fid !== undefined) row.resolved_file = fid;
    row.in_tree = fid !== undefined && !file.startsWith('..') && !file.split('/').includes('node_modules');
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
  // A NAME THE FILE IMPORTS UNDER AN ALIAS (`import { A as B }`), or one the evaluator left unresolved (`{expr}`), is
  // what that file's own import binds it to: the ref still names the alias, which no declaration carries.
  const importedAs = new Map();
  for (const row of store.table('import_names')) {
    if (row.declared_id !== undefined && str(row.local) !== null) importedAs.set(`${String(row.file)}#${row.local}`, row.declared_id);
  }
  const refIn = (ref, fileId) => (ref === null ? null
    : idOf(ref) ?? importedAs.get(`${String(fileId)}#${str(ref.name) ?? str(ref.expr) ?? ''}`) ?? null);

  // EVERY NESTED `$module` AND `$ref` OF AN ELEMENT, with its row id. COPIED ON THE WAY DOWN, never written in place:
  // a route's `load_children` is the very object a module's `forChild([...])` holds, and an id added to it here
  // would land on a full run's `routes` row but not on a partial run's, whose rows come back apart. Only the path to
  // an object that gains an id is copied; `memo` keeps a shared sub-value one value and stops a cycle.
  const nestedIds = (value, fileId, memo) => {
    if (value === null || typeof value !== 'object') return value;
    if (memo.has(value)) return memo.get(value);
    memo.set(value, value);
    let out = value;
    const put = (k, v) => {
      if (v === out[k]) return;
      if (out === value) out = Array.isArray(value) ? [...value] : { ...value };
      out[k] = v;
    };
    if (Array.isArray(value)) value.forEach((v, i) => put(i, nestedIds(v, fileId, memo)));
    else {
      for (const k of Object.keys(value)) put(k, nestedIds(value[k], fileId, memo));
      const lazy = asRef(out.$module);
      const lazyId = lazy === null ? null : idOf(lazy);
      if (lazyId !== null && lazy.id !== lazyId) put('$module', { ...lazy, id: lazyId });
      const ref = str(value.$ref) === null ? null : refIn({ name: value.$ref, file: value.file }, fileId);
      if (ref !== null && out.id !== ref) put('id', ref);
    }
    memo.set(value, out);
    return out;
  };

  // A MODULE'S LISTS CARRY THE ROW ID of each class they name. `reach` and every consumer joining a module to
  // its declarations otherwise re-derive it from file + name, which an alias defeats. INSIDE an element too (#75):
  // a lazy `loadChildren` in `RouterModule.forChild([...])` holds its module as `$module` - resolved as
  // `routes.load_children_module` is - and a provider object names its classes as `$ref`s.
  for (const row of store.table('ng_modules')) {
    for (const key of ['declarations', 'imports', 'exports', 'providers', 'bootstrap']) {
      if (!Array.isArray(row[key])) continue;
      const memo = new Map();
      const list = row[key];
      for (let i = 0; i < list.length; i++) {
        const out = nestedIds(list[i], row.file, memo);
        if (out !== list[i]) list[i] = out;
        const ref = asRef(out);
        const id = refIn(ref, row.file);
        if (id !== null) ref.id = id;
      }
    }
  }
  // `@Inject(TOKEN)` AND `inject(TOKEN)` NAME A CONST: the same top-level lookup an import gets. A token that is a
  // PROPERTY of a const object (`Tokens.Alpha`, the shape most token registries take) is no top-level declaration:
  // it joins the const that holds it, and `token` still says which property.
  const topId = (key) => (topLevel.has(key) && !refused.has(key) ? topLevel.get(key).id : null);
  for (const row of store.table('di')) {
    const ref = asRef(row.token_ref);
    const fid = str(ref?.file) === null ? undefined : idByPath.get(ref.file);
    if (fid === undefined) continue;
    const head = (str(row.token) ?? '').split('.')[0];
    const id = topId(`${fid}#${str(ref.name) ?? ''}`) ?? (head && head !== row.token ? topId(`${fid}#${head}`) : null);
    if (id !== null) row.token_id = id;
  }
  // An assignment's target property is the same kind of reference and was the only one left carrying a name
  // where the rest now carry a row.
  //
  // A `this.<x>` WRITE A FILE+NAME KEY REFUSES (two classes of one file declare `x`) IS THE WRITING CLASS'S OWN MEMBER,
  // or the nearest base's that declares it: the class the write sits in says which, so nothing is guessed.
  const memberOf = new Map();
  for (const m of store.table('members')) {
    const key = `${String(m.class)}#${str(m.name) ?? ''}`;
    if (!memberOf.has(key) || m.kind === 'setter') memberOf.set(key, m.id);
  }
  const classAt = new Map();
  for (const c of store.table('classes')) classAt.set(`${String(c.file)}#${str(c.name) ?? ''}`, c.id);
  const parentOf = new Map();
  for (const c of store.table('classes')) {
    const ext = asRef(c.extends);
    const fid = str(ext?.file) === null ? undefined : idByPath.get(ext.file);
    const up = fid === undefined ? undefined : classAt.get(`${fid}#${str(ext.name) ?? ''}`);
    if (up !== undefined) parentOf.set(c.id, up);
  }
  // A BASE CLASS AND EACH TYPE ARGUMENT OF IT CARRY THE `classes` ROW THE CHECKER RESOLVED THEM TO (#87): the declaration's
  // own place - its file and its name's line, which `TsClassRows.mjs::heritageRef` read off the symbol, an alias and a
  // barrel followed - never the name as written. A place two class rows share names neither; a library class has no row.
  const classAtLine = new Map();
  const sharedLine = new Set();
  for (const c of store.table('classes')) {
    const key = `${String(c.file)}#${String(c.line)}`;
    if (classAtLine.has(key)) sharedLine.add(key);
    else classAtLine.set(key, c.id);
  }
  const baseIds = (ref) => {
    if (ref === null) return;
    const fid = ref.external === true || str(ref.file) === null ? undefined : idByPath.get(ref.file);
    const key = `${String(fid)}#${String(ref.line)}`;
    if (fid !== undefined && typeof ref.line === 'number' && !sharedLine.has(key) && classAtLine.has(key)) ref.id = classAtLine.get(key);
    for (const arg of Array.isArray(ref.type_args) ? ref.type_args : []) baseIds(asRef(arg));
  };
  for (const c of store.table('classes')) baseIds(asRef(c.extends));
  const ownMember = (cls, name) => {
    for (let at = cls, i = 0; at !== undefined && i < 32; at = parentOf.get(at), i++) {
      const id = memberOf.get(`${String(at)}#${name}`);
      if (id !== undefined) return id;
    }
    return null;
  };
  for (const row of store.table('assignments')) {
    const ref = asRef(row.target_ref);
    let id = idOf(ref);
    if (id === null && row.scope === 'this' && row.class !== undefined && str(ref?.name) !== null) id = ownMember(row.class, ref.name);
    if (id !== null) row.target_id = id;
  }

  // A LAZY ROUTE'S TARGET IS THE OTHER HALF OF THE ROUTE TREE. `loadChildren` is a function, so its value is
  // a `$fn` - and the declaration it imports, which the evaluator resolved onto `$module`, is where that
  // route's children, guards and component actually live. Without an id the two halves sit in different
  // files with nothing between them.
  const lazyRef = (v) => asRef(asRef(v)?.$module);
  for (const row of store.table('routes')) {
    const componentId = refIn(asRef(row.component), row.file);
    if (componentId !== null) row.component_id = componentId;
    // EVERY GUARD LIST NAMES ITS CLASSES: a `canDeactivate`, `canMatch`, `canLoad` or `canActivateChild` (`guards`) guard
    // is the same `classes` row a `canActivate` one is, and had no id beside it.
    for (const [column, idsColumn] of [['can_activate', 'can_activate_ids'], ['can_deactivate', 'can_deactivate_ids'],
      ['can_match', 'can_match_ids'], ['can_load', 'can_load_ids'], ['guards', 'guards_ids']]) {
      const ids = (Array.isArray(row[column]) ? row[column] : []).map((g) => idOf(asRef(g))).filter((x) => x !== null);
      if (ids.length) row[idsColumn] = ids;
    }
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
    // THE CALLABLE THAT RUNS (`callee_id`, rows 81): a property that only names a function has `target_id` = the property, whose parameters are the function's.
    const callee = row.callee_id ?? row.target_id;
    const list = callee === undefined ? undefined : params.get(callee);
    const args = row.args;
    if (!list || !Array.isArray(args) || !args.length) continue;
    const rest = list.find((p) => p.rest);
    const names = args.map((_, i) => list[i]?.name ?? rest?.name ?? null);
    if (!names.some((n) => n !== null)) continue;
    row.arg_params = names;
  }
}
