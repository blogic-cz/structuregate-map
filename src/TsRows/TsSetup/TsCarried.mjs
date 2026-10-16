/**
 * THE STRUCTURES A PARTIAL RUN IS NOT HANDED BACK, and the guard that proves no pass reads one.
 *
 * A carried row (`TsRunIo.mjs::readCarry`) belongs to a file this run does not read again: it is loaded so the
 * rollups and the registry see a whole map, and it is never sent back unless its table is whole. Most of its bulk is
 * STRUCTURES - a TypeScript expression's tree above all - which no pass after the walk looks inside: the carry
 * rebuilt each one from its node rows and node parsed it, only to hold it. So `rows/partial/carry.rs` hands the cells
 * named here back as a STUB. The cell is still there, a list or an object as it was, so the spec is read off it as
 * before (`columnKinds`: it held a structure; `deriveJoins`: it is no id) and `storedForm` compares it as one value
 * that did not move - but anything that looks INSIDE a stub is refused.
 *
 * NOT TRUSTED, PROVED: a pass that reads a stub, a stub a pass replaced, or a stub that would be written back (a whole
 * table sends its carried rows) MARKS the run, and the run is asked again with every cell carried (`MAP-UNCARRIED`).
 * Marked as well as thrown, because a pass that catches its own errors would otherwise read a stub as nothing.
 *
 * `{table: {column: true}}` stubs every carried row's cell; `[column, value]` only the rows whose `column` is
 * `value` - a TEMPLATE expression's tree is read again (`TsTplReads.mjs` resolves every template read on every
 * run), a TypeScript expression's never.
 */
export const UNREAD_WHEN_CARRIED = {
  assignments: { calls: true, reads: true, value: true },
  branches: { condition: true },
  class_decorators: { args: true },
  enums: { members: true },
  expressions: {
    ast: ['lang', 'ts'], calls: true, identifiers: true, pipes: true, reads: true, strings: true, write_refs: true,
    writes: true,
  },
  gates: { calls: true, reads: true, strings: true },
  locals: { bindings: true, calls: true, reads: true },
  members: { decorators: true },
  returns: { calls: true, reads: true },
  switch_cases: { discriminant: true, label_exprs: true, labels: true },
  template_literals: { holes: true, parts: true },
  template_nodes: { directives: true },
};

/** What `storedForm` holds for a stub: one value, the same before and after, never a structure's text. */
export const UNCARRIED = '\u0000uncarried';

const STUBS = new WeakSet();
let refused = null;
let storing = 0;

/** `fn`, while a stub answers `toJSON` with `UNCARRIED`: only the store's own stored form may write one down - a pass
 *  that stringified a cell to compare it would compare a stub with a structure and say they differ. */
export function asStored(fn) {
  storing += 1;
  try {
    return fn();
  } finally {
    storing -= 1;
  }
}

/** The first stubbed cell anything asked about, or null: why the run has to be asked again. */
export const uncarriedRead = () => refused;

/** Marks the run: `label` is `<table>.<column>`. */
export function refuse(label) {
  refused ??= label;
}

export const isStub = (value) => value !== null && typeof value === 'object' && STUBS.has(value);

/**
 * A cell that held a list (`a`) or an object (`o`), and refuses every look inside. `toJSON` alone answers - `UNCARRIED`,
 * inside `asStored` - so a row holding one still has a stored form to compare.
 */
export function stub(kind, label) {
  const no = () => {
    refuse(label);
    throw new Error(`the typescript half read ${label} of a row it was handed back without it`);
  };
  const traps = {
    get: (_, key) => (key === 'toJSON' && storing ? () => UNCARRIED : no()),
    has: no, ownKeys: no, getOwnPropertyDescriptor: no, getPrototypeOf: no, setPrototypeOf: no, set: no,
    deleteProperty: no, defineProperty: no, isExtensible: no, preventExtensions: no, apply: no, construct: no,
  };
  const held = new Proxy(kind === 'a' ? [] : {}, traps);
  STUBS.add(held);
  return held;
}

/** The first `<table>.<column>` of a WHOLE table that holds a stub - a row it is about to send back - or null. */
export function stubSent(store, whole) {
  for (const name of whole) {
    for (const row of store.tables[name] ?? []) {
      for (const [column, value] of Object.entries(row)) if (isStub(value)) return `${name}.${column}`;
    }
  }
  return null;
}
