/**
 * THE ROWS, while they are still in node - and the two ways a row is created.
 *
 * Ported from the original Angular extractor, minus everything about writing JSON shards: this
 * half hands its rows to the rust store, which puts them in the same database the python and C# halves write.
 *
 * `add` is a row nobody will look up again. `intern` is a row that has an IDENTITY in the tree - a file
 * has exactly one row, whoever reaches it first - and `patch` is what makes that safe: `intern` returns
 * the existing id and NEVER runs the builder again, so whichever caller arrived first would otherwise
 * decide what the row says for good. In the tool this is ported from that cost a real map twice: a file
 * interned while another file's expressions were being evaluated kept only {path, abs, ext, project}, so
 * the extractor's own `tier` was dropped on the floor and the file read as never-parsed inventory while
 * its declarations were already in the map.
 *
 * IDS ARE HANDLES for joining inside one database and never a name to keep - which is why a comparison of
 * two maps compares what a row SAYS and not which number it got. They are handed out from the counters the caller
 * passes: CONTINUED from what the database recorded when another half has rows in it, and RESTARTED when
 * this half is alone, so the same tree always numbers the same way. The rust store decides which, and says
 * what goes wrong under each. A run that parses WHILE another half writes is handed a FLOOR: every prefix numbers
 * from above it, recorded or not, so the ids the other half hands out meanwhile are never these.
 */

/** What every row of this half is stamped with, so the database can drop exactly its own rows again.
 *
 * NOT `lang`: the map being reproduced already has an `expressions.lang` that says whether an expression
 * came from TypeScript or from a template, and a housekeeping column of that name would be compared
 * against it. `half` is this half's own word and nothing else uses it. */
export const HALF = 'typescript';

/** THE TABLES WHOSE ROWS ARE ALWAYS SOURCE IN ONE `.ts` FILE - a body's calls, locals, branches, a file's literals -
 *  get `file` as well as `owner_file`. Left empty, `calls c JOIN files f ON f.id = c.file`, the join every
 *  other half answers, found no TypeScript row at all. Not every table: a template's rows sit in its `.html`,
 *  and `expressions.file` is empty for a template expression on purpose (see `add`). */
const SOURCE_FILE_TABLES = new Set([
  'calls', 'assignments', 'locals', 'returns', 'raises', 'branches', 'handlers', 'switch_cases',
  'string_literals', 'number_literals', 'template_literals', 'regexes',
]);

/** `c:412` - a short lowercase prefix, a colon, digits. Walked rather than matched: this half ships in a
 *  repository whose build refuses a regular expression. */
function isId(value) {
  if (typeof value !== 'string') return false;
  const cut = value.indexOf(':');
  if (cut < 1 || cut > 4 || cut === value.length - 1) return false;
  for (const ch of value.slice(0, cut)) if (ch < 'a' || ch > 'z') return false;
  for (const ch of value.slice(cut + 1)) if (ch < '0' || ch > '9') return false;
  return true;
}

/** The same value with every id it holds reduced to its prefix, so two of them can be compared for what
 *  they SAY rather than for which rows they happen to name. */
function blankIds(value) {
  if (isId(value)) return `${value.slice(0, value.indexOf(':') + 1)}`;
  if (Array.isArray(value)) return value.map(blankIds);
  if (value && typeof value === 'object') {
    const out = {};
    for (const [k, v] of Object.entries(value)) out[k] = blankIds(v);
    return out;
  }
  return value;
}

/**
 * A ROW AS THE DATABASE WOULD HOLD IT, which is the only form two of them can honestly be compared in.
 *
 * SQLite has no boolean and no absent-versus-null: `true` comes back an integer 1, and a column set to
 * null is indistinguishable from one never set. So a row handed back and then re-derived IDENTICALLY
 * still differs as JavaScript - `load_children_recovery` was `true` when it was written and `1` when it
 * came back - and a check comparing the values would report a change that storing it could not make.
 *
 * The question the check asks is "would writing this row change the database", so it is asked in the
 * database's own terms.
 */
function storedForm(row) {
  const out = {};
  for (const key of Object.keys(row).sort()) {
    const value = row[key];
    if (value === null || value === undefined) continue;
    out[key] = typeof value === 'boolean' ? (value ? 1 : 0) : value;
  }
  return JSON.stringify(out);
}

