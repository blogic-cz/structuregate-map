/**
 * THE TRANSLATION KEYS A ROUTE'S PATH IS BUILT FROM (#74) - `route_keys`, one row per `{{key}}` of a route's own `path`.
 *
 * A translating router resolves `path: '{{routes.order.detail}}/:id'` at run time, and code elsewhere tests the same
 * key; with only the path text, a consumer joined the two by parsing `{{ }}` out of a string. Each key is a row:
 * `route` (the `routes` row), `file` (the route's), `ord` (the `/`-separated segment it sits in, from 0), `key`, and `translation` - a
 * `translations` row defining the key, NULL when no locale does. Derived HERE, beside the routes and the locales,
 * because `translations.referenced` is decided in this half (`TsLocales.mjs`) and a key a route names is referenced.
 *
 * A KEY IS WHAT THE CLOSURE'S `braced_names` READS (`rust/fbtcore/src/rows/ts/sourcescan.rs`): the text between `{{`
 * and `}}`, trimmed, every character a word character, `$` or `.`; a failed attempt moves on by one character. Two
 * readers of one convention must not disagree on which keys a path names. Scanned by hand - no pattern.
 */

const isKeyChar = (c) => (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || (c >= '0' && c <= '9')
  || c === '_' || c === '$' || c === '.';

/** Every `{{key}}` of `text`, in order. */
export function bracedKeys(text) {
  const out = [];
  let at = 0;
  for (;;) {
    const start = text.indexOf('{{', at);
    if (start < 0) break;
    const end = text.indexOf('}}', start + 2);
    if (end < 0) break;
    const name = text.slice(start + 2, end).trim();
    if (name === '' || ![...name].every(isKeyChar)) {
      at = start + 1;
      continue;
    }
    out.push({ key: name, at: start });
    at = end + 2;
  }
  return out;
}

/** `route_keys`, after the routes fold and the locales - before `translations.referenced` is decided. */
export function rollupRouteKeys(store) {
  const translationOf = new Map();
  for (const t of store.table('translations')) {
    const key = String(t.key);
    if (!translationOf.has(key)) translationOf.set(key, t.id);
  }
  let keys = 0;
  for (const r of store.table('routes')) {
    if (typeof r.path !== 'string' || !r.path.includes('{{')) continue;
    for (const { key, at } of bracedKeys(r.path)) {
      // THE SEGMENT, not the key's position among keys: `/` before the key counts the segments in front of it.
      let ord = 0;
      for (let i = r.path.indexOf('/'); i >= 0 && i < at; i = r.path.indexOf('/', i + 1)) ord += 1;
      // `file` IS THE ROUTE'S, as `routes` carries it - the key is written there - and `add` makes it `owner_file` too,
      // so a rebuild finds the row by the file its route came from. Without it the row had no file route the spec
      // could prove: `translation` lands on a locale file, `route` disagreed with it, and both were refuted.
      store.add('route_keys', 'rk', { route: r.id, file: r.file, ord, key, translation: translationOf.get(key) ?? null });
      keys += 1;
    }
  }
  return keys;
}
