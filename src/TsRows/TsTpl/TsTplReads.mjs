/**
 * A TEMPLATE READ, RESOLVED TO THE DECLARATION IT LANDS ON.
 *
 * Ported from the original Angular extractor. `{{ row.cells.length }}` publishes three
 * names; without this pass a consumer has to match them against candidate interfaces by hand, which is the
 * one lookup this map exists to have already performed. Each `Read` node in a template expression's stored
 * AST gains a `target` naming the member or type member it reaches.
 *
 * Imports are flat: see `TsDecls/TsTypeRef.mjs`.
 */

const str = (v) => (typeof v === 'string' ? v : null);

export function rollupTemplateReads(store) {
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
  const ownerOf = (ref) => {
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
  for (const row of store.table('shape_members')) {
    if (row.kind === 'index') { indexByShape.set(row.shape, row); continue; }
    if (row.kind === 'spread' || row.kind === 'call' || row.kind === 'construct') continue;
    const key = `shape:${String(row.shape)}#${String(row.name)}`;
    if (row.kind === 'setter') setterByOwner.set(key, row);
    if (row.kind === 'setter' && propByOwner.get(key)?.kind === 'getter') continue;
    propByOwner.set(key, row);
  }
  for (const row of store.table('type_members')) {
    // `path` is the dotted route; the direct children of an owner are the ones with no dot in it.
    const p = str(row.path);
    if (p !== null && !p.includes('.')) propByOwner.set(`type:${String(row.owner)}#${String(row.name)}`, row);
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
  const resolveRef = (ref, env) => {
    if (ref === null || typeof ref !== 'object') return null;
    const name = str(ref.name) ?? str(ref.text);
    const owner = ownerOf(ref);
    // A heritage argument names a parameter only by its text (`extends Base<T>`); a type_ref says so (`param`).
    if (ref.param === true || (owner === null && name !== null && env?.has(name))) return env?.get(name) ?? null;
    return owner === null ? null : bound(owner, ref.args ?? ref.type_args, env);
  };
  const bound = (owner, args, env) => {
    const params = paramsOf.get(`${owner.kind}:${owner.id}`) ?? [];
    const out = new Map();
    params.forEach((p, i) => {
      const r = resolveRef(Array.isArray(args) ? args[i] : null, env);
      if (r !== null) out.set(p, r);
    });
    return out.size ? { ...owner, env: out } : owner;
  };
  const baseTypedOf = (cls) => {
    const base = baseOfClass.get(cls.id) ?? null;
    return base === null ? null : bound(base, extendsOf.get(cls.id)?.type_args, cls.env ?? null);
  };

  /** The row `name` answers on `owner` - walking its bases, each bound as the class before it instantiates it - and the
   *  bindings of the owner it was found on. */
  const propWithEnv = (owner, name, write = false) => {
    if (owner?.kind === 'any') {
      const row = propOf(owner, name);
      return row === undefined ? null : { row, env: null };
    }
    let current = owner;
    const seen = new Set();
    while (current !== null && current !== undefined && !seen.has(current.id)) {
      seen.add(current.id);
      const key = `${current.kind}:${current.id}#${name}`;
      const hit = (write ? setterByOwner.get(key) : undefined) ?? propByOwner.get(key);
      if (hit !== undefined) return { row: hit, env: current.env ?? null };
      current = current.kind === 'class' ? baseTypedOf(current) : null;
    }
    return null;
  };

  const propOf = (owner, name) => {
    // SEVERAL TYPES AT ONCE - a member whose `type_refs` lists several (`A | B`, `A & B`, #97): the name is the ONE row
    // any of them answers with; two different rows is no answer, as picking one of `A | B` would claim the other away.
    if (owner?.kind === 'any') {
      const hits = new Map();
      for (const one of owner.of) {
        const hit = propOf(one, name);
        if (hit !== undefined) hits.set(hit.id, hit);
      }
      return hits.size === 1 ? [...hits.values()][0] : undefined;
    }
    let current = owner;
    const seen = new Set();
    while (current !== null && !seen.has(current.id)) {
      seen.add(current.id);
      const hit = propByOwner.get(`${current.kind}:${current.id}#${name}`);
      if (hit !== undefined) return hit;
      current = current.kind === 'class' ? baseOfClass.get(current.id) ?? null : null;
    }
    return undefined;
  };

  /** The owner a type_ref's ELEMENT names - what an array holds, which is what a loop variable is. */
  const elementOwnerOf = (ref) => (ref?.element === undefined ? null : ownerOf(ref.element));
  /** The owner a type_ref's UNWRAPPED field names - the innermost type behind single-argument wrappers. */
  const unwrappedOwnerOf = (ref) => (ref?.unwrapped === undefined ? null : ownerOf(ref.unwrapped));

  /** The owner a member's type names: its `type_ref`, or every owner its `type_refs` list names (#97) - one alone, or
   *  several as one `any` owner `propOf` asks together. Null when it names none: the chain stops there. */
  const ownerOfRow = (row, env = null) => {
    if (row.type_ref != null) return resolveRef(row.type_ref, env);
    const all = new Map();
    for (const ref of Array.isArray(row.type_refs) ? row.type_refs : []) {
      const one = ownerOf(ref);
      if (one !== null) all.set(`${one.kind}:${one.id}`, one);
    }
    return all.size === 0 ? null : all.size === 1 ? [...all.values()][0] : { kind: 'any', of: [...all.values()] };
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
    return { name, file, line, row: row.id, owner: ownerOfRow(row, found.env), member: row, env: found.env };
  };

  // Which class each template expression belongs to. EVERY template expression carries `node`; only some
  // carry `binding` - an interpolation, an `@if` condition and an `@for` iterable have none - so joining
  // through `binding` skipped thousands of expressions unconditionally, including every `{{ ... }}` in the app.
  const classOfTemplate = new Map();
  for (const t of store.table('templates')) if (t.class != null) classOfTemplate.set(t.id, t.class);

  const templateOfNode = new Map();
  const parentOfNode = new Map();
  const kindOfNode = new Map();
  const boundNames = new Map();
  const implicitAt = new Map();
  const aliasesAt = new Map();
  for (const n of store.table('template_nodes')) {
    templateOfNode.set(n.id, n.template);
    parentOfNode.set(n.id, n.parent ?? null);
    kindOfNode.set(n.id, n.kind);
    const names = [];
    for (const v of Array.isArray(n.variables) ? n.variables : []) {
      if (str(v.name) !== null) names.push(v.name);
      if (v.value === '$implicit' && typeof v.name === 'string') implicitAt.set(n.id, v.name);
      // AN `as` ALIAS HOLDS THE CONDITION'S VALUE: every name an `@if` branch binds, and an `*ngIf` wrapper's
      // `as u` (`ngIf`) or `let u` (`$implicit`) - the latter two only where an `ngIf` condition sits (below).
      const alias = n.kind === 'IfBlockBranch' || v.value === 'ngIf' || v.value === '$implicit';
      if (alias && typeof v.name === 'string') {
        if (!aliasesAt.has(n.id)) aliasesAt.set(n.id, new Set());
        aliasesAt.get(n.id).add(v.name);
      }
    }
    for (const r of Array.isArray(n.references) ? n.references : []) {
      if (str(r.name) !== null) names.push(r.name);
    }
    if (names.length) boundNames.set(n.id, names);
  }

  // A `@let` IS VISIBLE TO ITS SIBLINGS AND EVERYTHING UNDER THEM, not under itself - it is a leaf. So its name
  // is filed under its PARENT, or under the template when it sits at the top level.
  const letsUnder = new Map();
  for (const n of store.table('template_nodes')) {
    if (n.kind !== 'LetDeclaration' || str(n.name) === null) continue;
    const at = n.parent ?? `t:${String(n.template)}`;
    if (!letsUnder.has(at)) letsUnder.set(at, []);
    letsUnder.get(at).push(n.name);
  }

  // THE LINK RUNS FROM THE EXPRESSION TO THE BINDING, not the other way: a `bindings` row carries no
  // expression id, so reading one off it yielded nothing and no loop variable ever resolved.
  const iterableBindings = new Set();
  const conditionBindings = new Set();
  const nodeOfBinding = new Map();
  for (const b of store.table('bindings')) {
    if (b.node != null) nodeOfBinding.set(b.id, b.node);
    if (b.name === 'ngForOf') iterableBindings.add(b.id);
    if (b.name === 'ngIf') conditionBindings.add(b.id);
  }
  const iterableAstOfNode = new Map();
  // THE CONDITION AN ALIAS IS BOUND TO, by node: an `@if` branch's own expression, an `*ngIf` wrapper's `ngIf` binding.
  const conditionAstOfNode = new Map();
  for (const e of store.table('expressions')) {
    if (e.kind === 'block_IfBlockBranch' && e.node != null) { conditionAstOfNode.set(e.node, e.ast); continue; }
    if (e.lang !== 'ts' && e.binding != null && conditionBindings.has(e.binding)) {
      const nodeId = nodeOfBinding.get(e.binding);
      if (nodeId !== undefined) conditionAstOfNode.set(nodeId, e.ast);
    }
    // An `@for` block's collection is its own expression, on the block's node.
    if (e.kind === 'block_ForLoopBlock' && e.node != null) { iterableAstOfNode.set(e.node, e.ast); continue; }
    if (e.lang === 'ts' || e.binding == null || !iterableBindings.has(e.binding)) continue;
    const nodeId = nodeOfBinding.get(e.binding);
    if (nodeId !== undefined) iterableAstOfNode.set(nodeId, e.ast);
  }

  /**
   * WHAT A PIPE HANDS ON, derived from the pipe's OWN `transform` - never from its name.
   *
   * A pipe transforms the type, so a chain through one cannot be read as if the pipe were absent. A pipe
   * declared in this application IS in the map, and its class's `transform` member carries a `type_ref` like
   * any other member. A FRAMEWORK pipe is not - `async` lives in a dependency - so for that case the
   * SOURCE's `unwrapped` ref answers instead: `Observable<Foo[]>` unwraps to `Foo`, which is what `async`
   * yields per iteration. Both are properties of declarations; neither recognises a pipe by name.
   */
  const transformRefOfPipe = new Map();
  const transformOwnersOfPipe = new Map();
  const knownPipes = new Set();
  for (const row of store.table('pipes')) {
    const declared = str(row.pipe_name);
    if (declared !== null) knownPipes.add(declared);
    if (declared === null || row.class == null) continue;
    // A METHOD'S TYPE IS WHAT IT RETURNS (`return_ref`): its own `type_ref` is null, so no in-tree pipe ever typed a read.
    const transform = propByOwner.get(`class:${String(row.class)}#transform`);
    const returned = transform?.return_ref ?? transform?.type_ref ?? null;
    if (returned !== null) transformRefOfPipe.set(declared, returned);
    // A TRANSFORM THAT RETURNS SEVERAL TYPES (`cond ? a.x : a.y` infers `X | Y`) hands on the `any` owner of them, as a member's `type_refs` do.
    else if (Array.isArray(transform?.return_refs)) transformOwnersOfPipe.set(declared, ownerOfRow({ type_refs: transform.return_refs }));
  }

  /** The name of a pipe applied anywhere in an expression - which pipe produced the value being iterated. */
  const pipeNameOf = (ast) => {
    let found = null;
    const visit = (node) => {
      if (found !== null || !node || typeof node !== 'object') return;
      if (Array.isArray(node)) { for (const x of node) visit(x); return; }
      if (node.k === 'Pipe') { found = str(node.name); return; }
      for (const v of Object.values(node)) visit(v);
    };
    visit(ast);
    return found;
  };

  /** The names of a Read chain, root first - `a.b.c` reads as Read(c) over Read(b) over Read(a). */
  const chainOf = (node) => {
    const names = [];
    const nodes = [];
    // The ROOT is named as it is found, not indexed out afterwards: the walk ends at the receiver-most read,
    // so the last name seen IS the root, and saying so needs no positional pick.
    let root = null;
    let current = node;
    // A WRITE HEADS ITS CHAIN (#99): `(valueChange)="f.On = $event"` writes `On` through the read `f`, and the member written
    // is resolved as a read of it would be - so a consumer can tell which element SETS a member, not only which reads it.
    while (current && (current.k === 'Read' || current.k === 'SafeRead' || (current === node && current.k === 'Write'))) {
      const name = str(current.name);
      if (name === null) break;
      root = name;
      names.unshift(name);
      nodes.unshift(current);
      const receiver = current.receiver;
      current = receiver && typeof receiver === 'object' ? receiver : null;
    }
    // A chain bottoming on anything but the component's own context (a pipe, a call, a keyed read, `a ?? b`) names its
    // `base`: `resolveChain` asks what that VALUE is and reads the names off it. A literal or an unknown kind types nothing.
    // An arrow function's parameter is not the component's: `(x) => x.done` resolves no member `x`.
    const inContext = current === null || current.k === 'Implicit' || current.k === 'This';
    if (inContext) return nodes[0]?.param ? { root: null, names: [], nodes: [], base: null } : { root, names, nodes, base: null };
    return { root: null, names, nodes, base: current };
  };

  /** The `type_ref` whatever a read chain ends on carries - used to type a loop variable from its
   *  collection. THE ITERABLE CAN ITSELF BE A LOOP VARIABLE: an inner `*ngFor` roots its collection in the
   *  outer one's variable, which is no component property at all - every such scope and hundreds of property
   *  reads were indistinguishable from genuinely unresolvable ones. */
  const ownerRefOfChain = (ast, component, scope) => {
    let found = null;
    const visit = (node) => {
      if (found !== null || !node || typeof node !== 'object') return;
      if (Array.isArray(node)) { for (const x of node) visit(x); return; }
      if (node.k === 'Read' || node.k === 'SafeRead') {
        const { root, names } = chainOf(node);
        if (names.length && root !== null) {
          const isBound = scope.has(root);
          let cursor = isBound ? scope.get(root) ?? null : component;
          let last;
          for (const name of isBound ? names.slice(1) : names) {
            if (cursor === null) break;
            const found = propWithEnv(cursor, name);
            if (!found) { cursor = null; break; }
            last = found.row;
            cursor = ownerOfRow(found.row, found.env);
          }
          if (last) { found = last.type_ref; return; }
        }
      }
      for (const v of Object.values(node)) visit(v);
    };
    visit(ast);
    return found;
  };

  /**
   * WHAT AN `as` ALIAS IS: the owner of the condition it is bound to - only when the condition IS one read chain
   * (`user`, `row.user`, `user!`) or one pipe over one (`user$ | async`), every hop resolved. A condition that
   * tests something (`a && b`, `x.ok === true`), calls, or stops on a name no declaration answers types nothing:
   * the alias still shadows the component, but holds no owner. A pipe is read as a loop's is - its own
   * `transform` type, else (a framework pipe) what the piped value unwraps to.
   */
  const conditionOf = (ast) => {
    let at = ast;
    while (at && (at.k === 'Source' || at.k === 'NonNull')) at = at.k === 'Source' ? at.ast : at.expr;
    return at ?? null;
  };
  const chainEnd = (ast, component, scope) => {
    const at = conditionOf(ast);
    if (!at || (at.k !== 'Read' && at.k !== 'SafeRead')) return null;
    const { root, names } = chainOf(at);
    if (root === null || !names.length) return null;
    const isBound = scope.has(root);
    let owner = isBound ? scope.get(root) ?? null : component;
    let ref = null;
    for (const name of isBound ? names.slice(1) : names) {
      const found = owner === null ? null : propWithEnv(owner, name);
      if (!found) return null;
      ref = found.row.type_ref ?? null;
      owner = ownerOfRow(found.row, found.env);
    }
    return { owner, ref };
  };
  const aliasOwnerOf = (ast, component, scope) => {
    const at = conditionOf(ast);
    if (at?.k !== 'Pipe') return chainEnd(at, component, scope)?.owner ?? null;
    const pipeName = str(at.name);
    const piped = chainEnd(at.exp, component, scope);
    if (pipeName === null || piped === null) return null;
    return ownerOf(transformRefOfPipe.get(pipeName)) ?? transformOwnersOfPipe.get(pipeName)
      ?? (knownPipes.has(pipeName) ? null : unwrappedOwnerOf(piped.ref));
  };

  /**
   * The names the TEMPLATE binds at a node - a `*ngFor` variable, an `as` alias, a `#ref` - walking up the
   * ancestors that enclose it.
   *
   * A root read must not be resolved against the component when the template introduced that name. Angular's
   * AST makes a local read and a component read indistinguishable, so without this the resolution answers
   * with the SHADOWED member - measured on `*ngIf="cfg | async as cfg"`, where the alias shadows a property
   * of the same name. A wrong answer is worse than none, so a shadowed root resolves to nothing at all.
   *
   * OUTERMOST FIRST: an inner loop's collection can be rooted in an outer loop's variable, so resolving
   * inner-first asks about a name that has not been bound yet. An inner binding of the same name still wins,
   * being applied last, which is what shadowing means.
   *
   * `outer` leaves the node's OWN names out: a `Template`'s `templateAttrs` and an `@if` condition are evaluated BEFORE the names
   * they bind exist - in `*ngIf="x$ | async as x$"` the condition's `x$` is the component's member, not the alias.
   */
  const scopeAt = (nodeId, component, outer = false) => {
    const out = new Map();
    // `@let` names, untyped, from the template's top level down - see `letsUnder`.
    for (const name of letsUnder.get(`t:${String(templateOfNode.get(nodeId))}`) ?? []) out.set(name, null);
    const ancestors = [];
    let current = nodeId;
    const guard = new Set();
    while (current != null && !guard.has(current)) {
      guard.add(current);
      ancestors.unshift(current);
      current = parentOfNode.get(current) ?? null;
    }
    for (const id of ancestors) {
      if (id !== nodeId) for (const name of letsUnder.get(id) ?? []) out.set(name, null);
      if (outer && id === nodeId) continue;
      const iterable = iterableAstOfNode.get(id);
      const condition = conditionAstOfNode.get(id);
      for (const name of boundNames.get(id) ?? []) {
        if (condition !== undefined && aliasesAt.get(id)?.has(name)) {
          out.set(name, aliasOwnerOf(condition, component, out));
          continue;
        }
        // A LOOP VARIABLE, THREE WAYS, in order of how directly the map proves it: the pipe's own declared
        // return type, then what the piped value holds once its wrapper is unwrapped, then - for an unpiped
        // collection - the element of the collection itself. Anything else the template binds (an alias is above) is
        // a name this rollup cannot type, and it shadows the component either way.
        const pipeName = iterable === undefined ? null : pipeNameOf(iterable);
        const sourceRef = iterable === undefined ? null : ownerRefOfChain(iterable, component, out);
        const bound = implicitAt.get(id) !== name || iterable === undefined ? null
          : pipeName !== null
            ? elementOwnerOf(transformRefOfPipe.get(pipeName))
              ?? (knownPipes.has(pipeName) ? null : unwrappedOwnerOf(sourceRef))
            : elementOwnerOf(sourceRef);
        out.set(name, bound);
      }
    }
    return out;
  };

  let chains = 0;
  let hops = 0;

  const sameOwner = (a, b) => (a?.owner && b?.owner && a.owner.kind === b.owner.kind && a.owner.id === b.owner.id ? a : null);

  /**
   * WHAT AN EXPRESSION'S VALUE IS, as the declarations say: `{ owner, ref, row, env }` or null. A read chain is its last hop; a keyed
   * read is the ELEMENT of what it indexes; a call is what the METHOD it calls returns (`return_ref`); a pipe is what its own
   * `transform` returns, or - a framework pipe like `async` - what the piped value unwraps to; `a ?? b`, `a || b` and `c ? a : b`
   * are a value only when both sides name ONE owner. The expression is a finite tree, so every walk ends.
   */
  const valueOf = (node, component, scope) => {
    if (!node || typeof node !== 'object') return null;
    const same = (x, y) => sameOwner(valueOf(x, component, scope), valueOf(y, component, scope));
    switch (node.k) {
      case 'Source': return valueOf(node.ast, component, scope);
      case 'NonNull': return valueOf(node.expr, component, scope);
      case 'Read': case 'SafeRead': return resolveChain(node, component, scope);
      case 'KeyedRead': case 'SafeKeyedRead': {
        // A KEYED READ IS THE ELEMENT of an array, else what an INDEX SIGNATURE holds (rows 71): `items[k]` over `{ [k: string]: Item }`,
        // a `Record<string, Item>` or an interface with one - on the ref (`index`), else the index member of the shape it reached.
        const held = valueOf(node.receiver, component, scope);
        const ref = held?.ref?.element ?? held?.ref?.index
          ?? (held?.owner?.kind === 'shape' ? indexByShape.get(held.owner.id)?.type_ref : null) ?? null;
        const owner = ref === null ? null : ownerOf(ref);
        return owner === null ? null : { owner, ref, row: null, env: null };
      }
      case 'Call': case 'SafeCall': {
        const callee = valueOf(node.receiver, component, scope);
        const ref = callee?.row?.return_ref ?? null;
        const owner = ref === null ? null : resolveRef(ref, callee.env);
        return owner === null ? null : { owner, ref, row: null, env: null };
      }
      case 'Pipe': {
        const name = str(node.name);
        const declared = name === null ? undefined : transformRefOfPipe.get(name);
        // A PIPE IN THE MAP THAT CANNOT BE TYPED MUST REFUSE, not borrow its input's type (only a pipe this map lacks unwraps).
        const ref = declared ?? (name === null || knownPipes.has(name) ? null
          : valueOf(node.exp, component, scope)?.ref?.unwrapped ?? null);
        const several = ref === null && name !== null ? transformOwnersOfPipe.get(name) ?? null : null;
        if (several !== null) return { owner: several, ref: null, row: null, env: null };
        const owner = ref === null ? null : resolveRef(ref, null);
        return owner === null ? null : { owner, ref, row: null, env: null };
      }
      case 'Binary': return node.op === '??' || node.op === '||' ? same(node.left, node.right) : null;
      case 'Cond': return same(node.then, node.else);
      default: return null;
    }
  };

  /** A Read, SafeRead or Write chain: every name resolved from its root - the component, a name the template bound, or the
   *  `base` value it hangs from - each hop stamped with its `target`. Answers the LAST hop's value, or null when a hop is lost. */
  const resolveChain = (node, component, scope) => {
    const { root, names, nodes, base } = chainOf(node);
    if (!names.length || (root === null && base === null)) return null;
    // A ROOT BOUND BY THE TEMPLATE IS ALREADY THE OWNER - its name is consumed, not looked up: resolving
    // `item.label` by searching for a property called `item` ON the item's own type finds nothing.
    const isBound = base === null && scope.has(root);
    let cursor = base !== null ? valueOf(base, component, scope)?.owner ?? null
      : isBound ? scope.get(root) ?? null : component;
    let last = isBound && cursor !== null ? { owner: cursor, ref: null, row: null, env: null } : null;
    let resolvedAny = false;
    names.forEach((name, i) => {
      if (i < (isBound ? 1 : 0) || cursor === null) return;
      // THE MEMBER A WRITE HEADS IS ITS SETTER; every other hop is read.
      const hop = hopOf(cursor, name, node.k === 'Write' && i === names.length - 1);
      if (!hop) { cursor = null; last = null; return; }
      const target = nodes[i];
      if (target) {
        // COUNT WHAT WAS ADDED, not what was visited: the walk re-enters a chain through its own
        // receiver, so a per-visit counter reported far more hops than resolved nodes.
        if (target.target === undefined) { hops += 1; resolvedAny = true; }
        target.target = { name: hop.name, file: hop.file, line: hop.line, row: hop.row };
      }
      cursor = hop.owner;
      last = { owner: hop.owner, ref: hop.member.type_ref ?? null, row: hop.member, env: hop.env };
    });
    if (resolvedAny) chains += 1;
    return last;
  };

  const walk = (node, owner, scope) => {
    if (!node || typeof node !== 'object') return;
    if (Array.isArray(node)) { for (const x of node) walk(x, owner, scope); return; }
    if (node.k === 'Read' || node.k === 'SafeRead' || node.k === 'Write') resolveChain(node, owner, scope);
    for (const v of Object.values(node)) walk(v, owner, scope);
  };

  for (const row of store.table('expressions')) {
    if (row.lang === 'ts') continue;
    const nodeId = row.node;
    const tplId = nodeId == null ? undefined : templateOfNode.get(nodeId);
    const classId = tplId === undefined ? undefined : classOfTemplate.get(tplId);
    if (classId === undefined || nodeId == null) continue;
    const component = { kind: 'class', id: classId };
    const own = kindOfNode.get(nodeId) === 'Template' || row.kind === 'block_IfBlockBranch';
    walk(row.ast, component, scopeAt(nodeId, component, own));
  }
  return { chains, hops };
}
