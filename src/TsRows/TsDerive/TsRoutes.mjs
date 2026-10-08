/**
 * THE ROUTE TABLE - every route the application declares, wherever it declares it.
 *
 * Ported from the original Angular extractor. A route is an object literal in a const, or
 * written straight into `RouterModule.forChild([...])` inside a decorator; both are already evaluated by the
 * value evaluator, so this walks the VALUES rather than the source. Imports are flat: see
 * `TsDecls/TsTypeRef.mjs`.
 */
import { hasUnresolved, makeSpreadFolder } from './TsRouteFold.mjs';

const isObj = (v) => !!v && typeof v === 'object' && !Array.isArray(v);
const arr = (v) => (Array.isArray(v) ? v : v === undefined || v === null ? [] : [v]);

function refOf(v) {
  if (v === undefined || v === null) return null;
  if (typeof v === 'string') return { name: v };
  if (!isObj(v)) return v;
  if ('$ref' in v) return { name: v.$ref, file: v.file ?? null };
  if ('$call' in v) return { call: v.$call, args: v.$args };
  if ('$expr' in v) return { expr: v.$expr };
  return v;
}

export function rollupRoutes(store) {
  const foldSpread = makeSpreadFolder(store);
  const emitted = new Set();
  const walkedArrays = new Set();
  // An array's signature -> the TOP-LEVEL routes emitted from it, and the arrays each NgModule reaches. Kept
  // apart because the two halves are recorded on different visits, and signatures join them.
  const rootsByArray = new Map();
  const arraysByModule = new Map();
  let container = null;

  /** An unresolved CALL's arguments, under either spelling the map uses: the evaluator marks a call it could
   *  not follow as `$call`/`$args`, and a module reference normalises to `call`/`args`. Reading only the
   *  first found the routes declared in a const while missing every one written into a decorator. */
  const callArgs = (val) => {
    if (val === null || typeof val !== 'object' || Array.isArray(val)) return undefined;
    if (val.$call !== undefined) return val.$args;
    if (val.call !== undefined) return val.args;
    return undefined;
  };

  const looksLikeRoute = (o) => isObj(o)
    && ('path' in o || 'redirectTo' in o || 'loadChildren' in o || 'component' in o);

  const walk = (val, constRow, parentPath, parentId, arraySig) => {
    if (Array.isArray(val)) {
      // THE SAME ROUTE ARRAY IS REACHED TWICE, and not always from the same file: a module declares
      // `export const appRoutes` in one file and calls `forRoot(appRoutes)` in another, and the evaluator
      // RESOLVES that identifier so both walks see the same fully evaluated array. The ARRAY's own content
      // identifies it, with the parent path keeping two identical child arrays under different parents apart.
      const signature = `${parentPath}|${JSON.stringify(val)}`;
      let sig = arraySig;
      if (val.some(looksLikeRoute)) {
        sig = signature;
        // REGISTERED BEFORE THE DEDUP: a module reaching an array a const already contributed walks no
        // further - correctly, the rows exist - and registering after the early return would record the
        // association only for modules whose routes are written inline, which is the minority shape.
        if (container !== null && parentId === null) {
          const seen = arraysByModule.get(container);
          if (seen) seen.push(signature);
          else arraysByModule.set(container, [signature]);
        }
        if (walkedArrays.has(signature)) return;
        walkedArrays.add(signature);
      }
      val.forEach((v) => walk(v, constRow, parentPath, parentId, sig));
      return;
    }
    // A ROUTE ARRAY PASSED TO A CALL IS STILL A ROUTE ARRAY. Several modules write their routes inline as
    // `RouterModule.forChild([...])`; the evaluator records the unfollowable call with its ARGUMENTS
    // evaluated, so the routes are right there. Walking only bare arrays left those modules' routes unrecorded.
    const args = callArgs(val);
    if (args !== undefined && !looksLikeRoute(val)) {
      walk(args, constRow, parentPath, parentId, arraySig);
      return;
    }
    if (!looksLikeRoute(val)) return;

    // The evaluator MERGES an object-shaped spread into the parent, so an unfollowable spread call lands as
    // `$call`/`$args` keys ON THE ROUTE OBJECT - not under `$spread`. Detecting only `$spread` found 0 of
    // the affected routes; `$target` travels with it, because the callee's declaration is the whole basis.
    const spread = val.$spread ?? (val.$call !== undefined
      ? { $call: val.$call, $args: val.$args, $target: val.$target } : undefined);
    const folded = isObj(spread) ? foldSpread(spread) : null;
    // SPREAD-FIRST: the object's OWN properties win over what the callee contributed, and a key both sides
    // declare is NAMED on the row rather than resolved silently - the evaluator keeps no record of where the
    // spread stood among the properties.
    const effective = folded ? { ...folded.value, ...val } : val;
    const shadowed = folded ? Object.keys(folded.value).filter((k) => val[k] !== undefined) : [];
    const unresolvedSpread = spread !== undefined && spread !== null && folded === null;
    const full = [parentPath, effective.path].filter((s) => typeof s === 'string' && s !== '').join('/');

    // ONE ROUTE, SEEN TWICE, IS STILL ONE ROUTE: walking both the const and the decorator without this
    // produced nearly two rows per route. Keyed on the route's own identity WITHIN ITS FILE, so two genuinely
    // different routes are never merged.
    const componentName = refOf(effective.component)?.name;
    const identity = `${String(constRow.file)}|${full}|${String(componentName ?? '')}`
      + `|${String(effective.redirectTo ?? '')}`;
    if (emitted.has(identity)) return;
    emitted.add(identity);

    const fields = {
      path: effective.path ?? null, full_path: full || null,
      component: refOf(effective.component), redirect_to: effective.redirectTo ?? null,
      load_children: effective.loadChildren ?? null,
      load_component: effective.loadComponent ?? null,
      can_activate: arr(effective.canActivate).map(refOf),
      can_load: arr(effective.canLoad).map(refOf),
      guards: arr(effective.canActivateChild).map(refOf),
      data: effective.data ?? null,
      // Two fields a route may declare only through a factory, and which the fold now knows.
      // Published because a field the map HAS and does not write is indistinguishable from one it lacks.
      resolve: effective.resolve ?? null,
      run_guards_and_resolvers: effective.runGuardsAndResolvers ?? null,
      outlet: effective.outlet ?? null,
    };
    // A NESTED ROUTE'S PARENT IS A ROW, NOT A PREFIX. `full_path` carried the composition and nothing carried
    // the LINK, so walking back up meant re-deriving parentage from a string.
    const id = store.add('routes', 'rt', {
      const: constRow.id, file: constRow.file, const_name: constRow.name,
      parent_route: parentId ?? undefined,
      ...fields,
      children: Array.isArray(effective.children) ? effective.children.length : 0,
      // `false` = the fields above are the WHOLE story. `true` = something they hold is still unknown.
      fields_incomplete: unresolvedSpread || Object.values(fields).some(hasUnresolved),
      unresolved_spread: unresolvedSpread ? spread : undefined,
      folded_from: folded ? folded.from : undefined,
      folded_shadowed: shadowed.length ? shadowed : undefined,
    });
    if (parentId === null && arraySig !== null) {
      const roots = rootsByArray.get(arraySig);
      if (roots) roots.push(id);
      else rootsByArray.set(arraySig, [id]);
    }
    if (Array.isArray(effective.children)) walk(effective.children, constRow, full, id, arraySig);
  };

  /** Does this value CONTAIN a route array, at any depth reachable without guessing? STRUCTURAL: an array
   *  whose entries carry route properties, and an unresolved CALL's arguments too. Cycle detection, not a
   *  depth budget - an evaluated value can share sub-objects. */
  const containsRoute = (val, seen) => {
    if (val === null || typeof val !== 'object' || seen.has(val)) return false;
    seen.add(val);
    if (Array.isArray(val)) return val.some((v) => looksLikeRoute(v) || containsRoute(v, seen));
    const args = callArgs(val);
    return args !== undefined && containsRoute(args, seen);
  };

  for (const c of store.table('consts')) {
    if (containsRoute(c.value, new Set())) walk(c.value, c, '', null, null);
  }
  // A ROUTE ARRAY NEED NOT BE A CONST AT ALL: several modules write it straight into the decorator, so it
  // never becomes a `consts` row. The evaluated decorator argument holds exactly the same shape.
  for (const m of store.table('ng_modules')) {
    if (!containsRoute(m.imports, new Set())) continue;
    container = m.id;
    try {
      walk(m.imports, m, '', null, null);
    } finally {
      container = null;
    }
  }

  const moduleRoots = new Map();
  for (const [module, signatures] of arraysByModule) {
    const roots = [...new Set(signatures)].flatMap((s) => rootsByArray.get(s) ?? []);
    if (roots.length) moduleRoots.set(module, roots);
  }
  return moduleRoots;
}
