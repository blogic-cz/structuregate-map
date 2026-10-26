/**
 * WHICH OF SEVERAL SAME-SELECTOR CANDIDATES ANGULAR WOULD ACTUALLY USE.
 *
 * Ported from the original Angular extractor. A monorepo can declare `TooltipComponent`
 * and `TranslateDirective` twice, in a library and in the application, with identical selectors. A matcher
 * over the whole tree returns both, and it is RIGHT to: the compiler disambiguates by the consuming
 * component's NgModule scope. So every `renders` row gets `scope` - `declared`, `imported`, `out_of_scope`
 * or `unknown` - and a consumer that wants one answer filters on it while the alternatives stay visible
 * instead of being silently dropped. Imports are flat: see `TsDecls/TsTypeRef.mjs`.
 */
import { slash } from './TsPaths.mjs';

const refsOf = (row, field) => row[field] ?? [];

/**
 * THE SCOPE MACHINERY, built once and asked by both rollups that need it: which candidates a render's host can see
 * (`rollupModuleScope`) and which pipe a template's usage resolves to (`TsTpl/TsPipeRefs.mjs`, #86). `quiet()` stops its notes
 * once the render rollup has made them: the pipe rollup asks again of the same modules, and a note is no new fact.
 */
export function makeScopes(store, diag) {
  let note = (kind, detail) => diag.note(kind, detail);
  const classById = new Map(store.table('classes').map((c) => [c.id, c]));
  const modules = store.table('ng_modules');
  const fileIdByAbs = new Map(store.table('files').map((f) => [f.abs, f.id]));

  const byFileName = new Map();
  const byName = new Map();
  /**
   * WHICH MODULE DECLARES A CLASS - keyed by FILE, not by name.
   *
   * A `declarations` entry is whatever the module IMPORTED the class as, and a codebase can re-export
   * classes under aliases so several classes with the same declared name can coexist. Keying on the
   * alias while probing with the class's own name meant the two strings never met: many real, resolvable
   * render edges were stamped `unknown` - most of the unknown rows. The ref carries its declaring FILE,
   * which the alias cannot change.
   */
  const declaringModuleByFile = new Map();
  const declaringModule = new Map();
  for (const m of modules) {
    byFileName.set(`${String(m.file)}#${String(m.name)}`, m);
    const list = byName.get(String(m.name));
    if (list) list.push(m);
    else byName.set(String(m.name), [m]);
    for (const d of refsOf(m, 'declarations')) {
      if (d?.name) declaringModule.set(d.name, m);
      if (d?.file) declaringModuleByFile.set(slash(d.file), m);
    }
  }
  for (const [name, rows] of byName) {
    if (rows.length > 1) diag.note('ng_module_name_collision', { name, modules: rows.map((m) => m.id) });
  }

  const modulesByFile = new Map();
  for (const m of modules) {
    const list = modulesByFile.get(m.file);
    if (list) list.push(m);
    else modulesByFile.set(m.file, [m]);
  }

  /**
   * Names a file imports FROM A DEPENDENCY - the one case where "no file on the ref" does not mean "look it
   * up by name".
   *
   * A shared module imports `MaskModule` from an npm package and lists `MaskModule.forRoot()`. The
   * package's typings carry no visible decorator, so the ref resolves to nothing - and a name-only fallback
   * then found an UNRELATED repo-local class of that name and merged its exports into the scope, asserting
   * an import path that does not exist in source.
   */
  const npmNamesByFile = new Map();
  for (const im of store.table('imports')) {
    const resolved = typeof im.resolved === 'string' ? im.resolved : null;
    const fromDependency = resolved === null ? im.external === true : resolved.includes('node_modules');
    if (!fromDependency) continue;
    let set = npmNamesByFile.get(im.file);
    if (!set) { set = new Set(); npmNamesByFile.set(im.file, set); }
    for (const n of im.names ?? []) {
      // THE LOCAL NAME - what the file writes, and what `ref.name` spells. `as` is the name a rename came
      // THROUGH (see TsImports), so reading it first made an aliased dependency NgModule miss this guard.
      const bound = n.name;
      if (bound) set.add(bound);
    }
  }

  // ONE NOTE PER NAME AND FILE (rows 96): a ref is asked again for every scope that reaches it - one refusal was noted up to fifty times.
  const refused = new Set();
  const modulesForRef = (ref, ownerFile) => {
    const name = ref?.name ?? (typeof ref?.call === 'string' ? ref.call.split('.')[0] : null);
    if (!name) return [];
    if (ref?.file) {
      const fid = fileIdByAbs.get(slash(ref.file));
      if (fid !== undefined) {
        const exact = byFileName.get(`${String(fid)}#${name}`);
        if (exact) return [exact];
        // A DECLARABLE OF THAT FILE IS NO MODULE: a module exporting a component it declares beside itself
        // named the file's one NgModule, the export folded into a module already seen, and the component read
        // `out_of_scope` in every module importing it.
        if (declarableByFileName.has(`${String(fid)}#${name}`)) return [];
        // An ALIASED module import: the file is authoritative, and a file declaring exactly one NgModule
        // leaves nothing to guess.
        const inFile = modulesByFile.get(fid);
        if (inFile?.length === 1) return inFile;
      }
    }
    if (ownerFile != null && npmNamesByFile.get(ownerFile)?.has(name)) {
      const key = `${String(ownerFile)}\0${name}`;
      if (!refused.has(key)) {
        refused.add(key);
        // WHY IT IS A ROW (rows 121): `shadows_local` when the tree declares a module of the same name - the refusal is what keeps the package's
        // name off the local class - else `dependency`, a module (or a standalone declarable) a package ships, which is no fault at all.
        note('module_ref_from_dependency', { name, file: ownerFile, reason: byName.has(name) ? 'shadows_local' : 'dependency' });
      }
      return [];
    }
    return byName.get(name) ?? [];
  };

  /** A declarable (component / directive / pipe) resolved to its CLASS ID - the identity `renders.to_class`
   *  carries. Same rule as above: the file wins over the name it was imported as. */
  const declarableByFileName = new Map();
  const declarablesByFile = new Map();
  const declarableByName = new Map();
  for (const table of ['components', 'directives', 'pipes']) {
    for (const row of store.table(table)) {
      declarableByFileName.set(`${String(row.file)}#${String(row.name)}`, row.class);
      const inFile = declarablesByFile.get(row.file);
      if (inFile) inFile.push(row.class);
      else declarablesByFile.set(row.file, [row.class]);
      const named = declarableByName.get(String(row.name));
      if (named) named.push(row.class);
      else declarableByName.set(String(row.name), [row.class]);
    }
  }

  const classIdForRef = (ref) => {
    const name = ref?.name ?? null;
    if (ref?.file) {
      const fid = fileIdByAbs.get(slash(ref.file));
      if (fid !== undefined) {
        const exact = name === null ? undefined : declarableByFileName.get(`${String(fid)}#${name}`);
        if (exact !== undefined) return exact;
        const inFile = declarablesByFile.get(fid);
        if (inFile?.length === 1) return inFile[0] ?? null;
      }
    }
    if (name !== null) {
      const named = declarableByName.get(name);
      // A name matching TWO classes is exactly the ambiguity this rollup exists to resolve - refuse it.
      if (named?.length === 1) return named[0] ?? null;
    }
    return null;
  };

  // `ext`: the LIBRARY pipes in scope, read off their typings at extraction (`external_pipes`, `TsDecls/TsLibPipes.mjs`).
  const emptyScope = () => ({ ids: new Set(), names: new Set(), ext: new Map() });
  const merge = (into, from) => {
    for (const i of from.ids) into.ids.add(i);
    for (const n of from.names) into.names.add(n);
    for (const [k, v] of from.ext) into.ext.set(k, v);
  };
  const addExternal = (into, row, from) => {
    for (const p of Array.isArray(row?.external_pipes) ? row.external_pipes : []) {
      if (p?.from === from) into.ext.set(`${String(p.file)}#${String(p.line)}#${String(p.name)}`, p);
    }
  };
  const addRef = (into, ref) => {
    const cid = classIdForRef(ref);
    if (cid !== null) { into.ids.add(cid); return; }
    const name = ref?.name ?? (typeof ref?.call === 'string' ? ref.call.split('.')[0] : null);
    if (name) into.names.add(name);
  };

  const exportedOf = (m, seen = new Set()) => {
    const out = emptyScope();
    if (!m || seen.has(m.id)) return out;
    seen.add(m.id);
    for (const e of refsOf(m, 'exports')) {
      const asModules = modulesForRef(e, m.file); // a re-exported MODULE widens the scope
      if (asModules.length) {
        for (const im of asModules) merge(out, exportedOf(im, seen));
        continue;
      }
      addRef(out, e);
    }
    addExternal(out, m, 'exports');
    return out;
  };

  /** `imports` may hold a module, a `RouterModule.forChild(...)`, or a standalone component directly. */
  const importsOf = (list, ownerFile) => {
    const out = emptyScope();
    for (const i of list) {
      const asModules = modulesForRef(i, ownerFile);
      if (asModules.length) {
        for (const im of asModules) merge(out, exportedOf(im));
        continue;
      }
      addRef(out, i);
    }
    return out;
  };

  const scopeCache = new Map();
  const scopeOf = (m) => {
    if (!m) return null;
    const cached = scopeCache.get(m.id);
    if (cached) return cached;
    const declared = emptyScope();
    for (const d of refsOf(m, 'declarations')) addRef(declared, d);
    const imported = importsOf(refsOf(m, 'imports'), m.file);
    addExternal(imported, m, 'imports');
    const scope = { declared, imported };
    scopeCache.set(m.id, scope);
    return scope;
  };

  const fileAbsById = new Map(store.table('files').map((f) => [f.id, String(f.abs)]));
  // A STANDALONE component has no NgModule at all: its scope is its own `imports` array. Without this, every
  // render from a standalone component was `unknown` however the module lookup was keyed.
  const standaloneByClass = new Map();
  for (const c of store.table('components')) if (c.standalone === true) standaloneByClass.set(c.class, c);
  /** A standalone component's scope: its own `imports`, and the library pipes they make visible. */
  const standaloneScope = (c) => {
    const scope = importsOf(refsOf(c, 'imports'), c.file);
    addExternal(scope, c, 'imports');
    return scope;
  };
  /** The NgModule declaring a class - keyed by its FILE, then by its name. */
  const moduleOfClass = (classId) => {
    const fromClass = classById.get(classId);
    const fromFile = fromClass ? fileAbsById.get(fromClass.file) : undefined;
    return (fromFile ? declaringModuleByFile.get(fromFile) : undefined)
      ?? (typeof fromClass?.name === 'string' ? declaringModule.get(fromClass.name) : undefined);
  };
  return {
    scopeOf, standaloneScope, moduleOfClass, standaloneByClass,
    quiet: () => { note = () => {}; },
  };
}

