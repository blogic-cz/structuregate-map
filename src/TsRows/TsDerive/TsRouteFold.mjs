/**
 * AN UNRESOLVED SPREAD MEANS "UNKNOWN", NOT "NONE" - and it is usually knowable.
 *
 * Ported from the original Angular extractor. A route assembled as
 * `{ ...makeIt(ARG), path: '...', component: C }` keeps its guards and its data inside the callee's return
 * value. Reading the guard property straight off the object finds nothing and writes an empty array, which
 * reads as "this route is UNGUARDED": nearly half the rows said that while really carrying several guards apiece.
 * This binds the call's arguments to the callee's parameters and hands back the return value.
 *
 * Imports are flat: see `TsDecls/TsTypeRef.mjs`.
 */

const asObj = (v) => ((!!v && typeof v === 'object' && !Array.isArray(v)) ? v : null);

/**
 * Does this value still hold something the evaluator could not resolve?
 *
 * Tested BY SHAPE, on the markers the evaluator itself writes rather than on a list of their names kept in
 * step by hand: a marked object carrying a resolved `value` is knowledge, while one that kept only its
 * source text - an identifier it could not follow, a call it could not enter, a cycle - is not.
 */
export function hasUnresolved(value) {
  if (Array.isArray(value)) return value.some((v) => hasUnresolved(v));
  const obj = asObj(value);
  if (obj === null) return false;
  if (obj.$cycle !== undefined) return true;
  if (obj.$call !== undefined) return true;
  if (obj.$expr !== undefined && obj.value === undefined) return true;
  return Object.values(obj).some((v) => hasUnresolved(v));
}

function paramsOf(row) {
  const declared = row.params;
  if (!Array.isArray(declared)) return null;
  const out = [];
  for (const item of declared) {
    const one = asObj(item);
    if (one === null || one.rest === true) return null;
    out.push({
      name: String(one.name ?? ''), rest: false,
      hasDefault: one.default !== undefined, value: one.default,
    });
  }
  return out;
}

/** Every declaration that can carry parameters, by file+name, with AMBIGUITY REMOVED: two declarations
 *  answering to one name is the normal case in a monorepo, and a key that resolves twice resolves to none. */
function calleeIndex(store) {
  const byKey = new Map();
  const ambiguous = new Set();
  for (const table of ['functions', 'members']) {
    for (const row of store.table(table)) {
      if (typeof row.name !== 'string') continue;
      const key = `${String(row.file)}#${row.name}`;
      if (byKey.has(key)) ambiguous.add(key);
      else byKey.set(key, row);
    }
  }
  for (const key of ambiguous) byKey.delete(key);
  return byKey;
}

/** Every callee with exactly ONE return row. More than one is a branch this join cannot pick, and none
 *  means there is nothing to fold. */
function singleReturns(store) {
  const byMember = new Map();
  const many = new Set();
  for (const row of store.table('returns')) {
    if (row.member === undefined || row.member === null) continue;
    if (byMember.has(row.member)) many.add(row.member);
    else byMember.set(row.member, row);
  }
  for (const id of many) byMember.delete(id);
  return byMember;
}

/**
 * Substitute bound arguments into a callee's evaluated return value. Three shapes, all of them shapes the
 * evaluator writes: an unfollowed IDENTIFIER naming a bound parameter becomes that parameter's value; an
 * ARRAY-position spread of a bound array is spliced in, which is what the language does; and a CONDITIONAL
 * whose predicate is exactly a bound parameter holding a boolean collapses to the branch it selects. A
 * predicate that is any other expression keeps BOTH branches - the fold knows the value of a parameter, not
 * the truth of an expression over it.
 */
function substitute(value, env) {
  if (Array.isArray(value)) {
    const out = [];
    for (const item of value) {
      const marker = asObj(item);
      if (marker !== null && marker.$spread !== undefined) {
        const inner = substitute(marker.$spread, env);
        if (Array.isArray(inner)) out.push(...inner);
        else out.push({ ...marker, $spread: inner });
        continue;
      }
      out.push(substitute(item, env));
    }
    return out;
  }
  const obj = asObj(value);
  if (obj === null) return value;
  if (obj.$kind === 'Identifier' && typeof obj.$expr === 'string' && env.has(obj.$expr)) {
    return env.get(obj.$expr);
  }
  const out = {};
  for (const [key, v] of Object.entries(obj)) out[key] = substitute(v, env);
  if (typeof obj.$cond === 'string' && env.has(obj.$cond)) {
    const decided = env.get(obj.$cond);
    if (typeof decided === 'boolean') return decided ? out.$then : out.$else;
  }
  return out;
}

/** The indexes are built ONCE per run: resolving a callee per call site re-scans every declaration table,
 *  and the tables this reads are the largest in the map. */
export function makeSpreadFolder(store) {
  const idByPath = new Map();
  for (const f of store.table('files')) {
    idByPath.set(f.path, f.id);
    idByPath.set(f.abs, f.id);
  }
  const callees = calleeIndex(store);
  const returns = singleReturns(store);
  return (spread) => {
    const target = asObj(spread.$target);
    if (target === null) return null;
    const file = typeof target.file === 'string' ? idByPath.get(target.file) : undefined;
    const name = typeof target.name === 'string' ? target.name : null;
    if (file === undefined || name === null) return null;
    const callee = callees.get(`${file}#${name}`);
    if (callee === undefined) return null;
    const ret = returns.get(callee.id);
    if (ret === undefined || asObj(ret.value) === null) return null;
    const params = paramsOf(callee);
    if (params === null) return null;
    // A MISSING ARGUMENT IS THE DEFAULT, and a parameter with neither stays unbound: its references keep
    // their identifier marker, which is what the map says elsewhere about a value it does not know.
    const args = Array.isArray(spread.$args) ? spread.$args : [];
    const env = new Map();
    params.forEach((param, i) => {
      const given = args[i];
      if (given !== undefined) env.set(param.name, given);
      else if (param.hasDefault) env.set(param.name, param.value);
    });
    const bound = asObj(substitute(ret.value, env));
    return bound === null ? null : { value: bound, from: ret.id };
  };
}
