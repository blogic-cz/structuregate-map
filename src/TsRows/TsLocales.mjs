/**
 * THE OTHER HALF OF i18n - the keys that are DEFINED, not only the ones the code references.
 *
 * Ported from the original Angular extractor. Without it the translation files are
 * inventory rows with `parsed: false`, so the two questions anyone actually asks - "is this key defined?"
 * and "is it defined only for that locale?" - cannot be answered from the map at all.
 *
 * WHICH FILES ARE TRANSLATIONS IS THE CALLER'S CALL, never a guess: the `locales` directories are named in
 * `structuregate.ts.json`. Nothing here decides that a file "looks like" translations - that would be a rule
 * deciding for the consumer, and a frontend may have several locale directories plus override directories
 * that no rule would tell apart from ordinary config.
 *
 * A key is flattened to the DOTTED PATH the code uses, because that is what an `i18n_refs.key` holds and
 * the whole point is that the two join. The value is kept verbatim: it is the answer to "what does this
 * key say", and truncating it would make the map lie about the text it publishes.
 */
import { readFileSync } from 'node:fs';

import { slash } from './TsPaths.mjs';

/**
 * One leaf of a translation file: the dotted key and its text.
 *
 * AN ARRAY IS A LIST OF LEAVES, not nothing. `"North": ["Alpha", ...]` is read by the code as a whole -
 * `dict('regions.North')` - so dropping it called a defined key undefined. Each element is a leaf named
 * `key[i]` (`key[i].sub` inside an object), and carries `arrays`: the key of every array it sits in,
 * outermost first, which is how the coverage join knows those keys are defined. A number, a boolean or a
 * null is no text, in an array or out of it.
 */
function flatten(value, prefix, out, arrays = []) {
  if (typeof value === 'string') {
    out.push({ key: prefix, text: value, ...(arrays.length ? { arrays } : {}) });
    return;
  }
  if (value === null || typeof value !== 'object') return;
  if (Array.isArray(value)) {
    const inside = [...arrays, prefix];
    value.forEach((v, i) => flatten(v, `${prefix}[${i}]`, out, inside));
    return;
  }
  for (const [k, v] of Object.entries(value)) {
    flatten(v, prefix ? `${prefix}.${k}` : k, out, arrays);
  }
}

/**
 * Every `.json` inside the named locale directories, as `translations` rows.
 *
 * The file row is REUSED, not re-interned: these files are already in the inventory, and marking them
 * parsed here is what stops them reading as "present but never opened".
 */
/**
 * THE TWO HALVES JOINED: every key the code REFERENCES against every key a locale file DEFINES.
 *
 * `i18n_index` gains `defined_in` (the locales that carry the key) and `undefined_key` when none do. The
 * inverse - a translation nothing references - is published on the `translations` row as `referenced`, so
 * neither direction makes the consumer build the join itself.
 *
 * A DYNAMIC KEY IS NOT COUNTED AS MISSING. `translate('p.' + kind)` records its literal parts and a null
 * key, so the map cannot know what it resolves to; calling those undefined would report hundreds of false
 * gaps, which is the same lie as an empty guard list meaning "unguarded".
 */
export function rollupTranslationCoverage(store) {
  // AN ARRAY'S OWN KEY IS DEFINED where any of its elements is: a call reading the whole list is no gap.
  const byKey = new Map();
  const define = (key, locale) => {
    const hit = byKey.get(key);
    if (hit) hit.add(locale);
    else byKey.set(key, new Set([locale]));
  };
  for (const t of store.table('translations')) {
    define(String(t.key), String(t.locale));
    for (const a of Array.isArray(t.arrays) ? t.arrays : []) define(String(a), String(t.locale));
  }
  if (!byKey.size) return { defined: 0, missing: 0, unused: 0 };

  // REFERENCED MEANS THE WHOLE SOURCE, not the templates. Counting only the index called thousands of
  // translations unused when a large share are referenced from TypeScript - the kind of number that gets
  // quoted and acted on. The extra half is an exact JOIN, not a pattern test: a literal counts when it
  // EQUALS a key the locale files declare. Nothing is inferred from the shape of a string.
  const referenced = new Set();
  for (const row of store.table('i18n_index')) referenced.add(String(row.key));
  for (const table of ['string_literals', 'template_strings']) {
    for (const row of store.table(table)) {
      const text = typeof row.value === 'string' ? row.value : null;
      if (text !== null && byKey.has(text)) referenced.add(text);
    }
  }

  let defined = 0;
  let missing = 0;
  for (const row of store.table('i18n_index')) {
    const locales = byKey.get(String(row.key));
    if (locales) {
      row.defined_in = [...locales].sort();
      defined += 1;
    } else {
      row.undefined_key = true;
      missing += 1;
    }
  }
  let unused = 0;
  // AN ELEMENT IS REFERENCED WITH ITS ARRAY: the code reads the list, never `key[1]` by that name.
  for (const t of store.table('translations')) {
    const isReferenced = referenced.has(String(t.key))
      || (Array.isArray(t.arrays) && t.arrays.some((a) => referenced.has(String(a))));
    t.referenced = isReferenced;
    if (!isReferenced) unused += 1;
  }
  return { defined, missing, unused };
}

export function collectLocales(store, dirs, diag) {
  if (!dirs.length) return { files: 0, keys: 0 };
  const wanted = dirs.map((d) => slash(d).toLowerCase());
  let fileCount = 0;
  let keyCount = 0;
  for (const row of store.table('files')) {
    if (row.ext !== 'json') continue;
    const abs = String(row.abs).toLowerCase();
    if (!wanted.some((d) => abs.startsWith(`${d}/`))) continue;
    let parsed;
    try {
      // A UTF-8 BOM IS NOT JSON. Node does not strip one on a `utf8` decode, so `JSON.parse` throws and
      // the file vanishes - whole locale files and their real keys, with `parsed: false` and no diagnostic,
      // which is indistinguishable from a file that was never a candidate. The BOM is removed by CODE
      // POINT, and a file that still fails is COUNTED rather than skipped in silence.
      const text = readFileSync(row.abs, 'utf8');
      parsed = JSON.parse(text.codePointAt(0) === 0xFEFF ? text.slice(1) : text);
    } catch {
      diag?.note('locale_file_unparsed');
      continue;
    }
    const leaves = [];
    flatten(parsed, '', leaves);
    if (!leaves.length) continue;
    // The locale is the file's own name - `en`, `de`, `override_1`. It is not interpreted here: which of
    // those is a language and which an override is the caller's domain, not this parser's.
    const name = String(row.path).slice(String(row.path).lastIndexOf('/') + 1);
    const locale = name.endsWith('.json') ? name.slice(0, -'.json'.length) : name;
    for (const leaf of leaves) {
      store.add('translations', 'tr', { file: row.id, locale, key: leaf.key, text: leaf.text,
        ...(leaf.arrays ? { arrays: leaf.arrays } : {}) });
    }
    store.patch(row.id, { parsed: true, tier: row.tier ?? 'inventory', locale, keys: leaves.length });
    fileCount += 1;
    keyCount += leaves.length;
  }
  return { files: fileCount, keys: keyCount };
}
