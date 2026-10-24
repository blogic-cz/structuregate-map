/**
 * THE MEMBER A VALUE FILLS WHEN ONLY A TEMPLATE TYPES IT (rows 107).
 *
 * `rules() { return [{ Message: 'k' }]; }` declares no return type, and nothing in the program calls it - the template does,
 * `[rules]="rules()"`, and the checker never sees a template. So the extraction leaves such a value `returned_from` (the method,
 * how many arrays down it sits, the property it fills - `TsDecls/TsExpr/TsReadRefs.mjs::fieldReturned`), and this rollup takes
 * what the bound input DECLARES: every binding whose whole expression is a call of that method on the component (`input_usage`,
 * its `Read` resolved by the template reads), the input's `type_ref` indexed down the levels, and the property looked up on the
 * type there - an interface's own members, then each type it extends; a class's members, then its base; a type alias's object
 * members. One place across every such binding is `target` and `target_id`; none, or two, is nothing.
 *
 * RECOMPUTED ON EVERY RUN: a target set here is marked `target_via: 'template'` and dropped first, so a row handed back from the
 * last run never keeps an answer whose input or template has since moved. A kept row the answer then changes is what
 * `Store.verifyUntouched` finds, and the run is asked again over every hop.
 *
 * Imports are flat: see `TsDecls/TsTypeRef.mjs`.
 */

const LITERALS = ['string_literals', 'number_literals', 'template_literals', 'functions'];
const asRef = (v) => (v !== null && typeof v === 'object' && !Array.isArray(v) ? v : null);

