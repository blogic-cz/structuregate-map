/**
 * A KEY THAT REACHES i18n THROUGH A SERVICE CALL, not through a template carrier.
 *
 * Ported from the original Angular extractor. A carrier is a TEMPLATE shape; the
 * other half of an application's translations never goes near one - a component asks a translation service
 * for the string in TypeScript and the key is an ordinary call argument. Without this a whole prefix reads
 * as ZERO REFERENCES while its keys sit in `calls`: hundreds of keys a locale file defines were reachable only this
 * way. Imports are flat: see `TsTypeRef.mjs`.
 *
 * WHICH METHOD TAKES A KEY IS THE CALLER'S CALL (`structuregate.ts.json`), exactly as a carrier is: no
 * property of a declaration says that a parameter is a translation key. What the map DOES know is checked
 * here - the name is matched against the callee the CHECKER RESOLVED, never against the call text, so a
 * method name shared with an unrelated declaration cannot be swept in.
 *
 * ONLY A STATICALLY KNOWN STRING IS CLAIMED. A call whose key is a variable is counted and not published:
 * inventing a key from an expression is the pattern-matching this extractor forbids, and the call row still
 * carries the argument, evaluated, for a consumer that wants to follow it.
 */

/**
 * The keys an argument STATICALLY IS - a plain string, or either branch of a resolved conditional.
 *
 * A key chosen by a ternary is not a string at the top level, so a filter on the argument's own type counted
 * the call as dynamic and published neither branch while the evaluator had already resolved both. On every
 * path the call takes the argument IS one of them, so both are claimed and neither is guessed.
 *
 * NOTHING ELSE IS DESCENDED. An object or an array HOLDS strings rather than being one, and reaching into it
 * would claim `{icon: 'home'}` as a key. A RESOLVED CONSTANT is unwrapped BY SHAPE - a marked object
 * carrying a primitive - never by a list of the evaluator's marker names.
 */
function staticArgStrings(value) {
  if (typeof value === 'string') return value === '' ? [] : [value];
  if (value === null || typeof value !== 'object') return [];
  if ('$cond' in value) return [...staticArgStrings(value.$then), ...staticArgStrings(value.$else)];
  const resolved = value.value;
  return typeof resolved === 'string' && resolved !== '' ? [resolved] : [];
}

/**
 * WHICH DECLARATIONS OF A NAMED METHOD ACTUALLY TAKE A KEY - because a name matches more than one.
 *
 * Some declarations answering to a configured name BUILD a key rather than consume one: they take a
 * FRAGMENT and compose the real key from it, so publishing the argument claimed `'header'` and `'default'` as
 * translation keys - a few dozen rows, every one a string no locale defines.
 *
 * Two disqualifications, both on evidence the map already holds, and both DEFAULTING TO QUALIFIED so a
 * declaration is only ever dropped for a positive reason:
 *
 *   - THE CALLEE MUST BE A DECLARATION THIS MAP EXTRACTED. A local `const t = (key) => ...` is resolved by
 *     the checker to a real declaration and has no row anywhere, so nothing about it was ever described.
 *     Interface signatures count: a service reached through its interface is the normal shape here.
 *   - A DECLARATION THAT CALLS A TRANSLATION METHOD WITHOUT PASSING A BARE PARAMETER IN A KEY POSITION IS
 *     BUILDING THE KEY. A pass-through hands its own parameter straight on; a builder wraps it first.
 *
 * A KEY POSITION IS A `string` PARAMETER OF THE CALLEE. Reading "any bare parameter" instead let a builder
 * pass for a forwarder - a call handing its OPTIONS parameter through untouched while the key itself is
 * composed - and fragments were published as keys. Which argument fills which parameter is already
 * published (`calls.arg_params`), so the position is read off the callee's own signature.
 */
