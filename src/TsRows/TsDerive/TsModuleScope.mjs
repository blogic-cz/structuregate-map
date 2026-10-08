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

export function rollupModuleScope(store, diag) {
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

  const modulesForRef = (ref, ownerFile) => {
    const name = ref?.name ?? (typeof ref?.call === 'string' ? ref.call.split('.')[0] : null);
    if (!name) return [];
    if (ref?.file) {
      const fid = fileIdByAbs.get(slash(ref.file));
      if (fid !== undefined) {
        const exact = byFileName.get(`${String(fid)}#${name}`);
        if (exact) return [exact];
        // An ALIASED module import: the file is authoritative, and a file declaring exactly one NgModule
        // leaves nothing to guess.
        const inFile = modulesByFile.get(fid);
        if (inFile?.length === 1) return inFile;
      }
    }
    if (ownerFile != null && npmNamesByFile.get(ownerFile)?.has(name)) {
      diag.note('module_ref_from_dependency', { name, file: ownerFile });
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

  const emptyScope = () => ({ ids: new Set(), names: new Set() });
  const merge = (into, from) => {
    for (const i of from.ids) into.ids.add(i);
    for (const n of from.names) into.names.add(n);
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
    const scope = { declared, imported: importsOf(refsOf(m, 'imports'), m.file) };
    scopeCache.set(m.id, scope);
    return scope;
  };

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

  const fileAbsById = new Map(store.table('files').map((f) => [f.id, String(f.abs)]));
  // A STANDALONE component has no NgModule at all: its scope is its own `imports` array. Without this, every
  // render from a standalone component was `unknown` however the module lookup was keyed.
  const standaloneByClass = new Map();
  for (const c of store.table('components')) if (c.standalone === true) standaloneByClass.set(c.class, c);

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
    const fromClass = classById.get(r.from_class);
    const fromFile = fromClass ? fileAbsById.get(fromClass.file) : undefined;
    const mod = (fromFile ? declaringModuleByFile.get(fromFile) : undefined)
      ?? (typeof fromClass?.name === 'string' ? declaringModule.get(fromClass.name) : undefined);
    if (!mod) {
      const standalone = standaloneByClass.get(r.from_class);
      if (standalone) {
        // Its own `imports` array IS the scope - the rule the compiler applies to a standalone component.
        const hit = reach(importsOf(refsOf(standalone, 'imports'), standalone.file), r);
        const scopeName = hit === null ? 'out_of_scope' : 'imported';
        r.standalone = true;
        r.scope = scopeName;
        if (hit === 'name') r.scope_by_name = true;
        counts[scopeName] = (counts[scopeName] ?? 0) + 1;
        continue;
      }
    }
    const scope = scopeOf(mod);
    if (!scope || !mod) {
      r.scope = 'unknown';
      counts.unknown += 1;
      continue;
    }
    r.module = mod.id;
    const declaredHit = reach(scope.declared, r);
    const importedHit = declaredHit === null ? reach(scope.imported, r) : null;
    r.scope = declaredHit !== null ? 'declared' : importedHit !== null ? 'imported' : 'out_of_scope';
    if (declaredHit === 'name' || importedHit === 'name') r.scope_by_name = true;
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
      diag.note('selector_ambiguous_in_scope', { node: rows[0].node, tag: rows[0].tag,
        candidates: [...targets.values()].map((r) => ({ id: r.to, name: r.to_name, scope: r.scope })) });
    }
  }
  return counts;
}
