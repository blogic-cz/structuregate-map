/*
 * MapView.Model.js - the map's graph as the page reasons about it: the folder tree, the frontier of folders and
 * files on screen, every edge rolled up to that frontier, a neighbourhood N hops out, cycles, the matrix order and
 * the treemap layout. PURE: no DOM, so `node` runs it in the black-box suite (tests/map/MapView.Tests.ps1).
 *
 * The data is what `rust/fbtcore/src/view/model.rs` reads out of the database, as arrays:
 *   files         [path, lang, lines, errors, entry]
 *   functions     [file, qualname, line, endLine]
 *   fileEdges     [from, to, calls, imports, refs, renders]
 *   functionEdges [from, to, calls]
 */
(function (root) {
  'use strict';

  const KINDS = ['calls', 'imports', 'refs', 'renders'];

  /** The model: the folder tree over every file, and both directions of both edge sets. */
  function load(data) {
    const files = data.files.map((f, i) => ({ id: 'f:' + i, index: i, path: f[0], lang: f[1] || '?', lines: f[2] || 0,
      errors: f[3] || 0, entry: !!f[4], name: f[0].split('/').pop() }));
    const functions = data.functions.map((f, i) => ({ id: 'n:' + i, index: i, file: f[0], name: f[1], line: f[2], end: f[3] }));
    const folders = new Map();
    const folder = (path) => {
      if (folders.has(path)) return folders.get(path);
      const parent = path === '' ? null : folder(path.includes('/') ? path.slice(0, path.lastIndexOf('/')) : '');
      const node = { id: 'd:' + path, path, name: path === '' ? (data.root || '.').split('\\').join('/').split('/').filter(Boolean).pop() || '.' : path.split('/').pop(),
        parent, folders: [], files: [], lines: 0, count: 0, langs: {} };
      if (parent) parent.folders.push(node);
      folders.set(path, node);
      return node;
    };
    folder('');
    for (const file of files) {
      const home = folder(file.path.includes('/') ? file.path.slice(0, file.path.lastIndexOf('/')) : '');
      home.files.push(file);
      file.parent = home;
      for (let at = home; at; at = at.parent) {
        at.lines += file.lines;
        at.count += 1;
        at.langs[file.lang] = (at.langs[file.lang] || 0) + file.lines + 1;
      }
    }
    for (const node of folders.values()) {
      node.folders.sort((a, b) => b.count - a.count || a.name.localeCompare(b.name));
      node.files.sort((a, b) => b.lines - a.lines || a.name.localeCompare(b.name));
      node.lang = dominant(node.langs);
    }
    const fileOut = adjacency(files.length), fileIn = adjacency(files.length);
    for (const e of data.fileEdges) {
      const weights = { calls: e[2], imports: e[3], refs: e[4], renders: e[5] };
      fileOut[e[0]].push({ to: e[1], weights });
      fileIn[e[1]].push({ to: e[0], weights });
    }
    const fnOut = adjacency(functions.length), fnIn = adjacency(functions.length);
    for (const e of data.functionEdges) {
      fnOut[e[0]].push({ to: e[1], weights: { calls: e[2] } });
      fnIn[e[1]].push({ to: e[0], weights: { calls: e[2] } });
    }
    const byFile = adjacency(files.length);
    for (const fn of functions) byFile[fn.file].push(fn);
    return { data, files, functions, folders, root: folders.get(''), fileOut, fileIn, fnOut, fnIn, byFile };
  }

  function adjacency(n) {
    return Array.from({ length: n }, () => []);
  }

  function dominant(langs) {
    let best = '?', most = -1;
    for (const [lang, weight] of Object.entries(langs)) if (weight > most) { best = lang; most = weight; }
    return best;
  }

  /** A node by id: `d:<folder path>`, `f:<file>` or `n:<function>`. */
  function node(model, id) {
    if (id.startsWith('d:')) return model.folders.get(id.slice(2));
    if (id.startsWith('f:')) return model.files[+id.slice(2)];
    if (id.startsWith('n:')) return model.functions[+id.slice(2)];
    return undefined;
  }

  /**
   * WHAT IS ON SCREEN FIRST: the root's children, then the biggest folder opened while the count stays under `max` -
   * so a tree of one big `src/` shows inside it, and a wide tree shows its top level.
   */
  function frontier(model, max) {
    const visible = new Set();
    const children = (f) => [...f.folders.map((d) => d.id), ...f.files.map((x) => x.id)];
    for (const id of children(model.root)) visible.add(id);
    for (;;) {
      let best = null;
      for (const id of visible) {
        const n = id.startsWith('d:') ? node(model, id) : null;
        if (n && (!best || n.count > best.count)) best = n;
      }
      if (!best || visible.size - 1 + best.folders.length + best.files.length > max) break;
      visible.delete(best.id);
      for (const id of children(best)) visible.add(id);
    }
    return visible;
  }

  /** Open a folder: its children take its place. */
  function expand(model, visible, id) {
    const n = node(model, id);
    if (!n || !id.startsWith('d:') || !visible.has(id)) return visible;
    const next = new Set(visible);
    next.delete(id);
    for (const d of n.folders) next.add(d.id);
    for (const f of n.files) next.add(f.id);
    return next;
  }

  /** Close the folder holding `id`: everything under it collapses back into it. */
  function collapse(model, visible, id) {
    const n = node(model, id);
    const parent = n && n.parent;
    if (!parent || parent === model.root) return visible;
    const prefix = parent.path + '/';
    const next = new Set([...visible].filter((v) => {
      const p = v.slice(2);
      return !(p === parent.path || p.startsWith(prefix) || (v.startsWith('f:') && node(model, v).path.startsWith(prefix)));
    }));
    next.add(parent.id);
    return next;
  }

  /** Which visible node each file is drawn inside: itself, or the nearest open folder above it. */
  function owners(model, visible) {
    return model.files.map((f) => {
      if (visible.has(f.id)) return f.id;
      for (let at = f.parent; at; at = at.parent) if (visible.has(at.id)) return at.id;
      return null;
    });
  }

  /** Every file edge rolled up to the visible nodes: `[{from, to, calls, imports, refs, renders, total}]`. */
  function rollup(model, visible) {
    const owner = owners(model, visible);
    const edges = new Map();
    model.fileOut.forEach((out, from) => {
      for (const e of out) {
        const a = owner[from], b = owner[e.to];
        if (!a || !b || a === b) continue;
        const key = a + '\u0000' + b;
        let edge = edges.get(key);
        if (!edge) edges.set(key, edge = { from: a, to: b, calls: 0, imports: 0, refs: 0, renders: 0, total: 0 });
        for (const k of KINDS) { edge[k] += e.weights[k] || 0; edge.total += e.weights[k] || 0; }
      }
    });
    return [...edges.values()];
  }

  /**
   * N HOPS OUT from one file or function, along what it calls (`out`), what calls it (`in`) or both. Returns the nodes
   * with their distance and every edge between two of them.
   */
  function neighbourhood(model, id, hops, direction, max = 2500) {
    const fn = id.startsWith('n:');
    const out = fn ? model.fnOut : model.fileOut, inn = fn ? model.fnIn : model.fileIn;
    const prefix = fn ? 'n:' : 'f:';
    const start = +id.slice(2);
    const distance = new Map([[start, 0]]);
    let wave = [start];
    // A HUB FOUR HOPS OUT IS THE WHOLE TREE: the walk stops at `max` nodes and says so, rather than drawing a hairball.
    let truncated = false;
    for (let d = 1; d <= hops && !truncated; d++) {
      const next = [];
      for (const at of wave) {
        const steps = [...(direction !== 'in' ? out[at] : []), ...(direction !== 'out' ? inn[at] : [])];
        for (const e of steps) {
          if (distance.has(e.to)) continue;
          if (distance.size >= max) { truncated = true; break; }
          distance.set(e.to, d);
          next.push(e.to);
        }
        if (truncated) break;
      }
      wave = next;
    }
    const edges = [];
    for (const a of distance.keys()) {
      for (const e of out[a]) if (distance.has(e.to)) edges.push({ from: prefix + a, to: prefix + e.to, total: total(e.weights) });
    }
    return { nodes: [...distance].map(([i, d]) => ({ id: prefix + i, distance: d })), edges, truncated };
  }

  function total(weights) {
    let sum = 0;
    for (const v of Object.values(weights)) sum += v || 0;
    return sum;
  }

  /** Strongly connected components (Tarjan, iterative - a deep tree must not overflow the stack): id -> component. */
  function components(ids, edges) {
    const out = new Map(ids.map((id) => [id, []]));
    for (const e of edges) if (out.has(e.from) && out.has(e.to)) out.get(e.from).push(e.to);
    const index = new Map(), low = new Map(), on = new Set(), stack = [], component = new Map();
    let counter = 0, made = 0;
    for (const startId of ids) {
      if (index.has(startId)) continue;
      const work = [[startId, 0]];
      index.set(startId, counter); low.set(startId, counter); counter++; stack.push(startId); on.add(startId);
      while (work.length) {
        const top = work[work.length - 1];
        const [v, i] = top;
        const next = out.get(v);
        if (i < next.length) {
          top[1]++;
          const w = next[i];
          if (!index.has(w)) {
            index.set(w, counter); low.set(w, counter); counter++; stack.push(w); on.add(w);
            work.push([w, 0]);
          } else if (on.has(w)) low.set(v, Math.min(low.get(v), index.get(w)));
          continue;
        }
        work.pop();
        if (work.length) { const u = work[work.length - 1][0]; low.set(u, Math.min(low.get(u), low.get(v))); }
        if (low.get(v) === index.get(v)) {
          let w;
          do { w = stack.pop(); on.delete(w); component.set(w, made); } while (w !== v);
          made++;
        }
      }
    }
    return component;
  }

  /**
   * THE MATRIX ORDER: a node's dependencies BELOW it - components in dependency order, members of one cycle kept
   * together - so every cell above the diagonal is an edge that closes a cycle.
   */
  function matrixOrder(ids, edges) {
    const component = components(ids, edges);
    // Tarjan finishes a component only after everything it reaches: its numbering is already dependencies-first.
    return [...ids].sort((a, b) => component.get(a) - component.get(b) || a.localeCompare(b));
  }

  /** The ids that sit in a cycle of more than one node. */
  function cyclic(ids, edges) {
    const component = components(ids, edges);
    const size = new Map();
    for (const c of component.values()) size.set(c, (size.get(c) || 0) + 1);
    return new Set(ids.filter((id) => size.get(component.get(id)) > 1));
  }

  /** Squarified treemap: `items` [{weight, ...}] laid into the rectangle, each given {x, y, w, h}. */
  function squarify(items, x, y, w, h) {
    const sorted = items.filter((i) => i.weight > 0).sort((a, b) => b.weight - a.weight);
    const sum = sorted.reduce((s, i) => s + i.weight, 0);
    if (!sum || w <= 0 || h <= 0) return [];
    const scale = (w * h) / sum;
    const placed = [];
    let row = [], rest = sorted.map((i) => ({ item: i, area: i.weight * scale }));
    const worst = (r, side) => {
      const s = r.reduce((t, i) => t + i.area, 0);
      let max = 0, min = Infinity;
      for (const i of r) { max = Math.max(max, i.area); min = Math.min(min, i.area); }
      return Math.max((side * side * max) / (s * s), (s * s) / (side * side * min));
    };
    const lay = () => {
      const s = row.reduce((t, i) => t + i.area, 0);
      const horizontal = w >= h;
      const thick = s / (horizontal ? h : w);
      let offset = 0;
      for (const r of row) {
        const len = r.area / thick;
        placed.push(Object.assign({}, r.item, horizontal
          ? { x, y: y + offset, w: thick, h: len } : { x: x + offset, y, w: len, h: thick }));
        offset += len;
      }
      if (horizontal) { x += thick; w -= thick; } else { y += thick; h -= thick; }
      row = [];
    };
    while (rest.length) {
      const side = Math.min(w, h);
      const next = rest[0];
      if (!row.length || worst(row.concat(next), side) <= worst(row, side)) { row.push(next); rest = rest.slice(1); }
      else lay();
    }
    if (row.length) lay();
    return placed;
  }

  /** Files nothing in the map reaches: no incoming edge, not an entry point. */
  function unreached(model) {
    return new Set(model.files.filter((f) => !model.fileIn[f.index].length && !f.entry).map((f) => f.index));
  }

  const api = { load, node, frontier, expand, collapse, owners, rollup, neighbourhood, components, matrixOrder, cyclic,
    squarify, unreached, KINDS };
  if (typeof module === 'object' && module.exports) module.exports = api;
  root.MapModel = api;
})(typeof globalThis !== 'undefined' ? globalThis : this);
