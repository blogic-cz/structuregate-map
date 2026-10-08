/**
 * WHERE A TYPE IS DECLARED - the second hop of every chain this map is asked to follow.
 *
 * Ported from the original Angular extractor. A member's type as TEXT (`"Cell[]"`) is where
 * a consumer's walk stops: it has the name and no way to reach the declaration, so `row.cells` inside
 * `rows` was unresolvable with the answer sitting right there as a string.
 *
 * THIS FILE IS IN A SUBFOLDER AND ITS IMPORTS ARE FLAT. The exe stages every embedded script into ONE
 * temporary folder, so `./TsPaths.mjs` is right here at run time even though the sources sit a level
 * apart. The folder exists because `src/TsRows/` reached this repo's own 15-file limit.
 */
import { canonicalPath } from './TsPaths.mjs';

/**
 * A declaration, anchored at its NAME.
 *
 * `node.getStart()` on a decorated class lands on `@Injectable({`, not on the class. Measured in the tool
 * this is ported from: about a quarter of the resolved type refs pointed at a decorator line, in an
 * application where nearly every injected service is decorated.
 */
function declSite(d) {
  const sf = d.getSourceFile();
  const pos = d.name && typeof d.name.getStart === 'function' ? d.name.getStart() : d.getStart();
  return { file: canonicalPath(sf.fileName), line: sf.getLineAndCharacterOfPosition(pos).line + 1 };
}

/** A type node -> the declaration it names, or null when the checker cannot name one. */
export function makeTypeResolver(ts, checker) {
  /**
   * `{unwrapped}` - the innermost declared type reached by following wrappers that take EXACTLY ONE type
   * argument: `Observable<Foo[]>` -> `Foo`, `Promise<Bar>` -> `Bar`, `Foo[]` -> `Foo`.
   *
   * Termination is by CYCLE DETECTION over the types already visited, not by a depth number: a generic
   * that instantiates itself is the only way this could loop, and a counter would silently stop short of
   * a real answer instead of naming the loop.
   */
  function unwrappedOf(type) {
    const seen = new Set();
    let current = type;
    let found = null;
    while (!seen.has(current)) {
      seen.add(current);
      const args = checker.getTypeArguments(current);
      const [only] = args.length === 1 ? args : [];
      if (!only) break;
      const symbol = only.aliasSymbol ?? only.getSymbol();
      const [where] = (symbol?.declarations ?? []).map(declSite);
      const name = symbol?.getName();
      if (symbol && where && name !== undefined && !name.startsWith('__')) found = { name, ...where };
      current = only;
    }
    return found ? { unwrapped: found } : null;
  }

  /**
   * `{element}` when the type is an array whose element type is itself declared somewhere.
   *
   * ONE LEVEL, deliberately. Descending further to reach through `Observable<Foo[]>` was measured and made
   * things WORSE - element refs fell - because a declared type that happens to take a type
   * argument is not automatically a container.
   */
  function elementOf(type) {
    const args = checker.getTypeArguments(type);
    const [only] = args.length === 1 ? args : [];
    if (!only) return null;
    const symbol = only.aliasSymbol ?? only.getSymbol();
    const [where] = (symbol?.declarations ?? []).map(declSite);
    const name = symbol?.getName();
    if (!symbol || !where || name === undefined || name.startsWith('__')) return null;
    return { element: { name, ...where } };
  }

  return (node) => {
    let type;
    try {
      type = checker.getTypeAtLocation(node);
    } catch {
      return null;
    }
    if (!type) return null;
    // AN ALIAS IS NAMED WHERE THE AUTHOR NAMED IT. `type Foo = {...}` used at a member reaches the
    // anonymous object type unless the alias symbol is asked for first, and the alias is what a reader is
    // looking for.
    const symbol = type.aliasSymbol ?? type.getSymbol();
    const decls = symbol?.declarations ?? [];
    if (!symbol || !decls.length) return null;
    const name = symbol.getName();
    // An anonymous type's symbol is called `__type` / `__object` by the compiler - a placeholder, not a
    // name a consumer can do anything with. Publishing it would claim a declaration that has no identity.
    if (name.startsWith('__')) return null;
    // The checker's order is kept - see the note in `TsMap`'s describer.
    const places = decls.map(declSite);
    const [first, ...rest] = places;
    return {
      name, ...first, ...(rest.length ? { also: rest } : {}),
      ...(elementOf(type) ?? {}),
      ...(unwrappedOf(type) ?? {}),
    };
  };
}
