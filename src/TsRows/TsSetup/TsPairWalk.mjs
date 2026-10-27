/**
 * THE FIRST TWO PROJECTS WALKED AT ONCE, and stored as if one had followed the other.
 *
 * A full run walks one program at a time, and on a large workspace two of them are most of the walk. So the FIRST is
 * walked by a worker thread - its own program, its own checker, its own `Store` - while this thread walks the SECOND,
 * and the two are then merged into exactly the rows the walk one after the other writes:
 *
 *   - WHO OWNS A FILE is decided first: the second walks none of the files the first's program holds, as it would skip
 *     them once the first had read them (`extracted`). The worker says which they are as soon as its program is built.
 *   - EVERY ID MINTED WHILE THE TWO RUN CARRIES A MARK (`Store.mark`): the first's `\u0001a`, the second's `\u0001b`, and
 *     the merge numbers them in the order the walk one after the other would have - the first's, then the second's -
 *     from the counters both started at. An id left marked anywhere is a fault, never a row.
 *   - WHAT THE SECOND WOULD HAVE FOUND MADE is the first's: a file it interned that the first interned too, and an
 *     expression of a span and role the first described, are the first's rows - the second's copy goes. Nothing
 *     patches such a copy: the second patches only the files it reads, and the first never interns one of those. So
 *     does a note of an import the first's program already noted.
 *
 * A PARTIAL RUN NEVER PAIRS: what it reads again is decided project by project, each after the ones before it.
 */

import { isMainThread, MessageChannel, parentPort, receiveMessageOnPort, Worker, workerData } from 'node:worker_threads';
import { Diagnostics, Store } from './TsStore.mjs';
import { loadToolchain } from './TsResolve.mjs';
import { bindingNames, walkProjects } from './TsWalk.mjs';
import { emit } from './TsRunIo.mjs';

const FIRST = '\u0001a';
const SECOND = '\u0001b';
const NOTES = new Set(['unresolved_import', 'unresolved_node_builtin']);
const WORD = new Set('abcdefghijklmnopqrstuvwxyz_');

/** `prefix`, mark and number of a marked id, or null. */
function marked(id, mark) {
  if (typeof id !== 'string') return null;
  const at = id.indexOf(`:${mark}`);
  if (at <= 0) return null;
  const n = Number(id.slice(at + 1 + mark.length));
  return Number.isInteger(n) ? { prefix: id.slice(0, at), n } : null;
}

/** THE FIRST PROJECT WALKED INTO A STORE OF ITS OWN - by the worker, or by this thread when the worker could not. */
export function walkFirst({ ts, fe, nodeModules, project, projectId, propertyPaths, onFiles }) {
  const store = new Store({}, 0);
  store.mark(FIRST);
  const diag = new Diagnostics(() => store.currentFile);
  const noted = new Set();
  let told = false;
  const tell = (files) => { told = true; onFiles(files); };
  const { inAnyProgram, registry, components } = walkProjects({
    ts, store, fe, nodeModules, projects: [project], diag, noted, propertyPaths, first: { projectId, onFiles: tell },
  });
  if (!told) tell({ owned: [], reached: [...inAnyProgram] });
  // AN EXPRESSION'S SPAN BY ITS FILE'S PATH, since the file's id here is not the one it will have.
  const exprAt = [...(store.exprAt ?? [])].map(([key, id]) => {
    const cut = key.indexOf('|');
    return [`${store.rowOf(key.slice(0, cut))?.abs}${key.slice(cut)}`, id];
  });
  return {
    tables: store.tables, byKey: [...store.byKey], exprAt,
    scheme: [...store.scheme].map(([prefix, names]) => [prefix, [...names]]),
    rollup: [...store.rollupTables], perFile: [...store.perFileTables], interned: [...store.internedTables],
    diag: diag.items, noted: [...noted],
    // THE PREDICATES DO NOT CROSS A THREAD: rebuilt from the names on the other side.
    registry: registry.map(({ inputs: _i, outputs: _o, ...entry }) => entry), components,
  };
}

if (!isMainThread && workerData?.pairWalk) {
  const { nodeModules, angular, fe, project, projectId, propertyPaths, port, flag } = workerData;
  const signal = (message) => {
    port.postMessage(message);
    Atomics.add(flag, 0, 1);
    Atomics.notify(flag, 0);
  };
  try {
    const { ts } = await loadToolchain(nodeModules, angular);
    const first = walkFirst({ ts, fe, nodeModules, project, projectId, propertyPaths, onFiles: (files) => signal({ type: 'files', ...files }) });
    signal({ type: 'done', first });
  } catch (error) {
    signal({ type: 'error', message: String(error?.stack ?? error) });
  }
}

/**
 * THE PAIR, or null when the run cannot have one: a partial run, fewer than two projects, or a project that is a set of
 * roots rather than a program of its own.
 */
