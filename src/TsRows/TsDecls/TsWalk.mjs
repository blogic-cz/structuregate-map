/**
 * The passes over every program and every template: the files of each project, what is reachable, and the template rows.
 *
 * Moved out of `TsMap.mjs` so that file stays under the line ceiling; it imports FLAT, because every
 * staged script lands in one folder.
 */

import path from 'node:path';
import { canonicalPath, relativeTo } from './TsPaths.mjs';
import { names } from './TsConfig.mjs';
import { buildMatcher } from './TsTplMatch.mjs';
import { makeTemplateExtractor } from './TsTplExtract.mjs';
import { importRow } from './TsImports.mjs';
import { jsdocOf, locationOf as nodeLocation, makeSymbolAt } from './TsNodes.mjs';
import { makeAngularRows } from './TsAngular.mjs';
import { makeBodyRows } from './TsBody.mjs';
import { makeClassRows } from './TsClassRows.mjs';
import { makeDeclRows } from './TsDeclRows.mjs';
import { makeExprDescriber } from './TsExprDescribe.mjs';
import { makeReadRefs } from './TsReadRefs.mjs';
import { makeWriteResolver } from './TsWriteRefs.mjs';
import { makeModifiers } from './TsModifiers.mjs';
import { makeEvaluator } from './TsValue.mjs';
import { makeTypeResolver } from './TsTypeRef.mjs';
import { makeTypeRows } from './TsTypeRows.mjs';
import { collectComments, collectStrings, exportRow } from './TsText.mjs';
import { createProgram, programFilePaths, projectSourceFiles } from './TsProgram.mjs';
import { emit } from './TsRunIo.mjs';
import { makeReadLog, surfaceOf } from './TsReads.mjs';
import { shapeOf } from './TsShape.mjs';

/**
 * ONE PHYSICAL FILE, ONE ROW - and the backfill that keeps it true.
 *
 * `intern` does not run the builder for a row that already exists, and this is called by two kinds of
 * caller: the one EXTRACTING a file, which knows its tier and its size, and the ones that merely
 * REFERENCE it. When a reference arrives first the row is created bare, and the extractor's own `tier` was
 * then dropped on the floor - the file read as never-parsed inventory while its declarations were already
 * in the map.
 */
export function makeFileId(store, feRoot, projectId) {
  return (abs, extra = {}) => {
    const canon = canonicalPath(abs);
    const id = store.intern('files', 'f', canon, () => ({
      path: relativeTo(feRoot, canon), abs: canon,
      ext: path.extname(canon).slice(1), project: projectId, ...extra,
    }));
    if (Object.keys(extra).length) store.patch(id, extra);
    return id;
  };
}

/**
 * THE FILES OF EVERY PROJECT, one program at a time.
 *
 * A file shared through a tsconfig path alias belongs to SEVERAL programs (a lib is in the application's
 * program and in its own), and extracting it once per program duplicated its declarations - one element
 * then reported the same directive several times. The first program to reach a file owns it.
 */
