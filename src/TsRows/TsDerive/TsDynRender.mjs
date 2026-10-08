/**
 * A COMPONENT CREATED BY CODE IS A RENDER TOO.
 *
 * Ported from the original Angular extractor. `createComponent`, the factory it
 * supersedes, a CDK `ComponentPortal`, an application-level factory derived from either, a registry method's
 * `returns`, and `[ngComponentOutlet]` - each names a class that really is instantiated, and without them
 * the render graph stops at the component's creator and the target reads as dead code.
 *
 * EVERY API IS RECOGNISED BY ITS RESOLVED DECLARATION, never by its name: `createComponent` is Angular's
 * only where the checker resolved it into `@angular/core`. Imports are flat: see `TsDecls/TsTypeRef.mjs`.
 */
import { slash } from './TsPaths.mjs';

const ANGULAR_CORE = '/@angular/core/';
const CREATE_COMPONENT = 'createComponent';
const RESOLVE_FACTORY = 'resolveComponentFactory';
const OUTLET_INPUT = 'ngComponentOutlet';
/** THE CDK IS A THIRD CREATION API, and the framework's own: `new ComponentPortal(Cmp)` names the class and
 *  an overlay instantiates it. Nothing in `@angular/core` is involved, so the two shapes above cannot see
 *  it - and the miss is silent. `TemplatePortal` is deliberately absent: it carries a `TemplateRef`, not a
 *  component class, so there is no class to draw an edge to. */
const ANGULAR_CDK = '/@angular/cdk/';
const COMPONENT_PORTAL = 'ComponentPortal';

const str = (v) => (typeof v === 'string' ? v : null);
const arr = (v) => (Array.isArray(v) ? v : []);
const obj = (v) => (v !== null && typeof v === 'object' && !Array.isArray(v) ? v : null);

/** Is this evaluated call the named export of the framework, by RESOLVED declaration rather than by name? */
const isCoreCall = (v, name) => {
  const m = obj(v);
  if (m === null || m.$call === undefined) return false;
  return str(m.$target?.name) === name && String(m.$target?.file ?? '').includes(ANGULAR_CORE);
};

const emptySlots = () => ({ refs: [], params: [], calls: [] });

/**
 * WHAT THE COMPONENT POSITION IS - never what the value merely contains.
 *
 * A ternary or a `??` picks one of its branches at run time, so on every path the value IS one of them and
 * both are taken; a resolved wrapper and an `$await` carry their value one level in and are unwrapped BY
 * SHAPE. A bare identifier and a call are not dead ends either - they are the two ways an application
 * computes the class, and each is recorded as what it is so the caller can follow it exactly one HOP.
 *
 * Nothing else is descended: reaching into an object or an array would claim a class that was merely handed
 * to a constructor as the thing being rendered.
 */
function componentSlots(value, out, seen) {
  const v = obj(value);
  if (v === null || seen.has(v)) return;
  seen.add(v);
  if (typeof v.$ref === 'string') { out.refs.push({ name: v.$ref, file: v.file }); return; }
  // The deprecated factory route: the component is the argument of the RESOLVER, one call further in.
  if (isCoreCall(v, RESOLVE_FACTORY)) { componentSlots(arr(v.$args)[0], out, seen); return; }
  if (v.$cond !== undefined) { componentSlots(v.$then, out, seen); componentSlots(v.$else, out, seen); return; }
  if (v.$logic !== undefined) { for (const o of arr(v.$operands)) componentSlots(o, out, seen); return; }
  if (v.$await !== undefined) { componentSlots(v.$await, out, seen); return; }
  if (v.$call !== undefined) {
    const target = obj(v.$target);
    if (target !== null) out.calls.push({ name: target.name, file: target.file });
    return;
  }
  if (v.$expr !== undefined) {
    // An identifier in the component position names something the evaluator could not fold - a PARAMETER, if
    // the declaration around it has one by that name, which is what makes its own call sites creation sites.
    if (v.$kind === 'Identifier' && typeof v.$expr === 'string') out.params.push(v.$expr);
    return;
  }
  if (v.value !== undefined) componentSlots(v.value, out, seen);
}

/** The property an implicit read names - `[ngComponentOutlet]="cmp"`, not `svc.cmp` and not a call. */
function implicitReadName(ast) {
  const node = obj(ast);
  if (node === null || node.k !== 'Read') return null;
  const receiver = obj(node.receiver);
  if (receiver === null || (receiver.k !== 'Implicit' && receiver.k !== 'This')) return null;
  return str(node.name);
}

