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
  const ownerOf = (ref) => {
    const file = str(ref?.file);
    const name = str(ref?.name);
    if (file === null || name === null) return null;
    const fid = idByPath.get(file);
    return fid === undefined ? null : ownerByFileName.get(`${fid}#${name}`) ?? null;
  };

  // A property of a class and a property of a type are looked up in different tables and answer the same
  // question - keyed identically, so the walk does not care which kind of owner it is standing on.
  const propByOwner = new Map();
  for (const row of store.table('members')) {
    propByOwner.set(`class:${String(row.class)}#${String(row.name)}`, row);
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

  const propOf = (owner, name) => {
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

  const hopOf = (owner, name) => {
    const row = propOf(owner, name);
    if (!row) return null;
    const file = pathById.get(row.file);
    const line = typeof row.line === 'number' ? row.line : null;
    if (file === undefined || line === null) return null;
    return { name, file, line, row: row.id, owner: ownerOf(row.type_ref) ?? owner };
  };

  // Which class each template expression belongs to. EVERY template expression carries `node`; only some
  // carry `binding` - an interpolation, an `@if` condition and an `@for` iterable have none - so joining
  // through `binding` skipped thousands of expressions unconditionally, including every `{{ ... }}` in the app.
  const classOfTemplate = new Map();
  for (const t of store.table('templates')) if (t.class != null) classOfTemplate.set(t.id, t.class);

  const templateOfNode = new Map();
  const parentOfNode = new Map();
  const boundNames = new Map();
  const implicitAt = new Map();
  for (const n of store.table('template_nodes')) {
    templateOfNode.set(n.id, n.template);
    parentOfNode.set(n.id, n.parent ?? null);
    const names = [];
    for (const v of Array.isArray(n.variables) ? n.variables : []) {
      if (str(v.name) !== null) names.push(v.name);
      if (v.value === '$implicit' && typeof v.name === 'string') implicitAt.set(n.id, v.name);
    }
    for (const r of Array.isArray(n.references) ? n.references : []) {
      if (str(r.name) !== null) names.push(r.name);
    }
    if (names.length) boundNames.set(n.id, names);
  }

  // THE LINK RUNS FROM THE EXPRESSION TO THE BINDING, not the other way: a `bindings` row carries no
  // expression id, so reading one off it yielded nothing and no loop variable ever resolved.
  const iterableBindings = new Set();
  const nodeOfBinding = new Map();
  for (const b of store.table('bindings')) {
    if (b.node != null) nodeOfBinding.set(b.id, b.node);
    if (b.name === 'ngForOf') iterableBindings.add(b.id);
  }
  const iterableAstOfNode = new Map();
  for (const e of store.table('expressions')) {
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
  const knownPipes = new Set();
  for (const row of store.table('pipes')) {
    const declared = str(row.pipe_name);
    if (declared !== null) knownPipes.add(declared);
    if (declared === null || row.class == null) continue;
    const transform = propByOwner.get(`class:${String(row.class)}#transform`);
    if (transform?.type_ref != null) transformRefOfPipe.set(declared, transform.type_ref);
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
    while (current && (current.k === 'Read' || current.k === 'SafeRead')) {
      const name = str(current.name);
      if (name === null) break;
      root = name;
      names.unshift(name);
      nodes.unshift(current);
      const receiver = current.receiver;
      current = receiver && typeof receiver === 'object' ? receiver : null;
    }
    // A chain bottoming on a PIPE is resolvable - through what the pipe RETURNS. Everything else that is not
    // the component's own context (a call, a literal) is left alone rather than resolved against the wrong
    // scope.
    const rootedInContext = current === null || current.k === 'Implicit' || current.k === 'This';
    const pipeBase = current !== null && current.k === 'Pipe' ? current : null;
    if (pipeBase) return { root: null, names, nodes, pipeBase };
    return rootedInContext ? { root, names, nodes, pipeBase: null }
      : { root: null, names: [], nodes: [], pipeBase: null };
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
            const row = propOf(cursor, name);
            if (!row) { cursor = null; break; }
            last = row;
            cursor = ownerOf(row.type_ref) ?? cursor;
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
   */
  const scopeAt = (nodeId, component) => {
    const out = new Map();
    const ancestors = [];
    let current = nodeId;
    const guard = new Set();
    while (current != null && !guard.has(current)) {
      guard.add(current);
      ancestors.unshift(current);
      current = parentOfNode.get(current) ?? null;
    }
    for (const id of ancestors) {
      const iterable = iterableAstOfNode.get(id);
      for (const name of boundNames.get(id) ?? []) {
        // A LOOP VARIABLE, THREE WAYS, in order of how directly the map proves it: the pipe's own declared
        // return type, then what the piped value holds once its wrapper is unwrapped, then - for an unpiped
        // collection - the element of the collection itself. Anything else the template binds is a name this
        // rollup cannot type, and it shadows the component either way.
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

  const walk = (node, owner, scope) => {
    if (!node || typeof node !== 'object') return;
    if (Array.isArray(node)) { for (const x of node) walk(x, owner, scope); return; }
    if (node.k === 'Read' || node.k === 'SafeRead') {
      const { root, names, nodes, pipeBase } = chainOf(node);
      // A ROOT BOUND BY THE TEMPLATE IS ALREADY THE OWNER - its name is consumed, not looked up: resolving
      // `item.label` by searching for a property called `item` ON the item's own type finds nothing.
      //
      // A PIPE IN THE MAP THAT CANNOT BE TYPED MUST REFUSE, not borrow its input's type. The fallback below
      // unwraps the pipe's INPUT and uses it for its OUTPUT, which is only right when the pipe returns an
      // element of what it was given: over half the resolved reads leaned on it and were correct by coincidence.
      // So it is kept ONLY for a pipe this map does not contain - a framework pipe like `async`, whose whole
      // job is to unwrap the value it is handed.
      const pipeName = pipeBase === null ? null : str(pipeBase.name);
      const pipedOwner = pipeName === null ? null
        : ownerOf(transformRefOfPipe.get(pipeName))
          ?? (knownPipes.has(pipeName) ? null
            : unwrappedOwnerOf(ownerRefOfChain(pipeBase?.exp, owner, scope)));
      const isBound = root !== null && scope.has(root);
      const bound = pipeBase !== null ? pipedOwner : isBound ? scope.get(root) ?? null : owner;
      const skip = pipeBase === null && isBound ? 1 : 0;
      if (names.length && bound !== null && (root !== null || pipeBase !== null)) {
        let cursor = bound;
        let resolvedAny = false;
        names.forEach((name, i) => {
          if (i < skip || cursor === null) return;
          const hop = hopOf(cursor, name);
          if (!hop) { cursor = null; return; }
          const target = nodes[i];
          if (target) {
            // COUNT WHAT WAS ADDED, not what was visited: the walk re-enters a chain through its own
            // receiver, so a per-visit counter reported far more hops than resolved nodes.
            if (target.target === undefined) { hops += 1; resolvedAny = true; }
            target.target = { name: hop.name, file: hop.file, line: hop.line, row: hop.row };
          }
          cursor = hop.owner;
        });
        if (resolvedAny) chains += 1;
      }
    }
    for (const v of Object.values(node)) walk(v, owner, scope);
  };

  for (const row of store.table('expressions')) {
    if (row.lang === 'ts') continue;
    const nodeId = row.node;
    const tplId = nodeId == null ? undefined : templateOfNode.get(nodeId);
    const classId = tplId === undefined ? undefined : classOfTemplate.get(tplId);
    if (classId === undefined || nodeId == null) continue;
    const component = { kind: 'class', id: classId };
    walk(row.ast, component, scopeAt(nodeId, component));
  }
  return { chains, hops };
}
