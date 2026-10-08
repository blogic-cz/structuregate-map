/**
 * WHICH STRINGS IN A TEMPLATE EXPRESSION ARE TRANSLATION KEYS - the ones the AST PROVES, never the ones a
 * string LOOKS like.
 *
 * Ported from the original Angular extractor. Every rule here exists because its absence put a
 * key in the map that the application never looks up, and a list of "missing translations" built from those
 * is a list of things to go and not fix. Imports are flat: see `TsDecls/TsTypeRef.mjs`.
 */

/**
 * Strings passed THROUGH a translate pipe: `'a.b' | translate`, `x | translate: {...}`.
 *
 * A LITERAL THAT IS AN OPERAND OF A CONCATENATION IS NOT A KEY. `{{ base + '.title' | translate }}`
 * builds its key at run time, and collecting the literal half published `.title` as a referenced key -
 * about a quarter of one map's "referenced but undefined" keys were fragments like that. The fragments are still
 * recorded, honestly, as the literal parts of a dynamic key.
 */
export function pipeArgStrings(carriers, norm, out = [], seen = new Set()) {
  if (!norm || typeof norm !== 'object' || seen.has(norm)) return out;
  seen.add(norm);
  try {
    if (norm.k === 'Pipe' && typeof norm.name === 'string' && carriers.has(norm.name)) {
      const pipe = norm.name;
      // A LITERAL THAT PASSES THROUGH ANOTHER PIPE FIRST IS A BASE, NOT A KEY. `'a.b' | suffix: x |
      // translate` looks up whatever that pipe returns - one such pipe appends a suffix - so
      // `a.b` names an OBJECT in the locale file and was reported as an undefined key. A pipe is runtime
      // construction by another spelling, so the literal is left to the dynamic-key path.
      if (!transformedByPipe(norm.exp, carriers)) {
        collectLiteralStrings(norm.exp, (v) => out.push({ value: v, pipe }), new Set(), true);
      }
    }
    for (const v of Object.values(norm)) {
      if (Array.isArray(v)) v.forEach((x) => pipeArgStrings(carriers, x, out, seen));
      else if (v && typeof v === 'object') pipeArgStrings(carriers, v, out, seen);
    }
  } finally {
    seen.delete(norm);
  }
  return out;
}

/** Does a carrier's argument pass through a NON-carrier pipe on its way in? */
export function transformedByPipe(norm, carriers, seen = new Set()) {
  if (!norm || typeof norm !== 'object' || seen.has(norm)) return false;
  seen.add(norm);
  if (norm.k === 'Pipe' && typeof norm.name === 'string' && !carriers.has(norm.name)) return true;
  for (const v of Object.values(norm)) {
    if (Array.isArray(v)) {
      if (v.some((x) => transformedByPipe(x, carriers, seen))) return true;
    } else if (v && typeof v === 'object' && transformedByPipe(v, carriers, seen)) return true;
  }
  return false;
}

export function collectLiteralStrings(norm, emit, seen = new Set(), keysOnly = false) {
  if (!norm || typeof norm !== 'object' || seen.has(norm)) return;
  if (keysOnly) {
    // A `+` BUILDS A VALUE OUT OF PARTS; its literal halves are FRAGMENTS, not values in their own right.
    if (norm.k === 'Binary' && norm.op === '+') return;
    // A LITERAL PASSED TO A FUNCTION IS THAT FUNCTION'S INPUT, not the key. `[text]="getTitle('orderTotal')"`
    // builds its key inside `getTitle`; recording `orderTotal` claimed a key the application never looks up,
    // and many of one map's "referenced but undefined" keys came from this shape alone. The call itself is
    // still recorded, with its arguments, which is where a consumer should look.
    if (norm.k === 'Call' || norm.k === 'SafeCall') return;
    // A COMPARISON OPERAND IS A VALUE BEING TESTED, not a key: in `t === 'demo' ? 'a.b' : 'a.c'` only
    // the branches are keys. Only the CONDITION is skipped, so both branches are still collected.
    if (norm.k === 'Cond') {
      collectLiteralStrings(norm.then, emit, seen, keysOnly);
      collectLiteralStrings(norm.else, emit, seen, keysOnly);
      return;
    }
  }
  seen.add(norm);
  try {
    // THE EMPTY STRING IS NOT A KEY. `[text]="a ?? b ?? ''"` ends in a fallback meaning "nothing to show",
    // and publishing it produced an `i18n_refs` row whose key no locale can ever define.
    if (norm.k === 'Literal' && typeof norm.v === 'string' && (!keysOnly || norm.v !== '')) emit(norm.v);
    for (const v of Object.values(norm)) {
      if (Array.isArray(v)) v.forEach((x) => collectLiteralStrings(x, emit, seen, keysOnly));
      else if (v && typeof v === 'object') collectLiteralStrings(v, emit, seen, keysOnly);
    }
  } finally {
    seen.delete(norm);
  }
}

/** A `+` concatenation with at least one literal AND at least one non-literal operand - the runtime-built
 *  key case. Returns the operands in order, `null` for each dynamic one. */
export function dynamicKeyParts(norm, seen = new Set()) {
  if (!norm || typeof norm !== 'object' || seen.has(norm)) return null;
  seen.add(norm);
  try {
    if (norm.k === 'Binary' && norm.op === '+') {
      const parts = [];
      let sawLiteral = false;
      const operand = (n) => {
        if (!n || typeof n !== 'object') { parts.push(null); return; }
        if (n.k === 'Binary' && n.op === '+') { operand(n.left); operand(n.right); return; }
        if (n.k === 'Literal' && typeof n.v === 'string') { parts.push(n.v); sawLiteral = true; }
        else parts.push(null);
      };
      operand(norm);
      if (sawLiteral && parts.includes(null)) return parts;
    }
    for (const v of Object.values(norm)) {
      if (Array.isArray(v)) {
        for (const x of v) {
          const r = dynamicKeyParts(x, seen);
          if (r) return r;
        }
      } else if (v && typeof v === 'object') {
        const r = dynamicKeyParts(v, seen);
        if (r) return r;
      }
    }
  } finally {
    seen.delete(norm);
  }
  return null;
}

/** Where a template node or attribute IS, one-based, from the compiler's own spans. */
export function spanOf(node) {
  const start = node.sourceSpan?.start ?? node.keySpan?.start;
  const end = node.sourceSpan?.end;
  if (!start) return {};
  const out = { line: (start.line ?? 0) + 1, col: (start.col ?? 0) + 1 };
  if (end) out.end_line = (end.line ?? 0) + 1;
  return out;
}

/** A tag with a dash is a custom element - the HTML spec's own rule, not a project convention. */
export const isCustomElement = (tag) => tag.includes('-');