export function walkProjects({ ts, store, fe, nodeModules, projects, diag, noted, only = null, rereads = null }) {
  const inAnyProgram = new Set();
  // WHAT EACH FILE READ OUTSIDE ITSELF, and what it shows - see `TsSetup/TsReads.mjs`.
  const log = makeReadLog(ts, fe);
  const extracted = new Set();
  // WHAT PASS 2 NEEDS: every selector-matchable declaration, and every component whose template it parses.
  const registry = [];
  const components = [];
  for (const project of projects) {
    const projectId = store.add('projects', 'p', {
      name: project.name,
      dir: project.rel,
      project_type: project.projectType,
      source_root: project.sourceRoot,
      tsconfig: relativeTo(fe, project.tsconfig),
      // THE CONFIGS NOT CHOSEN ARE PART OF THE ANSWER. One is picked by priority and the rest decide
      // nothing - but which config a project is read through decides its entire file set, so a wrong pick
      // looks like a small project rather than like a mistake.
      tsconfig_candidates: project.tsconfigCandidates.map((c) => relativeTo(fe, c)),
      targets: project.targets,
    });

    let program;
    let checker;
    try {
      ({ program, checker } = createProgram(ts, project.tsconfig, nodeModules, diag, noted));
      checker = log.wrap(checker);
    } catch (error) {
      diag.note('program_failed');
      emit('MAP-NOTE', `the typescript half could not build the program of ${project.name} - ${error.message}`);
      continue;
    }

    for (const p of programFilePaths(program, fe)) inAnyProgram.add(canonicalPath(p));
    const files = projectSourceFiles(program, fe);
    if (!files.length) {
      // An empty program is indistinguishable from a project with no code, and it is how a wrong tsconfig
      // choice hides: picking a references-only config emptied several libs' programs while the run still
      // reported success. Never let that pass quietly.
      diag.note('project_program_empty');
      emit('MAP-NOTE', `the typescript half found no source file in ${project.name} through `
        + `${relativeTo(fe, project.tsconfig)} - check that the project's tsConfig names a config with `
        + 'files/include');
    }

    // WHICH OF ITS FILES A PARTIAL RUN READS AGAIN is decided here, before any of them is read: what they
    // read is owned by this project or an earlier one, and all of that has been decided already.
    if (rereads) {
      rereads.project(files.filter((sf) => !extracted.has(canonicalPath(sf.fileName))), () => {
        try { return createProgram(ts, project.tsconfig, nodeModules, diag, noted).program; } catch { return null; }
      });
    }
    const fileId = makeFileId(store, fe, projectId);
    const mine = [];
    const symbolAt = makeSymbolAt(ts, checker, log);
    const { modifiersOf, localExports } = makeModifiers(ts, log);
    const typeRefAt = makeTypeResolver(ts, checker);
    const readRefs = makeReadRefs(ts, checker, canonicalPath);
    const writeRefsOf = makeWriteResolver(ts, checker);
    // ONE ROW PER EXPRESSION, and `lang` says which side of the map it came from - a template expression
    // and a TypeScript one normalize to the SAME tree, so one consumer reads both.
    const describe = makeExprDescriber(ts, (node, role, ast, summary, writes) => {
      const sf = node.getSourceFile();
      const pos = sf.getLineAndCharacterOfPosition(node.getStart());
      const refs = writes.length ? writeRefsOf(node) : null;
      // A DECLARATION IN A FILE THIS RUN IS NOT RE-EXTRACTING ALREADY HAS ITS ROW. Resolving a
      // reference into another file describes what it lands on, and on a full run that file's own
      // walk had already described it. A partial run never walks it, so it described the same arrow
      // a second time under a second id, attributed to whichever file mentioned it.
      const rel = relativeTo(fe, canonicalPath(sf.fileName));
      if (only !== null && !only.has(rel)) {
        const home = fileId(sf.fileName);
        const held = store.handedBack('expressions', ['file', 'line', 'source'],
          { file: home, line: pos.line + 1, source: node.getText() });
        if (held !== null) { store.reused += 1; return held; }
      }
      return store.add('expressions', 'x', {
        // `col` WITH `line`, so a row keeps its id when its file is read again: another file's folded value
        // names it by `$expr_id`, and a re-numbered row left that reference pointing at nothing.
        lang: 'ts', role, file: fileId(sf.fileName), line: pos.line + 1, col: pos.character + 1,
        source: node.getText(), ast, ...summary,
        // The paths this expression PRODUCES, when it returns an object literal. `reads` says what it
        // consumes; omitted entirely when there is nothing to say, so the column means one thing.
        ...(writes.length ? { writes } : {}),
        // ...and WHERE each of those paths is declared, so the write side points at the same row a read of
        // it points at.
        ...(refs && refs.size ? { write_refs: Object.fromEntries(refs) } : {}),
      });
    }, readRefs.resolveRead, readRefs.resolveName, log);
    const { evalNode, propName } = makeEvaluator(ts, checker, symbolAt, describe);
    const typeRows = makeTypeRows({
      ts, checker, store, modifiersOf, typeRefAt, evalNode, propName, fileId,
    });
    // THE BODY WALK AND THE DECLARATIONS NEED EACH OTHER: a member's body is walked by the body rows, and
    // an inline function inside a body is declared as a `functions` row built from the class layer's own
    // parameter reader. The indirection is one object, filled in once both exist, rather than two copies of
    // either.
    const bodies = { collect: () => {} };
    const collectBody = (node, ownerId, classId) => bodies.collect(node, ownerId, classId);
    const angular = makeAngularRows({ store, feRoot: fe, diag });
    const classRows = makeClassRows({
      ts, checker, store, feRoot: fe, modifiersOf, typeRefAt, symbolAt, evalNode, propName,
      jsdocOf: (node) => jsdocOf(ts, node), collectBody, angular,
    });
    const declRows = makeDeclRows({
      ts, store, modifiersOf, localExports, evalNode, collectBody,
      paramsOf: classRows.paramsOf, typeText: classRows.typeText,
      jsdocOf: (node) => jsdocOf(ts, node),
    });
    /** An inline function - a callback, a nested arrow - as a `functions` row, so its body has an owner. It
     *  is anonymous by construction, so it is identified by POSITION, and `parent` says which body it sits
     *  in. */
    const declareInline = (node, parentId) => store.add('functions', 'fn', {
      file: fileId(node.getSourceFile().fileName), name: null, parent: parentId, inline: true,
      form: ts.isArrowFunction(node) ? 'arrow'
        : ts.isFunctionExpression(node) ? 'function-expression'
          : ts.isMethodDeclaration(node) ? 'method' : 'function',
      params: classRows.paramsOf(node), returns: node.type ? node.type.getText() : null,
      ...nodeLocation(node),
    });
    bodies.collect = makeBodyRows({
      ts, store, evalNode, describe, declFileOf: classRows.declFileOf, declareInline, propName,
    }).collectBody;
    for (const sf of files) {
      const abs = canonicalPath(sf.fileName);
      if (extracted.has(abs)) continue;
      extracted.add(abs);
      // A FILE THIS RUN IS NOT RE-EXTRACTING still needs its row and its program membership - what
      // the files being re-extracted resolve AGAINST is the declarations in it - but nothing is
      // read out of it, because its rows are already loaded. `fileId` interns, so the row it
      // returns is the loaded one.
      if (only && !only.has(relativeTo(fe, abs))) { fileId(sf.fileName); continue; }
      const fid = fileId(sf.fileName, {
        lines: sf.getLineAndCharacterOfPosition(sf.end).line + 1,
        chars: sf.text.length,
        tier: 'core',
      });
      // EVERYTHING MINTED FROM HERE BELONGS TO THIS FILE - see `Store.enterFile`. It is set around the
      // whole body rather than passed down, because the rows that need it are emitted eight call layers
      // deep and one of them, `calls`, cannot be attributed from its own columns at all.
      store.enterFile(fid);
      log.begin();
      mine.push([fid, sf, abs]);
      try {
        collectComments(ts, store, sf, fid);
      } catch {
        diag.note('comment_scan_failed');
      }
      // ONE STATEMENT FAILING IS NOT THE FILE FAILING. A checker round trip can throw on a construct this
      // compiler version does not expect, and losing the whole file's rows over one of them is how a map
      // goes quietly short.
      for (const statement of sf.statements) {
        try {
          if (ts.isImportDeclaration(statement)) importRow(ts, store, statement, fid, symbolAt, fe, checker);
          else if (ts.isExportDeclaration(statement) || ts.isExportAssignment(statement)) {
            exportRow(ts, store, statement, fid, symbolAt, fe);
          } else if (ts.isEnumDeclaration(statement)) typeRows.enumRow(statement, fid);
          else if (ts.isInterfaceDeclaration(statement)) {
            typeRows.interfaceRow(statement, fid, modifiersOf(statement).exported);
          } else if (ts.isTypeAliasDeclaration(statement)) typeRows.typeAliasRow(statement, fid);
          else if (ts.isVariableStatement(statement)) declRows.constRows(statement, fid);
          else if (ts.isClassDeclaration(statement)) {
            const cls = classRows.extractClass(statement, fid);
            // THE NAMES, COPIED, and never `cls` itself: an entry outlives the walk, and a closure over `cls`
            // reached the class's TypeScript nodes and through them every program this walk built - most
            // of a large Angular workspace's heap still held after the last project was done.
            const inputs = [...cls.io.inputs];
            const outputs = [...cls.io.outputs];
            const injectsTemplateRef = cls.injectsTemplateRef;
            for (const r of cls.angularRows) {
              registry.push({
                id: r.id, name: r.className, selector: r.selector, classId: r.classId,
                // CANONICAL, like every other emitted path: `sf.fileName` carries the IMPORT SPECIFIER's
                // spelling, so a module that imports `.../widgets/...` while the directory is `Widgets`
                // would otherwise emit a join key whose case matches no other row's.
                file: relativeTo(fe, canonicalPath(sf.fileName)), is_component: r.kind === 'component',
                // Duck-typed for the compiler's own matcher.
                inputs: { hasBindingPropertyName: (n) => inputs.includes(n) },
                outputs: { hasBindingPropertyName: (n) => outputs.includes(n) },
                // THE NAMES BEHIND THE PREDICATES, so this registry can be compared against the one
                // rebuilt from rows - see `TsTpl/TsRegistry.mjs`. Two closures are never equal.
                inputNames: inputs, outputNames: outputs,
                exportAs: null, ngTemplateGuards: [], hasNgTemplateContextGuard: false,
                // COMPUTED, not assumed: a directive whose constructor takes `TemplateRef` controls a
                // template, so a binding of its selector attribute ON an `<ng-template>` gates content.
                isStructural: r.kind !== 'component' && injectsTemplateRef,
              });
              components.push({
                id: r.id, class: r.classId, name: r.className, file: fid,
                file_path: relativeTo(fe, sf.fileName),
                template_abs: r.templateAbs, inline_template: r.inline, inline_line: r.inlineLine,
                template_file: r.templateAbs ? store.lookup('files', canonicalPath(r.templateAbs)) : null,
                is_component: r.kind === 'component',
              });
            }
          }
          else if (ts.isFunctionDeclaration(statement)) declRows.functionRow(statement, fid);
        } catch {
          diag.note('ts_statement_failed');
        }
      }
      try {
        collectStrings(ts, store, sf, { file: fid }, propName, evalNode);
      } catch {
        diag.note('string_scan_failed');
      }
      const read = log.end(abs);
      store.patch(fid, { reads: read.reads, reads_deep: read.deep });
      store.enterFile(null);
    }
    // WHAT EACH FILE SHOWS, after the project's files are read so no answer above came in another order. A
    // partial run already emitted the ones that could have moved; every other file it reads shows what it did.
    for (const [fid, sf, abs] of mine) {
      const rel = relativeTo(fe, abs);
      store.patch(fid, { shape: shapeOf(ts, sf) });
      if (!rereads) store.patch(fid, { surface: surfaceOf(ts, program, sf) });
      else if (rereads.checked.has(rel)) store.patch(fid, { surface: rereads.checked.get(rel) });
    }
  }
  return { inAnyProgram, registry, components };
}

