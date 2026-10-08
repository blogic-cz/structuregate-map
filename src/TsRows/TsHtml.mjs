/**
 * THE READABLE VIEW OF A PARSED TEMPLATE - one JSON per template, the tree in document order.
 *
 * Ported from the mirror the original Angular extractor writes beside its tables. The tables are the JOINABLE
 * view; this is the one a person opens. "Where is the parsed html" has to have a FILE as its answer, not
 * a query, and the two views join because every node here carries the same `n:` id the rows do.
 *
 * IT IS REBUILT FROM THE ROWS, not from the parser. The tool this comes from writes it while it walks the
 * template, straight out of the AST; here it is assembled from `template_nodes` and `bindings` after the
 * fact. That is deliberate - the mirror then cannot say anything the tables do not, which is the only
 * property that makes two views of one model worth having.
 *
 * OPTIONAL, because nothing reads it. It is written only when `--ts-html <dir>` names a directory, so the
 * ordinary map does not pay thousands of files for a view no build step opens.
 *
 * Imports are flat: see `TsDecls/TsTypeRef.mjs`.
 */
import { mkdirSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { slash } from './TsPaths.mjs';

/** The path a mirror file takes, without its extension - `a/b/c.component.html` -> `a/b/c.component`. */
function dropExt(rel) {
  const cut = rel.lastIndexOf('.');
  return cut > rel.lastIndexOf('/') ? rel.slice(0, cut) : rel;
}

/**
 * ONE NODE, AS THE MIRROR SPELLS IT.
 *
 * A LIST-SHAPED FIELD IS ALWAYS A LIST on the node kinds that can hold one - never absent because it
 * happens to be empty. Omission made `[]` and "this node kind has no such slot" the same value, so a
 * consumer had to write `(node.inputs ?? [])` at every read and could not tell a matched-nothing element
 * from a text node. Presence says what the kind CAN carry, emptiness what it does. `structural` is
 * Template-only: that is where the compiler puts a desugared `*ngIf`, and an element never has one.
 *
 * The block kinds carry ONE generic `expression_source` rather than a field per kind: a consumer reads
 * `kind` to know what the expression means. Naming them per kind invented four field names that had to be
 * kept in step with the compiler's block classes.
 */
function nodeOf(row, ctx, children) {
  const out = { kind: row.kind, line: row.line };
  if (children.length) out.children = children;
  out.id = row.id;
  const bound = ctx.bindingsByNode.get(row.id) ?? [];
  const boundOf = (kind) => bound.filter((b) => b.kind === kind).map((b) => ({
    name: b.name, source: b.source ?? null, expression: ctx.exprOfBinding.get(b.id) ?? null,
  }));
  const keys = ctx.keysByNode.get(row.id);

  if (row.kind === 'Element' || row.kind === 'Template') {
    out.tag = row.tag;
    out.directives = (Array.isArray(row.directives) ? row.directives : []).map((d) => d.name);
    out.references = Array.isArray(row.references) ? row.references : [];
    out.variables = Array.isArray(row.variables) ? row.variables : [];
    // FROM THE ATTRIBUTE BINDINGS, not from the row. Every static attribute is a `bindings` row of its
    // own - including its value - and `template_nodes.attributes` is a different slot that an Element
    // leaves empty. Reading the row instead published `[]` for every element that has attributes.
    out.attributes = bound.filter((b) => b.kind === 'attribute')
      .map((b) => ({ name: b.name, value: b.value ?? '' }));
    out.inputs = boundOf('input');
    out.outputs = boundOf('output');
    if (row.kind === 'Template') {
      out.structural = boundOf('template_attr').map((b) => ({ ...b, gate: true }));
    }
    if (keys) out.i18n_keys = keys;
    return out;
  }
  if (row.kind === 'Content') { out.selector = row.selector ?? null; return out; }
  if (row.kind === 'Text') { out.text = row.text ?? ''; return out; }
  if (row.kind === 'BoundText') {
    out.text = row.text ?? '';
    out.expression = ctx.exprOfNode.get(row.id) ?? null;
    if (keys) out.i18n_keys = keys;
    return out;
  }
  // A BLOCK. Its expression is the one the extractor recorded against the node, and a branch with none is
  // the `@else`.
  const expression = ctx.blockExprOfNode.get(row.id);
  if (expression) {
    out.expression_source = expression.source ?? null;
    out.expression = expression.id;
  }
  if (row.kind === 'IfBlockBranch' && !expression) out.else_branch = true;
  if (row.kind === 'SwitchBlockCase' && !expression) out.default_case = true;
  if (row.kind === 'ForLoopBlock') out.item = row.item;
  if (row.kind === 'DeferredBlock') out.triggers = row.triggers;
  if (typeof row.tag === 'string') out.tag = row.tag;
  return out;
}

/**
 * The mirror, written under `<dir>/html/` with `<dir>/html_index.json` beside it. Returns what it wrote.
 *
 * TWO COMPONENTS CAN SHARE ONE `templateUrl`, and the file name is taken from the SOURCE path, so the
 * second writer would silently overwrite the first and the mirror would answer for the wrong component -
 * its node ids belong to the other template. A second writer therefore carries the template id in its
 * name, and the index (which lists both) points at it. The same collision is reported as a diagnostic.
 */
export function writeHtmlMirror(store, dir) {
  const byTemplate = new Map();
  for (const n of store.table('template_nodes')) {
    if (!byTemplate.has(n.template)) byTemplate.set(n.template, []);
    byTemplate.get(n.template).push(n);
  }
  const bindingsByNode = new Map();
  for (const b of store.table('bindings')) {
    if (b.node === null || b.node === undefined) continue;
    if (!bindingsByNode.has(b.node)) bindingsByNode.set(b.node, []);
    bindingsByNode.get(b.node).push(b);
  }
  // A TEMPLATE NOBODY OWNS IS STILL PARSED - an `.html` file no `@Component` names, which
  // the half walks so a key living only there is not invisible. It is LABELLED rather than left null: a
  // reader scanning the index must not have to tell "no component" apart from "the field is missing".
  const ORPHAN = '(orphan template)';
  const componentName = new Map(store.table('components').map((c) => [c.id, c.name]));
  const filePath = new Map(store.table('files').map((f) => [f.id, f.path]));
  // WHICH EXPRESSION A BINDING, AN INTERPOLATION OR A BLOCK CARRIES. The mirror names them by id, which is
  // how the two views join: `expression: "x:42"` is a row a consumer can open.
  const exprOfBinding = new Map();
  const exprOfNode = new Map();
  const blockExprOfNode = new Map();
  for (const e of store.table('expressions')) {
    if (e.binding !== null && e.binding !== undefined) exprOfBinding.set(e.binding, e.id);
    if (e.node === null || e.node === undefined) continue;
    if (e.kind === 'interpolation') exprOfNode.set(e.node, e.id);
    // `block_track` is the `@for` TRACK expression and sits on the same node as the block's own - every
    // `@for` block carries both. Taking it too let it overwrite the one that matters,
    // so every one of them published what it tracks BY (`item.vendor.code`) instead of what it
    // iterates (`cartVM.items`).
    else if (String(e.kind).startsWith('block_') && e.kind !== 'block_track') {
      blockExprOfNode.set(e.node, e);
    }
  }
  const keysByNode = new Map();
  for (const r of store.table('i18n_refs')) {
    if (r.node === null || r.node === undefined || !r.key) continue;
    if (!keysByNode.has(r.node)) keysByNode.set(r.node, []);
    keysByNode.get(r.node).push(r.key);
  }
  const ctx = { bindingsByNode, exprOfBinding, exprOfNode, blockExprOfNode, keysByNode };

  const index = [];
  const written = new Set();
  for (const tpl of store.table('templates')) {
    const rows = byTemplate.get(tpl.id) ?? [];
    const kids = new Map();
    for (const r of rows) {
      const parent = r.parent ?? null;
      if (!kids.has(parent)) kids.set(parent, []);
      kids.get(parent).push(r);
    }
    // DOCUMENT ORDER IS `order`, which is the order the walk emitted them in - the mirror's whole claim.
    for (const list of kids.values()) list.sort((a, b) => (a.order ?? 0) - (b.order ?? 0));
    const build = (parent) => (kids.get(parent) ?? []).map((r) => nodeOf(r, ctx, build(r.id)));
    const tree = build(null);

    const source = tpl.path ?? filePath.get(tpl.file) ?? '';
    const inline = tpl.inline === 1 || tpl.inline === true;
    const rel = tpl.path ?? `inline/${filePath.get(tpl.file) ?? ''}`;
    let name = `${dropExt(rel)}.json`;
    if (written.has(name)) name = `${dropExt(rel)}.${String(tpl.id).split(':').join('')}.json`;
    written.add(name);

    const out = path.join(dir, 'html', name.split('/').join(path.sep));
    mkdirSync(path.dirname(out), { recursive: true });
    writeFileSync(out, JSON.stringify({
      template: tpl.id, component: componentName.get(tpl.component) ?? ORPHAN, source, inline, tree,
    }), 'utf8');
    index.push({
      template: tpl.id, component: componentName.get(tpl.component) ?? ORPHAN, source, inline,
      tree_file: slash(path.join('html', name)), roots: tree.length,
    });
  }
  writeFileSync(path.join(dir, 'html_index.json'), JSON.stringify(index), 'utf8');
  return { templates: index.length };
}
