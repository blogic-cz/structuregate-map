/**
 * WHERE A READ IN A TYPESCRIPT EXPRESSION IS DECLARED - the `target` every `Read` node carries.
 *
 * Moved out of `TsMap.mjs`, which is held in the structure baseline and may only shrink. Imports are flat:
 * see `TsTypeRef.mjs`.
 */

export function makeReadRefs(ts, checker, canonicalPath) {
  const at = (d) => {
    const sf = d.getSourceFile();
    return {
      file: canonicalPath(sf.fileName),
      line: sf.getLineAndCharacterOfPosition(d.getStart()).line + 1,
    };
  };

  /**
   * THE PROPERTY A READ LANDS ON, resolved to where it is DECLARED. A path published as a name alone
   * leaves the consumer matching it against candidate interfaces by hand. Declarations inside
   * dependencies are kept too - a library's own member is a fact, and dropping externals would make
   * "unresolved" mean two different things at once. EVERY declaration, not the first: a merged
   * interface or an overload set gives the same symbol several.
   */
  function resolveRead(node) {
    const decls = checker.getSymbolAtLocation(node)?.declarations ?? [];
    // THE CHECKER'S ORDER IS KEPT, AND THAT IS NOT AN OVERSIGHT. Its declaration order is not a fact
    // about the tree - a full run and a partial one disagree about a few `expressions`,
    // `assignments` and `functions` on nothing else - and sorting it makes the two agree. It was
    // tried, and a comparison refused it: sorting the declarations moved many rows away from
    // what the tool this map reproduces publishes, many of them in `members`. The reproduction is the
    // contract, so the instability stays and the partial-run comparison records it.
    const [first, ...rest] = decls.map(at);
    if (first === undefined) return null;
    return { name: node.getText(), ...first, ...(rest.length ? { also: rest } : {}) };
  }

  /** A declaration a module makes: a top-level `const`, a function, an enum. Never a local or a parameter. */
  const moduleLevel = (d) => {
    if (ts.isFunctionDeclaration(d) || ts.isEnumDeclaration(d)) return ts.isSourceFile(d.parent);
    if (!ts.isVariableDeclaration(d)) return false;
    const statement = d.parent?.parent;
    return statement !== undefined && ts.isVariableStatement(statement) && ts.isSourceFile(statement.parent);
  };

  /**
   * A BARE NAME, resolved only when it names what a module declares.
   *
   * An imported name is an ALIAS, and its own declaration is the import line in the file that reads it -
   * which is where nothing is declared. The aliased symbol is where the value lives.
   */
  function resolveName(node) {
    let symbol = checker.getSymbolAtLocation(node);
    if (symbol !== undefined && (symbol.flags & ts.SymbolFlags.Alias) !== 0) {
      symbol = checker.getAliasedSymbol(symbol);
    }
    const decls = (symbol?.declarations ?? []).filter(moduleLevel);
    const [first, ...rest] = decls.map(at);
    if (first === undefined) return null;
    return { name: node.text, ...first, ...(rest.length ? { also: rest } : {}) };
  }

  return { resolveRead, resolveName };
}