/**
 * REACHABILITY COMES FROM THE PROGRAM, NOT FROM THE WALK SET.
 *
 * A `.d.ts` is never walked - the declarations pass skips declaration files deliberately - but it IS a
 * root file of its project's tsconfig through an `include` glob. Deciding this from `parsed` stamped every
 * `.d.ts` of a real frontend as "dead code the build never ships", while the map's own `imports`
 * table showed reachable files resolving into them.
 */
export function markReachable(store, inAnyProgram, diag) {
  for (const f of store.table('files')) {
    if (f.parsed !== false) continue;
    if (f.ext !== 'ts') { f.reachable = null; continue; }
    const inProgram = inAnyProgram.has(f.abs);
    f.reachable = inProgram;
    // Counted, not silent: content deliberately not extracted is a different fact from code the build
    // drops on the floor.
    diag.note(inProgram ? 'declaration_file_not_walked' : 'ts_not_in_any_program', { file: f.path });
  }
  for (const f of store.table('files')) {
    if (f.tier === 'core' || f.tier === 'boundary') f.reachable = true;
    // EVERY ROW CARRIES A DETERMINATE ANSWER. A parsed file that was never tiered fell through both
    // branches and the key was simply ABSENT - a consumer reading `reachable` got `undefined`, which is
    // neither yes, no, nor "not applicable". `null` is the honest value: reachability is a program
    // concept and these are not program files.
    else if (f.reachable === undefined) f.reachable = null;
  }
}

