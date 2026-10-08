/**
 * THE SELECTOR REGISTRY, REBUILT FROM ROWS - what a run that re-extracts a fraction of the tree needs.
 *
 * The template pass matches every element against every selector-matchable declaration in the workspace.
 * That registry is built WHILE the declarations are extracted, so a run that skips unaffected files would
 * hold a fraction of it and resolve `<app-cart>` to nothing - a map that looks complete and is wrong.
 *
 * IT IS ONLY CORRECT BECAUSE THE SCOPE GUARD EXISTS. A change to any file that declares a selector, a
 * pipe, a directive or an NgModule forces a WHOLE rebuild (the dependency walk in `rows/partial/deps.rs`), so on a partial run no
 * selector has moved and last run's rows still describe the registry exactly.
 *
 * EXTRACTION ORDER IS RECOVERED FROM THE IDS, not assumed. The matcher is fed in the order declarations
 * were found, and `components` and `directives` are two tables sharing ONE `ng:` counter - so ordering the
 * union by that number is the order the walk produced them in. Fed in a different order the matcher can
 * pick a different directive for an element that two selectors both match.
 *
 * IT IS CHECKED ON EVERY FULL RUN rather than trusted: `compare` diffs this against the registry the
 * extraction built, and a mismatch is fatal. That is the whole reason this file can be relied on - the
 * other two reconstructions in this half each cost a defect that only a differential test found.
 *
 * Imports are flat: see `TsDecls/TsTypeRef.mjs`.
 */

/** The number in `ng:41`, which is the order the extraction minted it in. */
function ordinal(id) {
  const cut = String(id).indexOf(':');
  const n = Number(String(id).slice(cut + 1));
  return Number.isFinite(n) ? n : 0;
}

/**
 * `{class -> {inputs, outputs}}`, the binding property names in the order they were declared.
 *
 * NOT the member names: `@Input('aliasedName') prop` binds as the ALIAS, and `registerIo` writes exactly
 * that into `binding_name`. Reading `member` instead would make every aliased input unmatchable.
 */
function ioByClass(store) {
  const out = new Map();
  for (const row of store.table('io')) {
    if (!row.class) continue;
    let hit = out.get(row.class);
    if (!hit) { hit = { inputs: [], outputs: [] }; out.set(row.class, hit); }
    if (row.kind === 'input') hit.inputs.push(row.binding_name);
    else if (row.kind === 'output') hit.outputs.push(row.binding_name);
  }
  return out;
}

/**
 * The classes that inject a `TemplateRef`, which is what makes a directive STRUCTURAL.
 *
 * The extraction computes this while it walks the constructor and keeps it nowhere - `injectsTemplateRef`
 * lives on an in-memory object and is not a column. It IS recoverable, because the parameter it is read
 * from is a `di` row: in a real workspace every such class is findable this way.
 */
function injectsTemplateRef(store) {
  const out = new Set();
  for (const row of store.table('di')) {
    // THE RESOLVED DECLARATION, exactly as `TsClassRows` tests it - `type_ref.name` is `TemplateRef` and
    // the file it is declared in is Angular's own. A substring test on the type TEXT was tried first and
    // was wrong in BOTH directions: it accepts `Wrapper<TemplateRef>`, which injects no template, and it
    // missed the fixture's own `private tpl: TemplateRef<unknown>`. The differential check caught it on a
    // directive the real workspace does not happen to contain.
    const ref = row.type_ref;
    if (!row.class || !ref || typeof ref !== 'object') continue;
    if (ref.name === 'TemplateRef' && String(ref.file || '').includes('@angular/core')) out.add(row.class);
  }
  return out;
}

/** The registry, in extraction order, from rows alone. */
export function registryFromRows(store) {
  const io = ioByClass(store);
  const structural = injectsTemplateRef(store);
  const paths = new Map(store.table('files').map((f) => [f.id, f.path]));
  const rows = [...store.table('components'), ...store.table('directives')]
    .sort((a, b) => ordinal(a.id) - ordinal(b.id));
  return rows.map((r) => {
    const own = io.get(r.class) ?? { inputs: [], outputs: [] };
    const isComponent = r.is_component === true || r.is_component === 1;
    return {
      id: r.id,
      name: r.name,
      selector: r.selector ?? null,
      classId: r.class,
      file: paths.get(r.file) ?? null,
      is_component: isComponent,
      inputs: { hasBindingPropertyName: (n) => own.inputs.includes(n) },
      outputs: { hasBindingPropertyName: (n) => own.outputs.includes(n) },
      // THE NAMES BEHIND THE PREDICATES, carried so the two registries can be compared at all: two
      // closures are never equal, and the matcher only ever asks them questions. Nothing reads these but
      // `compare`.
      inputNames: own.inputs,
      outputNames: own.outputs,
      // CONSTANT IN THE EXTRACTION TOO, and copied rather than improved: `exportAs` IS on the row, but the
      // registry the matcher is fed has always passed null, and a reconstruction that quietly did better
      // would resolve an element the full run does not.
      exportAs: null,
      ngTemplateGuards: [],
      hasNgTemplateContextGuard: false,
      isStructural: !isComponent && structural.has(r.class),
    };
  });
}

/**
 * The two registries, compared - `null` when they agree, and the first disagreement otherwise.
 *
 * The predicates are compared by what they ANSWER, over the union of the names either side would accept:
 * two closures are never equal, and the matcher only ever asks them questions.
 */
export function compare(built, rebuilt) {
  if (built.length !== rebuilt.length) {
    return `count: extraction ${built.length}, rows ${rebuilt.length}`;
  }
  const plain = ['id', 'name', 'selector', 'classId', 'file', 'is_component', 'exportAs', 'isStructural'];
  for (let i = 0; i < built.length; i += 1) {
    const a = built[i];
    const b = rebuilt[i];
    for (const field of plain) {
      if ((a[field] ?? null) !== (b[field] ?? null)) {
        return `${a.name ?? a.id} #${i}: ${field} is ${JSON.stringify(a[field])} from the extraction `
          + `and ${JSON.stringify(b[field])} from the rows`;
      }
    }
    for (const side of ['inputNames', 'outputNames']) {
      // AS SETS, not as lists: the predicate the matcher is handed only ever tests membership, so a
      // different ORDER is the same registry and failing on it would be an alarm with nothing behind it.
      const mine = new Set(a[side] ?? []);
      const theirs = new Set(b[side] ?? []);
      const missing = [...mine].filter((n) => !theirs.has(n));
      const extra = [...theirs].filter((n) => !mine.has(n));
      if (missing.length || extra.length) {
        return `${a.name ?? a.id} #${i}: ${side} - the rows are missing ${JSON.stringify(missing)} `
          + `and invent ${JSON.stringify(extra)}`;
      }
    }
  }
  return null;
}
