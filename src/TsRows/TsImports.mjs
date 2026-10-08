/**
 * WHAT A FILE IMPORTS, and WHICH FILE that specifier actually resolved to.
 *
 * Ported from the import half of the original Angular extractor. The specifier alone is a
 * string: `@acme/core` says nothing about which file it is, and a consumer asking "who uses this class"
 * would have to re-implement module resolution to find out. The answer here is the COMPILER's - the
 * symbol the module specifier resolves to, and the file its declaration sits in.
 *
 * A MODULE SYMBOL CAN RESOLVE TO MORE THAN ONE FILE - a declaration file beside its implementation, an
 * ambient augmentation of a package. The first stays `resolved`; the rest are PUBLISHED as `resolved_also`
 * rather than discarded, because "where is this declared" answered with one of several places, and no sign
 * that others exist, reads as a certainty the checker never expressed.
 */
import { canonicalPath, relativeTo } from './TsPaths.mjs';
import { locationOf } from './TsNodes.mjs';
import { UNRECORDED } from './TsReads.mjs';

/**
 * THE NAMES, PARSED - not the statement's text.
 *
 * `default`, `named` (with the `as` a rename came through) and `namespace` are three different facts, and
 * a consumer asking which symbol a file takes from a module should not have to re-parse the clause.
 */
function namesOf(ts, statement) {
  const names = [];
  const clause = statement.importClause;
  const bindings = clause?.namedBindings;
  if (clause?.name) names.push({ name: clause.name.text, kind: 'default' });
  if (bindings && ts.isNamedImports(bindings)) {
    for (const element of bindings.elements) {
      names.push({ name: element.name.text, as: element.propertyName?.text ?? null, kind: 'named' });
    }
  }
  if (bindings && ts.isNamespaceImport(bindings)) names.push({ name: bindings.name.text, kind: 'namespace' });
  return names;
}

/** The file a module symbol stands for, or null for one with no source file (an ambient `declare module`). */
function fileOfModule(ts, moduleSym) {
  const decl = (moduleSym?.declarations ?? []).find((d) => ts.isSourceFile(d)) ?? moduleSym?.declarations?.[0];
  return decl ? decl.getSourceFile() : null;
}

/**
 * WHERE `name` IS REALLY DECLARED, reached from the module it was imported from - and every barrel on the way.
 *
 * An import resolves to the module it NAMES, and in a real frontend that is a barrel (`src/utils/index.ts`) a
 * third of the time: that many named in-tree specifiers are declared somewhere else. The checker's
 * `getAliasedSymbol` jumps straight to the declaration and loses the barrels in between, and a star
 * re-export is no alias at all - yet a partial run must know every one of those barrels, since re-pointing
 * any of them moves the declaration. So the chain is walked statement by statement, the compiler asked only
 * what a module specifier resolves to:
 *   `export * from './x'`            - the star whose module exports `name`
 *   `export { name } from './x'`      - into './x', under the name it had there
 *   `import { name } ...; export { name }` - into the module the import names
 */
function trace(ts, checker, moduleSym, name, via, depth = 0) {
  const file = fileOfModule(ts, moduleSym);
  if (!file || depth > 16) return null;
  const exported = checker.getExportsOfModule(moduleSym).find((s) => s.escapedName === name);
  if (!exported) return null;
  const into = (specifier, inner) => {
    const target = checker.getSymbolAtLocation(specifier);
    if (!target) return null;
    via.push(file);
    return trace(ts, checker, target, inner, via, depth + 1);
  };
  const decl = exported.declarations?.[0];
  if ((exported.flags & ts.SymbolFlags.Alias) === 0) {
    if (decl && decl.getSourceFile() === file) return { file, name };
    // A SEARCH, and its misses read nothing: which star module holds the name is the barrel's own export list,
    // which its surface covers (`TsSetup/TsReads.mjs`). The one it lands on is resolved again by `into`, and
    // that is what the importer read.
    const search = checker[UNRECORDED] ?? checker;
    for (const st of file.statements) {
      if (!ts.isExportDeclaration(st) || st.exportClause || !st.moduleSpecifier) continue;
      const target = search.getSymbolAtLocation(st.moduleSpecifier);
      if (target && search.getExportsOfModule(target).some((s) => s.escapedName === name)) return into(st.moduleSpecifier, name);
    }
    return decl ? { file: decl.getSourceFile(), name } : null;
  }
  if (decl && ts.isExportSpecifier(decl)) {
    const original = (decl.propertyName ?? decl.name).text;
    const statement = decl.parent.parent;
    if (statement.moduleSpecifier) return into(statement.moduleSpecifier, original);
    const local = checker.getExportSpecifierLocalTargetSymbol(decl);
    const imported = local?.declarations?.find((d) => ts.isImportSpecifier(d) || ts.isImportClause(d));
    if (imported) {
      const importDecl = ts.isImportClause(imported) ? imported.parent : imported.parent.parent.parent;
      const inner = ts.isImportClause(imported) ? 'default' : (imported.propertyName ?? imported.name).text;
      return into(importDecl.moduleSpecifier, inner);
    }
    const own = local?.declarations?.[0];
    return own ? { file: own.getSourceFile(), name: original } : null;
  }
  // `export =`, a namespace re-export: the checker's own answer, with no barrel to record.
  const final = checker.getAliasedSymbol(exported)?.declarations?.[0];
  return final ? { file: final.getSourceFile(), name } : null;
}

