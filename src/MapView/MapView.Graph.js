/*
 * MapView.Graph.js - the WebGL view (sigma.js over a graphology graph): the STRUCTURE - the folders and files on
 * screen, every edge rolled up to them, a folder opened by a double click - and the NEIGHBOURHOOD of one file or
 * function. Positions are kept across a change, so opening a folder lays its children out around where it stood
 * instead of throwing the whole picture away.
 */
(function (root) {
  'use strict';

  const M = root.MapModel;
  const LANG = { csharp: '#7c5cd6', python: '#2f74b5', ts: '#2b8fd8', typescript: '#e0457b', rust: '#c8743b',
    powershell: '#3e9b8f', sql: '#b5832f', markdown: '#8a8f98', razor: '#9b59b6', '?': '#9aa0a6' };
  const FADED = '#d7dade';
  const CYCLE = '#d93f3f', ACYCLIC = '#9aa0a6';
  const PALETTE = ['#4e79a7', '#f28e2b', '#e15759', '#76b7b2', '#59a14f', '#edc948', '#b07aa1', '#ff9da7', '#9c755f', '#bab0ac'];

  function langColor(lang) {
    return LANG[lang] || LANG['?'];
  }

  class GraphView {
    constructor(container, model, hooks) {
      this.container = container;
      this.model = model;
      this.hooks = hooks;
      this.positions = new Map();
      this.colorMode = 'language';
      this.hovered = null;
      this.selected = null;
      this.graph = new root.graphology.DirectedGraph();
      this.renderer = new root.Sigma(this.graph, container, {
        defaultEdgeType: 'arrow',
        renderEdgeLabels: false,
        labelRenderedSizeThreshold: 7,
        labelDensity: 0.6,
        zIndex: true,
        nodeReducer: (id, attrs) => this.reduceNode(id, attrs),
        edgeReducer: (id, attrs) => this.reduceEdge(id, attrs),
      });
      this.renderer.on('enterNode', ({ node }) => { this.hovered = node; this.renderer.refresh({ skipIndexation: true }); });
      this.renderer.on('leaveNode', () => { this.hovered = null; this.renderer.refresh({ skipIndexation: true }); });
      this.renderer.on('clickNode', ({ node }) => this.hooks.select(node));
      this.renderer.on('doubleClickNode', ({ node, event }) => {
        event.preventSigmaDefault();
        this.hooks.open(node);
      });
      this.renderer.on('rightClickNode', ({ node, event }) => {
        event.original.preventDefault();
        this.hooks.close(node);
      });
      this.renderer.on('clickStage', () => this.hooks.select(null));
    }

    /** The structure: these visible folders and files, and every edge between them. */
    showStructure(visible) {
      const edges = M.rollup(this.model, visible);
      const ids = [...visible];
      this.cycles = M.cyclic(ids, edges);
      this.rebuild(ids.map((id) => ({ id, item: M.node(this.model, id) })), edges);
      return { nodes: ids.length, edges: edges.length, cycles: this.cycles.size };
    }

    /** One file or function, and N hops of what it calls and what calls it. */
    showNeighbourhood(id, hops, direction) {
      const found = M.neighbourhood(this.model, id, hops, direction);
      this.cycles = M.cyclic(found.nodes.map((n) => n.id), found.edges);
      this.rebuild(found.nodes.map((n) => ({ id: n.id, item: M.node(this.model, n.id), distance: n.distance })), found.edges, id);
      return { nodes: found.nodes.length, edges: found.edges.length, cycles: this.cycles.size, truncated: found.truncated };
    }

    rebuild(nodes, edges, centre) {
      const graph = this.graph;
      graph.clear();
      const radius = Math.max(10, Math.sqrt(nodes.length) * 12);
      nodes.forEach((n, i) => {
        const item = n.item;
        const kept = this.positions.get(n.id) || this.near(item, i, nodes.length, radius, n.distance);
        graph.addNode(n.id, {
          x: kept.x, y: kept.y,
          label: label(n.id, item),
          size: size(n.id, item),
          lang: n.id.startsWith('n:') ? this.model.files[item.file].lang : item.lang,
          kind: n.id.slice(0, 1),
          distance: n.distance,
          zIndex: n.id === centre ? 2 : 1,
        });
      });
      for (const e of edges) {
        if (!graph.hasNode(e.from) || !graph.hasNode(e.to) || graph.hasEdge(e.from, e.to)) continue;
        graph.addEdge(e.from, e.to, { size: 0.6 + Math.log2(1 + e.total) * 0.7, weight: e.total, total: e.total });
      }
      if (nodes.length > 1) {
        const fa2 = root.graphologyLibrary.layoutForceAtlas2;
        const settings = Object.assign(fa2.inferSettings(graph), { barnesHutOptimize: nodes.length > 400, gravity: 1.2 });
        fa2.assign(graph, { iterations: nodes.length > 1500 ? 60 : 180, settings });
      }
      graph.forEachNode((id, a) => this.positions.set(id, { x: a.x, y: a.y }));
      this.communities = this.colorMode === 'community' ? this.louvain() : null;
      this.renderer.refresh();
      this.renderer.getCamera().animatedReset({ duration: 300 });
    }

    /** Where a new node starts: where its folder stood, or on a circle - FA2 then moves it. */
    near(item, i, n, radius, distance) {
      let at = item && item.parent;
      if (item && item.file !== undefined && !item.path) at = this.model.files[item.file].parent;
      for (; at; at = at.parent) {
        const p = this.positions.get(at.id);
        if (p) return { x: p.x + (Math.random() - 0.5) * 4, y: p.y + (Math.random() - 0.5) * 4 };
      }
      const r = distance !== undefined ? 6 + distance * radius / 3 : radius;
      const angle = (2 * Math.PI * i) / Math.max(1, n);
      return { x: Math.cos(angle) * r, y: Math.sin(angle) * r };
    }

    louvain() {
      try {
        return root.graphologyLibrary.communitiesLouvain(this.graph, { getEdgeWeight: 'weight' });
      } catch (error) {
        return null;
      }
    }

    setColorMode(mode) {
      this.colorMode = mode;
      this.communities = mode === 'community' ? this.louvain() : null;
      this.renderer.refresh({ skipIndexation: true });
    }

    setSelected(id) {
      this.selected = id && this.graph.hasNode(id) ? id : null;
      this.renderer.refresh({ skipIndexation: true });
      if (this.selected) {
        const display = this.renderer.getNodeDisplayData(this.selected);
        if (display) this.renderer.getCamera().animate({ x: display.x, y: display.y }, { duration: 300 });
      }
    }

    colorOf(id, attrs) {
      if (this.colorMode === 'cycles') return this.cycles.has(id) ? CYCLE : ACYCLIC;
      if (this.colorMode === 'community' && this.communities) return PALETTE[this.communities[id] % PALETTE.length];
      if (this.colorMode === 'distance' && attrs.distance !== undefined) return PALETTE[Math.min(attrs.distance, PALETTE.length - 1)];
      return langColor(attrs.lang);
    }

    reduceNode(id, attrs) {
      const out = Object.assign({}, attrs, { color: this.colorOf(id, attrs) });
      const focus = this.hovered || this.selected;
      if (focus && id !== focus && !this.graph.areNeighbors(id, focus)) {
        out.color = FADED;
        out.label = '';
        out.zIndex = 0;
      }
      if (id === this.selected) { out.highlighted = true; out.zIndex = 3; }
      return out;
    }

    reduceEdge(id, attrs) {
      const out = Object.assign({}, attrs, { color: '#b9bec6' });
      const focus = this.hovered || this.selected;
      if (focus) {
        const [a, b] = this.graph.extremities(id);
        if (a !== focus && b !== focus) out.hidden = true;
        else out.color = a === focus ? '#3d7be0' : '#e0803d';
      }
      return out;
    }
  }

  function label(id, item) {
    if (!item) return id;
    if (id.startsWith('d:')) return item.name + '/  ' + item.count;
    if (id.startsWith('f:')) return item.name;
    return item.name;
  }

  function size(id, item) {
    if (!item) return 3;
    if (id.startsWith('d:')) return 5 + Math.log2(1 + item.lines) * 1.3;
    if (id.startsWith('f:')) return 3 + Math.log2(1 + item.lines) * 0.9;
    return 3 + Math.log2(2 + (item.end - item.line)) * 0.8;
  }

  root.MapGraph = { GraphView, langColor, LANG };
})(typeof globalThis !== 'undefined' ? globalThis : this);
