/**
 * THE GATES A NODE SITS UNDER, outermost first.
 *
 * Ported from the original Angular extractor. A gate row says what one binding decides;
 * this says what has to hold for a node to render at all, which is the question a consumer asks about a
 * translation key or a component. Imports are flat: see `TsDecls/TsTypeRef.mjs`.
 */

export function rollupGateChains(store) {
  const gatesByNode = new Map();
  for (const g of store.table('gates')) {
    if (g.node == null) continue;
    const hit = gatesByNode.get(g.node);
    if (hit) hit.push(g.id);
    else gatesByNode.set(g.node, [g.id]);
  }
  const nodeRows = store.table('template_nodes');
  if (!gatesByNode.size || !nodeRows.length) return { nodes: 0, deepest: 0 };
  const rowById = new Map(nodeRows.map((n) => [n.id, n]));

  /**
   * A node's chain, memoized per node so the whole table costs ONE pass rather than one walk per node.
   *
   * A node with no gates of its own SHARES its parent's array instead of copying it: the arrays are written
   * straight into the rows and serialization expands each reference anyway, so sharing costs nothing at write
   * time and avoids allocating a copy per node in a table this size.
   */
  const chainById = new Map();
  const chainOf = (id) => {
    const cached = chainById.get(id);
    if (cached) return cached;
    const path = [];
    const onPath = new Set();
    let current = id;
    while (current != null && !onPath.has(current) && !chainById.has(current)) {
      onPath.add(current);
      path.push(current);
      current = rowById.get(current)?.parent ?? null;
    }
    let chain = current == null ? [] : chainById.get(current) ?? [];
    // Folded from the root DOWN, which is what makes the result outermost-first and every ancestor's chain a
    // prefix of its children's.
    for (let i = path.length - 1; i >= 0; i -= 1) {
      const own = gatesByNode.get(path[i]);
      chain = own ? [...chain, ...own] : chain;
      chainById.set(path[i], chain);
    }
    return chainById.get(id) ?? [];
  };

  let nodes = 0;
  let deepest = 0;
  for (const n of nodeRows) {
    const chain = chainOf(n.id);
    if (!chain.length) continue;
    n.gate_chain = chain;
    n.gate_depth = chain.length;
    nodes += 1;
    if (chain.length > deepest) deepest = chain.length;
  }
  return { nodes, deepest };
}