export function rollupDynamicRenders(store, diag) {
  const fileIdBy = new Map();
  for (const f of store.table('files')) {
    fileIdBy.set(f.path, f.id);
    fileIdBy.set(f.abs, f.id);
  }

  // THE TARGET IS RESOLVED BY FILE FIRST, and only a COMPONENT can be one. THE REF'S NAME IS THE IMPORTING
  // SIDE'S: `import { X as ShopX }` puts the ALIAS in the reference while the class keeps its own
  // name, so file+name alone missed a real dynamic render written that way.
  const componentByClass = new Map();
  const componentByFileName = new Map();
  const componentsByFile = new Map();
  const componentsByName = new Map();
  for (const c of store.table('components')) {
    componentByClass.set(c.class, c);
    componentByFileName.set(`${String(c.file)}#${String(c.name)}`, c);
    const inFile = componentsByFile.get(c.file);
    if (inFile) inFile.push(c); else componentsByFile.set(c.file, [c]);
    const named = componentsByName.get(String(c.name));
    if (named) named.push(c); else componentsByName.set(String(c.name), [c]);
  }

  const componentForRef = (ref) => {
    const name = str(ref.name);
    const file = str(ref.file);
    const fid = file === null ? undefined : fileIdBy.get(slash(file));
    if (fid !== undefined) {
      const exact = name === null ? undefined : componentByFileName.get(`${String(fid)}#${name}`);
      if (exact !== undefined) return { row: exact, byName: false };
      // Exactly one declared component in the referenced file IS the reference; two is an ambiguity the file
      // cannot settle, and it is refused rather than picked.
      const inFile = componentsByFile.get(fid);
      if (inFile?.length === 1) return { row: inFile[0], byName: false };
      // The referenced file is IN the map and declares no component: whatever the reference is, it is not
      // one, and a global name lookup would answer with an unrelated class.
      if (inFile === undefined) return null;
    }
    const named = name === null ? undefined : componentsByName.get(name);
    return named?.length === 1 ? { row: named[0], byName: true } : null;
  };

  // Every Angular registration by the class it decorates: the SOURCE of an edge may be any of them - a
  // service that creates a component is as real an origin as a component is.
  const registrationByClass = new Map();
  for (const table of ['components', 'directives', 'injectables', 'pipes', 'ng_modules']) {
    for (const row of store.table(table)) {
      if (row.class !== undefined && !registrationByClass.has(row.class)) {
        registrationByClass.set(row.class, row.id);
      }
    }
  }

  // WHICH CLASS A CALL SITE BELONGS TO. A call inside an arrow is owned by a `functions` row whose `parent`
  // chain leads back to the method that declares it - walking it keeps a `createComponent` inside a callback
  // attributed to the component it is written in, rather than dropped for having no class.
  const memberById = new Map(store.table('members').map((m) => [m.id, m]));
  const fnById = new Map(store.table('functions').map((f) => [f.id, f]));
  const ownerRow = (id) => {
    let cursor = id ?? null;
    const seen = new Set();
    while (cursor != null && !seen.has(cursor)) {
      seen.add(cursor);
      const member = memberById.get(cursor);
      if (member) return member;
      const fn = fnById.get(cursor);
      if (fn === undefined) return null;
      if (fn.parent === undefined) return fn;
      cursor = fn.parent;
    }
    return null;
  };

  // WHICH DECLARATION A `$target` NAMES. A nested `$call` inside an evaluated value carries `{name, file}`
  // and no id. File + name, and a key answering twice resolves to NEITHER: overloads and two classes in one
  // file are exactly where a name-keyed lookup binds to whichever was written last.
  const declByFileName = new Map();
  const declAmbiguous = new Set();
  for (const table of ['members', 'functions']) {
    for (const row of store.table(table)) {
      const name = str(row.name);
      if (name === null) continue;
      const key = `${String(row.file)}#${name}`;
      if (declByFileName.has(key)) declAmbiguous.add(key);
      else declByFileName.set(key, row.id);
    }
  }
  for (const key of declAmbiguous) declByFileName.delete(key);
  const declForRef = (ref) => {
    const name = str(ref.name);
    const file = str(ref.file);
    if (name === null || file === null) return null;
    const fid = fileIdBy.get(slash(file));
    return fid === undefined ? null : declByFileName.get(`${String(fid)}#${name}`) ?? null;
  };

  const returnsOf = new Map();
  for (const r of store.table('returns')) {
    if (r.member === undefined || r.value === undefined) continue;
    const list = returnsOf.get(r.member);
    if (list) list.push(r.value); else returnsOf.set(r.member, [r.value]);
  }

  const paramNames = (row) => arr(row?.params).map((p) => String(p.name ?? ''));
  const slotsOf = (value) => {
    const out = emptySlots();
    componentSlots(value, out, new Set());
    return out;
  };

  const sites = [];
  const creatorSlots = new Map();

  /** WHICH DECLARATION OWNS A FORWARDED PARAMETER. A framework call inside an arrow closes over the METHOD's
   *  parameters as easily as its own, and only a declaration a CALL SITE can name is a creator - so the
   *  owner chain is walked outward and the first declaration that really declares a parameter by that name
   *  is registered. Nothing is claimed for a name no declaration in the chain has: that is a local, and a
   *  local is where the graph stops. */
  const registerCreator = (from, names) => {
    for (const name of names) {
      let cursor = from ?? null;
      const walked = new Set();
      while (cursor != null && !walked.has(cursor)) {
        walked.add(cursor);
        const row = memberById.get(cursor) ?? fnById.get(cursor);
        if (row === undefined) break;
        if (paramNames(row).includes(name)) {
          const set = creatorSlots.get(row.id) ?? new Set();
          set.add(name);
          creatorSlots.set(row.id, set);
          break;
        }
        cursor = fnById.get(cursor)?.parent ?? null;
      }
    }
  };

  const addSite = (call, slots, via, extra = {}) => {
    const owner = ownerRow(call.member);
    const fromClass = owner?.class ?? null;
    if (fromClass == null) {
      // A call outside any class - a top-level function creating a component. There is no origin to draw
      // the edge FROM, so it is not drawn; the site is still reported, because the graph stops here too.
      diag.note('dynamic_render_no_owner',
        { call: call.id, file: owner?.file ?? null, line: call.line });
      return;
    }
    registerCreator(call.member, slots.params);
    sites.push({
      fromClass, slots, via, where: call, file: owner?.file ?? null,
      provenance: { call: call.id, ...extra },
    });
  };

  for (const call of store.table('calls')) {
    const name = str(call.target?.name);
    const file = String(call.target?.file ?? '');
    const framework = name === CREATE_COMPONENT && file.includes(ANGULAR_CORE) ? CREATE_COMPONENT
      : name === COMPONENT_PORTAL && call.new === true && file.includes(ANGULAR_CDK) ? COMPONENT_PORTAL
        : null;
    if (framework === null) continue;
    // POSITION IS THE FRAMEWORK'S SIGNATURE: all three take the component in the FIRST argument.
    // Destructured rather than indexed, so a call shaped some other way simply yields nothing.
    const [component] = arr(call.args);
    addSite(call, slotsOf(component), framework);
  }

  // EVERY CALL TO A CREATOR IS A CREATION SITE, and a creator discovered this way makes its own callers ones
  // too. Repeated until a round adds nothing; the set only grows and is bounded by the number of
  // declarations, so no iteration budget is named.
  const claimed = new Set();
  for (;;) {
    const before = sites.length;
    for (const call of store.table('calls')) {
      const creator = call.target_id;
      if (creator === undefined || claimed.has(call.id)) continue;
      const slotNames = creatorSlots.get(creator);
      if (slotNames === undefined) continue;
      // `arg_params` and `args` are produced together, one entry per argument, so reading them at the same
      // index is a ZIP. A call the labeller could not label names no position and is left alone.
      const argNames = arr(call.arg_params);
      const args = arr(call.args);
      const slots = emptySlots();
      let found = false;
      for (const [i, name] of argNames.entries()) {
        if (typeof name !== 'string' || !slotNames.has(name)) continue;
        found = true;
        componentSlots(args[i], slots, new Set());
      }
      if (!found) continue;
      claimed.add(call.id);
      addSite(call, slots, 'factory', { creator });
    }
    if (sites.length === before) break;
  }

  sites.push(...outletSites(store, componentByClass));

  /** THE CLASSES A SITE CAN CREATE - its own references, plus everything the callee in its component
   *  position RETURNS. One hop is not enough: a registry method returns the class and the helper that
   *  creates it takes it as a parameter, so the chain is followed through declarations until it repeats. */
  const candidatesOf = (slots) => {
    const out = slots.refs.map((ref) => ({ ref, returnedBy: null }));
    const seenDecls = new Set();
    const queue = slots.calls.map((ref) => declForRef(ref)).filter((d) => d != null);
    while (queue.length) {
      const decl = queue.shift();
      if (seenDecls.has(decl)) continue;
      seenDecls.add(decl);
      for (const value of returnsOf.get(decl) ?? []) {
        const inner = slotsOf(value);
        for (const ref of inner.refs) out.push({ ref, returnedBy: decl });
        for (const ref of inner.calls) {
          const next = declForRef(ref);
          if (next != null) queue.push(next);
        }
      }
    }
    return out;
  };

  let edges = 0;
  let unresolved = 0;
  for (const site of sites) {
    // EVERY SITE, ON EVERY RUN. These rows belong to no file - they are minted outside the per-file
    // walks and carry no `owner_file` - so they are rebuilt whole and the storing half does not hand
    // them back. Deriving only the re-extracted ones instead left the rest to be carried, and a
    // carried copy sat beside a fresh one naming the same edge through a different `call`: hundreds
    // of duplicate `renders`, which the path walk multiplied into several times the true render paths.
    const targets = new Map();
    for (const { ref, returnedBy } of candidatesOf(site.slots)) {
      const hit = componentForRef(ref);
      // A REFERENCE THAT IS NOT A COMPONENT IS NOT A RENDER. The evaluator publishes a `$ref` for any class,
      // enum or function, and one the map holds no `@Component` for cannot be instantiated as one.
      if (hit === null) { diag.note('dynamic_render_target_unresolved'); continue; }
      if (!targets.has(hit.row.class)) targets.set(hit.row.class, { ...hit, returnedBy });
    }
    if (!targets.size) {
      // A CREATION SITE WITH NO NAMEABLE COMPONENT IS THE FINDING, and it has to survive in the map: this is
      // a place the render graph provably stops, and a consumer reading the graph as complete has no other
      // way to learn that.
      diag.note('dynamic_render_component_not_static', { via: site.via, site: site.where.id,
        file: site.file, line: site.where.line ?? null,
        // THE CALLEE, as the call row spells it - `container.createComponent`, not `createComponent` and
        // not the mechanism. `via` already names the mechanism; repeating it here would lose the one
        // thing that says WHERE the graph stopped.
        source: site.where.callee ?? site.where.source ?? null });
      unresolved += 1;
      continue;
    }
    for (const [cls, { row: component, byName, returnedBy }] of targets) {
      store.add('renders', 'rd', {
        from_component: registrationByClass.get(site.fromClass) ?? null, from_class: site.fromClass,
        to: component.id, to_class: cls, to_name: component.name, kind: 'component',
        // The last-resort resolution, marked on the row exactly as module scope marks its own: an edge
        // decided by a name that happened to be unique is a weaker fact than one the file proved.
        ...(byName ? { target_by_name: true } : {}),
        // WHERE THE CLASS WAS WRITTEN, when it was not written at the creation site. Without it an edge into
        // dozens of components from one line is unauditable.
        ...(returnedBy === null ? {} : { returned_by: returnedBy }),
        // An outlet edge still happens AT a node and keeps it; a call has neither. No element MATCHED in
        // either case, so `tag` is empty rather than borrowed from the container the outlet sits on.
        template: site.where.template ?? null,
        node: site.where.node ?? null, tag: null,
        via: site.via, ...site.provenance,
        line: site.where.line ?? null, col: site.where.col ?? null,
        end_line: site.where.end_line ?? null,
      });
      edges += 1;
    }
  }
  return { edges, sites: sites.length, unresolved };
}

