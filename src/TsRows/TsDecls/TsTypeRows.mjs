/**
 * THE TYPES A FILE DECLARES: enums with their values, interfaces, aliases, and every member of them as its
 * own row.
 *
 * Ported from the original Angular extractor. Imports are flat: see `TsTypeRef.mjs`.
 *
 * The members inlined on a declaration row are a SUMMARY - no line, no id - and a member whose type is
 * itself an object literal contributed only its own name, so everything below the first level was absent.
 * A resolved read landing on such a property therefore pointed at a `file:line` with no row to join to:
 * the resolution was right and the last hop still had to be made by hand. Hence one row per member, at its
 * OWN line, with `path` carrying the dotted route and `parent` linking it to the member that contains it.
 */
import { locationOf } from './TsNodes.mjs';

export function makeTypeRows({ ts, checker, store, modifiersOf, typeRefAt, evalNode, propName, fileId }) {
  function enumRow(statement, fid) {
    const raw = ts.canHaveModifiers(statement) ? (ts.getModifiers(statement) ?? []) : [];
    store.add('enums', 'e', {
      file: fid,
      name: statement.name.text,
      exported: modifiersOf(statement).exported,
      const: raw.some((m) => m.kind === ts.SyntaxKind.ConstKeyword),
      members: statement.members.map((m) => {
        // THE VALUE THE COMPILER FOLDED, and only then the initializer itself. `getConstantValue` answers
        // for everything it can compute, including a member that inherits the previous one's number.
        const folded = checker.getConstantValue(m);
        return {
          name: propName(m.name),
          value: folded !== undefined ? folded : (m.initializer ? evalNode(m.initializer) : null),
        };
      }),
      ...locationOf(statement),
    });
  }

  function interfaceRow(statement, fid, exported) {
    const ifaceId = store.add('interfaces', 'i', {
      file: fid,
      name: statement.name.text,
      exported,
      extends: (statement.heritageClauses ?? []).flatMap((h) => h.types.map((t) => t.getText())),
      members: statement.members.map((m) => ({
        name: m.name ? propName(m.name) : (ts.SyntaxKind[m.kind] ?? 'unknown'),
        type: m.type ? m.type.getText() : null,
        optional: !!m.questionToken,
        kind: ts.SyntaxKind[m.kind] ?? 'unknown',
      })),
      ...locationOf(statement),
    });
    typeMemberRows(statement.members, ifaceId, fid, '', null);
    return ifaceId;
  }

  function typeAliasRow(statement, fid) {
    const aliasId = store.add('type_aliases', 'ta', {
      file: fid,
      name: statement.name.text,
      exported: modifiersOf(statement).exported,
      type: statement.type.getText(),
      ...locationOf(statement),
    });
    // An alias of an object type declares members exactly as an interface does; a consumer asking what a
    // property is should not have to care which of the two forms the author used.
    if (ts.isTypeLiteralNode(statement.type)) typeMemberRows(statement.type.members, aliasId, fid, '', null);
    // A COMPUTED TYPE HAS MEMBERS TOO. `Pick<X, 'a'|'b'>`, an intersection and every other mapped form
    // declare no syntax to walk, so an alias built that way contributed nothing and a read landing on it
    // stopped dead. The checker knows the resulting properties AND where each was originally declared.
    else typeMemberRowsFromType(statement.type, aliasId, fid);
    return aliasId;
  }

  function typeMemberRows(members, owner, fid, prefix, parent) {
    for (const m of members) {
      const name = m.name ? propName(m.name) : (ts.SyntaxKind[m.kind] ?? 'unknown');
      const path = prefix ? `${prefix}.${String(name)}` : String(name);
      const id = store.add('type_members', 'tm', {
        owner, file: fid, name, path, parent,
        type: m.type ? m.type.getText() : null,
        type_ref: m.type ? typeRefAt(m.type) : null,
        optional: !!m.questionToken,
        kind: ts.SyntaxKind[m.kind] ?? 'unknown',
        ...locationOf(m.name ?? m),
      });
      // An object type nested in a property, and the element type of an array of object types, are both
      // reachable structure - a consumer asking "what is under this property" gets the same answer either way.
      const t = m.type;
      if (t && ts.isTypeLiteralNode(t)) typeMemberRows(t.members, owner, fid, path, id);
      else if (t && ts.isArrayTypeNode(t) && ts.isTypeLiteralNode(t.elementType)) {
        typeMemberRows(t.elementType.members, owner, fid, path, id);
      }
    }
  }

  /**
   * Members of a type the checker COMPUTES - a mapped type, an intersection, a utility type. One level:
   * a computed type's properties are what the alias offers, and each property's own type is published as
   * `type_ref` exactly as everywhere else.
   */
  function typeMemberRowsFromType(node, owner, fid) {
    let props;
    try {
      const type = checker.getTypeAtLocation(node);
      // A PRIMITIVE HAS AN APPARENT TYPE, and asking it for properties answers with the built-in's members:
      // `type Kind = 'a' | 'b'` reported `length`, `charAt` and the rest of `String` as the alias's own
      // members, anchored in lib.es5.d.ts. Those are true of the VALUE and say nothing about the alias.
      const primitive = ts.TypeFlags.StringLike | ts.TypeFlags.NumberLike | ts.TypeFlags.BooleanLike
        | ts.TypeFlags.BigIntLike | ts.TypeFlags.ESSymbolLike;
      // EVERY CONSTITUENT, not the union itself: a union carries `TypeFlags.Union` and none of the
      // primitive flags, and `getPropertiesOfType` on a union returns what its members have in COMMON.
      const parts = type.isUnion() ? type.types : [type];
      if (parts.some((t) => (t.flags & primitive) !== 0)) return;
      // AN ARRAY IS NOT AN OBJECT SURFACE EITHER. `type Rows = Row[]` answered with `length`, `push` and the
      // rest of `Array`. Detected STRUCTURALLY, by the number index signature every array and tuple has, so
      // no type is recognised by name.
      if (parts.some((t) => checker.getIndexInfoOfType(t, ts.IndexKind.Number) !== undefined)) return;
      props = checker.getPropertiesOfType(type);
    } catch {
      return;
    }
    for (const prop of props ?? []) {
      const decl = prop.declarations?.find((d) => ts.isPropertySignature(d) || ts.isPropertyDeclaration(d));
      if (!decl) continue;
      store.add('type_members', 'tm', {
        owner, file: fileId(decl.getSourceFile().fileName), name: prop.getName(),
        path: prop.getName(), parent: null,
        type: decl.type ? decl.type.getText() : null,
        optional: !!decl.questionToken,
        kind: ts.SyntaxKind[decl.kind] ?? 'unknown',
        // WHERE THE PROPERTY REALLY LIVES. A computed type borrows its members, so the row is anchored at
        // the borrowed declaration and says so: `via: 'computed'` keeps the two kinds of row apart.
        via: 'computed',
        type_ref: decl.type ? typeRefAt(decl.type) : null,
        ...locationOf(decl),
      });
    }
  }

  return { enumRow, interfaceRow, typeAliasRow, typeMemberRows, typeMemberRowsFromType };
}
