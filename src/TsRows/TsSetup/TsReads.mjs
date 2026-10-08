/**
 * WHAT A FILE'S EXTRACTION READ OUTSIDE ITSELF, and what a file shows the files that read it.
 *
 * A partial run re-reads a file when something it READ changed. "Something it imports" is not that set: the
 * evaluator follows a constant through a barrel into the file that declares it, a type is printed from a
 * file two imports away, and a row names another file's expression by its line. So the set is RECORDED, not
 * derived: every way into another file goes through the checker - a symbol, its declarations, a type, a
 * signature - and every answer the checker gives while a file is being extracted is noted with the files its
 * declarations sit in (`files.reads`). A node reached from one of those is inside a file already noted.
 *
 * WHAT A FILE SHOWS is its declaration emit (`files.surface`): the `.d.ts` the compiler would write for it,
 * which is what every other file's TYPES depend on - the same rule `tsc --build` stops re-checking by. It is
 * hashed in a canonical form (union and intersection members sorted), because the checker prints a union in
 * the order its members were first created, and that order moves with what was asked first - a surface that
 * moved without the file moving would re-read its readers for nothing. Decorators are not in a `.d.ts` and
 * are what Angular reads, so their text is part of it too.
 */
import { createHash } from 'node:crypto';

import { canonicalPath, relativeTo } from './TsPaths.mjs';

const NONE = Object.freeze([]);

/** The checker under the read log, for a SEARCH whose misses read nothing - see `trace` in `TsImports.mjs`. */
export const UNRECORDED = Symbol('the checker, unrecorded');

export function makeReadLog(ts, feRoot) {
  let current = null;
  // WHAT IT ASKED ABOUT INSIDE ANOTHER FILE: a node of that file handed to the checker. The answer depends on
  // that file's own imports, not only on what it shows, so a reader of it is read again when those move.
  let deep = null;
  const root = canonicalPath(feRoot).toLowerCase() + '/';
  const add = (sf) => { if (sf && typeof sf.fileName === 'string') current.add(sf.fileName); };
  const node = (n) => { if (n && typeof n.kind === 'number' && typeof n.getSourceFile === 'function') add(n.getSourceFile()); };
  const symbol = (s) => {
    if (!s || typeof s !== 'object') return;
    for (const d of s.declarations ?? []) node(d);
    node(s.valueDeclaration);
  };
  // WHAT AN ANSWER NAMES. One level into a union's members, and no further: a member's own members are
  // declared in its file's surface, which a reader that depends on them has already noted.
  const note = (v) => {
    if (!v || typeof v !== 'object') return;
    if (Array.isArray(v)) { for (const x of v) note(x); return; }
    // BY SHAPE: the compiler's objects carry no tag outside a debugger. A node has a kind and a file, a symbol
    // a name, a signature its parameters, an index info its declaration, a type an id.
    if (typeof v.kind === 'number' && typeof v.getSourceFile === 'function') { node(v); return; }
    if (typeof v.escapedName === 'string') { symbol(v); return; }
    if (Array.isArray(v.parameters) || (v.declaration && typeof v.declaration.kind === 'number')) {
      node(v.declaration);
      return;
    }
    if (typeof v.flags === 'number' && typeof v.id === 'number') {
      symbol(v.symbol);
      symbol(v.aliasSymbol);
      for (const t of Array.isArray(v.types) ? v.types : []) { symbol(t?.symbol); symbol(t?.aliasSymbol); }
    }
  };
  const wrapped = new Map();
  return {
    /** The checker every pass is given: the same one, with each answer noted while a file is open. */
    wrap(checker) {
      return new Proxy(checker, {
        get(target, prop) {
          if (prop === UNRECORDED) return target;
          const value = target[prop];
          if (typeof value !== 'function') return value;
          let fn = wrapped.get(prop);
          if (fn === undefined || fn.target !== target) {
            fn = (...args) => {
              const out = value.apply(target, args);
              if (current !== null) {
                // A MODULE'S WHOLE EXPORT LIST IS NOT READ by asking for one name in it: the barrel walk searches
                // it, and lands where the checker then resolves a module - which is noted. A name added behind the
                // barrel still reaches its importers: they asked about the barrel's own nodes (`deep`), and it is
                // read again when the file behind it moves.
                if (prop !== 'getExportsOfModule') note(out);
                for (const a of args) {
                  if (a && typeof a.kind === 'number' && typeof a.getSourceFile === 'function') {
                    const sf = a.getSourceFile();
                    if (sf) { current.add(sf.fileName); deep.add(sf.fileName); }
                  }
                }
              }
              return out;
            };
            fn.target = target;
            wrapped.set(prop, fn);
          }
          return fn;
        },
      });
    },
    /**
     * A MEMO'S ANSWER, WITH WHAT FINDING IT READ. A cache hit asks the checker nothing, so the file that hits
     * it would record none of the reads behind the answer it was handed - computed, perhaps, while another
     * file was open. So each entry keeps what its computation read, and every hit adds that to the open file.
     */
    cached(cache, key, compute) {
      const hit = cache.get(key);
      if (hit !== undefined) {
        if (current !== null) {
          for (const name of hit.reads) current.add(name);
          for (const name of hit.deep) deep.add(name);
        }
        return hit.value;
      }
      const outer = current;
      const outerDeep = deep;
      current = new Set();
      deep = new Set();
      let value;
      let reads;
      let asked;
      try {
        value = compute();
      } finally {
        reads = current;
        asked = deep;
        current = outer;
        deep = outerDeep;
        if (current !== null) {
          for (const name of reads) current.add(name);
          for (const name of asked) deep.add(name);
        }
      }
      // Most answers read nothing outside the open file's own nodes; they share one empty list.
      cache.set(key, { value, reads: reads.size ? [...reads] : NONE, deep: asked.size ? [...asked] : NONE });
      return value;
    },
    /** A file read by its text alone, without the checker. */
    touch(sf) { if (current !== null && sf) current.add(sf.fileName); },
    begin() { current = new Set(); deep = new Set(); },
    /** The in-tree files the open file read, and those it asked about inside, as the map spells paths -
     *  itself and `node_modules` left out. */
    end(ownAbs) {
      const spell = (names) => {
        const out = new Set();
        for (const name of names) {
          const abs = canonicalPath(name);
          const low = abs.toLowerCase();
          if (abs === ownAbs || !low.startsWith(root) || low.includes('/node_modules/')) continue;
          out.add(relativeTo(feRoot, abs));
        }
        return [...out].sort();
      };
      const found = { reads: spell(current ?? []), deep: spell(deep ?? []) };
      current = null;
      deep = null;
      return found;
    },
  };
}

