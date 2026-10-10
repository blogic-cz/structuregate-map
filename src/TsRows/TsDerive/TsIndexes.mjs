/**
 * THE INVERTED VIEWS: a selector to what declares it, a class-level render graph, a translation key to every
 * reference of it, and an identifier to every gate that reads it.
 *
 * Ported from the original Angular extractor. Each is a question asked often enough that
 * a consumer building it from the base tables would be writing the same join every time. Imports are flat:
 * see `TsDecls/TsTypeRef.mjs`.
 */

/** Every selector a component or directive declares, and what it points at. */
export function rollupSelectors(store) {
  for (const table of ['components', 'directives']) {
    for (const row of store.table(table)) {
      if (!row.selector) continue;
      store.add('selector_index', 'si', {
        selector: row.selector, target: row.id,
        kind: table === 'components' ? 'component' : 'directive',
        class: row.class, name: row.name, file: row.file,
        template: row.template_file ?? null,
      });
    }
  }
}

/** The class-level render graph with a per-edge count - the "who embeds whom" view. */
export function rollupRenderGraph(store) {
  const aggregate = new Map();
  for (const r of store.table('renders')) {
    // AN OUT-OF-SCOPE CANDIDATE IS NOT AN EDGE (divergence #12). A selector matched across the whole workspace
    // links a native `<header>` to any component named `header`, in any app; Angular instantiates it only where
    // the template's scope reaches it. `renders` keeps the candidate with its `scope`; the graph - and the atlas
    // and view walking it - keeps what really renders, as `component_reach` already did.
    if (r.scope === 'out_of_scope') continue;
    const key = `${String(r.from_class)}>${String(r.to_class)}>${String(r.kind)}`;
    const hit = aggregate.get(key);
    // A DYNAMIC EDGE HAS NO TAG. Coercing it would put the string "null" among real element names, which
    // reads as an element the frontend does not have; the edge still counts, and `renders.via` says how it
    // was proved.
    if (hit) {
      hit.count += 1;
      if (r.tag != null) hit.tags.add(String(r.tag));
      continue;
    }
    aggregate.set(key, {
      from_class: r.from_class, to_class: r.to_class, to_name: String(r.to_name),
      kind: String(r.kind), count: 1, tags: new Set(r.tag == null ? [] : [String(r.tag)]),
    });
  }
  for (const v of aggregate.values()) store.add('render_graph', 'rg', { ...v, tags: [...v.tags] });
}

/** Every translation key the source references, with every reference of it. */
export function rollupI18nIndex(store) {
  const byKey = new Map();
  let dynamic = 0;
  for (const r of store.table('i18n_refs')) {
    if (!r.key) { dynamic += 1; continue; }
    let entry = byKey.get(r.key);
    if (!entry) {
      entry = { key: r.key, refs: [], kinds: new Set(), files: new Set() };
      byKey.set(r.key, entry);
    }
    entry.refs.push(r.id);
    entry.kinds.add(String(r.kind));
    if (r.file) entry.files.add(String(r.file));
  }
  for (const e of [...byKey.values()].sort((a, b) => a.key.localeCompare(b.key))) {
    store.add('i18n_index', 'ki', {
      key: e.key, count: e.refs.length, refs: e.refs,
      kinds: [...e.kinds], files: [...e.files],
      prefix: e.key.split('.')[0], section: e.key.split('.').slice(0, 2).join('.'),
    });
  }
  return { unique_keys: byKey.size, dynamic_refs: dynamic };
}

/** Which gate expressions read which identifiers - the flag-to-gated-node inversion. */
export function rollupGateIndex(store) {
  const byIdentifier = new Map();
  for (const g of store.table('gates')) {
    for (const id of g.identifiers ?? []) {
      let entry = byIdentifier.get(id);
      if (!entry) {
        entry = { identifier: id, gates: [], components: new Set() };
        byIdentifier.set(id, entry);
      }
      entry.gates.push(g.id);
      entry.components.add(g.component);
    }
  }
  for (const e of [...byIdentifier.values()].sort((a, b) => b.gates.length - a.gates.length)) {
    store.add('gate_index', 'gi', {
      identifier: e.identifier, gate_count: e.gates.length,
      // EVERY gate id. `gate_count` was true while the array stopped at 500, so the most-used identifier -
      // exactly the one worth inspecting - was the one whose list was incomplete.
      component_count: e.components.size, gates: e.gates,
    });
  }
}