/**
 * Is this render's TARGET in that scope - BY IDENTITY first, by name only if nothing resolved.
 *
 * The name path is what this rollup used to do alone, and it decided about a third of the edges wrongly: both
 * `TooltipComponent`s are named `TooltipComponent`, so a module importing only one copy stamped the other
 * `imported` too - the field could not disambiguate in exactly the case it exists for.
 */
const reach = (scope, r) => {
  if (scope.ids.has(r.to_class)) return 'id';
  return scope.names.has(String(r.to_name)) ? 'name' : null;
};

/**
 * THE SCOPE ONE CANDIDATE IS IN, for the host `from_class` (rows 80): `{ scope, standalone?, module?, byName? }` - the one rule `rollupModuleScope` stamps on every `renders`
 * row, asked of any (host, candidate) pair, so a pass that runs BEFORE that rollup (a template `#ref`'s type, `TsTpl/TsTplReads.mjs`) chooses the candidate `renders` does.
 */
export function judgeScope(scopes, r) {
  const { scopeOf, standaloneScope, moduleOfClass, standaloneByClass } = scopes;
  // A STANDALONE COMPONENT IS ASKED FIRST: no NgModule may declare it, and the module lookup below is keyed
  // by FILE - so one sharing a file with an NgModule took that module's scope, and a tag it never imports
  // read as `declared`.
  const standalone = standaloneByClass.get(r.from_class);
  if (standalone) {
    // IT IS IN ITS OWN SCOPE (rows 82): the compiler puts a standalone component itself among its dependencies, so its own
    // selector in its own template is a render Angular instantiates, imported or not - `declared`, as a module's own declaration is.
    if (r.to_class === r.from_class) return { scope: 'declared', standalone: true, byName: false };
    // Its own `imports` array IS the scope - the rule the compiler applies to a standalone component.
    const hit = reach(standaloneScope(standalone), r);
    return { scope: hit === null ? 'out_of_scope' : 'imported', standalone: true, byName: hit === 'name' };
  }
  const mod = moduleOfClass(r.from_class);
  const scope = scopeOf(mod);
  if (!scope || !mod) return { scope: 'unknown' };
  const declaredHit = reach(scope.declared, r);
  const importedHit = declaredHit === null ? reach(scope.imported, r) : null;
  return {
    scope: declaredHit !== null ? 'declared' : importedHit !== null ? 'imported' : 'out_of_scope',
    module: mod.id, byName: declaredHit === 'name' || importedHit === 'name',
  };
}

