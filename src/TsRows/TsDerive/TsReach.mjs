/**
 * WHICH ENTRY POINTS REACH A COMPONENT, and whether it is exclusive to one NgModule area.
 *
 * Ported from the original Angular extractor. "Nothing renders this component" is only a
 * claim when the whole frontend was parsed; in a narrower scope the templates that would render it were
 * never read, so `components.unused` stays NULL rather than becoming a false positive on hundreds of live
 * components. Imports are flat: see `TsDecls/TsTypeRef.mjs`.
 */
import { slash } from './TsPaths.mjs';

const asRef = (v) => (v != null && typeof v === 'object' ? v : null);
const str = (v) => (typeof v === 'string' ? v : null);

export function rollupComponentReach(store, { wholeApp }) {
  const classes = store.table('classes');
  const classIds = new Set(classes.map((c) => c.id));
  const fileIdBy = new Map();
  for (const f of store.table('files')) {
    fileIdBy.set(f.path, f.id);
    fileIdBy.set(f.abs, f.id);
  }
  const classByFileName = new Map(classes.map((c) => [`${String(c.file)}#${String(c.name)}`, c.id]));
  // A CLASS IS RESOLVED BY FILE + NAME, never by name alone: a monorepo can have two `AppComponent`s, two
  // `SharedModule`s and duplicate `TooltipComponent`s answering to one name.
  const classOfRef = (ref) => {
    const name = str(ref?.name);
    const file = str(ref?.file);
    if (name === null || file === null) return null;
    const fid = fileIdBy.get(slash(file));
    return fid === undefined ? null : classByFileName.get(`${String(fid)}#${name}`) ?? null;
  };

  const areaOfClass = new Map();
  const bootstrapped = new Set();
  for (const m of store.table('ng_modules')) {
    for (const d of m.declarations ?? []) {
      const cid = classOfRef(asRef(d));
      if (cid != null && !areaOfClass.has(cid)) areaOfClass.set(cid, m.id);
    }
    for (const b of m.bootstrap ?? []) {
      const cid = classOfRef(asRef(b));
      if (cid != null) bootstrapped.add(cid);
    }
  }

  const edges = new Map();
  const rendered = new Set();
  for (const r of store.table('renders')) {
    if (r.to_class == null || r.kind !== 'component') continue;
    rendered.add(r.to_class);
    if (r.scope === 'out_of_scope') continue;
    if (r.from_class == null || r.from_class === r.to_class) continue;
    const list = edges.get(r.from_class);
    if (list) { if (!list.includes(r.to_class)) list.push(r.to_class); }
    else edges.set(r.from_class, [r.to_class]);
  }

  // Entry point -> the routes that name it. A class routed from several paths is ONE entry point, and every
  // route that reaches it is published rather than one being chosen.
  const routesOfRoot = new Map();
  const routed = new Set();
  for (const r of store.table('routes')) {
    const cid = r.component_id;
    if (cid == null || !classIds.has(cid)) continue;
    routed.add(cid);
    const list = routesOfRoot.get(cid);
    if (list) list.push(r.id); else routesOfRoot.set(cid, [r.id]);
  }
  const roots = new Set([...routesOfRoot.keys(), ...bootstrapped]);

  /** Breadth-first from one entry point: the visited set terminates it, so the cycles these graphs contain -
   *  a wrapper rendering a child that renders the wrapper's sibling - cost nothing and truncate nothing. */
  const reachedBy = new Map();
  for (const root of roots) {
    const area = areaOfClass.get(root) ?? null;
    const routeIds = routesOfRoot.get(root) ?? [];
    const depthOf = new Map([[root, 0]]);
    let frontier = [root];
    while (frontier.length) {
      const next = [];
      for (const current of frontier) {
        const depth = depthOf.get(current) ?? 0;
        let hit = reachedBy.get(current);
        if (!hit) {
          hit = { roots: new Set(), routes: new Set(), areas: new Set(), areaUnknown: false, minDepth: depth };
          reachedBy.set(current, hit);
        }
        hit.roots.add(root);
        for (const rid of routeIds) hit.routes.add(rid);
        if (area == null) hit.areaUnknown = true;
        else hit.areas.add(area);
        if (depth < hit.minDepth) hit.minDepth = depth;
        for (const to of edges.get(current) ?? []) {
          if (depthOf.has(to)) continue;
          depthOf.set(to, depth + 1);
          next.push(to);
        }
      }
      frontier = next;
    }
  }

  const componentOfClass = new Map();
  for (const c of store.table('components')) if (!componentOfClass.has(c.class)) componentOfClass.set(c.class, c);

  let exclusive = 0;
  let shared = 0;
  for (const c of classes) {
    const hit = reachedBy.get(c.id);
    if (!hit) continue;
    const isExclusive = !hit.areaUnknown && hit.areas.size === 1;
    if (isExclusive) exclusive += 1; else shared += 1;
    store.add('component_reach', 'cr', {
      class: c.id, component: componentOfClass.get(c.id)?.id ?? null, file: c.file, name: c.name,
      is_root: hit.minDepth === 0, min_depth: hit.minDepth,
      roots: [...hit.roots], root_count: hit.roots.size,
      routes: [...hit.routes], route_count: hit.routes.size,
      areas: [...hit.areas], area_count: hit.areas.size,
      // An entry point no NgModule declares cannot be attributed, and a row reached from one says so instead
      // of letting a short `areas` list read as ownership.
      area_unknown: hit.areaUnknown,
      exclusive: isExclusive,
    });
  }

  let unused = 0;
  for (const c of store.table('components')) {
    const never = !rendered.has(c.class) && !routed.has(c.class) && !bootstrapped.has(c.class);
    c.unused = wholeApp ? never : null;
    if (wholeApp && never) unused += 1;
  }
  return { roots: roots.size, classes: reachedBy.size, exclusive, shared, unused: wholeApp ? unused : null };
}
