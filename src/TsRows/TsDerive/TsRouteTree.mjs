/**
 * THE ROUTE TREE CLOSED ACROSS A LAZY BOUNDARY - `parent_route` and `absolute_path`.
 *
 * Ported from the original Angular extractor. A lazily loaded module's routes
 * are declared in a different file from the route that loads them, so the two halves are joined by nothing
 * until the lazy target is resolved to the module and the module to the array it configures.
 * Imports are flat: see `TsDecls/TsTypeRef.mjs`.
 */

/** URL composition: empty and absent segments contribute nothing, which is what the router does with a
 *  pathless route - a grouping node, not a segment. */
const join = (base, segment) => [base, typeof segment === 'string' ? segment : '']
  .filter((s) => s !== '').join('/');

export function rollupRouteTree(store, moduleRoots, diag) {
  const routes = store.table('routes');
  const byId = new Map(routes.map((r) => [r.id, r]));
  const moduleByClass = new Map();
  for (const m of store.table('ng_modules')) moduleByClass.set(m.class, m.id);

  // COLLECTED BEFORE ANYTHING IS WRITTEN: a root claimed by two lazy parents is only visible once every
  // parent has been read, and writing the first and skipping the second would publish whichever came out of
  // the table first - the positional answer this codebase refuses everywhere else.
  const claims = new Map();
  for (const route of routes) {
    const target = route.load_children_module;
    if (target === undefined) continue;
    const module = moduleByClass.get(target);
    if (module === undefined) { diag.note('lazy_target_not_an_ng_module'); continue; }
    const roots = moduleRoots.get(module);
    if (roots === undefined) { diag.note('lazy_module_configures_no_routes'); continue; }
    for (const root of roots) {
      const claimed = claims.get(root);
      if (claimed) claimed.push(route.id);
      else claims.set(root, [route.id]);
    }
  }

  let lazy = 0;
  for (const [root, parents] of claims) {
    const row = byId.get(root);
    if (row === undefined) continue;
    if (parents.length > 1) { diag.note('route_claimed_by_several_lazy_parents'); continue; }
    // A ROOT OF A LAZILY LOADED MODULE HAS NO IN-FILE PARENT, by construction - it is the top of its own
    // array. One that already carries a parent is a shape this join did not predict, so it is left alone and
    // reported rather than overwritten.
    if (row.parent_route !== undefined) { diag.note('lazy_root_already_nested'); continue; }
    row.parent_route = parents[0];
    lazy += 1;
  }

  /**
   * The path from the application's root, composed down the parent chain.
   *
   * Built from each row's OWN `path` and not from `full_path`, which already carries the composition within
   * one declaration and would repeat every segment it shares with its parent. Memoised on the way back up,
   * with the chain being followed held on a path set - a lazy cycle is a real possibility once modules can
   * load each other, and it is refused rather than bounded by a depth number.
   */
  const absolute = new Map();
  const following = new Set();
  const pathOf = (row) => {
    const done = absolute.get(row.id);
    if (done !== undefined) return done;
    const parent = row.parent_route;
    if (parent === undefined) {
      const own = join('', row.path);
      absolute.set(row.id, own);
      return own;
    }
    if (following.has(row.id)) { diag.note('route_parent_cycle'); return null; }
    following.add(row.id);
    let base = null;
    try {
      const above = byId.get(parent);
      base = above === undefined ? null : pathOf(above);
    } finally {
      following.delete(row.id);
    }
    const value = base === null ? null : join(base, row.path);
    absolute.set(row.id, value);
    return value;
  };

  let composed = 0;
  for (const route of routes) {
    const value = pathOf(route);
    if (value === null) continue;
    route.absolute_path = value;
    composed += 1;
  }
  return { lazy, absolute: composed };
}