// The candidate keys, most identifying first. A row is claimed by the first of these that is unique
// inside its own file EVERYWHERE in the map - measured below, never assumed.
const KEY_CANDIDATES = [
  ['kind', 'name', 'class'], ['name', 'class'], ['kind', 'name'], ['name', 'line'], ['name'],
  ['selector'], ['path'], ['key'], ['line', 'col'],
  // LAST, so no table that had a key changes it: a type's member by its owner and path (the owner is itself a
  // claimed row), and a call by where it is and what it calls - the two a template's reads and a render name
  // from another file, and the ones a partial run otherwise re-numbered under a row it kept.
  ['owner', 'path'], ['line', 'col', 'callee'],
  // SPARSE: a row without one of these is left unclaimed rather than ruling the key out. A template
  // expression has no line of its own and is read again with its component; a TypeScript one is named by
  // `$expr_id` from other files, and is the reason this key exists. `source` too, because every operand of
  // `a || b || c` starts where the whole does: a large Angular workspace has many rows at one line and column.
  { columns: ['file', 'line', 'col', 'role', 'source'], sparse: true },
];

/** Every id a cell holds, however deeply it is nested. */
function idsIn(value) {
  if (isId(value)) return [value];
  if (Array.isArray(value)) return value.flatMap(idsIn);
  if (value && typeof value === 'object') return Object.values(value).flatMap(idsIn);
  return [];
}

export function keyOfRow(row, columns) {
  const parts = [];
  for (const column of columns) {
    const value = row[column];
    if (value === null || value === undefined) return null;
    if (typeof value === 'object') return null;
    parts.push(String(value));
  }
  return parts.join('\u0000');
}

/** The first candidate that identifies every row of this table uniquely inside its own file. */
function uniqueKeyFor(rows) {
  for (const entry of KEY_CANDIDATES) {
    const candidate = Array.isArray(entry) ? entry : entry.columns;
    const sparse = !Array.isArray(entry) && entry.sparse;
    const seen = new Set();
    let usable = true;
    for (const row of rows) {
      const home = row.owner_file ?? (typeof row.file === 'string' ? row.file : null);
      const key = keyOfRow(row, candidate);
      if (sparse && (home === null || key === null)) continue;
      if (home === null || key === null) { usable = false; break; }
      const full = `${home}\u0000${key}`;
      if (seen.has(full)) { usable = false; break; }
      seen.add(full);
    }
    if (usable && seen.size) return candidate;
  }
  return null;
}

export class Store {
  constructor(counters, floor = 0) {
    this.counters = { ...counters };
    this.floor = floor;
    // WHICH FILE IS BEING EXTRACTED - see `enterFile`. Null outside one, which is the rollups.
    this.currentFile = null;
    // ROWS HANDED BACK rather than produced here, and what each looked like on arrival - see `load`.
    this.hydrated = new Set();
    this.loadedAs = new Map();
    this.tables = {};
    this.byKey = new Map();
    this.rowById = new Map();
    // WHICH TABLE AN ID PREFIX NAMES, recorded where the row is written rather than listed somewhere.
    // A list would be a second statement of the same fact, and the one that drifted would be the list:
    // `k` serves BOTH `consts` and `i18n_refs`, so a prefix maps to a SET and never to one name.
    this.scheme = new Map();
    // WHICH TABLES A ROLLUP BUILT. A row minted while no file is open was made by a pass that reads the
    // WHOLE store - a rollup, an inverted view - and such a pass runs again in full on a partial run,
    // producing the table entire. Its old rows therefore have to go, all of them, and not only the ones
    // of the files being re-extracted.
    //
    // READ FROM `enterFile` AND NOT FROM `owner_file`. Two passes mint rows outside the per-file walks
    // and already know the file each row belongs to, so `add` copies that into `owner_file` - which
    // makes a rollup's rows look per-file while they are nothing of the kind. `translations` and
    // `i18n_refs` are exactly those two, and reading the column instead left both DOUBLED.
    this.rollupTables = new Set();
    // ...AND WHICH TABLES THE PER-FILE WALKS PUT A ROW IN. The two together separate the only
    // distinction that matters to a partial run: a table ONLY a rollup writes is rebuilt from
    // scratch every run, and a table BOTH write is per-file rows with a rollup's own added on top.
    // Handing the first kind back doubles it; not handing the second kind back loses it.
    this.perFileTables = new Set();
    // THE IDS A RE-EXTRACTED FILE'S ROWS ALREADY HAVE, by table and key - see `identityKeys` for what
    // is in here and why. Empty on a full run, which mints every id as it always did.
    this.claims = new Map();
    this.claimKeys = {};
    this.reclaimed = 0;
    // The lookups `handedBack` builds, by table and key. Empty on a full run, which is handed nothing.
    this.handed = new Map();
    this.reused = 0;
  }