/**
 * WHICH FILES A PARTIAL RUN READS AGAIN, decided one project at a time before that project's files are read.
 *
 * A file is read again when something it READ changed:
 *  - a changed file's readers, always - they may have seen its text, its values, its lines;
 *  - the readers of every file whose SURFACE changed;
 *  - and whoever asked the checker about a node INSIDE one of those readers (`deep`): that answer comes from
 *    the reader's own imports, one of which moved, whatever the reader itself shows.
 * A surface is looked at only where it can have moved: in a changed file, and in a reader of a surface that
 * moved. Each is emitted by a program of its own, so the extraction's checker answers exactly as it would
 * have, in the order it would have.
 *
 * ONE PASS IN PROJECT ORDER IS ENOUGH. The first program that holds a file owns it, and a file's program holds
 * everything it read, so whatever a file read is owned by its own project or an earlier one. A reader found
 * after its owner already ran would break that; it is collected in `late` and the run refuses.
 */
export function makeRereads({ ts, fe, changed, gone, readers, deepReaders, surfaces, coupled, only, release }) {
  const moved = new Set(gone);
  const checked = new Map();
  const done = new Set();
  const late = [];
  const readersOf = (rel) => readers[rel] ?? [];
  return {
    checked,
    moved,
    late,
    /** `files` are the project's own, not yet read by an earlier one; `open` builds the surface program. */
    project(files, open) {
      const owned = new Map(files.map((sf) => [relativeTo(fe, canonicalPath(sf.fileName)), sf]));
      const queue = [];
      for (const rel of owned.keys()) if (changed.has(rel)) queue.push(rel);
      for (const rel of moved) for (const r of readersOf(rel)) if (owned.has(r)) queue.push(r);
      let program = null;
      while (queue.length) {
        const rel = queue.pop();
        if (checked.has(rel) || !owned.has(rel)) continue;
        program ??= open();
        const sf = program ? program.getSourceFile(owned.get(rel).fileName) : undefined;
        const surface = sf ? surfaceOf(ts, program, sf) : null;
        checked.set(rel, surface);
        if (surface === null || surface !== surfaces[rel]) {
          moved.add(rel);
          for (const r of readersOf(rel)) if (owned.has(r) && !checked.has(r)) queue.push(r);
        }
      }
      const again = new Set();
      for (const rel of [...changed, ...moved]) for (const r of readersOf(rel)) again.add(r);
      for (const rel of moved) for (const r of readersOf(rel)) for (const r2 of deepReaders[r] ?? []) again.add(r2);
      const stack = [...again];
      while (stack.length) {
        for (const c of coupled[stack.pop()] ?? []) if (!again.has(c)) { again.add(c); stack.push(c); }
      }
      const fresh = [...again].filter((rel) => !only.has(rel));
      for (const rel of fresh) {
        if (done.has(rel)) late.push(rel);
        only.add(rel);
      }
      if (fresh.length) release(fresh);
      for (const rel of owned.keys()) done.add(rel);
    },
  };
}

// What a pass MEASURES of a file, as opposed to what names it.
const MEASURED = ['bytes', 'chars', 'lines', 'content_hash', 'tier', 'parsed', 'reachable', 'reads', 'reads_deep',
  'surface', 'shape'];

