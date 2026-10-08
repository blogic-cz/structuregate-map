/**
 * EVERY `<table>.<field>` THAT HOLDS AN ID, and the table(s) it points at.
 *
 * Ported from the original Angular extractor. The rows of this half are handed to a
 * consumer as SQLite, where nothing records a foreign key, so the map states its own. Imports are flat:
 * see `TsDecls/TsTypeRef.mjs`.
 */

/** An id looks like `<prefix>:<digits>`. Tested by STRUCTURE - no pattern matching, and no prefix list of
 *  its own: the prefix must be one `id_scheme` names, so the two can never disagree. */
function idPrefix(value, known) {
  if (typeof value !== 'string') return null;
  const cut = value.indexOf(':');
  if (cut <= 0 || cut === value.length - 1) return null;
  const prefix = value.slice(0, cut);
  if (!known.has(prefix)) return null;
  for (const ch of value.slice(cut + 1)) {
    if (ch < '0' || ch > '9') return null;
  }
  return prefix;
}

/**
 * The join spec, derived.
 *
 * The WHOLE of every table is scanned rather than a sample: a field present on a minority of rows - the
 * optional ones, which are exactly the ones a consumer is least sure about - would be invisible to a
 * sample and is the case this exists to cover.
 */
export function deriveJoins(store, idScheme, tables) {
  const known = new Set(Object.keys(idScheme));
  const out = {};
  for (const table of tables) {
    const seen = new Map();
    // A FOREIGN KEY HOLDS IDS OR NOTHING. Counting a field as a key because SOME of its values look like
    // ids published a join that resolved for a handful of rows and silently returned nothing for the rest -
    // the column held an attribute NAME for most kinds and an id for one. A field with even a single
    // non-null value that is not an id is not a key, and saying so is the whole point of the spec.
    const disqualified = new Set();
    for (const row of store.table(table)) {
      for (const [field, value] of Object.entries(row)) {
        // `owner_file` IS NOT A JOIN, it is housekeeping. It holds a file id, so the walk below would
        // read it as a foreign key and the ANCHOR spec would then answer "how does this row reach its
        // file" with a direct column for every table at once - burying the semantic route a consumer
        // actually wants (a binding reaches its file through its node and its template) and making the
        // join-proving machinery that derives those routes prove nothing. It exists so a REBUILD can find
        // the rows a file produced; it says nothing about what the row means.
        if (field === 'id' || field === 'owner_file' || value === null || value === undefined) continue;
        const prefix = idPrefix(value, known);
        if (prefix === null) {
          disqualified.add(field);
          continue;
        }
        const hit = seen.get(field);
        if (hit) hit.add(prefix);
        else seen.set(field, new Set([prefix]));
      }
    }
    for (const field of disqualified) seen.delete(field);
    for (const [field, prefixes] of seen) {
      // A field carrying ids of SEVERAL kinds is real - a body's owner is a member or a top-level function
      // - and both are named, in the order the scheme lists them, rather than one being chosen.
      const targets = [...prefixes].sort().map((p) => idScheme[p] ?? p);
      out[`${table}.${field}`] = targets.join(' | ');
    }
  }
  return Object.fromEntries(Object.entries(out).sort(([a], [b]) => a.localeCompare(b)));
}