  /**
   * ROWS THIS RUN DID NOT PRODUCE, put back so the rollups can see a whole map.
   *
   * The rollups are derived from every file - `component_reach` walks the render tree, `renders` resolves
   * a selector against every declaration - so on a run that re-extracts a fraction of the tree they would
   * be computed from a fraction and be WRONG while looking right. The storing half hands the rest back and
   * they are loaded here, ids and all: nothing is minted, because these rows already exist in the database
   * and everything that points at them still does.
   *
   * THEY ARE NOT WRITTEN BACK. The payload carries what this run produced, and a hydrated row that no pass
   * touched is already stored - sending it would be hundreds of MB to say nothing. `verifyUntouched` is what makes
   * that safe to assume.
   */
  load(name, rows, keyOf) {
    const list = this.table(name);
    for (const row of rows) {
      // THE STAMP GOES BACK ON. The carrying side drops `half` from every row it hands over - it is
      // the column it selected them BY, so repeating it in each of hundreds of thousands of rows says nothing - and a
      // row that comes back without it is stored unstamped. Nothing then finds it again: `drop_half`
      // leaves it, the closure reads `half = 'typescript'` and cannot see it, and the map still has
      // the rows while every query about them answers nothing. Measured: `renders` kept all its rows
      // of which a sliver were visible, and the render closure produced a tiny fraction of its paths.
      row.half = HALF;
      list.push(row);
      if (row.id) this.rowById.set(row.id, row);
      this.hydrated.add(row);
      if (row.id) this.loadedAs.set(row.id, storedForm(row));
      // AN INTERNED ROW GOES BACK INTO THE KEY INDEX, or the identity `intern` exists to protect
      // is lost the moment the row arrives from somewhere else: the next `intern` of the same key
      // finds nothing, mints a SECOND row for the one file, and every id already pointing at the
      // first one now names a row the map also has a duplicate of. `files` is the only interned
      // table, and the caller says what its key is rather than this guessing.
      const key = keyOf ? keyOf(row) : null;
      if (key && row.id) this.byKey.set(`${name} ${key}`, row.id);
    }
  }

  /** Rows this run made, which is the payload: everything except what was handed back. */
  produced(name) {
    return this.table(name).filter((row) => !this.hydrated.has(row));
  }

  /**
   * A HYDRATED ROW A PASS QUIETLY CHANGED - the one way this scheme can go wrong in silence.
   *
   * Three rollups reach back into rows they did not create: `translations.referenced` depends on refs from
   * the whole tree, `components.unused` on the whole render graph, `io.bound_count` on every binding. Those
   * tables are therefore declared WHOLE and written back entire. The danger is the fourth one nobody
   * noticed - a pass that mutates a hydrated row of some other table, whose change would then never be
   * stored and would read as stale for ever.
   *
   * So every hydrated row is remembered as it arrived, and compared again afterwards. A mismatch is a
   * FATAL rather than a repair: the row proves the WHOLE list is out of date, and guessing which other
   * rows are affected is exactly the kind of silence this check exists to break.
   */
  verifyUntouched(whole) {
    const wholly = new Set(whole);
    // EVERY TABLE, not the first one found. The answer to this check is a LIST - which passes reach
    // back into rows they were handed - and stopping at the first turns one question into as many runs
    // as there are tables, each costing a full walk of the tree.
    const found = [];
    for (const [name, rows] of Object.entries(this.tables)) {
      if (wholly.has(name)) continue;
      let count = 0;
      let first = null;
      const columns = new Set();
      for (const row of rows) {
        if (!this.hydrated.has(row) || !row.id) continue;
        const was = this.loadedAs.get(row.id);
        const now = storedForm(row);
        if (was === now) continue;
        count += 1;
        if (first === null) first = row.id;
        // WHICH COLUMN MOVED, AND WHETHER IT MOVED A FACT, because those are two different findings.
        // A column that now names `c:418` where it named `c:412` is a row whose id was re-minted; one
        // that names a different COMPONENT is a changed answer. Marked so the reader is not left
        // reading dozens of identical-looking values to find out which.
        const before = JSON.parse(was);
        for (const key of new Set([...Object.keys(before), ...Object.keys(row)])) {
          if (JSON.stringify(before[key]) === JSON.stringify(row[key])) continue;
          const sameFact = JSON.stringify(blankIds(before[key])) === JSON.stringify(blankIds(row[key]));
          columns.add(sameFact ? `${key} (ids only)` : key);
        }
      }
      if (count) found.push({ table: name, id: first, rows: count, columns: [...columns].sort() });
    }
    return found.length ? found : null;
  }