export function rollupTemplateFills(store) {
  const waiting = new Map();
  for (const table of LITERALS) {
    for (const row of store.table(table)) {
      // BACK TO WHAT THE EXTRACTION LEFT: a literal in a property carries the empty `target` text (`TsLiterals.mjs`), an inline function none.
      if (row.target_via === 'template') {
        if (table === 'functions') delete row.target;
        else row.target = '';
        delete row.target_id;
        delete row.target_via;
      }
      // A named member is an object; a literal the checker named nothing for keeps the empty text a property's literal carries.
      if (asRef(row.target) !== null || typeof row.returned_from_id !== 'string') continue;
      const list = waiting.get(row.returned_from_id);
      if (list === undefined) waiting.set(row.returned_from_id, [row]);
      else list.push(row);
    }
  }
  if (!waiting.size) return { filled: 0 };

  const files = new Map(store.table('files').map((f) => [f.id, f]));
  const byFileAbs = new Map();
  for (const f of files.values()) { byFileAbs.set(f.abs, f.id); byFileAbs.set(f.path, f.id); }
  const membersOf = new Map();
  for (const m of store.table('members')) {
    const list = membersOf.get(m.class);
    if (list === undefined) membersOf.set(m.class, [m]);
    else list.push(m);
  }
  const typeMembersOf = new Map();
  for (const t of store.table('type_members')) {
    // A TYPE'S OWN MEMBER: top level, and the declaration itself - never a computed copy another file made.
    if (t.parent != null || t.via != null || (t.owner_file !== undefined && t.owner_file !== t.file)) continue;
    const list = typeMembersOf.get(t.owner);
    if (list === undefined) typeMembersOf.set(t.owner, [t]);
    else list.push(t);
  }
  const declsAt = new Map();
  for (const table of ['interfaces', 'classes', 'type_aliases']) {
    for (const row of store.table(table)) declsAt.set(`${String(row.file)}#${row.line}#${String(row.name)}`, { table, row });
  }
  const classById = new Map(store.table('classes').map((c) => [c.id, c]));
  const declOf = (ref) => {
    const fid = ref === null ? undefined : byFileAbs.get(ref.file);
    return fid === undefined || typeof ref.line !== 'number' ? undefined : declsAt.get(`${String(fid)}#${ref.line}#${String(ref.name)}`);
  };

  /** Every row declaring `name` on the type a decl is, its own first and its bases' only when it has none - a visited set ends a cycle. `walked` gathers
   *  the file of every declaration looked in. */
  const lookup = (decl, name, seen, walked) => {
    if (decl === undefined || seen.has(decl.row.id)) return [];
    seen.add(decl.row.id);
    walked.add(decl.row.file);
    if (decl.table === 'classes') {
      const own = (membersOf.get(decl.row.id) ?? []).filter((m) => m.name === name && m.file === decl.row.file);
      if (own.length) return own;
      const base = asRef(decl.row.extends);
      const parent = base !== null && typeof base.id === 'string' ? classById.get(base.id) : undefined;
      return parent === undefined ? [] : lookup({ table: 'classes', row: parent }, name, seen, walked);
    }
    const own = (typeMembersOf.get(decl.row.id) ?? []).filter((t) => t.name === name);
    if (own.length || decl.table !== 'interfaces') return own;
    return (Array.isArray(decl.row.extends_refs) ? decl.row.extends_refs : []).flatMap((r) => lookup(declOf(asRef(r)), name, seen, walked));
  };

  const ioById = new Map(store.table('io').map((r) => [r.id, r]));
  const exprById = new Map(store.table('expressions').map((r) => [r.id, r]));
  // WHAT EACH METHOD'S CALLS ARE BOUND TO: the input member's declared type, by method row.
  const boundTypes = new Map();
  for (const use of store.table('input_usage')) {
    const ast = asRef(asRef(exprById.get(use.expression)?.ast)?.ast);
    const read = asRef(ast?.receiver);
    if (ast?.k !== 'Call' || read?.k !== 'Read' || asRef(read.receiver)?.k !== 'Implicit') continue;
    const method = asRef(read.target)?.row;
    if (typeof method !== 'string' || !waiting.has(method)) continue;
    const io = ioById.get(use.io);
    const input = io === undefined ? undefined : (membersOf.get(io.class) ?? []).find((m) => m.name === io.member);
    const typeRef = asRef(input?.type_ref);
    if (typeRef === null) continue;
    const list = boundTypes.get(method);
    const bound = { typeRef, file: input.file };
    if (list === undefined) boundTypes.set(method, [bound]);
    else list.push(bound);
  }

  // WHAT THE ANSWER READ, IN THE FILE IT IS WRITTEN TO: the input's file and every declaration looked in join the literal's file's `reads`, so the
  // reads every row's names are proved against (`rows/partial/remember.rs`) hold it, and a change there reads the file again. Its own file and a
  // library are left out, as the read log leaves them (`TsSetup/TsReads.mjs`).
  const readAlso = (fid, walked) => {
    const owner = files.get(fid);
    if (owner === undefined) return;
    const reads = new Set(Array.isArray(owner.reads) ? owner.reads : []);
    for (const id of walked) {
      const f = files.get(id);
      if (f === undefined || id === fid || typeof f.path !== 'string' || f.path.startsWith('..') || f.path.split('/').includes('node_modules')) continue;
      reads.add(f.path);
    }
    owner.reads = [...reads].sort();
  };
  let filled = 0;
  for (const [method, rows] of waiting) {
    const types = boundTypes.get(method);
    if (types === undefined) continue;
    for (const row of rows) {
      const back = asRef(row.returned_from);
      if (back === null || typeof back.property !== 'string' || typeof back.levels !== 'number') continue;
      const places = new Map();
      const walked = new Set();
      for (const { typeRef, file } of types) {
        walked.add(file);
        let ref = typeRef;
        for (let i = 0; i < back.levels && ref !== null; i++) ref = asRef(ref.element);
        if (ref === null) { places.set('?', null); continue; }
        const found = lookup(declOf(ref), back.property, new Set(), walked);
        if (!found.length) places.set('?', null);
        for (const hit of found) places.set(hit.id, hit);
      }
      if (places.size !== 1 || places.has('?')) continue;
      const [hit] = places.values();
      row.target = { name: back.property, file: files.get(hit.file)?.abs ?? null, line: hit.line };
      row.target_id = hit.id;
      row.target_via = 'template';
      walked.add(hit.file);
      readAlso(row.file, walked);
      filled += 1;
    }
  }
  return { filled };
}
