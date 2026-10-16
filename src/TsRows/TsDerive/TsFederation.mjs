/**
 * THE REMOTE A MODULE FEDERATION ROUTE LOADS (#76) - `routes.load_children_remote` and `load_children_remote_name`.
 *
 * `loadChildren: () => loadRemoteModule('demo-orders', './Module').then((m) => m.OrdersEntryModule)` is typed `any` by
 * the checker, so the evaluator keeps only what the syntax says (`$remote`: the remote, the exposed module, the picked
 * name - `TsDecls/TsExpr/TsValueRefs.mjs::remoteTarget`). The tree's own federation configs say where that is: the
 * `module-federation.config.{js,ts}` whose `name` is the remote, its `exposes[<module>]` the file, and the class the
 * pick names DECLARED in that file. Every link is read, none guessed - no class is matched by its name alone, and a
 * remote two configs claim, an exposed module no config lists or a file that is not in the map leaves the id NULL.
 * The name is published either way: `<remote>/<module>`, the request webpack itself resolves (`demo-orders/Module`).
 *
 * A CONFIG IS READ SYNTACTICALLY with the workspace's own TypeScript parser - never evaluated: `module.exports = {..}`,
 * `export default {..}` or a same-file `const` either names, `as`/`satisfies` looked through. A ROLLUP, run on every
 * run, partial too: `routes` is rebuilt whole, so a config that moved is read again with it, and nothing the
 * extraction of the route's own file read has to name the config.
 */
import { readFileSync } from 'node:fs';
import path from 'node:path';

import { canonicalPath } from './TsPaths.mjs';

const CONFIGS = new Set(['module-federation.config.js', 'module-federation.config.ts']);

/** The object literal an expression is, through `as`, `satisfies`, parentheses and a same-file `const`. */
function objectOf(ts, sf, e) {
  const seen = new Set();
  let at = e;
  while (at !== undefined && !seen.has(at)) {
    seen.add(at);
    if (ts.isObjectLiteralExpression(at)) return at;
    if (ts.isParenthesizedExpression(at) || ts.isAsExpression(at)
      || (ts.isSatisfiesExpression !== undefined && ts.isSatisfiesExpression(at))) { at = at.expression; continue; }
    if (!ts.isIdentifier(at)) return null;
    const name = at.text;
    at = undefined;
    for (const st of sf.statements) {
      if (!ts.isVariableStatement(st)) continue;
      for (const d of st.declarationList.declarations) {
        if (ts.isIdentifier(d.name) && d.name.text === name) at = d.initializer;
      }
    }
  }
  return null;
}

/** A property's key as written, or null when it is computed. */
const keyOf = (ts, p) => (ts.isIdentifier(p.name) || ts.isStringLiteral(p.name) || ts.isNoSubstitutionTemplateLiteral(p.name)
  ? p.name.text : null);
const textOf = (ts, e) => (e !== undefined && (ts.isStringLiteral(e) || ts.isNoSubstitutionTemplateLiteral(e)) ? e.text : null);

/** `{ name, exposes: Map<module, path> }` of one config's source, or null when it states no `name`. */
export function readConfig(ts, file, text) {
  const sf = ts.createSourceFile(file, text, ts.ScriptTarget.Latest, true,
    file.endsWith('.ts') ? ts.ScriptKind.TS : ts.ScriptKind.JS);
  let root = null;
  for (const st of sf.statements) {
    if (ts.isExportAssignment(st)) root = objectOf(ts, sf, st.expression);
    if (ts.isExpressionStatement(st) && ts.isBinaryExpression(st.expression)
      && st.expression.operatorToken.kind === ts.SyntaxKind.EqualsToken) {
      const left = st.expression.left;
      if (ts.isPropertyAccessExpression(left) && left.name.text === 'exports' && ts.isIdentifier(left.expression)
        && left.expression.text === 'module') root = objectOf(ts, sf, st.expression.right);
    }
  }
  if (root === null) return null;
  let name = null;
  const exposes = new Map();
  for (const p of root.properties) {
    if (!ts.isPropertyAssignment(p)) continue;
    const key = keyOf(ts, p);
    if (key === 'name') name = textOf(ts, p.initializer);
    if (key !== 'exposes') continue;
    const list = objectOf(ts, sf, p.initializer);
    for (const e of list?.properties ?? []) {
      const module = ts.isPropertyAssignment(e) ? keyOf(ts, e) : null;
      const target = module === null ? null : textOf(ts, e.initializer);
      if (target !== null) exposes.set(module, target);
    }
  }
  return name === null ? null : { name, exposes };
}

/** The `files` row an exposed path names: `./` from the config's folder, a bare path from the workspace root (Nx). */
function exposedFile(target, configDir, fe, fileByAbs) {
  const bases = target.startsWith('.') ? [configDir] : [fe, configDir];
  for (const base of bases) {
    const at = path.resolve(base, target);
    for (const candidate of [at, `${at}.ts`, path.join(at, 'index.ts')]) {
      const hit = fileByAbs.get(canonicalPath(candidate));
      if (hit !== undefined) return hit;
    }
  }
  return null;
}

/** Every route whose `load_children` loads a remote gains its name and, when the tree proves it, its class. */
export function rollupRemoteRoutes(ts, store, fe) {
  const routes = store.table('routes').filter((r) => r.load_children?.$remote !== undefined);
  if (!routes.length) return { remotes: 0, resolved: 0 };
  const fileByAbs = new Map();
  const configs = new Map();
  for (const f of store.table('files')) {
    fileByAbs.set(canonicalPath(f.abs), f.id);
    if (!CONFIGS.has(path.basename(String(f.path)))) continue;
    let config = null;
    try {
      config = readConfig(ts, String(f.abs), readFileSync(f.abs, 'utf8'));
    } catch {
      config = null;
    }
    if (config === null) continue;
    const list = configs.get(config.name) ?? [];
    list.push({ ...config, dir: path.dirname(String(f.abs)) });
    configs.set(config.name, list);
  }
  const classAt = new Map();
  const twice = new Set();
  for (const c of store.table('classes')) {
    const key = `${String(c.file)}#${String(c.name)}`;
    if (classAt.has(key)) twice.add(key); else classAt.set(key, c.id);
  }
  let resolved = 0;
  for (const r of routes) {
    const { remote, expose, pick } = r.load_children.$remote;
    r.load_children_remote_name = `${remote}/${expose.startsWith('./') ? expose.slice(2) : expose}`;    const claimed = configs.get(remote) ?? [];
    const target = claimed.length === 1 ? claimed[0].exposes.get(expose) : undefined;
    const fid = target === undefined ? null : exposedFile(target, claimed[0].dir, fe, fileByAbs);
    const key = `${String(fid)}#${String(pick)}`;
    if (fid === null || typeof pick !== 'string' || twice.has(key) || !classAt.has(key)) continue;
    r.load_children_remote = classAt.get(key);    resolved += 1;
  }
  return { remotes: routes.length, resolved };
}
