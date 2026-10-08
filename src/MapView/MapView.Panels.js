/*
 * MapView.Panels.js - everything around the graph: the folder tree, the details of what is selected (with the
 * `--map-query` command that prints it), the DEPENDENCY MATRIX and the TREEMAP. Both are drawn on a canvas: a
 * matrix of a few hundred folders or a treemap of thousands of files is one draw, not thousands of elements.
 */
(function (root) {
  'use strict';

  const M = root.MapModel;
  const color = (lang) => root.MapGraph.langColor(lang);
  const el = (tag, attrs, ...kids) => {
    const e = document.createElement(tag);
    for (const [k, v] of Object.entries(attrs || {})) {
      if (k === 'onclick') e.addEventListener('click', v);
      else if (k === 'text') e.textContent = v;
      else e.setAttribute(k, v);
    }
    for (const kid of kids) if (kid) e.append(kid);
    return e;
  };

  /** The folder tree: opened on demand, a row per folder and file, a click selects. */
  function renderTree(container, model, select) {
    container.replaceChildren();
    const row = (item, depth) => {
      const folder = item.id.startsWith('d:');
      const line = el('div', { class: 'row' + (folder ? ' folder' : ''), 'data-id': item.id, style: '--depth:' + depth });
      const caret = el('span', { class: 'caret', text: folder ? '▸' : '' });
      line.append(caret, el('span', { class: 'dot', style: 'background:' + color(item.lang) }),
        el('span', { class: 'name', text: item.name }), el('span', { class: 'count', text: folder ? item.count : item.lines }));
      const kids = el('div', { class: 'kids' });
      line.addEventListener('click', (event) => {
        event.stopPropagation();
        if (folder && (event.target === caret || !kids.childElementCount)) toggle();
        select(item.id);
      });
      const toggle = () => {
        if (kids.childElementCount) { kids.replaceChildren(); caret.textContent = '▸'; return; }
        caret.textContent = '▾';
        for (const d of item.folders) kids.append(row(d, depth + 1));
        for (const f of item.files) kids.append(row(f, depth + 1));
      };
      return el('div', {}, line, kids);
    };
    const top = row(model.root, 0);
    container.append(top);
    top.firstChild.click();
  }

  /** What is selected, its edges in both directions, and the command that prints it. */
  function renderDetails(container, model, id, actions) {
    container.replaceChildren();
    if (!id) {
      container.append(el('p', { class: 'hint', text: 'Click a node, a tree row or a matrix cell. Double-click a folder to open it, right-click to close it.' }));
      return;
    }
    const item = M.node(model, id);
    const db = model.data.db || '<db>';
    const list = (title, entries) => {
      if (!entries.length) return null;
      const box = el('div', { class: 'list' }, el('h4', { text: title + ' (' + entries.length + ')' }));
      for (const e of entries.slice(0, 40)) {
        box.append(el('div', { class: 'link', onclick: () => actions.select(e.id) },
          el('span', { class: 'dot', style: 'background:' + color(e.lang) }), el('span', { class: 'name', text: e.label }),
          el('span', { class: 'count', text: e.weight })));
      }
      if (entries.length > 40) box.append(el('div', { class: 'hint', text: '… ' + (entries.length - 40) + ' more' }));
      return box;
    };
    const fileEntries = (edges) => [...edges].sort((a, b) => sum(b.weights) - sum(a.weights))
      .map((e) => ({ id: 'f:' + e.to, label: model.files[e.to].path, lang: model.files[e.to].lang, weight: kinds(e.weights) }));
    const command = (text) => el('pre', { class: 'command', title: 'click to copy', onclick: () => navigator.clipboard && navigator.clipboard.writeText(text), text });
    if (id.startsWith('d:')) {
      const langs = Object.entries(item.langs).sort((a, b) => b[1] - a[1]).map(([l]) => l).join(', ');
      container.append(el('h3', { text: (item.path || item.name) + '/' }),
        el('p', { text: item.count + ' files, ' + item.lines + ' lines - ' + langs }),
        el('button', { onclick: () => actions.open(id), text: 'Open in the graph' }));
      const inside = (f) => item.path === '' || f.path.startsWith(item.path + '/');
      const out = new Map(), inn = new Map();
      model.files.forEach((f, i) => {
        if (!inside(f)) return;
        for (const e of model.fileOut[i]) if (!inside(model.files[e.to])) out.set(e.to, (out.get(e.to) || 0) + sum(e.weights));
        for (const e of model.fileIn[i]) if (!inside(model.files[e.to])) inn.set(e.to, (inn.get(e.to) || 0) + sum(e.weights));
      });
      const entries = (m) => [...m].sort((a, b) => b[1] - a[1]).map(([i, w]) => ({ id: 'f:' + i, label: model.files[i].path, lang: model.files[i].lang, weight: w }));
      container.append(list('Depends on', entries(out)), list('Used by', entries(inn)));
      return;
    }
    if (id.startsWith('f:')) {
      const flags = [item.lang, item.lines + ' lines', item.errors ? item.errors + ' errors' : '', item.entry ? 'entry point' : '',
        actions.unreached.has(item.index) ? 'nothing in the map reaches it' : ''].filter(Boolean).join(' · ');
      container.append(el('h3', { text: item.path }), el('p', { text: flags }),
        el('button', { onclick: () => actions.neighbourhood(id), text: 'Neighbourhood' }),
        command('structuregate --map-query "' + db + '" --file "' + item.path + '"'),
        list('Calls / imports', fileEntries(model.fileOut[item.index])), list('Used by', fileEntries(model.fileIn[item.index])),
        list('Functions', model.byFile[item.index].map((f) => ({ id: f.id, label: f.name + '  :' + f.line, lang: item.lang, weight: f.end - f.line + 1 }))));
      return;
    }
    const file = model.files[item.file];
    const fnEntries = (edges) => edges.map((e) => {
      const f = model.functions[e.to];
      return { id: f.id, label: f.name + '  (' + model.files[f.file].name + ')', lang: model.files[f.file].lang, weight: e.weights.calls };
    });
    container.append(el('h3', { text: item.name }), el('p', { text: file.path + ':' + item.line + '-' + item.end }),
      el('button', { onclick: () => actions.neighbourhood(id), text: 'Neighbourhood' }),
      command('structuregate --map-query "' + db + '" --cat "' + file.path + '" --lines ' + item.line + '-' + item.end),
      list('Calls', fnEntries(model.fnOut[item.index])), list('Called by', fnEntries(model.fnIn[item.index])));
  }

  function sum(weights) {
    let s = 0;
    for (const v of Object.values(weights)) s += v || 0;
    return s;
  }

  function kinds(weights) {
    return M.KINDS.filter((k) => weights[k]).map((k) => weights[k] + ' ' + k).join(', ');
  }

  /**
   * THE DEPENDENCY MATRIX of the visible nodes: row depends on column, ordered so a dependency sits below what uses
   * it - every coloured cell ABOVE the diagonal closes a cycle, and is drawn red.
   */
  class MatrixView {
    constructor(canvas, model, hooks) {
      this.canvas = canvas;
      this.model = model;
      this.hooks = hooks;
      this.tip = hooks.tip;
      canvas.addEventListener('mousemove', (e) => this.hover(e));
      canvas.addEventListener('mouseleave', () => { this.tip.hidden = true; });
      canvas.addEventListener('click', (e) => { const c = this.cell(e); if (c) this.hooks.pick(c); });
    }

    show(visible) {
      const edges = M.rollup(this.model, visible);
      // ONLY WHAT DEPENDS OR IS DEPENDED ON: a row with no cell is space that shrinks every other row.
      const linked = new Set();
      for (const e of edges) { linked.add(e.from); linked.add(e.to); }
      const ids = [...visible].filter((id) => linked.has(id));
      this.left = visible.size - ids.length;
      this.order = M.matrixOrder(ids, edges);
      this.at = new Map(this.order.map((id, i) => [id, i]));
      this.edges = new Map(edges.map((e) => [this.at.get(e.from) + ',' + this.at.get(e.to), e]));
      this.max = edges.reduce((m, e) => Math.max(m, e.total), 1);
      this.upward = edges.filter((e) => this.at.get(e.to) > this.at.get(e.from)).length;
      this.draw();
      return { nodes: ids.length, edges: edges.length, upward: this.upward, left: this.left };
    }

    draw() {
      const c = this.canvas, ratio = window.devicePixelRatio || 1;
      const width = c.clientWidth, height = c.clientHeight;
      c.width = width * ratio; c.height = height * ratio;
      const g = c.getContext('2d');
      g.setTransform(ratio, 0, 0, ratio, 0, 0);
      g.clearRect(0, 0, width, height);
      const n = this.order.length;
      // LABELS ONLY WHERE THEY FIT: below 7 px a name is unreadable, and its margin is better spent on cells - the
      // tooltip names every cell either way.
      const fit = (margin) => Math.max(1, Math.min(28, (Math.min(width, height) - margin - 10) / Math.max(1, n)));
      this.margin = Math.min(220, width * 0.3);
      this.cellSize = fit(this.margin);
      if (this.cellSize < 7) { this.margin = 10; this.cellSize = fit(this.margin); }
      const s = this.cellSize, m = this.margin;
      const style = getComputedStyle(document.body);
      g.fillStyle = style.getPropertyValue('--grid');
      g.fillRect(m, m, n * s, n * s);
      g.strokeStyle = style.getPropertyValue('--muted');
      g.beginPath(); g.moveTo(m, m); g.lineTo(m + n * s, m + n * s); g.stroke();
      for (const [key, e] of this.edges) {
        const [r, col] = key.split(',').map(Number);
        const t = Math.log(1 + e.total) / Math.log(1 + this.max);
        g.fillStyle = col > r ? 'rgba(217,63,63,' + (0.35 + 0.65 * t) + ')' : 'rgba(61,123,224,' + (0.25 + 0.75 * t) + ')';
        g.fillRect(m + col * s, m + r * s, Math.max(1, s - 0.5), Math.max(1, s - 0.5));
      }
      if (s >= 7) {
        g.fillStyle = style.getPropertyValue('--text');
        g.font = Math.min(12, s - 1) + 'px system-ui, sans-serif';
        g.textBaseline = 'middle';
        this.order.forEach((id, i) => {
          const name = short(M.node(this.model, id), id);
          g.textAlign = 'right'; g.fillText(name, m - 6, m + i * s + s / 2, m - 10);
          g.save(); g.translate(m + i * s + s / 2, m - 6); g.rotate(-Math.PI / 2); g.textAlign = 'left';
          g.fillText(name, 0, 0, m - 10); g.restore();
        });
      }
    }

    cell(event) {
      if (!this.order) return null;
      const rect = this.canvas.getBoundingClientRect();
      const col = Math.floor((event.clientX - rect.left - this.margin) / this.cellSize);
      const row = Math.floor((event.clientY - rect.top - this.margin) / this.cellSize);
      if (row < 0 || col < 0 || row >= this.order.length || col >= this.order.length) return null;
      return { from: this.order[row], to: this.order[col], edge: this.edges.get(row + ',' + col) };
    }

    hover(event) {
      const c = this.cell(event);
      if (!c) { this.tip.hidden = true; return; }
      const a = short(M.node(this.model, c.from), c.from), b = short(M.node(this.model, c.to), c.to);
      this.tip.textContent = a + '  →  ' + b + (c.edge ? '\n' + kinds(c.edge) : '\nno edge');
      this.tip.style.left = event.clientX + 14 + 'px';
      this.tip.style.top = event.clientY + 14 + 'px';
      this.tip.hidden = false;
    }
  }

  function short(item, id) {
    if (!item) return id;
    return id.startsWith('d:') ? (item.path || item.name) + '/' : item.path || item.name;
  }

  /** THE TREEMAP of one folder: area by lines, one level of children drawn inside each folder, a click drills in. */
  class TreemapView {
    constructor(canvas, model, hooks) {
      this.canvas = canvas;
      this.model = model;
      this.hooks = hooks;
      this.current = model.root;
      this.mode = 'language';
      this.unreached = M.unreached(model);
      canvas.addEventListener('click', (e) => this.click(e));
      canvas.addEventListener('mousemove', (e) => this.hover(e));
      canvas.addEventListener('mouseleave', () => { this.hooks.tip.hidden = true; });
    }

    show(folder) {
      if (folder) this.current = folder;
      this.draw();
      this.hooks.crumbs(this.current);
    }

    fill(item) {
      if (this.mode === 'unreached' && !item.files) return this.unreached.has(item.index) ? '#d93f3f' : '#9aa0a6';
      if (this.mode === 'errors' && !item.files) return item.errors ? '#d93f3f' : '#9aa0a6';
      if (this.mode === 'fanin' && !item.files) {
        const t = Math.min(1, Math.log(1 + this.model.fileIn[item.index].length) / Math.log(30));
        return 'rgba(61,123,224,' + (0.15 + 0.85 * t) + ')';
      }
      return color(item.lang);
    }

    draw() {
      const c = this.canvas, ratio = window.devicePixelRatio || 1;
      const width = c.clientWidth, height = c.clientHeight;
      c.width = width * ratio; c.height = height * ratio;
      const g = c.getContext('2d');
      g.setTransform(ratio, 0, 0, ratio, 0, 0);
      g.clearRect(0, 0, width, height);
      this.boxes = [];
      this.style = getComputedStyle(document.body);
      this.nest(g, this.current, 0, 0, width, height, 0);
    }

    /**
     * NESTED WHILE THERE IS ROOM: a folder is a labelled frame its children are laid out inside, a file a filled
     * block. A folder holding nothing but one folder (`rust/fbtcore/src`) is one frame, not three levels spent.
     */
    nest(g, folder, x, y, w, h, depth) {
      const through = (f) => {
        let at = f;
        while (at.files.length === 0 && at.folders.length === 1) at = at.folders[0];
        return at;
      };
      const kids = [...folder.folders.map((d) => ({ item: d, shown: through(d), weight: Math.max(1, d.lines) })),
        ...folder.files.map((f) => ({ item: f, weight: Math.max(1, f.lines) }))];
      const style = this.style;
      for (const box of M.squarify(kids, x, y, w, h)) {
        if (depth === 0) this.boxes.push(box);
        const item = box.item;
        const inner = box.shown || item;
        const room = box.w > 60 && box.h > 40 && box.w * box.h > 6000;
        if (item.files && depth < 4 && room) {
          g.fillStyle = depth % 2 ? style.getPropertyValue('--grid') : style.getPropertyValue('--panel');
          g.fillRect(box.x, box.y, box.w, box.h);
          this.nest(g, inner, box.x + 2, box.y + 16, box.w - 4, box.h - 18, depth + 1);
        } else {
          g.fillStyle = this.fill(item);
          g.globalAlpha = item.files ? 0.6 : 0.9;
          g.fillRect(box.x, box.y, Math.max(0, box.w - 1), Math.max(0, box.h - 1));
          g.globalAlpha = 1;
        }
        g.strokeStyle = style.getPropertyValue(depth ? '--line' : '--bg');
        g.lineWidth = depth ? 1 : 2;
        g.strokeRect(box.x, box.y, box.w, box.h);
        if (box.w > 40 && box.h > 14) {
          g.fillStyle = item.files && depth < 4 && room ? style.getPropertyValue('--text') : '#fff';
          g.font = (depth ? 11 : 12) + 'px system-ui, sans-serif';
          g.textBaseline = 'top';
          const name = item.files ? (inner === item ? item.name : item.name + inner.path.slice(item.path.length)) + '/' : item.name;
          g.fillText(name + '  ' + item.lines, box.x + 4, box.y + 3, box.w - 8);
        }
      }
    }

    at(event) {
      const rect = this.canvas.getBoundingClientRect();
      const x = event.clientX - rect.left, y = event.clientY - rect.top;
      return (this.boxes || []).find((b) => x >= b.x && x < b.x + b.w && y >= b.y && y < b.y + b.h);
    }

    click(event) {
      const box = this.at(event);
      if (!box) return;
      if (box.item.files) this.show(box.item);
      this.hooks.select(box.item.id);
    }

    hover(event) {
      const box = this.at(event);
      const tip = this.hooks.tip;
      if (!box) { tip.hidden = true; return; }
      const item = box.item;
      tip.textContent = (item.path || item.name) + (item.files ? '/' : '') + '\n' + item.lines + ' lines' +
        (item.files ? ', ' + item.count + ' files' : ', ' + item.lang);
      tip.style.left = event.clientX + 14 + 'px';
      tip.style.top = event.clientY + 14 + 'px';
      tip.hidden = false;
    }
  }

  root.MapPanels = { renderTree, renderDetails, MatrixView, TreemapView, el };
})(typeof globalThis !== 'undefined' ? globalThis : this);
