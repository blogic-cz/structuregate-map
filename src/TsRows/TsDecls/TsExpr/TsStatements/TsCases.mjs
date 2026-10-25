/**
 * WHAT A `switch` TESTS AND WHAT EACH LABEL SELECTS (rows 114, #127) - the facts `TsBody.mjs` writes on a `switch_cases` row, as
 * the C# half writes them (`src/Map/CsRows/CsBody/CsRowsCases.cs`), so "which switches over `Hue` handle no `Blue`" is one query
 * for both languages.
 *
 * `discriminant_type` is the switched ENUM's name when the checker types the discriminant by one enum (the enum itself, a member,
 * or a union of one enum's members; `null`/`undefined` aside), else the checker's own text of the type. `discriminant_enum` is that
 * enum's declaration `{name, file, line}` (`TsRefs.mjs::rollupRefIds` adds `discriminant_enum_id`, the `enums` row). Per label, in
 * the order of `labels`: `label_symbols` - `Enum.Member` for an enum member, else the name the label binds to, `''` for none - and
 * `label_values` - the constant it compares with (a member's value, a literal's), `''` when it has none. FROM THE CHECKER, never
 * the label's text: `case Kinds.Red` through an alias import or a re-export is still `Hue.Red`.
 *
 * Imports are flat: see `TsTypeRef.mjs`.
 */
export function makeCaseFacts(ts, checker, fileOf) {
  /** The enum declaration a symbol is, or a member of; null for any other symbol. */
  const enumDecl = (symbol) => {
    let sym = symbol;
    if (sym && (sym.flags & ts.SymbolFlags.Alias) !== 0) sym = checker.getAliasedSymbol(sym);
    if (!sym) return null;
    if ((sym.flags & ts.SymbolFlags.EnumMember) !== 0) {
      const parent = sym.declarations?.[0]?.parent;
      return parent && ts.isEnumDeclaration(parent) ? parent : null;
    }
    if ((sym.flags & ts.SymbolFlags.Enum) !== 0) return sym.declarations?.find((d) => ts.isEnumDeclaration(d)) ?? null;
    return null;
  };

  /** The one enum a type is, or a union of members of; null otherwise. */
  const enumOfType = (type) => {
    const own = enumDecl(type.aliasSymbol ?? type.symbol);
    if (own) return own;
    if (!type.isUnion()) return null;
    const parts = type.types.filter((t) => (t.flags & (ts.TypeFlags.Null | ts.TypeFlags.Undefined)) === 0);
    const found = new Set(parts.map((t) => enumDecl(t.symbol)));
    return found.size === 1 ? [...found][0] : null;
  };

  function discriminant(expr) {
    try {
      const type = checker.getTypeAtLocation(expr);
      const decl = enumOfType(type);
      if (!decl) return { discriminant_type: checker.typeToString(type) };
      const line = decl.getSourceFile().getLineAndCharacterOfPosition(decl.name.getStart()).line + 1;
      return { discriminant_type: decl.name.text, discriminant_enum: { name: decl.name.text, file: fileOf(decl), line } };
    } catch {
      return {};
    }
  }

  function label(expr) {
    try {
      let sym = checker.getSymbolAtLocation(ts.isPropertyAccessExpression(expr) ? expr.name : expr);
      if (sym && (sym.flags & ts.SymbolFlags.Alias) !== 0) sym = checker.getAliasedSymbol(sym);
      const owner = sym && (sym.flags & ts.SymbolFlags.EnumMember) !== 0 ? enumDecl(sym) : null;
      const symbol = owner ? `${owner.name.text}.${sym.getName()}` : sym ? sym.getName() : '';
      let value = owner && sym.valueDeclaration ? checker.getConstantValue(sym.valueDeclaration) : undefined;
      if (value === undefined) {
        const type = checker.getTypeAtLocation(expr);
        if (type.isStringLiteral() || type.isNumberLiteral()) value = type.value;
      }
      return [symbol, value === undefined ? '' : String(value)];
    } catch {
      return ['', ''];
    }
  }

  /** `label_symbols` and `label_values` of a case group's labels, in order. */
  function labels(exprs) {
    const facts = exprs.map(label);
    return { label_symbols: facts.map(([s]) => s), label_values: facts.map(([, v]) => v) };
  }

  return { discriminant, labels };
}