function keyTakingDecls(store, methods, known) {
  const qualified = new Set();
  const paramsById = new Map();
  const keyById = new Map();
  const stringParams = new Map();
  for (const table of ['members', 'functions', 'type_members']) {
    for (const row of store.table(table)) {
      if (typeof row.name !== 'string' || !methods.has(row.name)) continue;
      const key = `${String(row.file)}#${row.name}`;
      qualified.add(key);
      keyById.set(row.id, key);
      const declared = Array.isArray(row.params) ? row.params : [];
      paramsById.set(row.id, declared.map((p) => String(p.name ?? '')));
      const strings = stringParams.get(key) ?? new Set();
      for (const p of declared) if (p.type === 'string') strings.add(String(p.name ?? ''));
      stringParams.set(key, strings);
    }
  }

  const forwards = new Set();
  const wraps = new Set();
  for (const call of store.table('calls')) {
    const name = call.target?.name;
    if (typeof name !== 'string' || !methods.has(name)) continue;
    const callee = typeof call.target?.file === 'string' ? known.get(call.target.file) : undefined;
    if (callee === undefined) continue;
    const owner = call.member;
    if (owner === undefined) continue;
    const params = paramsById.get(owner);
    if (params === undefined) continue;
    // `arg_params` and `args` are produced together, one entry per argument, so reading them at the same
    // index is a zip and not a choice among candidates. A callee whose parameters the map never described
    // names no key position, and the test falls back to any bare parameter rather than disqualifying a
    // declaration on missing evidence.
    const keyNames = stringParams.get(`${String(callee)}#${name}`);
    const argNames = Array.isArray(call.arg_params) ? call.arg_params : [];
    const isBare = (a, i) => {
      if (!a || typeof a !== 'object' || a.$kind !== 'Identifier') return false;
      if (typeof a.$expr !== 'string' || !params.includes(a.$expr)) return false;
      if (keyNames === undefined || keyNames.size === 0) return true;
      const fills = argNames[i];
      return typeof fills === 'string' && keyNames.has(fills);
    };
    if ((Array.isArray(call.args) ? call.args : []).some(isBare)) forwards.add(owner);
    else wraps.add(owner);
  }
  for (const [id, key] of keyById) if (wraps.has(id) && !forwards.has(id)) qualified.delete(key);
  return qualified;
}

export function rollupI18nCalls(store, methods, only = null) {
  if (!methods.size) return { refs: 0, dynamic: 0 };
  const fileOf = new Map();
  for (const table of ['members', 'functions']) {
    for (const row of store.table(table)) fileOf.set(row.id, row.file);
  }

  // THE CALLEE MUST BE DECLARED IN THE TREE UNDER EXTRACTION. A method name is not unique across a
  // dependency boundary: `error` is declared by an application's notification service AND by the
  // platform's console, and matching on the name alone would publish `console.error('Something failed')` as
  // a translation key.
  //
  // "IN THE TREE" IS THE INVENTORY, NOT EVERY ROW `files` HAPPENS TO HOLD. A file row is also MINTED as a
  // side effect of resolving a reference into a dependency, and one such row - the platform's own DOM
  // typings - was enough to let `console.error` pass a membership test built from every path in the table.
  // Only the inventory assigns a TIER, so a tier is what says a file is part of the tree.
  const known = new Map();
  for (const f of store.table('files')) {
    if (f.tier === undefined) continue;
    known.set(String(f.path), f.id);
    known.set(String(f.abs), f.id);
  }

  const takesKey = keyTakingDecls(store, methods, known);
  let refs = 0;
  let dynamic = 0;
  let skipped = 0;
  for (const call of store.table('calls')) {
    // A CALL IN A FILE THIS RUN IS NOT RE-EXTRACTING ALREADY HAS ITS REF. The rows written below
    // belong to the file the CALL is in - they are handed its `file`, which is what `owner_file`
    // becomes - so a partial run was given them back and deriving them again puts a copy beside each.
    //
    // NOT IN `keyTakingDecls`, WHICH IS WHERE THIS FIRST WENT. That pass decides which declarations
    // TAKE a key, and it has to see every call in the tree to do it: a method is only recognised as
    // key-taking because some caller somewhere passes it one. Filtering it would have made a service
    // stop being recognised because the files that call it were not re-read.
    if (only !== null && !only.has(call.owner_file)) { skipped += 1; continue; }
    const name = call.target?.name;
    if (typeof name !== 'string' || !methods.has(name)) continue;
    const home = typeof call.target?.file === 'string' ? known.get(call.target.file) : undefined;
    if (home === undefined || !takesKey.has(`${String(home)}#${name}`)) continue;
    // EVERY statically known argument, not the first. A notification method takes a title key AND a message
    // key, and a translation method takes a key plus an options object - which is no kind of string and so
    // contributes nothing. Reading one position would drop half of the first.
    const keys = (Array.isArray(call.args) ? call.args : []).flatMap(staticArgStrings);
    if (!keys.length) { dynamic += 1; continue; }
    for (const key of keys) {
      store.add('i18n_refs', 'k', {
        key, kind: 'service_call', source: 'ts',
        file: fileOf.get(call.member) ?? null,
        call: call.id, member: call.member, method: name, line: call.line,
      });
      refs += 1;
    }
  }
  return { refs, dynamic, skipped };
}
