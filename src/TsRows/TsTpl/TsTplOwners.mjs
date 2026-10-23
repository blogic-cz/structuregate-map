/**
 * WHERE A TYPE_REF LANDS and what a member of it is (split out of `TsTplReads.mjs`, rows 72): the owner a ref names (a class, an interface, an alias,
 * an enum, an anonymous shape), the member a name answers on it - its bases walked, a generic bound as it is instantiated - and the owner that
 * member's own type names. The walk over template expressions that asks these questions is `TsTplReads.mjs`.
 *
 * Imports are flat: see `TsDecls/TsTypeRef.mjs`.
 */

const str = (v) => (typeof v === 'string' ? v : null);

export function makeOwners(store) {
  const files = store.table('files');
  const pathById = new Map(files.map((f) => [f.id, f.path]));
  const idByPath = new Map();
  for (const f of files) {
    idByPath.set(f.path, f.id);
    idByPath.set(f.abs, f.id);
  }

  // Where a `type_ref` lands: the interface, alias or class that declares that type, by file + name.
  const ownerByFileName = new Map();
  for (const row of store.table('interfaces')) {
    ownerByFileName.set(`${String(row.file)}#${String(row.name)}`, { kind: 'type', id: row.id });
  }
  for (const row of store.table('type_aliases')) {
    ownerByFileName.set(`${String(row.file)}#${String(row.name)}`, { kind: 'type', id: row.id });
  }
  for (const row of store.table('classes')) {
    ownerByFileName.set(`${String(row.file)}#${String(row.name)}`, { kind: 'class', id: row.id });
  }
  // AN ENUM IS AN OWNER TOO: a component holding one (`Kinds = Kind`, typed `typeof Kind`) is read `Kinds.Alpha`, and the
  // chain stopped at the field - the largest share of every stop on a real workspace. Its members answer by name.
  for (const row of store.table('enums')) {
    if (row.half !== 'typescript') continue;
    ownerByFileName.set(`${String(row.file)}#${String(row.name)}`, { kind: 'enum', id: row.id });
  }
  // AN ANONYMOUS OBJECT TYPE IS AN OWNER TOO (rows 71): a type literal or object literal is a `shapes` row, reached by the file, line and
  // column of its `{` - what a shape ref (`{shape: true, file, line, col}`) carries - and answers its members by name like any owner.
  const shapeByPlace = new Map();
  for (const row of store.table('shapes')) shapeByPlace.set(`${String(row.file)}#${String(row.line)}#${String(row.col)}`, { kind: 'shape', id: row.id });
  // THE PROPERTIES OF A MAPPED TYPE (`Pick<Dto, 'a'>`, rows 72) are an owner of their own: each name answers with the member row declared where the
  // mapping took it from (`rowByPlace`), so only the kept names resolve. One owner per ref, as a ref is one list.
  const propsOwners = new Map();
  const rowByPlace = new Map();
  const ownerOf = (ref) => {
    // A MAPPED TYPE A DECLARED ALIAS NAMES (`type T = Pick<Slot<unknown>, 'Result'>`, rows 73) is that alias: its own computed members carry the types the checker
    // instantiated - `Result: Base` - where `props` hold the source's DECLARED rows, a type parameter with no binding. `props` answer an anonymous mapping only.
    const declared = Array.isArray(ref?.props) && str(ref?.file) !== null && str(ref?.name) !== null && idByPath.has(ref.file)
      ? ownerByFileName.get(`${String(idByPath.get(ref.file))}#${String(ref.name)}`) ?? null : null;
    if (declared !== null && declared.kind === 'type') return declared;
    if (Array.isArray(ref?.props)) {
      if (!propsOwners.has(ref)) propsOwners.set(ref, { kind: 'props', id: propsOwners.size, by: new Map(ref.props.map((p) => [p.name, p])) });
      return propsOwners.get(ref);
    }
    const file = str(ref?.file);
    if (file === null) return null;
    const fid = idByPath.get(file);
    if (fid === undefined) return null;
    if (ref.shape === true) return shapeByPlace.get(`${fid}#${String(ref.line)}#${String(ref.col)}`) ?? null;
    const name = str(ref?.name);
    return name === null ? null : ownerByFileName.get(`${fid}#${name}`) ?? null;
  };

  // A property of a class and a property of a type are looked up in different tables and answer the same
  // question - keyed identically, so the walk does not care which kind of owner it is standing on.
  const propByOwner = new Map();
  // AN ACCESSOR PAIR IS ONE PROPERTY, and a READ is answered by its GETTER, a WRITE by its SETTER - whichever is declared last
  // used to win both, so `get value()` then `set value(v)` pointed every read at the setter. `propByOwner` keeps a getter
  // over a setter of the name, `setterByOwner` the setter a write (`(x)="value = $event"`) lands on.
  const setterByOwner = new Map();
  for (const row of store.table('members')) {
    const key = `class:${String(row.class)}#${String(row.name)}`;
    if (row.kind === 'setter') setterByOwner.set(key, row);
    if (row.kind === 'setter' && propByOwner.get(key)?.kind === 'getter') continue;
    propByOwner.set(key, row);
  }
  // AN ENUM MEMBER IS NO ROW OF ITS OWN: it answers as the enum's row, at the member's line, its type the end of the chain.
  for (const row of store.table('enums')) {
    if (row.half !== 'typescript') continue;
    for (const m of Array.isArray(row.members) ? row.members : []) {
      if (str(m?.name) === null || typeof m.line !== 'number') continue;
      propByOwner.set(`enum:${String(row.id)}#${m.name}`, { id: row.id, file: row.file, line: m.line, type_ref: null });
    }
  }
  // A SHAPE'S MEMBERS answer by name as a class's do (getter over setter, a write the setter); a spread, an index, a call and a
  // construct signature have no name a read can spell. An INDEX answers a keyed read instead (`indexByShape`).
  const indexByShape = new Map();
  // A SPREAD HOLDS WHAT IT SPREADS (rows 72): `{ ...x, column: 1 }` has every name `x`'s type declares, the LAST spread first - it overrides the
  // ones before it. A name the literal spells itself is found before any spread is asked.
  const spreadsByShape = new Map();
  for (const row of store.table('shape_members')) {
    if (row.kind === 'index') { indexByShape.set(row.shape, row); continue; }
    if (row.kind === 'spread') {
      if (!spreadsByShape.has(row.shape)) spreadsByShape.set(row.shape, []);
      spreadsByShape.get(row.shape).unshift(row);
      continue;
    }
    if (row.kind === 'call' || row.kind === 'construct') continue;
    const key = `shape:${String(row.shape)}#${String(row.name)}`;
    if (row.kind === 'setter') setterByOwner.set(key, row);
    if (row.kind === 'setter' && propByOwner.get(key)?.kind === 'getter') continue;
    propByOwner.set(key, row);
  }
  for (const row of store.table('type_members')) {
    // `path` is the dotted route; the direct children of an owner are the ones with no dot in it.
    const p = str(row.path);
    if (p === null || p.includes('.')) continue;
    const key = `type:${String(row.owner)}#${String(row.name)}`;
    // A NAME SEVERAL OPERANDS DECLARE IS TWO ROWS (rows 75): the checker's COMPUTED row, typed as every declaration together types it, and the literal's own. The
    // computed one answers a read - the literal's alone would lose what the other operand says of the property.
    if (propByOwner.get(key)?.via === 'computed' && row.via !== 'computed') continue;
    propByOwner.set(key, row);
  }

  for (const table of ['members', 'type_members', 'shape_members']) {
    for (const row of store.table(table)) {
      if (row.kind === 'spread' || row.kind === 'index' || typeof row.line !== 'number') continue;
      const key = `${String(row.file)}#${String(row.line)}#${String(row.name)}`;
      if (!rowByPlace.has(key)) rowByPlace.set(key, row);
    }
  }

  // A MEMBER A COMPONENT INHERITS IS STILL ITS MEMBER. `propByOwner` holds DECLARED members, so a template
  // reading a field its base class declares resolved to nothing - whole lists came out unknown for
  // that reason alone, with the members and the base both in the map and only the hop between them missing.
  // `extends` carries the base's FILE, so a shared class name cannot merge two hierarchies, and the walk
  // stops on a visited set because `extends` is data and data can name a loop.
  const baseOfClass = new Map();
  for (const row of store.table('classes')) baseOfClass.set(row.id, ownerOf(row.extends));

  // A GENERIC IS READ AS IT IS INSTANTIATED (#98): `Concrete extends Base<Payload>` binds `Base`'s `T` to `Payload`, and
  // `Wrapped: Wrapper<T>` binds `Wrapper`'s own parameter to whatever `T` is bound to. An owner carries those bindings as
  // `env` (parameter name -> owner); a member typed by a parameter (`type_ref.param`) takes its binding, an unbound one
  // names nothing. The arguments are finite terms of a declaration, so the walk into them ends.
  const paramsOf = new Map();
  const extendsOf = new Map();
  for (const row of store.table('classes')) {
    if (Array.isArray(row.type_params)) paramsOf.set(`class:${String(row.id)}`, row.type_params);
    extendsOf.set(row.id, row.extends ?? null);
  }
  for (const row of store.table('interfaces')) {
    if (Array.isArray(row.type_params)) paramsOf.set(`type:${String(row.id)}`, row.type_params);
  }
  // A GENERIC ALIAS IS ONE TOO (rows 72): `type Tip<TData> = { data: TData }` binds `TData` from `Tip<{ .. }>` as an interface's parameters are.
  for (const row of store.table('type_aliases')) {
    if (Array.isArray(row.type_params)) paramsOf.set(`type:${String(row.id)}`, row.type_params);
  }
  // A PARAMETER NOBODY PASSED holds this, not null: null is an argument that names no declaration (`Box<string>`), which stays nothing.
  const NOT_PASSED = Symbol('not passed');
  const resolveRef = (ref, env) => {
    if (ref === null || typeof ref !== 'object') return null;
    // WHAT A `keyvalue` PIPE HANDS ON (rows 80, `TsTplHops.mjs`): a `{ key, value }` pair of the owners its two refs name.
    if (ref.pair === true) return { kind: 'pair', id: 'pair', key: resolveRef(ref.key ?? null, env), keyRef: ref.key ?? null, value: resolveRef(ref.value ?? null, env), valueRef: ref.value ?? null };
    // A CONSTRAINT THAT NAMES SEVERAL (`T extends A | B`) is a list of refs, not one.
    if (Array.isArray(ref.type_refs) && ref.file === undefined) return ownerOfRow(ref, env);
    const name = str(ref.name) ?? str(ref.text);
    const owner = ownerOf(ref);
    // A heritage argument names a parameter only by its text (`extends Base<T>`); a type_ref says so (`param`).
    // `T & { .. }` (rows 85): a parameter's owner takes the literals the intersection dropped too, as a declaration's does below.
    if (ref.param === true || (owner === null && name !== null && env?.has(name))) return withShapes(paramOwner(ref, name, env), ref);
    // THE ALIAS WRITTEN STANDS IN for a type the checker took past it (`type Desc = Readonly<{ .. }>` is `Readonly`, a library type, with `alias` Desc): the
    // alias's own computed members answer. Its arguments are the library type's, never the alias's own.
    if (owner === null) {
      const written = ownerOf(ref.alias);
      return written === null ? null : bound(written, undefined, env);
    }
    return withShapes(bound(owner, ref.args ?? ref.type_args, env), ref);
  };
  // `X & { .. }` (rows 72): the literals the intersection dropped from the ref answer beside `X`, as one `and` owner, in the order the parts are
  // written - a literal written ahead of `X` (`before`) declares a name first. `X` naming nothing leaves the literals alone.
  const withShapes = (one, ref) => {
    const dropped = Array.isArray(ref.shapes) ? ref.shapes.filter((r) => ownerOf(r) !== null) : [];
    if (!dropped.length) return one;
    const parts = [...dropped.filter((r) => r.before === true).map(ownerOf), ...(one === null ? [] : [one]), ...dropped.filter((r) => r.before !== true).map(ownerOf)];
    return parts.length === 1 ? parts[0] : { kind: 'any', op: 'and', of: parts };
  };
  // WHAT A TYPE PARAMETER IS (rows 72): what its instantiation passed; the DEFAULT when it passed none (`Slot` is `Slot<Dto>`); and outside any
  // instantiation - the generic class's own template, `row: T` - the CONSTRAINT (`T extends Row` reads as `Row`). A bound it lacks names nothing.
  const paramOwner = (ref, name, env) => {
    if (name !== null && env?.has(name)) {
      const held = env.get(name);
      // NOTHING PASSED reads as the default, else the constraint (rows 74): an element's inputs feed a generic component's parameters (`instance`) and one nothing feeds has no argument either.
      return held === NOT_PASSED ? resolveRef(ref.default ?? ref.constraint ?? null, env) : held;
    }
    return resolveRef(ref.constraint ?? null, env);
  };
  const bound = (owner, args, env) => {
    const params = paramsOf.get(`${owner.kind}:${owner.id}`) ?? [];
    const out = new Map();
    params.forEach((p, i) => out.set(p, Array.isArray(args) && i < args.length ? resolveRef(args[i], env) : NOT_PASSED));
    // A LITERAL TAKES THE BINDINGS OF THE DECLARATION IT IS WRITTEN IN (`Combination: { Items: { Result: TItem }[] }` in a generic class): its members
    // are typed by the parameters of that class, never its own.
    if (!out.size && owner.kind === 'shape' && env?.size) return { ...owner, env };
    return out.size ? { ...owner, env: out } : owner;
  };
  /** A GENERIC DECLARATION AS AN ELEMENT INSTANTIATES IT (rows 74): `fed` maps a type parameter's name to what the input bound on the element fed it (an owner, or null for
   *  a value that names nothing); a parameter nothing fed is `NOT_PASSED`, read as its default, else its constraint. A declaration with no parameters is itself. */
  const paramNames = (owner) => paramsOf.get(`${owner.kind}:${owner.id}`) ?? [];
  const instance = (owner, fed) => {
    const names = paramNames(owner);
    return names.length ? { ...owner, env: new Map(names.map((p) => [p, fed.has(p) ? fed.get(p) : NOT_PASSED])) } : owner;
  };
  const baseTypedOf = (cls) => {
    const base = baseOfClass.get(cls.id) ?? null;
    return base === null ? null : bound(base, extendsOf.get(cls.id)?.type_args, cls.env ?? null);
  };

  // AN INTERFACE'S BASES (rows 72): `interface SavedDoc extends Doc` has the members `Doc` declares. `extends_refs` is one ref per base, in order, each as
  // a member's `type_ref` is - arguments included - or null for a base that names no declaration (a library type).
  const extendsOfType = new Map();
  for (const row of store.table('interfaces')) {
    if (Array.isArray(row.extends_refs)) extendsOfType.set(row.id, row.extends_refs);
  }
  const basesOf = (current) => (current.kind === 'class' ? [baseTypedOf(current)]
    : current.kind === 'type' ? (extendsOfType.get(current.id) ?? []).map((ref) => resolveRef(ref, current.env ?? null))
      : current.kind === 'shape' ? (spreadsByShape.get(current.id) ?? []).map((row) => ownerOfRow(row)) : []);

  /** The row `name` answers on `owner` - walking its bases, each bound as the type before it instantiates it - and the
   *  bindings of the owner it was found on: `{row, env, groups}`. SEVERAL TYPES AT ONCE (`any`: a member whose `type_refs` lists several - `A | B`,
   *  `A & B`, #97 - or a ref with `shapes`) answer by how they combine. An UNION (`or`) answers with the ONE declaration any of its arms answers with, the
   *  same place reached through several arms being one (`A | B` over two aliases of one generic) and two different places being no answer, as picking one of
   *  `A | B` would claim the other away. An INTERSECTION (`and`) is ALL its parts, so a name several declare is the FIRST part's - the checker's one
   *  property whose `declarations` list them in written order. `groups` is what the cursor goes on from: one list per arm of a union, each the rows of
   *  every part of an intersection that declares the name. The bases are visited nearest first and the walk stops on a visited set, because `extends`
   *  is data and data can name a loop. */
  const propWithEnv = (owner, name, write = false) => {
    if (owner?.kind === 'any') {
      const arms = [];
      for (const one of owner.of) {
        const hit = propWithEnv(one, name, write);
        if (hit !== null) arms.push(hit);
      }
      if (owner.op === 'and') {
        const seenRows = new Set();
        const flat = arms.flatMap((arm) => arm.groups.flat()).filter((h) => !seenRows.has(h.row.id) && seenRows.add(h.row.id));
        return flat.length ? { ...flat[0], groups: [flat] } : null;
      }
      const placeOf = (h) => `${String(h.row.file)}#${String(h.row.line)}#${String(h.row.col ?? '')}`;
      if (arms.length === 0 || arms.some((a) => placeOf(a) !== placeOf(arms[0]))) return null;
      return { ...arms[0], groups: arms.flatMap((a) => a.groups) };
    }
    const queue = [owner];
    const seen = new Set();
    while (queue.length) {
      const current = queue.shift();
      if (current === null || current === undefined) continue;
      // A BASE THAT IS SEVERAL TYPES AT ONCE (a spread of `X & { .. }`) answers as `any` does.
      if (current.kind === 'any') {
        const hit = propWithEnv(current, name, write);
        if (hit !== null) return hit;
        continue;
      }
      if (seen.has(`${current.kind}:${current.id}`)) continue;
      seen.add(`${current.kind}:${current.id}`);
      if (current.kind === 'props') {
        const at = current.by.get(name);
        const row = at === undefined ? undefined : rowByPlace.get(`${String(idByPath.get(at.file))}#${String(at.line)}#${name}`);
        if (row === undefined) continue;
        const one = { row, env: null };
        return { ...one, groups: [[one]] };
      }
      const key = `${current.kind}:${current.id}#${name}`;
      const hit = (write ? setterByOwner.get(key) : undefined) ?? propByOwner.get(key);
      if (hit !== undefined) {
        const one = { row: hit, env: current.env ?? null };
        return { ...one, groups: [[one]] };
      }
      queue.push(...basesOf(current));
    }
    return null;
  };

  /** An alias of an intersection as the parts it is made of (`type_aliases.type_refs`), bound by the alias's own arguments; any other owner is itself. */
  const aliasParts = new Map();
  for (const row of store.table('type_aliases')) {
    if (row.type_ref != null || Array.isArray(row.type_refs)) aliasParts.set(row.id, row);
  }
  const expandAlias = (owner) => {
    const row = owner?.kind === 'type' ? aliasParts.get(owner.id) : undefined;
    return row === undefined ? owner : ownerOfRow(row, owner.env ?? null) ?? owner;
  };

  /** The owner a type_ref's ELEMENT names - what an array holds, which is what a loop variable is. A parameter element (`T[]`) reads as `resolveRef` does. */
  const elementOwnerOf = (ref, env = null) => {
    const element = elementRefOf(ref);
    return element === null ? null : resolveRef(element, env);
  };
  /** The ref of what an array holds, or null. AN ARRAY OF ARRAYS (rows 73): `Item[][]`'s `element` names the library `Array` and no more - the element's own
   *  `element` is on the ref's first `args`, which is the ref of that element whole - so the one a loop inside a loop takes is read from there. */
  const elementRefOf = (ref) => {
    const element = ref?.element;
    if (element === undefined || element === null) return null;
    const whole = Array.isArray(ref.args) ? ref.args[0] : null;
    const same = whole && typeof whole === 'object' && whole.name === element.name && whole.file === element.file && whole.line === element.line;
    return element.element === undefined && same ? whole : element;
  };

  /** The owner a member's type names: its `type_ref`, or every owner its `type_refs` list names (#97) - one alone, or
   *  several as one `any` owner `propWithEnv` asks together. Null when it names none: the chain stops there. */
  const ownerOfRow = (row, env = null) => {
    if (row.type_ref != null) return resolveRef(row.type_ref, env);
    const all = new Map();
    for (const ref of Array.isArray(row.type_refs) ? row.type_refs : []) {
      // EACH PART IS BOUND BY THE ENV IT WAS REACHED IN (rows 74): `{ item: T } & { more: number }` in `Box<Item>` answers `item` as `Item`; an anonymous part
      // (a shape) takes the enclosing declaration's parameters as it does when it is the member's whole type.
      const part = resolveRef(ref, env);
      // AN ALIAS OF AN INTERSECTION AMONG THE PARTS OF ONE (`A & Vm`, `type Vm = B & { .. }`, rows 74) IS ITS OWN PARTS: the type refs name the alias the author wrote, and the
      // alias's members are the ones each of them declares - read through the declarations, never the alias's computed copies of them.
      const one = row.type_refs_of === 'and' ? expandAlias(part) : part;
      if (one !== null) all.set(one.kind === 'any' ? `any:${all.size}` : `${one.kind}:${one.id}`, one);
    }
    return all.size === 0 ? null : all.size === 1 ? [...all.values()][0] : { kind: 'any', op: row.type_refs_of === 'and' ? 'and' : 'or', of: [...all.values()] };
  };

  // A HOP WHOSE TYPE IS UNKNOWN ENDS THE CHAIN. It kept the PREVIOUS owner, so `{{ x.y }}` over an untyped `x` looked
  // `y` up on the component again and named the component's own `y` - a target the read never reaches.
  const hopOf = (owner, name, write = false) => {
    const found = propWithEnv(owner, name, write);
    if (!found) return null;
    const row = found.row;
    const file = pathById.get(row.file);
    const line = typeof row.line === 'number' ? row.line : null;
    if (file === undefined || line === null) return null;
    // A NAME SEVERAL TYPES DECLARE goes on as what each of them declares it as: an intersection of the parts' (`deep` may be the second's alone), a
    // union of the arms' - and an arm whose type is unknown makes the union's so (a type of `X | any` names nothing).
    const distinct = (owners) => owners.filter((o, i) => o.kind === 'any' || owners.findIndex((p) => p.kind === o.kind && p.id === o.id) === i);
    const join = (owners, op) => (owners.length < 2 ? owners[0] ?? null : { kind: 'any', op, of: owners });
    const perGroup = found.groups.map((g) => join(distinct(g.map((h) => ownerOfRow(h.row, h.env)).filter((o) => o !== null)), 'and'));
    const next = found.groups.length < 2 && found.groups[0].length < 2 ? ownerOfRow(row, found.env)
      : perGroup.some((o) => o === null) ? null : join(distinct(perGroup), 'or');
    return { name, file, line, row: row.id, owner: next, member: row, env: found.env };
  };

  return { ownerOf, propByOwner, indexByShape, resolveRef, elementOwnerOf, elementRefOf, ownerOfRow, hopOf, instance, paramNames, baseTypedOf };
}