export function pairUp({ ts, fe, nodeModules, angular, projects, propertyPaths, partial }) {
  if (partial || angular === undefined || process.env.STRUCTUREGATE_TS_PAIR === '0' || projects.length < 2 || projects[0].roots || projects[1].roots) return null;
  const [one, two] = projects;
  const channel = new MessageChannel();
  const flag = new Int32Array(new SharedArrayBuffer(4));
  let worker = null;
  let seen = 0;
  let early = null;
  let done = null;
  let fork = null;
  let firstId = null;
  const next = () => {
    for (;;) {
      const got = receiveMessageOnPort(channel.port1);
      if (got) return got.message;
      Atomics.wait(flag, 0, seen, 1000);
      seen = Atomics.load(flag, 0);
    }
  };
  const local = (why) => {
    emit('MAP-NOTE', `the typescript half walks ${one.name} after all, on this thread - ${why}`);
    let files = null;
    done = walkFirst({ ts, fe, nodeModules, project: one, projectId: firstId, propertyPaths, onFiles: (f) => { files = f; } });
    return files;
  };
  return {
    isFirst: (project) => project === one,
    isSecond: (project) => project === two,
    start(projectId) {
      firstId = projectId;
      try {
        worker = new Worker(new URL(import.meta.url), {
          workerData: { pairWalk: true, nodeModules, angular, fe, project: one, projectId, propertyPaths, port: channel.port2, flag },
          transferList: [channel.port2],
        });
        worker.unref();
      } catch (error) {
        early = error;
      }
    },
    /** BEFORE THE SECOND READS A FILE: the files the first owns, and what its program reached. */
    fork(store, diag, registry, components) {
      fork = {
        counters: { ...store.counters }, floor: store.floor, diag: diag.items.length, registry: registry.length, components: components.length,
        lengths: Object.fromEntries(Object.entries(store.tables).map(([name, rows]) => [name, rows.length])),
      };
      store.mark(SECOND);
    },
    owned(extracted, inAnyProgram) {
      let files = null;
      if (worker === null) files = local(`no worker could be started: ${early?.message}`);
      while (files === null) {
        const message = next();
        if (message.type === 'files') files = message;
        else if (message.type === 'error') files = local(message.message);
      }
      for (const abs of files.owned) extracted.add(abs);
      for (const abs of files.reached) inAnyProgram.add(abs);
    },
    /** AFTER THE SECOND: the first's rows put before its own, every marked id numbered. */
    join(store, diag, noted, registry, components) {
      while (done === null) {
        const message = next();
        if (message.type === 'done') done = message.first;
        else if (message.type === 'error') local(message.message);
      }
      worker?.terminate();
      merge(store, diag, noted, registry, components, done, fork);
      done = null;
      emit('MAP-NOTE', `the typescript half walked ${one.name} and ${two.name} at once`);
    },
  };
}

