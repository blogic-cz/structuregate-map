/**
 * WHICH PIPE A TEMPLATE'S `x | name` RUNS - resolved as Angular resolves it, by the SCOPE of the template's component (#86).
 *
 * `expressions.pipes` (and `template_strings.pipes`) name the pipes an expression applies, and a name is not a pipe: two
 * NgModules may each declare a `demoFormat`, and which one a template runs is decided by the module declaring its
 * component - its own declarations, and what each module it imports exports, transitively - or, for a standalone
 * component, by its own `imports`. So `pipe_refs` stands beside the name list, one `{name, ...}` per name in the same order:
 *
 *   - `id`: the `pipes` row of the one in-tree pipe of that name in scope;
 *   - `external: true` and `class`: the one LIBRARY pipe of that name in scope (`DatePipe` through `CommonModule`), read off
 *     its typings at extraction (`external_pipes`, `TsDecls/TsLibPipes.mjs`) - a library pipe has no `pipes` row to name;
 *   - `ambiguous: true` and no id: two or more pipes of that name in scope. Angular itself warns (NG0313, dev mode only)
 *     and takes the LAST one registered - an order no row records, so none is picked here;
 *   - the name alone: the scope holds no pipe of that name (Angular's compiler refuses the template, NG8004), or no scope is
 *     known (a component no NgModule declares, an orphan template).
 *
 * Every name the scope does not settle is a `diagnostics` row, once per (template, name): `pipe_unresolved_in_scope`
 * (with `reason`: `not_in_scope`, `no_module`, `no_component`) or `pipe_ambiguous_in_scope` (with `candidates`).
 *
 * OVER THE WHOLE STORE, carried rows too, on every run: a kept row's scope did not move unless a scope file's shape did,
 * which reads every file again, so a kept row is given back exactly what it holds (`Store.verifyUntouched` proves it). A
 * template expression's `pipes` is carried for that (`TsSetup/TsCarried.mjs`). After `rollupModuleScope`, whose machinery
 * it shares (`TsDerive/TsModuleScope.mjs::makeScopes`). Imports are flat: see `TsDecls/TsTypeRef.mjs`.
 */

export function rollupPipeRefs(store, diag, scopes) {
  const pipeByClass = new Map();
  for (const p of store.table('pipes')) pipeByClass.set(p.class, p);
  const components = new Map(store.table('components').map((c) => [c.id, c]));

  // ONE SCOPE PER COMPONENT, and per name what it resolves to: a template asks the same few names many times.
  const answers = new Map();
  const visibleOf = (component) => {
    if (!component) return { reason: 'no_component' };
    const standalone = scopes.standaloneByClass.get(component.class);
    let ids;
    let ext;
    if (standalone) {
      const scope = scopes.standaloneScope(standalone);
      ({ ids, ext } = scope);
    } else {
      const scope = scopes.scopeOf(scopes.moduleOfClass(component.class));
      if (!scope) return { reason: 'no_module' };
      ids = new Set([...scope.declared.ids, ...scope.imported.ids]);
      ext = new Map([...scope.declared.ext, ...scope.imported.ext]);
    }
    const byName = new Map();
    const put = (name, candidate) => {
      const list = byName.get(name);
      if (list) list.push(candidate);
      else byName.set(name, [candidate]);
    };
    for (const cid of ids) {
      const p = pipeByClass.get(cid);
      if (p && typeof p.pipe_name === 'string') put(p.pipe_name, { id: p.id, class: p.name });
    }
    for (const lib of ext.values()) put(String(lib.name), { external: true, class: lib.class, file: lib.file });
    return { byName };
  };
  const resolveIn = (component) => {
    const key = component?.id ?? null;
    if (!answers.has(key)) answers.set(key, visibleOf(component));
    return answers.get(key);
  };

  const reported = new Set();
  const report = (kind, name, row, detail) => {
    const at = `${kind}\u0000${String(row.template)}\u0000${name}`;
    if (reported.has(at)) return;
    reported.add(at);
    diag.note(kind, { name, template: row.template ?? null, component: row.component ?? null, expression: row.id, ...detail });
  };

  const refOf = (name, row) => {
    const visible = resolveIn(components.get(row.component));
    if (!visible.byName) {
      report('pipe_unresolved_in_scope', name, row, { reason: visible.reason });
      return { name };
    }
    const candidates = visible.byName.get(name) ?? [];
    if (candidates.length === 1) {
      const [one] = candidates;
      return one.external ? { name, external: true, class: one.class } : { name, id: one.id };
    }
    if (candidates.length === 0) {
      report('pipe_unresolved_in_scope', name, row, { reason: 'not_in_scope' });
      return { name };
    }
    report('pipe_ambiguous_in_scope', name, row, {
      candidates: candidates.map((c) => (c.external ? { class: c.class, file: c.file, external: true } : { id: c.id, class: c.class })),
    });
    return { name, ambiguous: true };
  };

  let resolved = 0;
  let usages = 0;
  const refsByExpression = new Map();
  for (const e of store.table('expressions')) {
    // A TYPESCRIPT EXPRESSION APPLIES NO PIPE, and its `pipes` is not carried on a partial run.
    if (e.lang === 'ts' || !Array.isArray(e.pipes) || !e.pipes.length) continue;
    e.pipe_refs = e.pipes.map((name) => refOf(String(name), e));
    refsByExpression.set(e.id, e.pipe_refs);
    usages += e.pipe_refs.length;
    resolved += e.pipe_refs.filter((r) => r.id !== undefined || r.external === true).length;
  }
  // A TEMPLATE STRING CARRIES ITS EXPRESSION'S PIPES, and so their resolution - copied, never the same objects.
  for (const s of store.table('template_strings')) {
    const refs = refsByExpression.get(s.expression);
    if (refs && Array.isArray(s.pipes) && s.pipes.length) s.pipe_refs = refs.map((r) => ({ ...r }));
  }
  return { usages, resolved };
}