/**
 * PASS 2 - every core component's template, then the templates of what those render.
 *
 * WHICH PIPE OR ELEMENT CARRIES A TRANSLATION KEY IS APPLICATION KNOWLEDGE, so it is an INPUT
 * (`structuregate.ts.json`), checked against what the frontend actually declares: a carrier that matches no
 * pipe and no selector is reported inert rather than quietly doing nothing. None given means no `i18n_refs`
 * is claimed - said out loud rather than looking like an application with no i18n.
 */
export function runTemplatePass({ ng, store, diag, fe, registry, components, config, expand = true }) {
  const declared = new Set();
  for (const row of store.table('pipes')) if (row.pipe_name) declared.add(String(row.pipe_name));
  for (const table of ['directives', 'components']) {
    for (const row of store.table(table)) if (row.selector) declared.add(String(row.selector));
  }
  const given = names(config, 'i18nCarriers').map((c) => {
    const cut = c.indexOf(':');
    return cut < 0 ? { name: c, input: null } : { name: c.slice(0, cut), input: c.slice(cut + 1) };
  });
  const carriers = new Map(given.filter((c) => declared.has(c.name)).map((c) => [c.name, c.input]));
  for (const c of given) if (!declared.has(c.name)) diag.note('i18n_carrier_not_declared');
  const gateInputs = new Set(names(config, 'gateInputs'));

  const { matcher } = buildMatcher(ng, registry, diag);
  const extractor = makeTemplateExtractor({ ng, store, diag, feRoot: fe, matcher, carriers, gateInputs });

  const byId = new Map(components.map((c) => [c.id, c]));
  const limit = typeof config.data?.templateDepth === 'number' ? config.data.templateDepth : 1;
  const done = new Set();
  let queue = components.slice();
  let depth = 0;
  let parsed = 0;
  while (queue.length) {
    const batch = queue;
    queue = [];
    const rendered = new Set();
    for (const comp of batch) {
      if (done.has(comp.id)) continue;
      done.add(comp.id);
      const before = store.table('renders').length;
      // THE TEMPLATE'S ROWS BELONG TO THE TEMPLATE'S FILE - the `.html` when there is one, and the
      // component's own `.ts` when the template is inline. This pass runs OUTSIDE the per-file walk, so
      // without it `bindings`, `gates`, `template_nodes`, `template_strings` and every template-side
      // `expressions` row carry no file at all and a rebuild could not find them to replace.
      store.enterFile(comp.template_file ?? comp.file);
      if (extractor.extract(comp)) parsed += 1;
      store.enterFile(null);
      for (const r of store.table('renders').slice(before)) rendered.add(r.to);
    }
    depth += 1;
    if (depth > limit || !expand) break;
    for (const id of rendered) {
      const c = byId.get(id);
      if (c && !done.has(c.id) && c.is_component) queue.push(c);
    }
  }
  return {
    parsed,
    extractor,
    // WHAT THE MAP WAS CONFIGURED WITH, handed back rather than recomputed. `carriers` is the
    // DECLARED subset and `given` is what was asked for; a consumer that sees only one of them
    // cannot tell a carrier that does nothing from one that was never named.
    options: {
      template_depth: limit,
      i18n_carriers: [...carriers].map(([name, input]) => ({ name, input })),
      i18n_carriers_given: given,
      gate_inputs: [...gateInputs],
    },
  };
}