/**
 * A CHANGED FILE'S ROW STARTS EMPTY, as a full run's does. `files` rows are handed back for every file - they
 * are the interned table, never replaced - and every pass that measures one fills it only where nothing is
 * there yet, so a changed file kept its old size: `bytes` from before the edit, not the file's new size.
 */
export function emptyChanged(store, changed) {
  const moved = new Set(changed);
  for (const f of store.table('files')) {
    if (!moved.has(f.path)) continue;
    for (const field of MEASURED) delete f[field];
  }
}

/**
 * A HANDED-BACK FILE NAMES ITS PROJECT BY AN ID THE RUN HAS JUST GIVEN AWAY. `projects` is rebuilt whole every
 * run under fresh ids, and a `files` row that was not made again kept the old one - a reference to nothing, on
 * every partial run there ever was. The rows handed back for that table say which name each old id was.
 */
export function repointProjects(store, heldProjects) {
  const nameOf = new Map(heldProjects.map((p) => [p.id, p.name]));
  const idOf = new Map(store.table('projects').map((p) => [p.name, p.id]));
  for (const f of store.table('files')) {
    if (f.project === undefined || f.project === null || store.rowOf(f.project)) continue;
    const now = idOf.get(nameOf.get(f.project));
    if (now !== undefined) f.project = now;
  }
}

/** `makeRereads` over a partial run's state, releasing the handed-back rows of each file it adds. */
export function startRereads({ ts, fe, store, state, planned, now, only }) {
  const fileIdOf = new Map();
  return makeRereads({
    ts, fe, changed: new Set(planned.changed), gone: planned.changed.filter((rel) => !now.has(rel)),
    readers: state.readers ?? {}, deepReaders: state.deep_readers ?? {}, surfaces: state.surfaces ?? {},
    coupled: state.coupled ?? {}, only,
    release: (rels) => {
      if (!fileIdOf.size) for (const f of store.table('files')) fileIdOf.set(f.path, f.id);
      store.release(new Set(rels.map((rel) => fileIdOf.get(rel)).filter(Boolean)));
    },
  });
}

/**
 * WHY A RUN THAT STOPPED SHORT OF EVERY HOP HAS TO BE ASKED AGAIN, or null. It is not an error: the retry
 * reads every hop, which re-reads whatever file holds the row that broke. `walked` asks only what is known
 * after the walk - a file that turned out to need reading after its project was read; otherwise `touched`
 * is what `Store.verifyUntouched` found.
 */
export function cutShort(store, rereads, touched, walked) {
  if (walked) {
    return rereads.late.length
      ? `${rereads.late.length} file(s) turned out to need reading after their project was read, first ${rereads.late[0]}`
      : null;
  }
  const broken = store.dangling();
  if (broken.length) {
    return 'a row kept from a file not read again names one that is gone: '
      + broken.map((b) => `${b.table}.${b.column} -> ${b.names}`).join(', ');
  }
  if (touched !== null) {
    return 'a row kept from a file not read again names one that was: '
      + touched.map((t) => `${t.table}.${t.columns.join('/')} (e.g. ${t.id})`).join(', ');
  }
  return null;
}

/** A file's surface, as a hash - see the header. Null when the compiler emits nothing for it. */
export function surfaceOf(ts, program, sf) {
  let text = '';
  try {
    // `forceDtsEmit` (the sixth argument): what the compiler's own builder asks, so it answers under `noEmit`.
    program.emit(sf, (name, data) => { if (name.endsWith('.d.ts')) text += data; }, undefined, true, undefined, true);
  } catch {
    return null;
  }
  const hash = createHash('sha256');
  hash.update(text ? canonical(ts, text) : '');
  for (const d of decoratorTexts(ts, sf)) hash.update('\u0000' + d);
  return hash.digest('hex').slice(0, 32);
}

/** The declaration text with every union and intersection in one order, whatever order it was printed in. */
function canonical(ts, text) {
  const dts = ts.createSourceFile('surface.d.ts', text, ts.ScriptTarget.Latest, true);
  const print = (n) => {
    if (ts.isUnionTypeNode(n) || ts.isIntersectionTypeNode(n)) {
      const parts = n.types.map(print).sort();
      return `(${parts.join(ts.isUnionTypeNode(n) ? ' | ' : ' & ')})`;
    }
    const kids = [];
    ts.forEachChild(n, (c) => { kids.push(c); });
    if (!kids.length) return n.getText(dts);
    let out = '';
    let at = n.getStart(dts);
    for (const c of kids) {
      out += dts.text.slice(at, c.getStart(dts)) + print(c);
      at = c.end;
    }
    return out + dts.text.slice(at, n.end);
  };
  return print(dts);
}

function decoratorTexts(ts, sf) {
  const out = [];
  const visit = (n) => {
    if (ts.isDecorator(n)) out.push(n.getText(sf));
    else ts.forEachChild(n, visit);
  };
  visit(sf);
  return out;
}