  /**
   * A ROW THIS RUN WAS HANDED, found by the columns that identify it.
   *
   * A pass reaching into a file the run is NOT re-extracting must not make a second row for something
   * already there. The describer does exactly that: resolving a reference into another file describes
   * the declaration it lands on, and on a full run the file's own walk had already described it, so
   * nothing was added. A partial run never walks that file, so it described it again - the same arrow
   * function under a second id, attributed to whichever file happened to mention it.
   *
   * The index is built once per (table, key) and only when something asks, because a full run never
   * does.
   */
  handedBack(name, columns, row) {
    const shape = `${name}\u0000${columns.join(',')}`;
    let index = this.handed.get(shape);
    if (index === undefined) {
      index = new Map();
      for (const held of this.table(name)) {
        if (!this.hydrated.has(held) || !held.id) continue;
        const key = keyOfRow(held, columns);
        if (key !== null && !index.has(key)) index.set(key, held.id);
      }
      this.handed.set(shape, index);
    }
    const key = keyOfRow(row, columns);
    return key === null ? null : (index.get(key) ?? null);
  }

  /** One row by id, whatever table it is in - what the anchor spec follows a join with. */
  rowOf(id) {
    return this.rowById.get(id);
  }

  /**
   * THE TABLES A ROLLUP BUILDS FROM SCRATCH - measured, published, and read back by the next
   * partial run.
   *
   * A partial run must not be handed these back: the pass that owns one reads the WHOLE store and
   * writes the table entire, so a carried row would sit beside a freshly derived copy of itself.
   * Measured on a large workspace, loading them doubled `component_reach`, `routes`
   * and eleven more.
   *
   * IT CANNOT BE DECIDED BY THE RUN THAT NEEDS IT. A partial run's per-file rows arrive already
   * made, so it cannot see which tables its own walks would have filled - `renders` looks like a
   * rollup's table to it, and dropping those rows would empty the render tree. So the FULL
   * run measures it and publishes it, exactly as it publishes which columns hold a structure.
   */
  rebuiltTables() {
    return [...this.rollupTables].filter((name) => !this.perFileTables.has(name)).sort();
  }