export function rollupModuleScope(store, diag, scopes = makeScopes(store, diag)) {
  const counts = { declared: 0, imported: 0, out_of_scope: 0, unknown: 0 };
  for (const r of store.table('renders')) {
    // NGMODULE SCOPE IS A RULE ABOUT SELECTOR MATCHING, and an edge the compiler never matched is not
    // subject to it: a component named in TypeScript is imported by the FILE, and a creator service belongs
    // to no NgModule at all. Judged by the template rule these came out `out_of_scope`, and the reach table
    // DROPS an out-of-scope edge - so the very components it exists to reach stayed unreachable.
    if (r.via !== undefined && r.via !== 'element') {
      r.scope = 'direct_reference';
      counts.direct_reference = (counts.direct_reference ?? 0) + 1;
      continue;
    }
    const judged = judgeScope(scopes, r);
    if (judged.standalone) r.standalone = true;
    if (judged.module !== undefined) r.module = judged.module;
    r.scope = judged.scope;
    if (judged.byName) r.scope_by_name = true;
    counts[r.scope] = (counts[r.scope] ?? 0) + 1;
  }

  // Where a single element still matched more than one IN-SCOPE candidate, say so. Deduped by TARGET id and
  // not by name: the two colliding components share a name, so a name-based set collapsed the very
  // ambiguity this check exists to report.
  const perNode = new Map();
  for (const r of store.table('renders')) {
    if (r.scope === 'out_of_scope' || !r.node) continue;
    const list = perNode.get(r.node);
    if (list) list.push(r);
    else perNode.set(r.node, [r]);
  }
  for (const [, rows] of perNode) {
    const targets = new Map(rows.filter((r) => r.kind === 'component').map((r) => [r.to, r]));
    if (targets.size > 1) {
      // NO SCOPE TO BE IN (rows 93): a template no component owns (an `index.html`, an orphan `.html`) keeps every candidate as `unknown` - nothing chose among
      // them, which is not an ambiguity Angular would meet in a scope, so it is said apart.
      const scoped = rows.some((r) => r.scope !== 'unknown');
      diag.note(scoped ? 'selector_ambiguous_in_scope' : 'selector_ambiguous_without_scope', { node: rows[0].node, tag: rows[0].tag,
        candidates: [...targets.values()].map((r) => ({ id: r.to, name: r.to_name, scope: r.scope })) });
    }
  }
  return counts;
}

