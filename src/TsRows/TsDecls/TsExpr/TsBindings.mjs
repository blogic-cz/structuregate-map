/**
 * WHAT A NAME A DESTRUCTURING BINDS CARRIES BESIDES ITS PLACE (rows 77): `default`, the evaluated default of `{ a = 1 }`; `type`, the checker's text for the bound name, as a member
 * without an annotation has it; `index`, its position in an ARRAY pattern - holes (`[, x]`) bind nothing, yet count, so `x` is index 1.
 *
 * ONE reader for the three places a pattern binds (a `locals` row's `bindings`, a destructured `const`'s per-name `consts` row, a parameter's `bindings` item): a default or a type
 * read one way in one of them and another in the next is a fact two consumers join differently.
 *
 * Imports are flat: see `TsTypeRef.mjs`.
 */

export function makeBindingFacts(ts, checker, evalCell) {
  /** The type of the bound name as the checker prints it, as `typeText` prints an un-annotated property's; null when it cannot say. */
  const typeOf = (name) => {
    try {
      return checker.typeToString(checker.getTypeAtLocation(name), name, ts.TypeFormatFlags.NoTruncation);
    } catch {
      return null;
    }
  };

  /** `{type, default?}` of one binding element, and `index` for an element of an array pattern (its position among the pattern's elements, holes counted). */
  return (element, index = null) => ({
    ...(index === null ? {} : { index }),
    type: typeOf(element.name),
    ...(element.initializer ? { default: evalCell(element.initializer) } : {}),
  });
}
