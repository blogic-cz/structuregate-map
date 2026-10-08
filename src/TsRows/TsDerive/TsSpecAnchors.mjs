/**
 * WHICH JOIN REACHES A ROW'S OWN FILE, PROVEN PER FIELD.
 *
 * Ported from the original Angular extractor. Every consumer of this map asks the
 * same question of every row - "what file is this from" - and before the spec existed each reader spelled
 * its own chain of `file` / `class` / `member` / `template` / `node`, so two readers disagreed about the
 * same row and the one resolving fewer rows was indistinguishable from a map that lacked them.
 *
 * The obvious graph walk is WRONG, which is why each step is proven rather than assumed: `renders` carries
 * `from_class`, so a walk anchors a render edge to the component's `.ts` when the row was read from its
 * `.html`. Imports are flat: see `TsDecls/TsTypeRef.mjs`.
 */

const ROOT = 'files';

/** The column that MEANS "the file this row was read from" - the map's own vocabulary, set at every site
 *  that writes a row. Wherever a row also carries a line the claim is checked rather than trusted; a
 *  column that points at some OTHER file is named for what it resolved (`resolved_file`, `template_file`). */
const FILE_COLUMN = 'file';

/**
 * The source lines a row SPANS, in its own file's numbering.
 *
 * `line`/`end_line` for a declaration or a node, `lines` for a whole file, and `lines` shifted by
 * `line_offset` for an inline template - which is not a detail: without the offset, a small share of
 * `template_nodes` fall outside the template they belong to, and a spec that disqualified `.template`
 * over them would push every template row onto a longer route for no reason.
 */
function rowRange(row) {
  const line = row.line;
  if (typeof line === 'number') {
    const end = row.end_line;
    return { start: line, end: typeof end === 'number' ? end : line };
  }
  const lines = row.lines;
  if (typeof lines === 'number') {
    const offset = row.line_offset;
    const base = typeof offset === 'number' ? offset : 0;
    return { start: base + 1, end: base + lines };
  }
  return null;
}

/** Every `<table>.<field>` from the published join spec, grouped by table, targets split as `joins` words
 *  them. */
function candidatesByTable(joins, present) {
  const out = new Map();
  for (const [key, to] of Object.entries(joins)) {
    const cut = key.lastIndexOf('.');
    const table = key.slice(0, cut);
    if (!present.has(table)) continue;
    const list = out.get(table) ?? [];
    list.push({ field: key.slice(cut + 1), to, targets: to.split('|').map((t) => t.trim()) });
    out.set(table, list);
  }
  return out;
}

/** The tables an id can belong to, by its published prefix - a LIST, because some prefixes serve several. */
function tablesOfId(id, idScheme) {
  const cut = id.indexOf(':');
  if (cut <= 0) return [];
  const named = idScheme[id.slice(0, cut)];
  return named ? named.split('|').map((t) => t.trim()) : [];
}