/**
 * `[ngComponentOutlet]="cmp"` - the template half, resolved one hop and only one.
 *
 * The binding's expression reads a property of the component class, and what that property HOLDS is what
 * gets rendered: its initializer, or any value assigned to it anywhere in the class. EVERY candidate is
 * published, because an outlet property written from three places really can render three components and
 * choosing one would be a guess about runtime state.
 */
function outletSites(store, componentByClass) {
  const bindings = store.table('bindings').filter((b) => b.name === OUTLET_INPUT);
  if (!bindings.length) return [];
  const classOfComponent = new Map();
  const fileOfComponent = new Map();
  for (const [cls, component] of componentByClass) {
    classOfComponent.set(component.id, cls);
    fileOfComponent.set(component.id, component.file);
  }
  const astOfBinding = new Map();
  for (const x of store.table('expressions')) if (x.binding != null) astOfBinding.set(x.binding, x.ast);
  const membersOfClass = new Map();
  for (const m of store.table('members')) membersOfClass.set(`${String(m.class)}#${String(m.name)}`, m);
  const assignedTo = new Map();
  for (const a of store.table('assignments')) {
    if (a.target_id === undefined) continue;
    const list = assignedTo.get(a.target_id);
    if (list) list.push(a.value); else assignedTo.set(a.target_id, [a.value]);
  }

  const sites = [];
  for (const b of bindings) {
    // The binding's own component IS the origin - a template belongs to exactly one, so there is no owner
    // chain to walk here and nothing to resolve by name.
    const fromClass = classOfComponent.get(b.component) ?? null;
    if (fromClass == null) continue;
    const name = implicitReadName(astOfBinding.get(b.id));
    const member = name === null ? undefined : membersOfClass.get(`${String(fromClass)}#${name}`);
    const slots = emptySlots();
    if (member !== undefined) {
      componentSlots(member.value, slots, new Set());
      for (const value of assignedTo.get(member.id) ?? []) componentSlots(value, slots, new Set());
    }
    sites.push({
      fromClass, slots, via: OUTLET_INPUT, where: b,
      file: fileOfComponent.get(b.component) ?? null, provenance: { binding: b.id },
    });
  }
  return sites;
}
