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

/** Whether a declaration is a type literal (`{ a: string }` in a type) or an object literal (`{ a: 1 }` in code): the two anonymous types a ref can reach. */
export function isShapeNode(ts, d) {
  return !!d && (ts.isTypeLiteralNode(d) || ts.isObjectLiteralExpression(d));
}

/** Where a literal's `{` is, in the spelling every ref uses (absolute canonical file) - the key a `shapes` row and a shape ref share. */
export function shapeSite(d) {
  const sf = d.getSourceFile();
  const at = sf.getLineAndCharacterOfPosition(d.getStart());
  return { file: canonicalPath(sf.fileName), line: at.line + 1, col: at.character + 1 };
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
      const site = holdOf(only);
      if (site) found = site;
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
    const site = holdOf(only);
    return site ? { element: site } : null;
  }

  /**
   * WHAT AN ELEMENT OR AN UNWRAPPED TYPE IS, as a ref head: its declaration's site, or - when it is a TYPE PARAMETER (`visible: T[]`) - the whole
   * `param` ref with the constraint and default it carries (rows 72). A bare site named a parameter as a class of that name, which nothing
   * declares, so a loop over `T[]` typed its variable with nothing and the constraint (`T extends Row`) was never reached. An element that is
   * an unnamed intersection or union is the ONE ref its parts name, or the list of them (below).
   */
  function holdOf(type) {
    const symbol = type.aliasSymbol ?? type.getSymbol();
    const isParam = (symbol?.declarations ?? []).some((d) => ts.isTypeParameterDeclaration(d));
    if (isParam) return refOfType(type, new Set());
    // AN ELEMENT THAT IS AN UNNAMED INTERSECTION OR UNION (`(Row & { open: boolean })[]`, `(Row | null)[]`) names what its parts name, as a member's
    // type does (`refsOfParts`); the symbol it has none of named nothing, so a loop over such a list typed its variable with nothing.
    if (!symbol && (type.isUnion() || type.isIntersection())) {
      const { refs, and } = refsOfParts(type);
      // SEVERAL (`Observable<A & B>`, `(A | B)[]`) are the list a member's `type_refs` is: a bag of refs, which a read takes as the one owner they make.
      return refs.length === 1 ? refs[0] : refs.length > 1 ? { type_refs: refs, type_refs_of: and ? 'and' : 'or' } : null;
    }
    return siteOf(symbol);
  }

  /**
   * THE ELEMENT A STRING OR NUMBER INDEX SIGNATURE HOLDS (rows 71), as a ref, or null: `items: { [k: string]: Item }`, `interface Bag { [k: string]: Item }`
   * and `Record<string, Item>` read `items[key]` as an `Item`. An array's number index is its `element`, already published, so a type with
   * one has no `index`; a type with neither names nothing.
   */
  function indexOf(type, seen) {
    if (!ts.IndexKind) return null;
    for (const kind of [ts.IndexKind.String, ts.IndexKind.Number]) {
      let held;
      try { held = checker.getIndexTypeOfType(type, kind); } catch { held = null; }
      const ref = held ? refOfType(held, seen, null, true) : null;
      if (ref) return { index: ref };
    }
    return null;
  }

  /**
   * WHAT A SYMBOL IS DECLARED AS, as a ref's head: `{name, file, line}`, or - the compiler's `__type` of a type literal and `__object` of an
   * object literal, which name nothing - `{shape: true, file, line, col}`, the place of the `{` that a `shapes` row (`TsExpr/TsShapes.mjs`)
   * is keyed by. Any other anonymous type (a function type, a mapped type, a class expression) names nothing, as before.
   */
  function siteOf(symbol) {
    const decls = symbol?.declarations ?? [];
    const name = symbol?.getName();
    if (!decls.length || name === undefined) return null;
    if (!name.startsWith('__')) return { name, ...declSite(decls[0]) };
    return isShapeNode(ts, decls[0]) ? { shape: true, ...shapeSite(decls[0]) } : null;
  }

  /** The declaration ONE type names, or null: a primitive, `null` and an anonymous type that is no literal name none; a type or object
   *  literal is a SHAPE ref (`siteOf`). `seen` guards the walk into type arguments and index signatures against a type that is its own argument. */
  /**
   * THE PROPERTIES OF A MAPPED TYPE (rows 72): `Pick<Dto, 'a'>`, `Omit<Dto, 'b'>` and `Partial<Dto>` are no declaration - the checker's symbol is the
   * library's alias or an anonymous `__type` - yet each property they hold is declared where the original is. `props` lists `{name, file, line}` of
   * that declaration, so a read past the type reaches the member, and only the members the mapping kept. Null for any other type, and for a mapped
   * type whose properties no declaration backs (`Record<'a' | 'b', X>`).
   */
  function propsOf(type) {
    if (!(type.flags & ts.TypeFlags.Object) || !((type.objectFlags ?? 0) & ts.ObjectFlags.Mapped)) return null;
    let list;
    try { list = checker.getPropertiesOfType(type); } catch { return null; }
    const props = [];
    for (const p of list) {
      const d = p.declarations?.[0];
      if (d) props.push({ name: p.getName(), ...declSite(d) });
    }
    return props.length ? { props } : null;
  }

  function refOfType(type, seen = new Set(), as = null, noIndex = false) {
    // AN ALIAS IS NAMED WHERE THE AUTHOR NAMED IT. `type Foo = {...}` used at a member reaches the
    // anonymous object type unless the alias symbol is asked for first, and the alias is what a reader is
    // looking for.
    const symbol = as ?? type.aliasSymbol ?? type.getSymbol();
    const decls = symbol?.declarations ?? [];
    if (!symbol || !decls.length) return propsOf(type);
    // An anonymous type's symbol is called `__type` / `__object` by the compiler - a placeholder, not a name. A literal is reached by
    // WHERE its `{` is (rows 71); any other anonymous type would claim a declaration that has no identity.
    const head = siteOf(symbol);
    if (head === null) return propsOf(type);
    // The checker's order is kept - see the note in `TsMap`'s describer.
    const rest = head.shape ? [] : decls.slice(1).map(declSite);
    // A TYPE PARAMETER IS NAMED AS ONE (#98), and a generic reference keeps its ARGUMENTS in order, each the declaration it
    // names or null: a template read past `Wrapped: Wrapper<T>` on `Concrete extends Base<Payload>` binds `T` to `Payload`
    // with them (`TsTplReads.mjs`), where the declaration's own `T` named nothing.
    const param = decls.some((d) => ts.isTypeParameterDeclaration(d));
    const fresh = !seen.has(type);
    let args = [];
    if (!param && !as && fresh) {
      seen.add(type);
      // THE ARGUMENTS OF THE NAME THE REF HEADS (rows 72): an alias (`type Tip<T> = { data: T }`) keeps them as `aliasTypeArguments`, and a type literal it
      // instantiates is no type reference, so `getTypeArguments` threw and `Tip<{..}>` named its argument nowhere - `T` stayed unbound.
      const own = type.aliasSymbol ? type.aliasTypeArguments : undefined;
      try { args = (own ?? checker.getTypeArguments(type)).map((a) => refOfType(a, seen)); } catch { args = []; }
    }
    // A PARAMETER'S BOUNDS (rows 72): what it is constrained to (`T extends Row`) and what it is when nothing is passed (`T = Dto`). A read past a
    // member typed `T` binds it from the arguments; with none it takes the default of an instantiation that passed nothing, else the constraint.
    const bounds = param ? boundsOf(decls.find((d) => ts.isTypeParameterDeclaration(d))) : {};
    const element = elementOf(type) ?? {};
    return {
      ...head, ...(rest.length ? { also: rest } : {}),
      ...(param ? { param: true, ...bounds } : {}),
      ...(args.some((a) => a !== null) ? { args } : {}),
      ...element,
      ...(propsOf(type) ?? {}),
      ...(unwrappedOf(type) ?? {}),
      ...(noIndex || param || as || !fresh || element.element ? {} : indexOf(type, seen) ?? {}),
    };
  }

  /** The declarations a type parameter is bounded by, `{constraint?, default?}`, each a ref. `open` holds the parameters being asked, so `T extends Node<T>`
   *  stops where it began - a parameter met again inside its own bound has none. */
  const open = new Set();
  function boundsOf(decl) {
    if (!decl || open.has(decl)) return {};
    open.add(decl);
    try {
      // A CONSTRAINT THAT IS A UNION (`T extends A | B`) is the list of what its members name, as a member's type is.
      const one = (node) => {
        const answer = node ? resolve(node) : null;
        return answer?.type_ref ?? (answer?.type_refs ? { type_refs: answer.type_refs, type_refs_of: answer.type_refs_of } : null);
      };
      const constraint = one(decl.constraint);
      const fallback = one(decl.default);
      return { ...(constraint ? { constraint } : {}), ...(fallback ? { default: fallback } : {}) };
    } finally { open.delete(decl); }
  }

  /**
   * THE ENUM A MEMBER BELONGS TO, or null for any other symbol. A single-member enum's type IS its member's literal type
   * (`enum Kind { Alpha }`: `kind: Kind` is `Kind.Alpha`), so the checker's symbol names the MEMBER and the row said `Alpha`
   * where the author wrote `Kind`.
   */
  function enumOfMember(symbol) {
    if (!symbol || (symbol.flags & ts.SymbolFlags.EnumMember) === 0) return null;
    const parent = symbol.declarations?.[0]?.parent;
    return (parent && ts.isEnumDeclaration(parent) ? checker.getSymbolAtLocation(parent.name) : null) ?? null;
  }

  /**
   * Whether the type node AS WRITTEN names an enum itself (`Kind`), never one of its members (`Kind.Alpha`). `Kind | null` and `(Kind)`
   * name it too: with strictNullChecks OFF the checker drops `null` from the union and answers the enum's lone member, so the
   * written node is looked through - parentheses, and the parts of a union that are not `null`/`undefined`, exactly one of which must be it.
   */
  function writtenNamesEnum(node) {
    if (!node) return false;
    if (ts.isParenthesizedTypeNode(node)) return writtenNamesEnum(node.type);
    if (ts.isUnionTypeNode(node)) {
      const nullish = (t) => t.kind === ts.SyntaxKind.LiteralType && t.literal.kind === ts.SyntaxKind.NullKeyword
        || t.kind === ts.SyntaxKind.UndefinedKeyword;
      const rest = node.types.filter((t) => !nullish(t));
      return rest.length === 1 && writtenNamesEnum(rest[0]);
    }
    if (!ts.isTypeReferenceNode(node)) return false;
    let sym = checker.getSymbolAtLocation(ts.isQualifiedName(node.typeName) ? node.typeName.right : node.typeName);
    if (sym && (sym.flags & ts.SymbolFlags.Alias)) sym = checker.getAliasedSymbol(sym);
    return !!sym && (sym.flags & ts.SymbolFlags.Enum) !== 0;
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
    // IN WRITTEN ORDER: an intersection's first declaration of a name is its first part's (`refsOfParts`'s `and`), so the parts are taken from the
    // front and a nested one is put back in its place - a stack read from the back reversed them.
    const parts = [...type.types];
    while (parts.length) {
      const part = parts.shift();
      if (!part.aliasSymbol && !part.getSymbol() && (part.isUnion() || part.isIntersection())) {
        parts.unshift(...part.types);
        continue;
      }
      const whole = (part.flags & ts.TypeFlags.EnumLiteral) !== 0 ? checker.getBaseTypeOfLiteralType(part) : part;
      const ref = refOfType(whole, new Set(), enumOfMember(whole.getSymbol()));
      if (ref) out.set(ref.props ? `props${ref.props.map((q) => `${q.name}@${q.file}:${q.line}`).join(',')}` : `${ref.file}\u0000${ref.line}\u0000${ref.name ?? ref.col}`, ref);
    }
    // A LITERAL PART NAMES A DECLARATION ONLY WHEN NOTHING ELSE DOES (rows 71): `X & { .. }` and `X | { .. }` are `X`, as they were, and only
    // parts that are all literals (`{ a } | { b }`) are listed as shapes.
    const all = [...out.values()];
    const named = all.filter((r) => !r.shape);
    const order = (a, b) => (a < b ? -1 : a > b ? 1 : 0);
    // A UNION'S refs are sorted by place (the checker's order is not a full and a partial run's); an INTERSECTION'S stay as written, since the part
    // written first answers a name two parts declare (`and`, rows 72).
    const and = type.isIntersection();
    // AN INTERSECTION THAT NAMES SEVERAL (`A & B & { .. }`) LISTS ITS LITERALS TOO, in place: each is a part that declares names, and none is `X` to hang from.
    const base = named.length && !(and && named.length > 1) ? named : all;
    const sorted = and ? base : base.sort((a, b) => order(a.file, b.file) || a.line - b.line
      || order(a.name ?? '', b.name ?? '') || (a.col ?? 0) - (b.col ?? 0));
    // THE LITERALS AN INTERSECTION DROPS ARE ITS MEMBERS TOO (rows 72): `X & { Extra?: E }` is `X`, and `Extra` is declared by the literal alone -
    // the ref lists them as `shapes`, which a template read walks beside `X` (`TsTplOwners.mjs::resolveRef`), each `before` when written ahead of `X`.
    // A union's literal is not listed: its members are no member of the union.
    const at = named.length === 1 ? all.indexOf(named[0]) : -1;
    const dropped = and && at >= 0 ? all.map((r, i) => (r.shape ? { ...r, ...(i < at ? { before: true } : {}) } : null)).filter((r) => r !== null) : [];
    return { refs: dropped.length ? [{ ...sorted[0], shapes: dropped }] : sorted, and };
  }

  /**
   * THE TYPE ALIAS A TYPE NODE WRITES (rows 71), `{name, file, line}`, or null: `type Desc = Readonly<{..}>` as `kind: Desc` names `Desc`, though
   * the checker's type is `Readonly<..>` (an alias to a generic instantiation carries the OUTER alias) and `type_ref` follows it there. The
   * written name's symbol is asked, an import followed, so the alias the author wrote is reachable whatever `type_ref` reached. A class, an
   * interface or an enum written directly is no alias and needs none.
   */
  function aliasWritten(node) {
    if (!node || !ts.isTypeReferenceNode(node)) return null;
    let sym = checker.getSymbolAtLocation(ts.isQualifiedName(node.typeName) ? node.typeName.right : node.typeName);
    if (sym && (sym.flags & ts.SymbolFlags.Alias)) sym = checker.getAliasedSymbol(sym);
    const decl = (sym?.declarations ?? []).find((d) => ts.isTypeAliasDeclaration(d));
    return decl ? { name: sym.getName(), ...declSite(decl) } : null;
  }

  /**
   * `{type_ref}` - the ONE declaration the node's type names (a union with one such member, `Codes | null`,
   * names it, an intersection with one named part too) - or `{type_ref: null, type_refs}` when a union or an intersection names SEVERAL: picking one of `A | B` would claim
   * the other is not accepted. `type_refs` is absent otherwise, so the two never repeat each other. `given` is a type the
   * caller already holds - a borrowed member's type INSTANTIATED for the alias that borrows it (#96) - asked instead.
   */
  const resolve = (node, given = null) => {
    let type = given;
    try {
      type ??= checker.getTypeAtLocation(node);
    } catch {
      return { type_ref: null };
    }
    if (!type) return { type_ref: null };
    // `this['items'][0]['name']` and every other type the checker keeps unresolved names what it is constrained to: the property's own type.
    if ((type.flags & ts.TypeFlags.IndexedAccess) !== 0) type = checker.getBaseConstraintOfType(type) ?? type;
    if (type.aliasSymbol || type.getSymbol() || !(type.isUnion() || type.isIntersection())) {
      // WRITTEN AS THE ENUM, NAMED AS THE ENUM: `Kind.Alpha` as written still names the member.
      const owner = type.aliasSymbol ? null : enumOfMember(type.getSymbol());
      const ref = refOfType(type, new Set(), owner && writtenNamesEnum(node) ? owner : null);
      const alias = ref ? aliasWritten(node) : null;
      return { type_ref: alias ? { ...ref, alias } : ref };
    }
    const { refs, and } = refsOfParts(type);
    // `type_refs_of` says how the listed types combine: `and` for an intersection (a name is the first part's that declares it), `or` for a union.
    return refs.length > 1 ? { type_ref: null, type_refs: refs, type_refs_of: and ? 'and' : 'or' } : { type_ref: refs[0] ?? null };
  };
  return resolve;
}