export function deriveAnchors(store, idScheme, joins, tables) {
  const present = new Set(tables);
  const candidates = candidatesByTable(joins, present);
  const proven = new Map();
  const hops = new Map([[ROOT, 0]]);
  const range = new Map();
  const ranged = new Set();

  const loadRanges = (table) => {
    if (ranged.has(table) || !present.has(table)) return;
    ranged.add(table);
    for (const row of store.table(table)) range.set(row.id, rowRange(row));
  };

  const record = (table, step) => {
    const list = proven.get(table) ?? [];
    list.push(step);
    // Shortest route first, then by name - a tie is two fields that reach the same file, so the order only
    // has to be STABLE, and nothing may depend on which of the two a reader takes.
    list.sort((a, b) => a.hops - b.hops || a.field.localeCompare(b.field));
    proven.set(table, list);
    const reach = step.hops;
    if (!hops.has(table) || reach < hops.get(table)) hops.set(table, reach);
  };

  const reachable = (c) => {
    const depths = c.targets.map((t) => hops.get(t)).filter((d) => d !== undefined);
    return depths.length === c.targets.length ? 1 + Math.min(...depths) : null;
  };

  /** The file a row resolves to through the steps proven SO FAR. Cycles cannot occur: every step strictly
   *  decreases `hops`, and a step is only ever proven against targets that already reach `files`. */
  const fileOf = (table, row) => {
    for (const step of proven.get(table) ?? []) {
      const hit = fileOfValue(row[step.field]);
      if (hit) return hit;
    }
    return null;
  };

  /** The file ONE id resolves to - a `files` id is the answer, anything else is followed. */
  function fileOfValue(value) {
    if (typeof value !== 'string') return null;
    for (const target of tablesOfId(value, idScheme)) {
      if (target === ROOT) return value;
      const next = store.rowOf(value);
      if (!next) continue;
      const hit = fileOf(target, next);
      if (hit) return hit;
    }
    return null;
  }

  /** Containment: every row where both this row and its target have a range. One row outside settles it. */
  const containment = (table, c) => {
    for (const target of c.targets) loadRanges(target);
    let tested = 0;
    let outside = 0;
    for (const row of store.table(table)) {
      const own = rowRange(row);
      const value = row[c.field];
      const target = own && typeof value === 'string' ? range.get(value) : undefined;
      if (!own || !target) continue;
      tested += 1;
      if (own.start < target.start || own.start > target.end) outside += 1;
    }
    return { tested, outside };
  };

  /**
   * Agreement: does this field land on the same file as another route to it?
   *
   * The comparison walks the routes already ADMITTED first, and only then the field's still-unjudged peers
   * - because the fields that need this test are the ones on rows where no admitted route is present at
   * all. About half of `expressions` is that case: a template-side row carries `node` and
   * `template` and no `file`, so `file` cannot vouch for either and the two have to vouch for each other.
   * Disqualified fields are excluded from both sides, which is what keeps a refuted route
   * (`renders.from_class`) from being used as evidence for anything.
   */
  const agreement = (table, c, fields) => {
    const peers = fields.filter((o) => o.field !== c.field && !refuted.has(`${table}.${o.field}`)
      && reachable(o) !== null);
    let tested = 0;
    let differ = 0;
    for (const row of store.table(table)) {
      const mine = fileOfValue(row[c.field]);
      if (!mine) continue;
      let theirs = fileOf(table, row);
      for (const peer of peers) {
        if (theirs) break;
        theirs = fileOfValue(row[peer.field]);
      }
      if (!theirs) continue;
      tested += 1;
      if (mine !== theirs) differ += 1;
    }
    return { tested, differ };
  };

  const judged = new Set();
  const refuted = new Set();
  // CONTAINMENT, then the `file` COLUMN, then AGREEMENT - strongest evidence first, each to a fixed point:
  // proving one step can make another field's target anchorable, and admitting a step gives the next
  // agreement test something to compare against. A weaker pass only runs when no stronger one still moves.
  for (let pass = 0; pass < present.size + 2; pass++) {
    let changed = false;
    for (const [table, fields] of candidates) {
      for (const c of fields) {
        const key = `${table}.${c.field}`;
        if (judged.has(key)) continue;
        const reach = reachable(c);
        if (reach === null) continue;
        const { tested, outside } = containment(table, c);
        if (!tested) continue; // untestable here - a later pass takes it
        judged.add(key);
        if (outside) {
          refuted.add(key);
          continue;
        }
        record(table, { field: c.field, to: c.to, hops: reach, proof: 'containment', tested });
        changed = true;
      }
    }
    if (changed) continue;
    for (const [table, fields] of candidates) {
      for (const c of fields) {
        const key = `${table}.${c.field}`;
        if (judged.has(key) || c.field !== FILE_COLUMN || c.to !== ROOT) continue;
        judged.add(key);
        record(table, { field: c.field, to: c.to, hops: 1, proof: 'column', tested: 0 });
        changed = true;
      }
    }
    if (changed) continue;
    for (const [table, fields] of candidates) {
      for (const c of fields) {
        const key = `${table}.${c.field}`;
        if (judged.has(key)) continue;
        if (reachable(c) === null) continue;
        const { tested, differ } = agreement(table, c, fields);
        if (!tested) continue;
        judged.add(key);
        if (differ) {
          refuted.add(key);
          continue;
        }
        record(table, { field: c.field, to: c.to, hops: reachable(c), proof: 'agreement', tested });
        changed = true;
      }
    }
    if (!changed) break;
  }

  const anchors = {};
  for (const table of [...proven.keys()].sort()) anchors[table] = proven.get(table);
  // ROOT is not unanchored, it is the root: a `files` row IS the file, so there is no route to publish.
  const unanchored = [...candidates.keys()].filter((t) => t !== ROOT && !proven.has(t)).sort();
  return { anchors, unanchored };
}