/**
 * ONE ROW PER NAME AN IMPORT BINDS, saying it in the SOURCE's order - `local` is what this file calls it,
 * `imported` what the module exports - and where that name is DECLARED. `imports.names` keeps its own
 * shape: its `as` is the name a rename came THROUGH, the old tool's contract, which consumers already read.
 */
function nameRows(ts, store, checker, statement, importId, fileId, feRoot) {
  const moduleSym = checker.getSymbolAtLocation(statement.moduleSpecifier);
  const clause = statement.importClause;
  const whole = !!clause?.isTypeOnly;
  const bound = [];
  if (clause?.name) bound.push({ node: clause.name, local: clause.name.text, imported: 'default', kind: 'default', typeOnly: whole });
  const bindings = clause?.namedBindings;
  if (bindings && ts.isNamedImports(bindings)) {
    for (const el of bindings.elements) {
      bound.push({ node: el, local: el.name.text, imported: (el.propertyName ?? el.name).text, kind: 'named', typeOnly: whole || el.isTypeOnly });
    }
  }
  if (bindings && ts.isNamespaceImport(bindings)) {
    bound.push({ node: bindings, local: bindings.name.text, imported: '*', kind: 'namespace', typeOnly: whole });
  }
  for (const b of bound) {
    const via = [];
    const found = b.kind === 'namespace' || !moduleSym ? null : trace(ts, checker, moduleSym, b.imported, via);
    const path = (sf) => relativeTo(feRoot, canonicalPath(sf.fileName));
    store.add('import_names', 'ib', {
      file: fileId,
      import: importId,
      local: b.local,
      imported: b.imported,
      kind: b.kind,
      type_only: b.typeOnly,
      declared: found ? path(found.file) : null,
      declared_name: found ? found.name : null,
      via: [...new Set(via.map(path))].filter((p) => !found || p !== path(found.file)),
      ...locationOf(b.node),
    });
  }
}

export function importRow(ts, store, statement, fileId, symbolAt, feRoot, checker) {
  const spec = statement.moduleSpecifier.text;
  const declarations = [...new Set((symbolAt(statement.moduleSpecifier)?.declarations ?? [])
    .map((d) => canonicalPath(d.getSourceFile().fileName)))];
  const resolved = declarations[0] ?? null;
  const also = declarations.slice(1);
  const importId = store.add('imports', 'im', {
    file: fileId,
    module: spec,
    // A RELATIVE SPECIFIER IS THIS REPOSITORY'S OWN. Everything else is a package - including a tsconfig
    // path alias, which is exactly how a monorepo's libraries are imported, so `external` is about the
    // SPELLING and `resolved` is about the file.
    external: !spec.startsWith('.'),
    resolved: resolved ? relativeTo(feRoot, resolved) : null,
    ...(also.length ? { resolved_also: also.map((p) => relativeTo(feRoot, p)) } : {}),
    names: namesOf(ts, statement),
    type_only: !!statement.importClause?.isTypeOnly,
    ...locationOf(statement),
  });
  if (checker) nameRows(ts, store, checker, statement, importId, fileId, feRoot);
}