    /**
   * WHICH IDS HAVE TO SURVIVE A RE-EXTRACTION, and what identifies the row that holds one.
   *
   * A partial run re-extracts a file and mints fresh ids for its rows. Every row in ANOTHER file that
   * named one of them then points at nothing - so the reference passes resolve them again, onto the new
   * ids, and those rows are per-file rows the run may not rewrite. Measured on a large workspace one changed
   * template did that to dozens of `calls`, `expressions` and `assignments`, every one of them the same
   * fact under a different number.
   *
   * The way out is not to renumber. A declaration that is still there is the SAME declaration, so it
   * keeps its id - and then nothing outside the file has to be touched at all.
   *
   * THIS IS MEASURED, NOT LISTED. A table needs stable ids exactly when some row in another file names
   * one of its rows, which is the same walk `rows/partial/deps.rs` makes over every id-valued cell. The KEY is measured
   * too: the first candidate that identifies a row uniquely inside its own file, everywhere in the map. A
   * list of either would be a second statement of a fact the rows already carry, and the one that drifted
   * would be the list.
   */
  identityKeys() {
    const fileOf = new Map();
    const tableOf = new Map();
    for (const [name, rows] of Object.entries(this.tables)) {
      for (const row of rows) {
        if (!row.id) continue;
        tableOf.set(row.id, name);
        const home = row.owner_file ?? (typeof row.file === 'string' ? row.file : null);
        if (home) fileOf.set(row.id, home);
      }
    }

    // A TARGET NAMED FROM ANOTHER FILE. `files` is left out: its rows ARE files, they are never
    // re-minted because `intern` keys them by path, and every id in the map would otherwise name it.
    const crossing = new Set();
    for (const rows of Object.values(this.tables)) {
      for (const row of rows) {
        const home = row.owner_file ?? (typeof row.file === 'string' ? row.file : null);
        if (!home) continue;
        for (const [column, value] of Object.entries(row)) {
          if (column === 'id' || column === 'owner_file' || column === 'file') continue;
          for (const id of idsIn(value)) {
            const target = fileOf.get(id);
            if (target !== undefined && target !== home) crossing.add(tableOf.get(id));
          }
        }
      }
    }
    crossing.delete('files');
    crossing.delete(undefined);

    const out = {};
    for (const name of [...crossing].sort()) {
      const key = uniqueKeyFor(this.tables[name] ?? []);
      // A TABLE WITH NO KEY IS REPORTED BY ITS ABSENCE. Guessing one would re-point a reference at a
      // row that merely looks alike, which is worse than minting a fresh id and resolving again.
      if (key) out[name] = key;
    }
    return out;
  }

  /** `{prefix -> "table | table"}`, the shape the published spec and every consumer read it in. */
  idScheme() {
    const out = {};
    for (const [prefix, names] of [...this.scheme].sort(([a], [b]) => a.localeCompare(b))) {
      out[prefix] = [...names].sort().join(' | ');
    }
    return out;
  }

  /**
   * THE ID THIS ROW HELD LAST TIME, when it is a row anything outside its file can name.
   *
   * A declaration that is still there is the SAME declaration. Minting it a fresh number makes every
   * reference to it from another file dangle, the reference passes resolve them onto the new number,
   * and those are per-file rows a partial run may not rewrite - so the run either fails or stores a
   * map whose joins point at rows that are gone.
   *
   * A row whose key is NOT in the claims is new, or moved, and is minted as usual: this reuses an id,
   * it never invents a correspondence.
   */
  claimed(name, cells) {
    const columns = this.claimKeys[name];
    if (!columns || this.currentFile === null) return null;
    const key = keyOfRow(cells, columns);
    if (key === null) return null;
    const at = `${name}\u0000${this.currentFile}\u0000${key}`;
    const held = this.claims.get(at);
    if (held === undefined) return null;
    // ONCE: two rows this run makes under one key must not both take the id - the second is minted.
    this.claims.delete(at);
    this.reclaimed += 1;
    return held;
  }

  /** What a partial run was handed: `{table: {file \u0000 key: id}}` and the key columns per table. */
  loadClaims(claims, keys) {
    this.claimKeys = keys ?? {};
    for (const [table, rows] of Object.entries(claims ?? {})) {
      for (const [key, id] of Object.entries(rows)) this.claims.set(`${table}\u0000${key}`, id);
    }
    return this.claims.size;
  }

  /**
   * ROWS HANDED BACK FOR FILES THIS RUN DECIDED TO READ AGAIN after they were loaded - see
   * `TsSetup/TsReads.mjs`. They go, and the ids of the ones anything outside their file can name become claims,
   * exactly as if the storing half had never sent them. In place, because a pass may hold a table's list.
   */
  release(fileIds) {
    if (!fileIds.size) return 0;
    let released = 0;
    for (const [name, rows] of Object.entries(this.tables)) {
      if (name === 'files') continue;
      const columns = this.claimKeys[name];
      let at = 0;
      for (const row of rows) {
        if (!this.hydrated.has(row) || !fileIds.has(row.owner_file)) { rows[at++] = row; continue; }
        this.hydrated.delete(row);
        if (row.id) {
          this.rowById.delete(row.id);
          this.loadedAs.delete(row.id);
          const key = columns ? keyOfRow(row, columns) : null;
          if (key !== null) this.claims.set(`${name}\u0000${row.owner_file}\u0000${key}`, row.id);
        }
        released += 1;
      }
      rows.length = at;
    }
    this.handed.clear();
    return released;
  }