function merge(store, diag, noted, registry, components, first, fork) {
  store.mark('');
  // THE SECOND'S COPIES OF WHAT THE FIRST MADE: by interned key, and by an expression's span.
  const firstKey = new Map(first.byKey);
  const dropped = new Map();
  for (const [key, id] of store.byKey) {
    if (marked(id, SECOND) && firstKey.has(key)) dropped.set(id, firstKey.get(key));
  }
  const firstAt = new Map(first.exprAt);
  for (const [key, id] of store.exprAt ?? []) {
    if (!marked(id, SECOND)) continue;
    const cut = key.indexOf('|');
    const twin = firstAt.get(`${store.rowOf(key.slice(0, cut))?.abs}${key.slice(cut)}`);
    if (twin !== undefined) dropped.set(id, twin);
  }
  // THE NUMBERS: the first's from where the counters stood, then the second's kept ones after them.
  const ids = new Map();
  const numbered = (rows, mark, skip) => {
    const byPrefix = new Map();
    for (const row of rows) {
      const m = marked(row.id, mark);
      if (!m || skip.has(row.id)) continue;
      if (!byPrefix.has(m.prefix)) byPrefix.set(m.prefix, []);
      byPrefix.get(m.prefix).push([m.n, row.id]);
    }
    return byPrefix;
  };
  const firstRows = Object.values(first.tables).flat();
  const secondRows = Object.entries(store.tables).flatMap(([name, rows]) => rows.slice(fork.lengths[name] ?? 0));
  const used = {};
  for (const byPrefix of [numbered(firstRows, FIRST, new Set()), numbered(secondRows, SECOND, dropped)]) {
    for (const [prefix, list] of byPrefix) {
      list.sort((a, b) => a[0] - b[0]);
      const base = (fork.counters[prefix] ?? fork.floor) + (used[prefix] ?? 0);
      list.forEach(([, id], i) => ids.set(id, `${prefix}:${base + i + 1}`));
      used[prefix] = (used[prefix] ?? 0) + list.length;
    }
  }
  for (const [id, twin] of dropped) ids.set(id, ids.get(twin));
  for (const [prefix, n] of Object.entries(used)) store.counters[prefix] = (fork.counters[prefix] ?? fork.floor) + n;
  // AN ID INSIDE A STRING TOO - `!x:12` is the `else` side of a branch: each marked id in it, its prefix the word
  // before the colon and its number the digits after the mark.
  const within = (text) => {
    let out = '';
    let from = 0;
    for (let at = text.indexOf(':\u0001'); at >= 0; at = text.indexOf(':\u0001', at + 2)) {
      let start = at;
      while (start > from && WORD.has(text[start - 1])) start -= 1;
      let end = at + 3;
      while (end < text.length && text[end] >= '0' && text[end] <= '9') end += 1;
      const id = ids.get(text.slice(start, end));
      if (id === undefined) throw new Error(`the paired walk left an id it could not number: ${JSON.stringify(text)}`);
      out += text.slice(from, start) + id;
      from = end;
    }
    return out + text.slice(from);
  };
  const renumber = (value) => {
    if (typeof value === 'string') return value.includes(':\u0001') ? within(value) : value;
    if (Array.isArray(value)) {
      for (let i = 0; i < value.length; i += 1) value[i] = renumber(value[i]);
      return value;
    }
    if (value === null || typeof value !== 'object') return value;
    const keys = Object.keys(value);
    if (!keys.some((k) => k.includes(':\u0001'))) {
      for (const k of keys) value[k] = renumber(value[k]);
      return value;
    }
    const out = {};
    for (const k of keys) out[renumber(k)] = renumber(value[k]);
    return out;
  };
  // EVERY TABLE: what stood before the pair, the first's rows, the second's kept ones - each renumbered in place.
  const renumberRows = (rows) => rows.map((row) => renumber(row));
  for (const id of dropped.keys()) store.rowById.delete(id);
  for (const name of new Set([...Object.keys(store.tables), ...Object.keys(first.tables)])) {
    const rows = store.table(name);
    const at = fork.lengths[name] ?? 0;
    const theirs = renumberRows(first.tables[name] ?? []);
    const mine = renumberRows(rows.slice(at).filter((row) => !dropped.has(row.id)));
    rows.length = at;
    for (const row of theirs) rows.push(row);
    for (const row of mine) rows.push(row);
  }
  // ...AND THE TABLES IN THE ORDER THEY WERE FIRST WRITTEN: before the pair, then the first's, then the second's.
  const tables = {};
  for (const name of [...Object.keys(fork.lengths), ...Object.keys(first.tables), ...Object.keys(store.tables)]) {
    if (!(name in tables)) tables[name] = store.tables[name];
  }
  store.tables = tables;
  for (const [id] of [...store.rowById]) if (id.includes(':\u0001')) store.rowById.delete(id);
  for (const rows of Object.values(store.tables)) for (const row of rows) if (typeof row.id === 'string' && !store.rowById.has(row.id)) store.rowById.set(row.id, row);
  const byKey = new Map();
  for (const [key, id] of first.byKey) byKey.set(key, ids.get(id) ?? id);
  for (const [key, id] of store.byKey) if (!byKey.has(key)) byKey.set(key, ids.get(id) ?? id);
  store.byKey = byKey;
  const exprAt = new Map();
  for (const [key, id] of first.exprAt) {
    const cut = key.indexOf('|');
    exprAt.set(`${byKey.get(`files ${key.slice(0, cut)}`)}${key.slice(cut)}`, ids.get(id));
  }
  for (const [key, id] of store.exprAt ?? []) {
    const cut = key.indexOf('|');
    const k = `${renumber(key.slice(0, cut))}${key.slice(cut)}`;
    if (!exprAt.has(k)) exprAt.set(k, ids.get(id) ?? id);
  }
  store.exprAt = exprAt;
  for (const [prefix, names] of first.scheme) for (const name of names) store.note(name, prefix);
  for (const name of first.rollup) store.rollupTables.add(name);
  for (const name of first.perFile) store.perFileTables.add(name);
  for (const name of first.interned) store.internedTables.add(name);
  // THE NOTES: the second's of an import the first noted were never written.
  const firstNoted = new Set(first.noted);
  const items = diag.items;
  const before = items.slice(0, fork.diag);
  const after = items.slice(fork.diag).filter((n) => !(NOTES.has(n.kind) && firstNoted.has(`${n.module}|${n.from}`)));
  diag.items = [...before, ...first.diag, ...after].map((n) => renumber(n));
  diag.counts = {};
  for (const n of diag.items) diag.counts[n.kind] = (diag.counts[n.kind] ?? 0) + 1;
  diag.total = diag.items.length;
  for (const key of first.noted) noted.add(key);
  registry.splice(fork.registry, 0, ...first.registry.map((entry) => ({
    ...entry, inputs: bindingNames(entry.inputNames), outputs: bindingNames(entry.outputNames),
  })));
  for (let i = 0; i < registry.length; i += 1) {
    const { inputs, outputs, ...rest } = registry[i];
    registry[i] = { ...renumber(rest), inputs, outputs };
  }
  components.splice(fork.components, 0, ...first.components);
  for (let i = 0; i < components.length; i += 1) components[i] = renumber(components[i]);
}
