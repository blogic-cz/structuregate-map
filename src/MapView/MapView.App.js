/*
 * MapView.App.js - the page: the data out of its own `<script type="application/json">`, the model, and the four
 * views wired to one selection - a node clicked anywhere is the node every panel shows.
 */
(function (root) {
  'use strict';

  const M = root.MapModel, P = root.MapPanels;
  const $ = (id) => document.getElementById(id);
  const FRONTIER = 120;

  const data = JSON.parse($('map-data').textContent);
  const model = M.load(data);
  const unreached = M.unreached(model);
  const state = { view: 'graph', mode: 'structure', visible: M.frontier(model, FRONTIER), focus: null, selected: null };

  $('title').textContent = model.root.name;
  $('summary').textContent = model.files.length + ' files · ' + model.functions.length + ' functions · ' +
    data.fileEdges.length + ' file edges · ' + data.functionEdges.length + ' call edges' +
    (data.truncated ? ' (heaviest kept)' : '');

  const tip = $('tip');
  const actions = {
    select: (id) => select(id),
    open: (id) => open(id),
    neighbourhood: (id) => neighbourhood(id),
    unreached,
  };
  const graph = new root.MapGraph.GraphView($('graph'), model, {
    select: (id) => select(id),
    open: (id) => open(id),
    close: (id) => close(id),
  });
  const matrix = new P.MatrixView($('matrix'), model, {
    tip,
    pick: (cell) => select(cell.from),
  });
  const treemap = new P.TreemapView($('treemap'), model, {
    tip,
    select: (id) => select(id),
    crumbs: (folder) => crumbs(folder),
  });

  function status(text) {
    $('status').textContent = text;
  }

  function showGraph() {
    if (state.mode === 'structure') {
      const shown = graph.showStructure(state.visible);
      status(shown.nodes + ' nodes, ' + shown.edges + ' edges' + (shown.cycles ? ', ' + shown.cycles + ' in cycles' : '') +
        ' - double-click a folder to open it, right-click to close it');
    } else {
      const shown = graph.showNeighbourhood(state.focus, +$('hops').value, $('direction').value);
      status('neighbourhood of ' + label(state.focus) + ': ' + shown.nodes + ' nodes, ' + shown.edges + ' edges' +
        (shown.truncated ? ' - stopped at ' + shown.nodes + ' nodes: take fewer hops or one direction' : ''));
    }
    $('back').hidden = state.mode === 'structure';
    graph.setSelected(state.selected);
  }

  function showMatrix() {
    const shown = matrix.show(state.visible);
    status(shown.nodes + ' rows - ' + shown.edges + ' dependencies, ' + shown.upward +
      ' of them above the diagonal (red): each one closes a cycle' + (shown.left ? ' · ' + shown.left + ' with no dependency left out' : ''));
  }

  function render() {
    for (const view of ['graph', 'matrix', 'treemap']) {
      $(view + '-pane').hidden = state.view !== view;
      $('tab-' + view).classList.toggle('active', state.view === view);
    }
    if (state.view === 'graph') showGraph();
    if (state.view === 'matrix') showMatrix();
    if (state.view === 'treemap') {
      treemap.show();
      status((treemap.current.path || treemap.current.name) + '/: ' + treemap.current.count + ' files, ' + treemap.current.lines +
        ' lines - area is lines; click a folder to go in, a crumb to go back');
    }
  }

  function select(id) {
    state.selected = id;
    P.renderDetails($('details'), model, id, actions);
    if (state.view === 'graph') graph.setSelected(id);
    for (const row of document.querySelectorAll('#tree .row.selected')) row.classList.remove('selected');
    const row = id && document.querySelector('#tree .row[data-id="' + CSS.escape(id) + '"]');
    if (row) { row.classList.add('selected'); row.scrollIntoView({ block: 'nearest' }); }
  }

  function open(id) {
    if (id.startsWith('d:')) {
      state.mode = 'structure';
      if (!state.visible.has(id)) state.visible = reveal(id);
      state.visible = M.expand(model, state.visible, id);
      state.view = 'graph';
      render();
      select(id);
    } else neighbourhood(id);
  }

  /** A folder not on screen yet: open every folder above it until it is. */
  function reveal(id) {
    let visible = state.visible;
    const chain = [];
    for (let at = M.node(model, id); at && at !== model.root; at = at.parent) chain.unshift(at);
    for (const folder of chain) if (!visible.has(folder.id) && folder.parent && visible.has(folder.parent.id)) visible = M.expand(model, visible, folder.parent.id);
    return visible;
  }

  function close(id) {
    state.visible = M.collapse(model, state.visible, id);
    state.mode = 'structure';
    render();
  }

  function neighbourhood(id) {
    if (id.startsWith('d:')) return open(id);
    state.mode = 'neighbourhood';
    state.focus = id;
    state.view = 'graph';
    render();
    select(id);
  }

  function label(id) {
    const n = M.node(model, id);
    return n ? n.path || n.name : id;
  }

  function crumbs(folder) {
    const box = $('crumbs');
    box.replaceChildren();
    const chain = [];
    for (let at = folder; at; at = at.parent) chain.unshift(at);
    chain.forEach((f, i) => {
      if (i) box.append(' / ');
      box.append(P.el('a', { href: '#', text: f.name, onclick: (e) => { e.preventDefault(); treemap.show(f); select(f.id); } }));
    });
  }

  // SEARCH: files, folders and functions by any part of their name; Enter opens the first.
  const index = [...[...model.folders.values()].filter((f) => f !== model.root).map((f) => ({ id: f.id, text: f.path + '/' })),
    ...model.files.map((f) => ({ id: f.id, text: f.path })),
    ...model.functions.map((f) => ({ id: f.id, text: f.name + '  ' + model.files[f.file].path }))];
  const search = $('search'), results = $('results');
  search.addEventListener('input', () => {
    const q = search.value.trim().toLowerCase();
    results.replaceChildren();
    if (q.length < 2) { results.hidden = true; return; }
    const found = [];
    for (const entry of index) {
      if (entry.text.toLowerCase().includes(q)) found.push(entry);
      if (found.length >= 30) break;
    }
    for (const entry of found) {
      results.append(P.el('div', { class: 'link', onclick: () => pick(entry.id) }, P.el('span', { class: 'kind', text: entry.id.slice(0, 1) }),
        P.el('span', { class: 'name', text: entry.text })));
    }
    results.hidden = !found.length;
  });
  search.addEventListener('keydown', (e) => {
    if (e.key === 'Enter' && results.firstChild) results.firstChild.click();
    if (e.key === 'Escape') { results.hidden = true; search.blur(); }
  });
  function pick(id) {
    results.hidden = true;
    search.value = '';
    if (id.startsWith('d:')) open(id); else neighbourhood(id);
  }

  for (const view of ['graph', 'matrix', 'treemap']) $('tab-' + view).addEventListener('click', () => { state.view = view; render(); });
  $('back').addEventListener('click', () => { state.mode = 'structure'; render(); });
  $('reset').addEventListener('click', () => { state.mode = 'structure'; state.visible = M.frontier(model, FRONTIER); render(); });
  $('hops').addEventListener('change', () => state.mode === 'neighbourhood' && render());
  $('direction').addEventListener('change', () => state.mode === 'neighbourhood' && render());
  $('colour').addEventListener('change', (e) => graph.setColorMode(e.target.value));
  $('treemap-colour').addEventListener('change', (e) => { treemap.mode = e.target.value; treemap.draw(); });
  let resizing = 0;
  window.addEventListener('resize', () => {
    clearTimeout(resizing);
    resizing = setTimeout(() => { if (state.view === 'matrix') matrix.draw(); if (state.view === 'treemap') treemap.draw(); }, 120);
  });

  // THE LEGEND: the languages this map holds, in the colours every view uses.
  const langs = [...new Set(model.files.map((f) => f.lang))].sort();
  $('legend').replaceChildren(...langs.map((l) => P.el('span', { class: 'chip' },
    P.el('span', { class: 'dot', style: 'background:' + root.MapGraph.langColor(l) }), l)));

  P.renderTree($('tree'), model, (id) => select(id));
  render();
  select(null);
})(typeof globalThis !== 'undefined' ? globalThis : this);