  /**
   * WHAT A ROW HANDED BACK NAMES THAT NO ROW IS ANY MORE - an id, however deeply a cell nests it, of a row a
   * file read again minted afresh. A `$expr_id` inside a member's folded value is such a reference, and no
   * graph of plain columns sees it. The first few, for the note.
   */
  dangling(limit = 5) {
    const out = [];
    for (const [name, rows] of Object.entries(this.tables)) {
      for (const row of rows) {
        if (!this.hydrated.has(row)) continue;
        for (const [column, value] of Object.entries(row)) {
          if (column === 'id' || column === 'owner_file' || column === 'half') continue;
          const gone = idsIn(value).find((id) => !this.rowById.has(id));
          if (gone !== undefined) {
            out.push({ table: name, id: row.id, column, names: gone });
            if (out.length >= limit) return out;
            break;
          }
        }
      }
    }
    return out;
  }

  note(name, prefix) {
    const hit = this.scheme.get(prefix);
    if (hit) hit.add(name);
    else this.scheme.set(prefix, new Set([name]));
  }

  id(prefix) {
    const n = (this.counters[prefix] ?? this.floor) + 1;
    this.counters[prefix] = n;
    return `${prefix}:${n}`;
  }

  table(name) {
    return (this.tables[name] ??= []);
  }

  /**
   * WHICH FILE IS BEING EXTRACTED, so every row minted while it is can say so without being told.
   *
   * 21 of the 53 tables carry no `file` of their own - a binding hangs off a template node, a call off a
   * member - and even the ones that do are not always about the file the row was EXTRACTED from. That is
   * fine while the half replaces its rows WHOLE. It stops being fine the moment a
   * rebuild replaces one file's rows: python has to find them, and for those tables it can only do it by
   * walking a parent chain. That chain was written out by hand and MEASURED INCOMPLETE - many `calls`
   * rows are module-scope calls (`Symbol.for('demo')`) carrying `line`, `col` and
   * `callee` and NO id link at all. Nothing could ever attribute them, so a partial run would leave them
   * behind: stale rows that still look real.
   *
   * Stamping here fixes the whole class rather than that one table, and mechanically - a new table, or a
   * new row shape inside an old one, is covered the day it is written instead of the day someone
   * remembers to extend a list. Hand-written relation lists have been wrong twice in this half already.
   *
   * `null` OUTSIDE A FILE, which is the rollups: their rows are derived from every file and belong to
   * none, and they are recomputed whole on every run.
   */
  enterFile(fileId) {
    this.currentFile = fileId ?? null;
  }

  add(name, prefix, cells) {
    (this.currentFile === null ? this.rollupTables : this.perFileTables).add(name);
    this.note(name, prefix);
    const id = this.claimed(name, cells) ?? this.id(prefix);
    const stored = { id, ...cells, half: HALF };
    // A COLUMN OF OUR OWN, never `file`. Stamping `file` where it was empty CHANGED A FACT rather than
    // adding one: the map being reproduced leaves `expressions.file` empty for a TEMPLATE expression, and
    // filling it made every one of those rows differ. `owner_file` is a field only this map has, which the
    // bar allows and the gate lists rather than compares.
    //
    // NEVER ON `files`, whose rows ARE files - it would point at whichever file happened to be open when a
    // reference to it was interned.
    // THE SCOPE FIRST, AND THE ROW'S OWN `file` WHEN THERE IS NO SCOPE. Two passes mint rows outside the
    // per-file walks and already know the file each belongs to: the locale reader (`translations`, one row
    // per key of a locale file) and the i18n call rollup (`i18n_refs` derived from `calls`). Reading
    // `file` as the fallback covers both, and covers the next one without it having to be noticed.
    if (name !== 'files') {
      const owner = this.currentFile ?? (typeof stored.file === 'string' ? stored.file : null);
      if (owner) stored.owner_file = owner;
    }
    if (SOURCE_FILE_TABLES.has(name) && stored.file === undefined && this.currentFile !== null) stored.file = this.currentFile;
    this.table(name).push(stored);
    this.rowById.set(id, stored);
    return id;
  }

