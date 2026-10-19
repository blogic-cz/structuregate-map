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

/** A type node -> `{type_ref, type_refs?}`: the declaration(s) it names, `type_ref: null` when the checker names none. */
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

  /** The declaration ONE type names, or null: an anonymous type, a primitive, `null` name none. */
  function refOfType(type) {
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
  }

  /**
   * A UNION NAMES WHAT ITS MEMBERS NAME (#88). `code: Codes | string` is a union type with no symbol of its own, so
   * asking it alone named nothing and the parameter lost the in-tree type it accepts. Each member is asked instead;
   * one that is a member of an enum (`Kind | undefined` flattens to `Kind.A | Kind.B | undefined`) is asked as its
   * enum, so the enum is named once and not each member. The answers are deduplicated and sorted by place: the
   * checker interns a union's members in the order they are first asked for, which a full and a partial run do not
   * share. A union whose own alias names it (`type Id = Codes | string`) is that alias, as before.
   *
   * AN INTERSECTION NAMES WHAT ITS PARTS NAME TOO (#97): `Payload & { Extra?: Extra }` - what a generic's
   * `TPayload & {..}` instantiates to - has no symbol of its own, so it named nothing and every read past it stopped.
   * Its parts are asked as a union's are; an anonymous part names nothing, so `X & {..}` names `X`. A part that is
   * itself an unaliased union or intersection (`(A & {..}) | null`) is walked into - the parts are finite and the
   * checker builds no cycle of them.
   */
  function refsOfParts(type) {
    const out = new Map();
    const parts = [...type.types];
    while (parts.length) {
      const part = parts.pop();
      if (!part.aliasSymbol && !part.getSymbol() && (part.isUnion() || part.isIntersection())) {
        parts.push(...part.types);
        continue;
      }
      const whole = (part.flags & ts.TypeFlags.EnumLiteral) !== 0 ? checker.getBaseTypeOfLiteralType(part) : part;
      const ref = refOfType(whole);
      if (ref) out.set(`${ref.file}\u0000${ref.line}\u0000${ref.name}`, ref);
    }
    return [...out.values()].sort((a, b) => (a.file < b.file ? -1 : a.file > b.file ? 1 : a.line - b.line
      || (a.name < b.name ? -1 : a.name > b.name ? 1 : 0)));
  }

  /**
   * `{type_ref}` - the ONE declaration the node's type names (a union with one such member, `Codes | null`,
   * names it, an intersection with one named part too) - or `{type_ref: null, type_refs}` when a union or an intersection names SEVERAL: picking one of `A | B` would claim
   * the other is not accepted. `type_refs` is absent otherwise, so the two never repeat each other. `given` is a type the
   * caller already holds - a borrowed member's type INSTANTIATED for the alias that borrows it (#96) - asked instead.
   */
  return (node, given = null) => {
    let type = given;
    try {
      type ??= checker.getTypeAtLocation(node);
    } catch {
      return { type_ref: null };
    }
    if (!type) return { type_ref: null };
    if (type.aliasSymbol || type.getSymbol() || !(type.isUnion() || type.isIntersection())) return { type_ref: refOfType(type) };
    const refs = refsOfParts(type);
    return refs.length > 1 ? { type_ref: null, type_refs: refs } : { type_ref: refs[0] ?? null };
  };
}
