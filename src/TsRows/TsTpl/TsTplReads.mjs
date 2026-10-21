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

import { makeOwners } from './TsTplOwners.mjs';

const str = (v) => (typeof v === 'string' ? v : null);

export function rollupTemplateReads(store) {
  const { ownerOf, propByOwner, indexByShape, resolveRef, elementOwnerOf, ownerOfRow, hopOf } = makeOwners(store);

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
    else if (Array.isArray(transform?.return_refs)) transformOwnersOfPipe.set(declared, ownerOfRow({ type_refs: transform.return_refs, type_refs_of: transform.return_refs_of }));
  }

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

  /**
   * WHAT AN `as` ALIAS AND A LOOP VARIABLE ARE: the owner of the VALUE of the condition or the collection, as `valueOf` reads it (rows 72) - a
   * read chain, a call (`svc.list()`), a keyed read, a pipe, `a ?? b`. They went through a chain walk of their own that took the type of the FIRST
   * name it met anywhere in the expression, so a call typed its variable with an argument's collection and an un-annotated method typed nothing.
   * A condition that tests something (`a && b`, `x.ok === true`) is no value and types nothing: the alias still shadows the component.
   */
  const conditionOf = (ast) => {
    let at = ast;
    while (at && (at.k === 'Source' || at.k === 'NonNull')) at = at.k === 'Source' ? at.ast : at.expr;
    return at ?? null;
  };
  const aliasOwnerOf = (ast, component, scope) => valueOf(conditionOf(ast), component, scope)?.owner ?? null;
  /** A LOOP VARIABLE is the ELEMENT of the collection. A pipe hands on its own: an in-tree one's `transform` return, whose element it is; a framework
   *  one's (`async`, `slice`) UNWRAPPED innermost type, which already IS the element (`Observable<Foo[]>` -> `Foo`); one the map cannot type, none. */
  const loopOwnerOf = (ast, component, scope) => {
    const at = conditionOf(ast);
    const held = at ? valueOf(at, component, scope) : null;
    if (held === null) return null;
    if (at.k !== 'Pipe') return elementOwnerOf(held.ref, held.env);
    const name = str(at.name);
    if (name !== null && transformRefOfPipe.has(name)) return elementOwnerOf(held.ref, held.env);
    return name === null || knownPipes.has(name) ? null : held.owner;
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
        // A LOOP VARIABLE is the element of its collection (`loopOwnerOf`). Anything else the template binds (an alias is above) is a name this
        // rollup cannot type, and it shadows the component either way.
        const bound = implicitAt.get(id) !== name || iterable === undefined ? null : loopOwnerOf(iterable, component, out);
        out.set(name, bound);
      }
    }
    return out;
  };

  let chains = 0;
  let hops = 0;

  // SEVERAL OWNERS AT ONCE (`any`) have no id: two of them are never one owner.
  const sameOwner = (a, b) => (a?.owner && b?.owner && a.owner.kind !== 'any' && a.owner.kind === b.owner.kind && a.owner.id === b.owner.id ? a : null);

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
        // A VALUE KEEPS ITS REF WITH NO OWNER (rows 72): an array's owner is `Array`, which no row declares, but its `element` is what a loop over
        // `list()[0].items` takes - a value with a ref and no owner ends a chain and still types a loop.
        return ref === null ? null : { owner: resolveRef(ref, held?.env ?? null), ref, row: null, env: null };
      }
      case 'Call': case 'SafeCall': {
        const callee = valueOf(node.receiver, component, scope);
        const ref = callee?.row?.return_ref ?? null;
        // A METHOD RETURNING SEVERAL TYPES (`return_refs`) is the `any` owner of them, as a member typed by several is.
        if (ref === null && Array.isArray(callee?.row?.return_refs)) return { owner: ownerOfRow({ type_refs: callee.row.return_refs, type_refs_of: callee.row.return_refs_of }), ref: null, row: null, env: null };
        return ref === null ? null : { owner: resolveRef(ref, callee.env), ref, row: null, env: null };
      }
      case 'Pipe': {
        const name = str(node.name);
        const declared = name === null ? undefined : transformRefOfPipe.get(name);
        // A PIPE IN THE MAP THAT CANNOT BE TYPED MUST REFUSE, not borrow its input's type (only a pipe this map lacks unwraps).
        const exp = declared === undefined && name !== null && !knownPipes.has(name) ? valueOf(node.exp, component, scope) : null;
        const ref = declared ?? exp?.ref?.unwrapped ?? null;
        const several = ref === null && name !== null ? transformOwnersOfPipe.get(name) ?? null : null;
        if (several !== null) return { owner: several, ref: null, row: null, env: null };
        return ref === null ? null : { owner: resolveRef(ref, declared ? null : exp?.env ?? null), ref, row: null, env: null };
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
