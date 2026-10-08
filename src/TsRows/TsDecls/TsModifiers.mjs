/**
 * WHAT A DECLARATION IS MARKED AS - and the one form of `export` that a modifier flag cannot see.
 *
 * Ported from the original Angular extractor. Imports are flat: see `TsTypeRef.mjs`.
 */

/**
 * THE NAMES A FILE EXPORTS IN A TRAILING `export { ... }` STATEMENT.
 *
 * `ModifierFlags.Export` sees only the keyword ON the declaration, and a codebase routinely writes the
 * other form - `class X { ... }` at the top of the file and `export { X };` at the bottom. In the tool this
 * is ported from, most of the classes the map called `exported: false` were in fact the file's public
 * surface, imported and routed elsewhere: the statement was recorded and the declaration was recorded, and
 * nothing connected them.
 *
 * A re-export (`export { X } from './other'`) names a declaration in ANOTHER file and is excluded - it says
 * nothing about the local one. For `export { A as B }` the LOCAL name is what a declaration answers to, so
 * `propertyName` wins over `name`: the alias is the outside world's word for it.
 */
export function makeModifiers(ts, log = null) {
  const cache = new Map();

  const localExports = (sf) => {
    // THE FILE IS READ BY ITS TEXT, whichever file asks - see `TsSetup/TsReads.mjs`.
    log?.touch(sf);
    const hit = cache.get(sf);
    if (hit) return hit;
    const names = new Set();
    for (const st of sf.statements) {
      if (!ts.isExportDeclaration(st) || st.moduleSpecifier) continue;
      const clause = st.exportClause;
      if (!clause || !ts.isNamedExports(clause)) continue;
      for (const e of clause.elements) names.add((e.propertyName ?? e.name).text);
    }
    cache.set(sf, names);
    return names;
  };

  const exportedByStatement = (node) => {
    if (!node.parent || !ts.isSourceFile(node.parent)) return false;
    const named = node.name;
    if (!named || !ts.isIdentifier(named)) return false;
    return localExports(node.getSourceFile()).has(named.text);
  };

  const modifiersOf = (node) => {
    const flags = ts.getCombinedModifierFlags(node);
    return {
      static: (flags & ts.ModifierFlags.Static) !== 0,
      readonly: (flags & ts.ModifierFlags.Readonly) !== 0,
      abstract: (flags & ts.ModifierFlags.Abstract) !== 0,
      exported: (flags & ts.ModifierFlags.Export) !== 0 || exportedByStatement(node),
      visibility: (flags & ts.ModifierFlags.Private) !== 0 ? 'private'
        : (flags & ts.ModifierFlags.Protected) !== 0 ? 'protected' : 'public',
    };
  };

  // BOTH, because a VARIABLE STATEMENT HAS NO NAME OF ITS OWN: `exportedByStatement` cannot answer for it,
  // the names are one level down on each declaration, and the caller asks the same question per declaration.
  return { modifiersOf, localExports };
}
