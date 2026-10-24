/**
 * THE APPLICATION KNOWLEDGE NO COMPILER CAN SUPPLY - `structuregate.ts.json`, read and not re-derived.
 *
 * Which pipe carries a translation key, which DOM properties count as visibility in this application,
 * which methods take a key as an argument, which directories hold translations: none of that is in the
 * source in a form a parser can recognise, and every attempt to sniff it is a rule deciding for the
 * consumer. This file is ported VERBATIM from the predecessor rather than re-invented, and the
 * two lists in it say why - without its visibility inputs in `gateInputs` some gates go uncounted, and without the
 * `i18nCalls` list `i18n_index` loses a large share of its keys.
 *
 * `i18nCalls` names the methods that take ONE key; `i18nDictionaryCalls` names the methods that return EVERY key
 * under their first argument as a prefix (`getTranslationDictionary('a.b')`), so a call of one references all of
 * them - see `TsI18nCalls.mjs`.
 *
 * WHERE IT IS FOUND. `--config` names it; otherwise every ancestor of the workspace root is tried. The
 * tool this is ported from looks up from its WORKING DIRECTORY, which is the consumer's own tree - this
 * exe is launched from the tree being mapped, so the equivalent is the workspace and its ancestors, and a
 * config that lives anywhere else has to be named. It is ABSENT that is the supported default: a
 * single-project workspace needs none, and the map then publishes structural gates and every string
 * literal and claims no i18n.
 */
import { existsSync, readFileSync } from 'node:fs';
import path from 'node:path';

const CONFIG_NAME = 'structuregate.ts.json';

function ancestors(from) {
  const out = [];
  let dir = path.resolve(from);
  for (;;) {
    out.push(dir);
    const up = path.dirname(dir);
    if (up === dir) return out;
    dir = up;
  }
}

/** The config file, or '' when there is none. `given` is what `--config` named. */
export function findConfig(given, feRoot) {
  if (given) {
    const abs = path.resolve(given);
    if (!existsSync(abs)) throw new Error(`--ts-config ${given}: no such file (${abs})`);
    return abs;
  }
  for (const dir of ancestors(feRoot)) {
    const p = path.join(dir, CONFIG_NAME);
    if (existsSync(p)) return p;
  }
  return '';
}

/**
 * `{ file, dir, data }` - the config, with the directory it was read from, because every path inside it is
 * relative to THAT file and not to the workspace. A config committed beside the project stays valid on
 * another machine only if it is resolved that way.
 */
export function loadConfig(given, feRoot) {
  const file = findConfig(given, feRoot);
  if (!file) return { file: '', dir: feRoot, data: {} };
  let data;
  try {
    data = JSON.parse(readFileSync(file, 'utf8'));
  } catch (error) {
    throw new Error(`${file}: ${error.message}`);
  }
  if (data === null || typeof data !== 'object' || Array.isArray(data)) {
    throw new Error(`${file}: expected a JSON object at the top level`);
  }
  return { file, dir: path.dirname(file), data };
}

/** A list field, as absolute directories resolved against the FE root. `locales` is workspace-relative in
 *  the config's own documentation, unlike `feRoot`/`nodeModules`, which are relative to the config. */
export function directories(config, key, feRoot) {
  const value = config.data?.[key];
  if (!Array.isArray(value)) return [];
  return value.filter((v) => typeof v === 'string').map((v) => path.resolve(feRoot, v));
}

/** A list field of plain strings, as written. */
export function names(config, key) {
  const value = config.data?.[key];
  return Array.isArray(value) ? value.filter((v) => typeof v === 'string') : [];
}

/**
 * WHICH STRINGS ARE PROPERTY PATHS, AND FROM WHERE (rows 98, #109): application knowledge, so an INPUT - `propertyPaths`, each
 * `{ member: 'Type.Member', root: 'this' | { argument: n } }`, as `{type, member, root: {kind, index?}}`. A string filling that member is resolved segment
 * by segment from the root's type (`TsExpr/TsReadRefs.mjs::pathRefs`); none given, none resolved. A spec of another shape is handed to `invalid` by its
 * member text and skipped, never guessed into one.
 */
export function propertyPaths(config, invalid) {
  const out = [];
  for (const spec of Array.isArray(config.data?.propertyPaths) ? config.data.propertyPaths : []) {
    const member = typeof spec?.member === 'string' ? spec.member : '';
    const cut = member.lastIndexOf('.');
    const root = spec?.root === 'this' ? { kind: 'this' }
      : Number.isInteger(spec?.root?.argument) && spec.root.argument >= 0 ? { kind: 'argument', index: spec.root.argument } : null;
    if (cut <= 0 || cut === member.length - 1 || root === null) { invalid(member); continue; }
    out.push({ type: member.slice(0, cut), member: member.slice(cut + 1), root });
  }
  return out;
}
