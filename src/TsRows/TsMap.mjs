/**
 * THE TYPESCRIPT HALF OF THE DEEP MAP - the node end, which PARSES and never stores.
 *
 *   node TsMap.mjs --root <tree> --rows <payload.json> [--state <state.json>] [--node-modules <dir>]
 *                  [--config <structuregate.ts.json>]
 *
 * Why the half is split here: `better-sqlite3` is a native addon, so a node that could write the database
 * would end the property every consumer of this exe depends on - two files, one of them a single NativeAOT
 * binary. The rows therefore leave as a payload FILE (tens of megabytes on a real workspace, which is more
 * than a pipe or a command line carries) and the rust store writes them into the database.
 *
 * WHAT IT IS MEASURED BY: every row the predecessor map published has exactly one row here carrying
 * the same facts, and this map publishes no row that one did not. A table is not done because its rows
 * exist, it is done because a row-by-row comparison of the two finds no difference.
 *
 * Protocol out, one record per line, the same shape the other halves speak:
 *   MAP-TOOLCHAIN|typescript <v>|@angular/compiler <v>|<tier>
 *   MAP-NOTE|<text>            something worth knowing that is not a failure
 *   MAP-SKIP|<reason>          there is no Angular workspace here, so there is nothing to map
 *   MAP-ROWS|<table>|<n>       one per table put in the payload
 *   MAP-FATAL|<message>        this half cannot run at all
 *   MAP-DONE|<files>           LAST line; its absence means the half died half way
 */
import { readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';

import { canonicalPath, relativeTo } from './TsPaths.mjs';
import { directories, loadConfig, names } from './TsConfig.mjs';
import { compare as compareRegistry, registryFromRows } from './TsRegistry.mjs';
import { rollupGateChains } from './TsGateChain.mjs';
import { rollupTemplateReads } from './TsTplReads.mjs';
import { collectLocales, rollupTranslationCoverage } from './TsLocales.mjs';
import { rollupGateIndex, rollupI18nIndex, rollupRenderGraph, rollupSelectors } from './TsIndexes.mjs';
import { rollupDynamicRenders } from './TsDynRender.mjs';
import { rollupRoutes } from './TsRoutes.mjs';
import { rollupRouteTree } from './TsRouteTree.mjs';
import { rollupInputUsage } from './TsInputUsage.mjs';
import { rollupModuleScope } from './TsModuleScope.mjs';
import { rollupStateGraph } from './TsNgrx.mjs';
import { deriveJoins } from './TsSpecJoins.mjs';
import { writeHtmlMirror } from './TsHtml.mjs';
import { deriveAnchors } from './TsSpecAnchors.mjs';
import { rollupComponentReach } from './TsReach.mjs';
import { rollupI18nCalls } from './TsI18nCalls.mjs';
import { rollupCallParams, rollupRefIds } from './TsRefs.mjs';
import { cutShort, emptyChanged, repointProjects, startRereads } from './TsReads.mjs';
import { shapeOfText } from './TsShape.mjs';
import { affectedFiles, fingerprint, inventory, knownLines, markParsed, plannedHashes } from './TsInventory.mjs';
import { discoverProjects, findWorkspaceRoot } from './TsProjects.mjs';
import { loadToolchain, resolveToolchain, versionMismatch } from './TsResolve.mjs';
import { Diagnostics, Store } from './TsStore.mjs';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { emit, parseArgs, readCarry, readJson, writePayload } from './TsRunIo.mjs';
import { markReachable, runTemplatePass, walkProjects } from './TsWalk.mjs';
import { forgetParses } from './TsProgram.mjs';

/**
 * The commit the mapped tree is at, or `{ available: false }`.
 *
 * NO GIT IS A SUPPORTED STATE, not an error: this exe is deployed into trees that are checkouts and into
 * ones that are not, and a map that refused to be written without a repository would be useless in half of
 * them. Every field is read with one call each, and a call that fails simply leaves the record absent.
 */
function revisionOf(tree) {
  const git = (...args) => execFileSync('git', ['-C', tree, ...args], {
    encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'],
  }).trim();
  try {
    const commit = git('rev-parse', 'HEAD');
    return {
      available: true,
      commit,
      short: git('rev-parse', '--short=11', 'HEAD'),
      branch: git('rev-parse', '--abbrev-ref', 'HEAD'),
      committed_at: git('show', '-s', '--format=%cI', 'HEAD'),
      dirty: git('status', '--porcelain').length > 0,
    };
  } catch {
    return { available: false };
  }
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  if (!args.root || !args.rows) {
    emit('MAP-FATAL', 'both --root and --rows are required');
    return 1;
  }

  // THE WORKSPACE, NOT THE ROOT. structuregate is pointed at a repository; an Angular workspace is the
  // directory holding `angular.json` / `nx.json`, which in a real repository is several levels down. A
  // half that mapped the root would build one program out of every project at once.
  const fe = args.fe || findWorkspaceRoot(args.root);
  if (!fe) {
    // NOT A FAILURE. Every other repository this exe is deployed into has no Angular workspace in it, and
    // a half that errored there would make the map unusable everywhere it is not needed.
    emit('MAP-SKIP', `no Angular workspace (angular.json / nx.json / workspace.json) at or under ${args.root}`);
    return 0;
  }

  const state = readJson(args.state, {});
  const found = resolveToolchain(fe, args.nodeModules);
  if (!found.nodeModules) {
    emit('MAP-FATAL', 'no node_modules holding typescript + @angular/compiler. Tried: '
      + found.tried.join(', ') + ' - install the workspace dependencies, or pass --ts-node-modules '
      + '<dir> naming a checkout that has them');
    return 1;
  }

  let toolchain;
  try {
    toolchain = await loadToolchain(found.nodeModules);
  } catch (error) {
    emit('MAP-FATAL', `the workspace's typescript + @angular/compiler could not be loaded - ${error.message}`);
    return 1;
  }
  const { ts, ng, tsVersion, ngVersion } = toolchain;
  // THE COMPILER'S OWN CONFIG PARSER, OR NOTHING. A tsconfig and an angular.json are JSONC - Angular's own
  // ship with `//` comments - so `JSON.parse` throws on them and this half reads them with the parser the
  // compiler exposes. TypeScript 7, the native port, does not expose it: discovery then read every config
  // as unparseable and reported `no project was discovered` about a workspace that plainly has one. A
  // missing API is said out loud, because the alternative is a map that is empty for a reason nobody can see.
  if (typeof ts.parseConfigFileTextToJson !== 'function') {
    emit('MAP-FATAL', `typescript ${tsVersion} does not expose parseConfigFileTextToJson, which is how a `
      + 'JSONC tsconfig and angular.json are read - this half cannot discover projects with it');
    return 1;
  }
  emit('MAP-TOOLCHAIN', `typescript ${tsVersion}`, `@angular/compiler ${ngVersion}`, found.tier);
  // A MAJOR MISMATCH IS ANNOUNCED, not discovered later from a missing gate: Angular's own parser
  // error-recovers, so a 17.x compiler reading an 18.x template records `@let` as text and says nothing.
  const warning = versionMismatch(fe, ngVersion);
  if (warning) emit('MAP-NOTE', `the typescript half ${warning}`);

  // READ BEFORE ANYTHING IS WALKED, because the early exit below has to know it: a changed `gateInputs`
  // or `i18nCalls` list is a different map over the same bytes, and a run that skipped on the hashes
  // alone would publish the OLD answer for a new question.
  let config;
  try {
    config = loadConfig(args.config, fe);
  } catch (error) {
    emit('MAP-FATAL', error.message);
    return 1;
  }
  if (config.file) emit('MAP-NOTE', `the typescript half read ${config.file}`);

  // NOTHING CHANGED, NOTHING TO DO. This half cannot be incremental per FILE - its rows are semantic, so
  // renaming a component changes the resolved rows of files that did not change, and a per-file cache
  // would go stale in silence. It CAN refuse to work when the answer is already right: the tree's hashes
  // against the ones the database recorded, and the setup that produced them against the one recorded
  // beside those. A run that would rewrite nearly a million identical rows is minutes spent to change nothing.
  //
  // `rows` IS WHAT THIS EXTRACTOR WRITES, and it is part of the setup for the same reason the compiler is:
  // a new column over unchanged source is a different map over the same bytes. Without it a deployed exe
  // that adds one reads a consumer's tree as unchanged and keeps the old rows for ever. Raise it whenever
  // a row gains or loses what it carries. 2: `returns.expression`. 3: `locals.expression`.
  // 4: `branches.condition_expr`, `switch_cases.discriminant_expr`/`label_exprs`, a bare name's `target`.
  // 5: `assignments.expression`. 6: a template literal's `$expr_id`. 7: `import_names`, `exports.resolved`.
  // 8: `files.reads`, `files.surface`. 9: a TypeScript expression's `col`. 10: `files.shape`. 11: `regexes`.
  // 12: `handlers`, `raises`, a `try` row in `branches`. 13: `gate_values.listed_json`, and a conditional
  // config read per branch - derived in rust, but over rows an unchanged tree would otherwise keep.
  // 14: an arrow-function property's or `const`'s concise body is an implicit `returns` row keyed on its
  // member or function, and a function-valued property carries `members.params`. 15: no row
  // changes - `file_text` is read from the workspace, not the root, and an unchanged tree never wrote it.
  // 16: a structural directive the template hands its constant to is a `gate_values` row on the member it is
  // compared with - derived in rust, over rows an unchanged tree would otherwise keep.
  // 17: `stylesheets`, `style_rules`, `class_hides` - derived in rust (`cssmap/`) after these rows are stored.
  // 18: `file` beside `owner_file` on the tables whose rows are always one `.ts` file's source.
  // 19: `gate_values` reads an OR as the union of its sides, a call as its callee's one return, a flag written
  // before the first render (or initialised and never written) both ways, and a LIST a template hands to a
  // structural directive - derived in rust, over rows an unchanged tree would otherwise keep.
  // 20: `choices` on a statement row inside a conditional's arm; `key_reach` never folds to `in []`;
  // a directive's list tested through a method parameter in a callback and an `@Input` flag read
  // through every binding of it - the last three derived in rust.
  // 21: `key_reach` publishes `in []` again where no way permits a member - it renders for nobody; a
  // directive renders through a `this.` call made inside a callback.
  const setup = createHash('sha256').update(JSON.stringify({
    typescript: tsVersion, angular: ngVersion, node: process.version, tier: found.tier,
    config: config.file, data: config.data, rows: 24,
  })).digest('hex');
  // THE DATABASE BEING WRITTEN IS NOT SOURCE. Its `-wal` and `-journal` siblings are not either, nor the
  // last run's trace beside it; all change because of the run, so a tree holding them could never read as unchanged.
  const skipFiles = new Set(args.db
    ? ['', '-wal', '-shm', '-journal', '.last-run.jsonl'].map((suffix) => canonicalPath(args.db + suffix))
    : []);

  // ONLY THE FILES THAT CARRY A HASH. A few of a workspace's `files` rows are program files the
  // compiler reached OUTSIDE the tree - a `.d.ts` in the borrowed node_modules - and the walk that
  // produces hashes does not go there, so they are recorded with none. Counting them made the comparison
  // a few rows off and the exit never fired.
  const recorded = Object.entries(state.shas ?? {}).filter(([, hash]) => hash);
  // A DIFFERENT SETUP IS A DIFFERENT MAP OVER THE SAME BYTES, so the recorded hashes say nothing about it
  // and every file has to be read again. The hashes are still walked when a PLAN was asked for, because
  // the caller wants an answer either way.
  const sameSetup = recorded.length > 0 && state.setup === setup;
  let planned = null;
  let why = null;
  let now = new Map();
  if (sameSetup || args.plan) {
    // THE PLAN RUN HAS JUST HASHED THIS TREE, and the rows handed back were chosen by ITS answer - so the
    // same answer is the right one here too, and a second walk over every file bought nothing.
    now = (!args.plan && plannedHashes(args.hashes))
      || await fingerprint({ root: fe, feRoot: fe, skipDirs: new Set(args.skipDirs), skipFiles });
    const was = new Map(recorded);
    // BOTH DIRECTIONS ARE A CHANGE: a file whose hash moved or that the database never saw, and a file the
    // database holds that the tree no longer has. Comparing only the first left a deleted file's rows in
    // the map for ever.
    const changed = [];
    for (const [rel, hash] of now) if (was.get(rel) !== hash) changed.push(rel);
    for (const [rel] of recorded) if (!now.has(rel)) changed.push(rel);
    if (sameSetup && !changed.length) {
      emit('MAP-UNCHANGED', now.size);
      if (args.plan) writeFileSync(args.plan, JSON.stringify({ unchanged: true }), 'utf8');
      return 0;
    }
    // THE GUARD THE GRAPH CANNOT BE: its edges say which templates render a component TODAY, so they are
    // valid only while the changed files keep resolving the same way. A changed SELECTOR, or an NgModule's
    // declarations, can make a template match something it never matched - an edge describing a fact that
    // has not happened yet. A file the database has never seen can declare one too.
    //
    // So a change to any file that DECLARES something a template names rebuilds everything - unless what
    // the file declares did not move: its SHAPE (`TsSetup/TsShape.mjs`, every token outside a function body)
    // is the one recorded. One-sided, as the surface is: node hashed it then and hashes it now, so nothing in
    // another language has to agree. A file the database has never seen, or recorded with no shape, still
    // reads everything.
    const scope = new Set(state.scope ?? []);
    const shapes = state.shapes ?? {};
    const shapeNow = (rel) => {
      try {
        const abs = path.resolve(fe, rel);
        return shapeOfText(ts, abs, readFileSync(abs, 'utf8'));
      } catch {
        return null;
      }
    };
    const moved = changed.filter((rel) => !was.has(rel)
      || (scope.has(rel) && (!shapes[rel] || shapeNow(rel) !== shapes[rel])));
    // WHY THE HALF RUNS, said by the caller: how many files moved, by extension, and whether the setup did.
    why = { setup_moved: recorded.length > 0 && !sameSetup, recorded: recorded.length, changed: changed.length, template: moved.length, exts: {} };
    for (const rel of changed) why.exts[path.extname(rel) || rel] = (why.exts[path.extname(rel) || rel] ?? 0) + 1;
    // WHAT THE PLAN CAN SAY WITHOUT A PROGRAM: the changed files, and the template or component that is read
    // with each. Which of their READERS go too is decided by the parse run, which has the programs to tell
    // whether what a file shows moved - see `TsSetup/TsReads.mjs`. ONLY OVER PROVED READS: until a full run
    // has found every file its rows name among them, every hop of dependents is read, as before.
    const cut = Boolean(state.reads_proven) && !args.everyHop;
    const graph = cut ? state.coupled : state.deps;
    planned = (sameSetup && !moved.length)
      ? { changed: changed.sort(), affected: [...affectedFiles(graph, changed)].sort(), cut }
      : null;
    if (sameSetup && moved.length) {
      emit('MAP-NOTE', `the typescript half is reading everything: ${moved.length} changed file(s) can `
        + 'move what a template resolves');
    }
  }

  // THE PLAN IS ALL THAT WAS ASKED FOR. It costs a hash walk and no program, so the caller can decide what
  // to do - and tell the storing half which files to dump back - before anything pays for a type checker.
  if (args.plan) {
    writeFileSync(args.plan, JSON.stringify({
      unchanged: false,
      full: planned === null,
      changed: planned ? planned.changed : [],
      affected: planned ? planned.affected : [],
      hashes: Object.fromEntries(now),
      why,
    }), 'utf8');
    emit('MAP-PLAN', planned ? planned.changed.length : -1, planned ? planned.affected.length : -1);
    return 0;
  }

  // A PARTIAL RUN NEEDS BOTH: a plan that says a fraction is enough, and the rest of the rows to
  // put back. Either alone is a full run, and saying so here is what keeps the two from disagreeing.
  // WHICH TABLES A ROLLUP OWNS, as the last FULL run measured them. A partial run cannot work it
  // out - its per-file rows arrive already made - so without it there is nothing to do but read
  // everything, which is the answer that cannot be wrong.
  const rebuilt = new Set(state.rebuilt ?? []);
  const partial = planned !== null && Boolean(args.carry) && rebuilt.size > 0;
  if (planned !== null && Boolean(args.carry) && !rebuilt.size) {
    emit('MAP-NOTE', 'the typescript half is reading everything: the map does not say which tables '
      + 'are rebuilt from scratch, which only a full run can measure');
  }
  if (planned !== null && !args.carry) {
    emit('MAP-NOTE', 'the typescript half is reading everything: nothing handed back the rows it '
      + 'would not have re-extracted');
  }

  // THE IDS RESTART WHEN THIS HALF IS THE ONLY ONE IN THE DATABASE - the rust store decides, and says why and
  // why it is a question about the database rather than a constant. A PARTIAL RUN NEVER RESTARTS THEM:
  // the rows it is about to load already carry ids, and numbering from 1 again would hand a new row an
  // id one of them points at.
  const store = new Store(partial ? (state.counters ?? {}) : (state.alone ? {} : (state.counters ?? {})));
  const affected = partial ? new Set(planned.affected) : null;
  let repointLater = null;
  if (partial) {
    const carried = readCarry(args.carry);
    if (!carried || typeof carried.tables !== 'object' || carried.tables === null) {
      // NOT A FULL RUN INSTEAD. The caller asked for a partial one and this half was going to
      // extract a fraction; falling back silently would write that fraction over a whole map.
      emit('MAP-FATAL', `the rows handed back could not be read from ${args.carry}`);
      return 1;
    }
    // A CELL THAT HELD A STRUCTURE COMES BACK AS TEXT, and has to be turned back before a pass reads
    // it. SQLite has no array and no object, the carrying side passes those cells through WITHOUT
    // decoding them - which is what makes it cheap - and a reader cannot tell one from a string that
    // happens to start with a bracket. So the map says which columns they are: `json_columns`, written
    // by the run that stored them and handed back in the state. Guessing instead is the mistake that
    // published `enums.members` as text and made every member read as one character.
    const structured = state.json_columns ?? {};
    const booleans = state.bool_columns ?? {};
    // THE IDS THE RE-EXTRACTED FILES' ROWS ALREADY HAVE. Handed over beside the rows, because the
    // rows themselves are the ones being replaced and these are all that is kept of them.
    const reclaimable = store.loadClaims(carried.claims, state.identity ?? {});
    let heldProjects = [];
    let loaded = 0;
    let skipped = 0;
    for (const [name, rows] of Object.entries(carried.tables)) {
      if (!Array.isArray(rows)) continue;
      // A TABLE A ROLLUP BUILDS FROM SCRATCH IS NOT PUT BACK - see `Store.rebuiltTables`. The pass
      // that owns it runs again in full, so a carried row would sit beside a fresh copy of itself.
      if (name === 'projects') { heldProjects = rows; skipped += rows.length; continue; }
      if (rebuilt.has(name)) { skipped += rows.length; continue; }
      const columns = structured[name] ?? [];
      const flags = booleans[name] ?? [];
      for (const row of rows) {
        for (const column of columns) {
          if (typeof row[column] !== 'string') continue;
          try { row[column] = JSON.parse(row[column]); } catch { /* left as the text it is */ }
        }
        // A BOOLEAN COMES BACK AN INTEGER, and a pass that asks `=== true` of it gets false. The
        // column is turned back the same way a structured one is, off the same measurement.
        for (const column of flags) {
          if (row[column] === 0 || row[column] === 1) row[column] = row[column] === 1;
        }
      }
      // `files` is the one INTERNED table, keyed by the absolute path - see `makeFileId`.
      store.load(name, rows, name === 'files' ? (row) => row.abs : null);
      loaded += rows.length;
    }
    emptyChanged(store, planned.changed);
    repointLater = heldProjects;
    emit('MAP-NOTE', `the typescript half put back ${loaded} row(s) it is not re-extracting and `
      + `left ${skipped} to be derived again, and is reading ${affected.size} file(s) of ${now.size}`);
    if (reclaimable) {
      emit('MAP-NOTE', `the typescript half can keep ${reclaimable} id(s) of the rows it re-extracts, `
        + 'so nothing outside those files has to be re-pointed');
    }
  }
  const diag = new Diagnostics(() => store.currentFile);
  const projects = discoverProjects(ts, fe);
  if (!projects.length) {
    emit('MAP-FATAL', `no project was discovered in ${fe} - neither project.json, nor angular.json, `
      + 'nor a root tsconfig naming its own sources');
    return 1;
  }

  // WHICH FILES THIS RUN READS AGAIN beyond the ones that changed - see `TsSetup/TsReads.mjs`.
  const rereads = (partial && planned.cut) ? startRereads({ ts, fe, store, state, planned, now, only: affected }) : null;
  const { inAnyProgram, registry, components } = walkProjects({
    ts, store, fe, nodeModules: found.nodeModules, projects, diag, noted: new Set(),
    // WHICH FILES THIS RUN OPENS. Every other file in the program is still WALKED for its ids and
    // its program membership - a declaration in it is what the ones being re-extracted resolve
    // against - but nothing is extracted from it, because its rows are already loaded.
    only: affected,
    rereads,
  });
  if (repointLater) repointProjects(store, repointLater);
  forgetParses();
  if (rereads) {
    const late = cutShort(store, rereads, null, true);
    if (late) { emit('MAP-RETRY', late); return 0; }
    emit('MAP-NOTE', `the typescript half read ${affected.size} file(s) again for ${planned.changed.length} `
      + `changed: ${rereads.checked.size} surface(s) looked at, ${rereads.moved.size} moved`);
  }

  // THE REGISTRY REBUILT FROM ROWS IS CHECKED AGAINST THE ONE THE EXTRACTION BUILT, on every full run.
  //
  // A run that re-extracts only the files that changed cannot build this registry - the declarations it
  // skipped are exactly what the matcher needs - so it has to be recovered from the rows the last run
  // stored (`TsTpl/TsRegistry.mjs`). That is a THIRD reconstruction in this half, and the first two each
  // shipped a defect that only a differential test caught: the closure's `localeCompare` ordering and its
  // truthy empty `Set`. So this one is not trusted, it is PROVED - every full run computes both and
  // refuses to continue if they differ, which means the partial path can never drift away from the full
  // one unnoticed. A few thousand entries; the comparison costs nothing.
  // A PARTIAL RUN CANNOT BUILD THIS REGISTRY - the declarations it skipped are exactly what the
  // matcher needs - so it is recovered from the rows the last run stored, which are loaded above. That
  // reconstruction is not trusted, it is PROVED: every FULL run computes both and refuses to continue
  // if they differ, so the partial path cannot drift away from the full one unnoticed.
  const matching = partial ? registryFromRows(store) : registry;
  if (!partial) {
    const drift = compareRegistry(registry, registryFromRows(store));
    if (drift !== null) {
      emit('MAP-FATAL', `the registry rebuilt from rows does not match the one extracted - ${drift}`);
      return 1;
    }
  }

  // ---------------------------------------------------------------- PASS 2: the templates
  const templatePass = runTemplatePass({
    ng, store, diag, fe, registry: matching, components, config,
    // A COMPONENT THIS RUN IS NOT RE-EXTRACTING ALREADY HAS ITS TEMPLATE'S ROWS, so following the
    // render queue into it would parse the same template twice and store both copies.
    expand: !partial,
  });
  emit('MAP-NOTE', `the typescript half parsed ${templatePass.parsed} template(s)`);

  // A TEMPLATE READ BECOMES A DECLARATION. After every template is parsed, because a read can land on a
  // member of a component whose own template has not been reached yet, and after the declarations exist.
  // WHAT HAS TO HOLD FOR A NODE TO RENDER AT ALL, outermost first - a gate row says what one binding
  // decides, and this says what the node sits under.
  const chains = rollupGateChains(store);
  emit('MAP-NOTE', `the typescript half put ${chains.nodes} node(s) under a gate `
    + `(deepest chain ${chains.deepest})`);

  const reads = rollupTemplateReads(store);
  emit('MAP-NOTE', `the typescript half resolved ${reads.hops} template read(s) in ${reads.chains} chain(s)`);

  // A TEMPLATE OUTSIDE THE INVENTORY ROOT is never walked, so its file row would keep no size at all - and
  // `chars` is set by NOTHING else. The parsed template already knows: copy it back before the walk, so the
  // walk's own `??=` cannot overwrite it.
  const sizesFromTemplates = () => {
    const fileById = new Map(store.table('files').map((f) => [f.id, f]));
    for (const t of store.table('templates')) {
      const row = t.file ? fileById.get(t.file) : undefined;
      if (!row || t.inline) continue;
      row.lines ??= t.lines;
      row.chars ??= t.chars;
      // TIER TOO, not only the sizes: `tier` is otherwise set by the .ts extractors and the inventory
      // walk alone, so a template pulled in from outside that dir ended up with sizes and no tier at
      // all.
      row.tier ??= 'core';
    }
  };
  sizesFromTemplates();

  // EVERY FILE IN SCOPE GETS A ROW, parsed or not - a .scss, a fixture .json, an .html nobody references.
  // A consumer asking "is that all of it?" cannot otherwise tell an absent file from an unparsed one.
  const inv = await inventory({ store, root: fe, feRoot: fe, skipDirs: new Set(args.skipDirs), skipFiles,
    now, known: knownLines(args.lines) });
  const parsedAbs = new Set();
  for (const f of store.table('files')) {
    if (f.tier === 'core' || f.tier === 'boundary') parsedAbs.add(f.abs);
  }
  for (const t of store.table('templates')) {
    if (t.path) parsedAbs.add(canonicalPath(path.resolve(fe, t.path)));
  }
  markParsed(store, parsedAbs);
  // AN .html IN SCOPE THAT NO COMPONENT POINTS AT is still part of the frontend. Parsed as an ORPHAN
  // template rather than left as a file row with no content behind it.
  let orphans = 0;
  for (const f of store.table('files')) {
    if (f.ext !== 'html' || f.parsed) continue;
    // ITS ROWS ARE ITS OWN FILE'S - the orphan IS the template, so there is no component to borrow from.
    store.enterFile(f.id);
    const res = templatePass.extractor.extract({
      id: null, class: null, name: '(orphan template)', file: f.id, file_path: f.path,
      template_abs: f.abs, template_file: f.id, inline_template: null, is_component: false,
    });
    store.enterFile(null);
    if (res) { f.parsed = true; orphans += 1; }
  }
  if (orphans) emit('MAP-NOTE', `the typescript half parsed ${orphans} orphan template(s)`);
  // AND NOT AGAIN FOR THE ORPHANS, though it is tempting. They are parsed after the inventory, so the
  // pass above ran before their `templates` rows existed and a few file rows keep no `chars` at all. The
  // tool this map reproduces leaves them empty too, and filling them moved those `files` rows away from
  // it. A partial run has those template rows handed back before it starts, so it fills what a full
  // run leaves empty - which is the difference the partial-run comparison records, not a gap to close here.
  markReachable(store, inAnyProgram, diag);

  // THE TRANSLATION FILES, LAST. They are inventory rows until something opens them, and they are opened
  // after reachability precisely so a locale file keeps the `reachable: null` that a .json deserves.
  // THE ROUTES, before the refs rollup: their `component`, guards and lazy targets are references that pass
  // resolves, and `moduleRoots` is what closes the tree across a lazy boundary once they are ids.
  const moduleRoots = rollupRoutes(store);

  // A REFERENCE BECOMES A ROW ID, once every declaration exists: `di.resolved`, `calls.target`,
  // `assignments.target_ref` and `imports.resolved` all name a declaration by file and name until this runs.
  // AFTER the inventory, because an import that resolved into a file only the walk knows about could not be
  // joined before that file had a row.
  rollupRefIds(store);
  // ...and then which PARAMETER each argument fills, which needs the call targets the pass above resolved.
  rollupCallParams(store);

  const tree = rollupRouteTree(store, moduleRoots, diag);
  emit('MAP-NOTE', `the typescript half closed ${tree.lazy} lazy route boundary(ies) and composed `
    + `${tree.absolute} absolute path(s)`);

  // THE OTHER ROUTE A KEY TRAVELS: a service CALL, invisible to a template carrier. AFTER `arg_params`, and
  // that order is load-bearing: this pass decides whether a declaration CONSUMES a key or BUILDS one by
  // asking which argument lands in a `string` parameter, and with the labels still empty every caller looks
  // like a builder. Run too early it published only a fraction of the keys and dropped the method `t` entirely.
  // THE FILE IDS THIS RUN RE-EXTRACTED, for a rollup that writes rows belonging to a FILE: their rows
  // for every other file came back with the rest, so deriving them again doubles them.
  const affectedIds = partial
    ? new Set(store.table('files').filter((f) => affected.has(f.path)).map((f) => f.id))
    : null;
  const i18nCalls = rollupI18nCalls(store, new Set(names(config, 'i18nCalls')), affectedIds);
  if (i18nCalls.refs) {
    emit('MAP-NOTE', `the typescript half found ${i18nCalls.refs} translation key(s) in service calls, `
      + `and ${i18nCalls.dynamic} call(s) with no static key`);
  }
  emit('MAP-NOTE', `the typescript half skipped ${i18nCalls.skipped ?? -1} call(s) whose refs came `
    + `back with the rest (${affectedIds === null ? 'full run' : `${affectedIds.size} file(s) re-read`})`);

  const locales = collectLocales(store, directories(config, 'locales', fe), diag);

  // A COMPONENT CREATED BY CODE IS A RENDER TOO - before the scope pass, which has a rule for an edge the
  // compiler never matched, and after the refs rollup, whose ids this pass follows.
  const dynamic = rollupDynamicRenders(store, diag);
  emit('MAP-NOTE', `the typescript half found ${dynamic.edges} render edge(s) created in code across `
    + `${dynamic.sites} site(s), ${dynamic.unresolved} with no statically nameable component`);

  // WHICH OF SEVERAL SAME-SELECTOR CANDIDATES ANGULAR WOULD USE - before anything that reads `renders.scope`.
  const scopes = rollupModuleScope(store, diag);
  emit('MAP-NOTE', `the typescript half scoped renders: `
    + Object.entries(scopes).map(([k, v]) => `${k}=${v}`).join(', '));

  // WHICH ENTRY POINTS REACH A COMPONENT, and which inputs a template really binds - both read
  // `renders.scope`, so both run after it. `wholeApp` is true here: this half always maps the whole
  // workspace, which is the only case where "nothing renders this" is a claim and not a scope artefact.
  const reach = rollupComponentReach(store, { wholeApp: true });
  emit('MAP-NOTE', `the typescript half reached ${reach.classes} class(es) from ${reach.roots} entry `
    + `point(s), ${reach.exclusive} exclusive to one area, ${reach.shared} shared; `
    + `${reach.unused} component(s) nothing renders`);
  const inputs = rollupInputUsage(store);
  emit('MAP-NOTE', `the typescript half bound ${inputs.bound} input(s) across ${inputs.declared} declared, `
    + `${inputs.unused} never bound`);

  // THE STATE GRAPH, from the evaluated values of the consts that declare it.
  const stateGraph = rollupStateGraph(store);
  emit('MAP-NOTE', `the typescript half found ${stateGraph.actions} action(s), ${stateGraph.handlers} `
    + `reducer handler(s) and ${stateGraph.selectors} selector(s)`);

  // THE INVERTED VIEWS, last: each is a join over tables that all have to be complete first.
  rollupSelectors(store);
  rollupRenderGraph(store);
  const index = rollupI18nIndex(store);
  rollupGateIndex(store);
  const coverage = rollupTranslationCoverage(store);
  emit('MAP-NOTE', `the typescript half indexed ${index.unique_keys} translation key(s), `
    + `${coverage.defined} defined and ${coverage.missing} undefined, `
    + `${coverage.unused} translation(s) never referenced`);
  if (locales.files) {
    emit('MAP-NOTE', `the typescript half read ${locales.keys} translation key(s) from `
      + `${locales.files} locale file(s)`);
  }
  emit('MAP-NOTE', `the typescript half walked ${inv.files} file(s) under ${relativeTo(args.root, fe)} `
    + `and noted ${diag.total} diagnostic(s)`);

  // TWO COMPONENTS CAN SHARE ONE `templateUrl`, and the map has to say so: the nodes of that .html belong
  // to whichever component was parsed with it, so a consumer joining the OTHER component's bindings back
  // to them finds nothing. Measured: a real tree does share one. The tool this is ported from
  // discovers it while writing its readable `html/` mirror - a feature this half does not have - but the
  // fact is about the TREE, not about the mirror, and it is read off the rows instead.
  const seenSource = new Set();
  const filePathById = new Map(store.table('files').map((f) => [f.id, f.path]));
  const componentName = new Map(store.table('components').map((c) => [c.id, c.name]));
  for (const t of store.table('templates')) {
    const rel = t.path ?? `inline/${filePathById.get(t.file) ?? ''}`;
    const cut = rel.lastIndexOf('.');
    const key = cut > rel.lastIndexOf('/') ? rel.slice(0, cut) : rel;
    if (seenSource.has(key)) {
      diag.note('template_shared_by_components',
        { source: rel, template: t.id, component: componentName.get(t.component) ?? null });
    }
    seenSource.add(key);
  }

  // THE CLOSURE IS DERIVED FROM THE ROWS AND NO LONGER RUNS HERE. It needs EVERY row, which this half
  // only has while the whole store is alive - and that is exactly what a partial run cannot promise. The
  // rows it would have to be handed back are hundreds of MB of JSON, close to a `JSON.parse` ceiling of
  // about 537 MB, so the pass lives in the rust store, on the side that can read the database. What
  // travels instead is the feature API it needs, which only this half has read.
  const declared = {
    checks: names(config, 'featureChecks'),
    enum: config.data?.featureEnum ?? '',
  };

  // EVERY ROW CARRIES THE COLUMN, and for this half its value is always null. `module` is the SCOPE a map
  // was taken of - the old tool writes one map per module directory as well as one for the whole
  // workspace - and this half maps the workspace. A column that is absent rather than null is a column a
  // consumer cannot ask about, which is why the tool being reproduced writes it on every row.
  for (const f of store.table('files')) f.module ??= null;

  // THE READABLE VIEW, only when asked for: thousands of files that no build step opens are not free.
  if (args.html) {
    const mirror = writeHtmlMirror(store, args.html);
    emit('MAP-NOTE', `the typescript half mirrored ${mirror.templates} template(s) to ${args.html}`);
  }

  // WHAT COULD NOT BE ANSWERED, AS ROWS. A count says how many; only the items say which, and a map that
  // reports a count of diagnostics it cannot name is asking to be believed. The tool this is ported from
  // publishes them beside the tables as `diagnostics.json`; here they are a table, because the container
  // already has one and a query beats a second file.
  for (const item of diag.items) store.emit('diagnostics', item);

  // THE SHA MAP IS WHAT THE DATABASE RECORDS THIS HALF BY. It is the file's own content hash, taken in the
  // same streamed read the line count already costs - see TsInventory. A file whose bytes could not be
  // read carries none, and is therefore not claimed as recorded.
  const shas = {};
  for (const f of store.table('files')) {
    // THE STORED ONE WHEN THIS RUN DID NOT READ THE FILE. Every batch carries the WHOLE sha map,
    // because what has LEFT the tree is decided against it - and a partial run that listed only
    // the files it re-read would say the tree had just lost every other one.
    const sha = f.content_hash ?? f.sha;
    if (sha) { f.sha = sha; shas[f.path] = sha; }
  }

  // THE MAP STATES ITS OWN FOREIGN KEYS AND ITS OWN FILE ROUTE. SQLite records neither, and a consumer
  // that wants either had to write its own chain of `file` / `class` / `member` / `template` / `node` -
  // which several of them did, each a slightly different subset, so two readers disagreed about the same row
  // and the one resolving fewer rows looked exactly like a map that lacked them. Derived LAST, because
  // both are read off the finished rows rather than declared anywhere.
  const tableNames = Object.keys(store.tables).sort();
  // WHICH COLUMNS HELD A STRUCTURE. SQLite has no array and no object, so a cell that was a list or a
  // map is stored as JSON text - and a reader cannot tell it from a string that happens to start with a
  // bracket. Guessing it back cost the consumer an `AttributeError` deep in a derivation: `enums.members`
  // came back as text and every member read as a character. The map says so instead.
  const jsonColumns = {};
  // AND WHICH HELD A BOOLEAN, for the same reason and with the same cost. SQLite has no boolean
  // either: `true` comes back an integer 1, and a pass asking `call.new === true` of a row that was
  // handed back gets false. One `ComponentPortal` render edge disappeared that way, and with it every
  // render path that went through it - a whole component unreachable, from a cell that still said 1.
  const boolColumns = {};
  for (const table of tableNames) {
    const structured = new Set();
    const boolean = new Set();
    for (const row of store.tables[table]) {
      for (const [column, value] of Object.entries(row)) {
        if (value !== null && typeof value === 'object') structured.add(column);
        if (typeof value === 'boolean') boolean.add(column);
      }
    }
    if (structured.size) jsonColumns[table] = [...structured].sort();
    if (boolean.size) boolColumns[table] = [...boolean].sort();
  }
  const idScheme = store.idScheme();
  const joins = deriveJoins(store, idScheme, tableNames);
  const { anchors, unanchored } = deriveAnchors(store, idScheme, joins, tableNames);
  emit('MAP-NOTE', `the typescript half proved a file route for ${Object.keys(anchors).length} table(s) `
    + `over ${Object.keys(joins).length} join(s)`
    + (unanchored.length ? `, and none for ${unanchored.join(', ')}` : ''));

  // WHAT THIS MAP IS, for a consumer that puts it in a derived artifact's provenance block. The tool
  // being reproduced publishes these three in `index.json`, and a downstream consumer writes them into every
  // artifact it derives, so a reader can tell which extraction an answer came from. SQLite records none
  // of it, so the half that knows states it - beside the join spec, under the same key.
  const provenance = {
    setup,
    generated_at_ms: Date.now(),
    // WHICH SOURCE THIS MAP IS OF, recorded at EXTRACTION time. A reader asked later would report the
    // revision it is standing on, which is not the one the rows describe. `dirty` stays because a map
    // built from an uncommitted tree is not the commit it names.
    source_revision: revisionOf(fe),
    toolchain: {
      typescript: tsVersion, angular_compiler: ngVersion, node: process.version,
      tier: found.tier, warning: warning ?? null,
    },
    options: {
      ...templatePass.options,
      i18n_calls: names(config, 'i18nCalls'),
      locales: names(config, 'locales'),
      feature_checks: names(config, 'featureChecks'),
      feature_enum: config.data?.featureEnum ?? '',
      config: config.file,
      // WHERE THIS PROJECT KEEPS WHAT IT HAS ALREADY WORKED OUT ABOUT ITSELF. Nothing in the extraction
      // reads it - it is a pointer the atlas prints, so the records stay findable from the one document
      // a reader opens.
      // ABSOLUTE, because it is a pointer a reader follows from a document that may be read anywhere.
      // Resolved against the CONFIG's own directory, like every other path that file declares.
      project_docs: typeof config.data?.projectDocs === 'string'
        ? path.resolve(config.dir, config.data.projectDocs) : null,
    },
  };

  // WHICH TABLES THIS RUN REBUILT ENTIRELY, so the storing half removes its old rows for them
  // instead of only the re-extracted files'. NOT A LIST: a list is a second statement of a fact the
  // code already has, and the one that drifted would be the list. A table is whole exactly when this
  // run minted a row in it while no file was open - which is what a ROLLUP does, and a rollup reads
  // the whole store and so produces its table entire every time.
  //
  // READING `owner_file` INSTEAD WAS WRONG, and the whole-database differential is what said so:
  // `add` copies a row's own `file` into `owner_file` when no file is open, so the two passes that
  // mint rows outside the per-file walks - the locale reader and the i18n call rollup - produced rows
  // that LOOKED per-file. `translations`, `i18n_refs`, `routes`, `component_reach` and nine more came
  // out DOUBLED: the old rows were never removed and the new ones landed beside them.
  //
  // A ROLLUP THAT REACHES BACK INTO A ROW IT DID NOT MAKE is covered by the same rule, because it
  // minted rows in that table too. The one that mutates without minting is what `verifyUntouched` is
  // for: it fails the run rather than storing a change that would never be written.
  // ...AND THE ONES A ROLLUP CHANGES WITHOUT MINTING A ROW, which no rule can read off the rows
  // because nothing about them was made here: `translations.referenced` depends on refs from the whole
  // tree, `components.unused` on the whole render graph, `io.bound_count` on every binding. They are
  // named, and `verifyUntouched` is what proves the list complete - it fails the run on the fourth one
  // nobody noticed rather than storing a change that would never be written. It found `io` this way.
  const rewritten = ['translations', 'components', 'io'];
  const whole = [...new Set([...rewritten, ...store.rollupTables, ...Object.entries(store.tables)
    .filter(([, rows]) => rows.length && rows.some((r) => r.owner_file === undefined))
    .map(([name]) => name)])].sort();

  if (partial) {
    const touched = store.verifyUntouched(whole);
    // A RUN THAT STOPPED SHORT OF EVERY HOP and broke a row it kept is asked again over every hop.
    const stop = rereads ? cutShort(store, rereads, touched, false) : null;
    if (stop) { emit('MAP-RETRY', stop); return 0; }
    if (touched !== null) {
      const named = touched.map((t) => `${t.table}.${t.columns.join('/')} `
        + `(${t.rows} row(s), e.g. ${t.id})`).join(', ');
      emit('MAP-FATAL', `rows this run did not produce were changed by it: ${named}. Those tables `
        + 'are not ones this run rebuilds whole, so the changes would never be stored and would read '
        + 'as stale for ever');
      return 1;
    }
  }

  // A PER-FILE TABLE SENDS ONLY WHAT THIS RUN MADE; a whole one sends everything, because the
  // storing half is about to delete all of it. On a full run the two are the same thing.
  if (partial) {
    // MEASURED, because a rule that stops matching looks exactly like a tree that changed: an id kept
    // is a reference nothing outside the file had to be re-pointed for.
    emit('MAP-NOTE', `the typescript half kept ${store.reclaimed} id(s) that already named these rows`);
  }

  const wholly = new Set(whole);
  const sending = {};
  for (const name of Object.keys(store.tables)) {
    sending[name] = (partial && !wholly.has(name)) ? store.produced(name) : store.table(name);
  }

  // ROW ORDER IS A FACT AND A PARTIAL RUN DOES NOT REPRODUCE IT YET. The closure walks `renders` in
  // the order the rows arrive, and `render_path.path_index` numbers the paths of one component
  // positionally, so a consumer joining on the index reads a different path when the order moves.
  // A partial run builds a whole table as CARRIED-then-PRODUCED, and most of the render
  // paths come out at an index holding a different path - the same walks, proven identical as a SET
  // over the same components, in a different order.
  //
  // SORTING THE PAYLOAD BY ID DOES NOT FIX IT, which was worth finding out: the ids of the rows this
  // run re-derived are all ABOVE every carried one, so they sort to the end while a full run
  // interleaves them. The order a full run produces is its extraction order, and nothing the partial
  // run holds records where a re-derived row sat in it.
  //
  // The fix is to stop the enumeration depending on arrival order at all - give `_load_edges` a
  // stable sort over something the TREE decides - and that changes what a full run publishes too, so
  // it has to be re-proved against the tool this map reproduces before it is done.

  const payload = {
    all: !partial,
    partial,
    affected: partial ? [...affected].sort() : [],
    whole,
    first: true,
    final: true,
    fe: relativeTo(path.resolve(args.root), fe),
    shas,
    read: [],
    counters: store.counters,
    declared,
    tables: sending,
    spec: { id_scheme: idScheme, joins, anchors, unanchored, tables: tableNames,
            json_columns: jsonColumns, bool_columns: boolColumns,
            // A PARTIAL RUN REPUBLISHES THE ANSWER IT WAS GIVEN. It cannot measure one - its per-file
            // rows arrive already made - and publishing what it CAN see would shrink the list to the
            // rollups alone and make the next partial run drop `renders`.
            rebuilt: partial ? [...rebuilt].sort() : store.rebuiltTables(),
            // WHICH IDS A RE-EXTRACTION MUST NOT CHANGE, and what identifies the row holding one. A
            // partial run republishes what it was given, for the same reason it republishes `rebuilt`:
            // it cannot measure this over a map it only partly walked.
            identity: partial ? (state.identity ?? {}) : store.identityKeys() },
    provenance,
  };
  writePayload(args.rows, payload);
  for (const table of Object.keys(sending).sort()) emit('MAP-ROWS', table, sending[table].length);
  emit('MAP-DONE', Object.keys(shas).length);
  return 0;
}

main().then((code) => process.exit(code), (error) => {
  emit('MAP-FATAL', `${error && error.message ? error.message : error}`);
  process.exit(1);
});