/** Where an `out_of_scope` candidate is published - beside `renders`, never in it. */
export const ASIDE = 'renders_out_of_scope';

/**
 * `renders` IS WHAT ANGULAR INSTANTIATES (#40): an `out_of_scope` candidate - a tag matched to a component the
 * host's scope cannot see - is moved to `renders_out_of_scope`, so a query over `renders` agrees with `render_graph`
 * and `render_path` (`closure.rs::load_edges` drops the same rows). Kept in `renders`, two modules each declaring
 * one `demo-box` gave each host an edge to both. `unknown` stays: nothing proves which candidate renders.
 *
 * LAST, after every pass that reads `renders.scope`, and IN PLACE: the rows keep their ids, and a row handed back
 * from this table is loaded into `renders` again (`TsMap.mjs`), so the scope is decided over every candidate on
 * every run. A row moves only when its `scope` says so, so an unchanged row lands where it was stored.
 */
export function setAsideOutOfScope(store) {
  const all = store.table('renders');
  const aside = store.table(ASIDE);
  let at = 0;
  for (const r of all) {
    if (r.scope === 'out_of_scope') aside.push(r);
    else all[at++] = r;
  }
  all.length = at;
  if (aside.length) store.note(ASIDE, 'rd');
  return aside.length;
}

/** The claims handed back, the set-aside table's folded into `renders`, where this run holds its rows. */
export function claimsBack(claims) {
  const out = { ...(claims ?? {}) };
  if (out[ASIDE]) out.renders = { ...(out.renders ?? {}), ...out[ASIDE] };
  delete out[ASIDE];
  return out;
}

/** A WHOLE `renders` takes its set-aside candidates with it: they are one table split by `scope`. */
export function withAside(whole) {
  return whole.includes('renders') && !whole.includes(ASIDE) ? [...whole, ASIDE].sort() : whole;
}