  /**
   * A row that NOTHING JOINS TO, so it is handed no id of its own.
   *
   * The four closure tables are like that: `gate_values` is keyed `(gate, enum, dimension)` and
   * `key_reach` by its `key`, and the tool being reproduced gives neither an id column. Minting one here
   * would publish a field the old map has not got in a table whose whole point is to match it row for
   * row - and, worse, would spend a counter the ids that ARE joined on are handed out from.
   */
  emit(name, cells) {
    // BY WHETHER THE ROW BELONGS TO A FILE, not only by whether one is open. A row emitted outside
    // every walk that nevertheless SAYS which file it is about is a per-file row - a diagnostic
    // raised while a file was being read and written out at the end is exactly that - and a partial
    // run has to be handed it back rather than derive it again from a tree it did not walk.
    const owner = this.currentFile ?? (typeof cells.owner_file === 'string' ? cells.owner_file : null);
    (owner === null ? this.rollupTables : this.perFileTables).add(name);
    this.table(name).push({ ...cells, half: HALF });
  }

  /** One row per KEY. The builder runs only the first time; everything after is `patch`. */
  intern(name, prefix, key, build) {
    (this.currentFile === null ? this.rollupTables : this.perFileTables).add(name);
    const k = `${name} ${key}`;
    const hit = this.byKey.get(k);
    if (hit) return hit;
    this.note(name, prefix);
    const id = this.id(prefix);
    this.byKey.set(k, id);
    const stored = { ...build(id), id, half: HALF };
    this.table(name).push(stored);
    this.rowById.set(id, stored);
    return id;
  }

  /** Fields merged into a row that is already stored - the backfill `intern` cannot do for itself.
   *  Indexed by id, so a patch is a lookup and never a scan of the table. */
  patch(id, fields) {
    const row = this.rowById.get(id);
    if (!row) return;
    for (const [key, value] of Object.entries(fields)) {
      if (value !== undefined) row[key] = value;
    }
  }

  lookup(name, key) {
    return this.byKey.get(`${name} ${key}`);
  }

  get rows() {
    return Object.values(this.tables).reduce((n, list) => n + list.length, 0);
  }
}

/**
 * WHAT COULD NOT BE ANSWERED, counted by kind.
 *
 * The old map publishes these as `diagnostics.json` beside the tables rather than as a table, so nothing
 * here is compared row by row. They are still collected, because a count that is suspiciously clean is
 * itself a finding - an unresolved import that nobody records reads as a resolved one.
 */
export class Diagnostics {
  /**
   * `fileNow` says which file is being extracted, so a note can record where it was raised.
   *
   * WITHOUT IT A PARTIAL RUN CANNOT KEEP ONE. The notes are emitted at the end of the run, when no
   * file is open, so they read as rows belonging to nobody - rebuilt whole every time. A run that
   * walked a fraction of the tree then published a fraction of the diagnostics and the rest were
   * lost: the count dropped, and the ones that went were about files nothing had asked it to read.
   */
  constructor(fileNow = () => null) {
    this.fileNow = fileNow;
    this.counts = {};
    this.items = [];
    this.total = 0;
  }

  /**
   * ONE NOTE, WITH WHAT IT IS ABOUT. A tally answers "how many did you fail to resolve" and nothing
   * else: a map that says it noted a thousand diagnostics and cannot name one of them is asking to be
   * believed. The detail is whatever identifies the thing - a file, a name, a node - and it is published
   * as a row, so "which imports did not resolve" is a query rather than a re-run.
   */
  note(kind, detail = null) {
    this.counts[kind] = (this.counts[kind] ?? 0) + 1;
    this.total += 1;
    // WHERE IT WAS RAISED, not where it is written. A note raised outside every per-file walk - by a
    // rollup reading the whole store - keeps none, and is derived again on every run.
    const owner = this.fileNow();
    const item = detail ? { kind, ...detail } : { kind };
    this.items.push(owner ? { ...item, owner_file: owner } : item);
  }
}
