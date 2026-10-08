/**
 * THE TEMPLATE PASS - the real Angular template AST, flattened into tables.
 *
 * Ported from the original Angular extractor. `parseTemplate` gives the same AST the
 * compiler compiles: elements, bound attributes with PARSED expressions, structural directives desugared
 * into `Template` nodes, and the v17 block syntax. Every node becomes a row with a parent id, so the tree is
 * reconstructable, and bindings, gates, i18n refs and rendered-child edges hang off the node id.
 *
 * NODE DISPATCH IS `instanceof` AGAINST THE COMPILER'S OWN CLASSES. The bundle renames colliding classes
 * (`Element` ships as `Element$1`), so dispatching on `constructor.name` drops every element to the generic
 * branch - measured: hundreds of templates, almost no bindings. Never reintroduce a name switch here.
 *
 * Imports are flat: see `TsDecls/TsTypeRef.mjs`.
 */
import { readFileSync } from 'node:fs';

import { relativeTo } from './TsPaths.mjs';
import { makeExprNormalizer } from './TsTplExpr.mjs';
import { makeEmitters, probeBindingTypes } from './TsTplEmit.mjs';
import { isCustomElement, spanOf } from './TsTplKeys.mjs';
import { childOwnedBindings, matchNode } from './TsTplMatch.mjs';

/** Angular's own container tags, asked of ANGULAR. They are compiler constructs and not components, so "no
 *  directive matched" is the correct outcome for them and must not be reported as a gap - it was, nearly all
 *  of the notes. */
const isBuiltinTag = (ng, tag) => ng.isNgContainer(tag) || ng.isNgTemplate(tag) || ng.isNgContent(tag);

/** A template AST node, by the interface every `TmplAst*` class implements - so a child-bearing field is
 *  found STRUCTURALLY instead of by name, and a node type this compiler version added is walked without
 *  being listed. */
const isTmplNode = (v) => !!v && typeof v === 'object' && typeof v.visit === 'function';

export function makeTemplateExtractor({ ng, store, diag, feRoot, matcher, carriers, gateInputs }) {
  const { normalizeExpr } = makeExprNormalizer(ng);
  const { classType, styleType } = probeBindingTypes(ng);
  const { emitRender, emitExpr, emitBound } = makeEmitters({
    store, normalizeExpr, carriers, gateInputs, classType, styleType,
  });

  // Node classes, narrowest first. Everything not listed still gets a row through the generic branch.
  const NODE_KINDS = [
    ['Template', ng.TmplAstTemplate], ['Element', ng.TmplAstElement], ['Content', ng.TmplAstContent],
    ['BoundText', ng.TmplAstBoundText], ['Text', ng.TmplAstText], ['Icu', ng.TmplAstIcu],
    ['IfBlockBranch', ng.TmplAstIfBlockBranch], ['IfBlock', ng.TmplAstIfBlock],
    ['SwitchBlockCase', ng.TmplAstSwitchBlockCase], ['SwitchBlock', ng.TmplAstSwitchBlock],
    ['ForLoopBlockEmpty', ng.TmplAstForLoopBlockEmpty], ['ForLoopBlock', ng.TmplAstForLoopBlock],
    ['DeferredBlockPlaceholder', ng.TmplAstDeferredBlockPlaceholder],
    ['DeferredBlockLoading', ng.TmplAstDeferredBlockLoading],
    ['DeferredBlockError', ng.TmplAstDeferredBlockError],
    ['DeferredBlock', ng.TmplAstDeferredBlock], ['UnknownBlock', ng.TmplAstUnknownBlock],
  ].filter((e) => typeof e[1] === 'function');

  // NOT children, though they ARE TmplAst nodes: a bound attribute, event, reference or variable belongs TO
  // a node, it is not a node under it. Detecting children by "has a visit() method" alone made every binding
  // a tree child and nearly doubled the map's rows.
  const ATTRIBUTE_KINDS = [
    ng.TmplAstBoundAttribute, ng.TmplAstBoundEvent, ng.TmplAstTextAttribute, ng.TmplAstReference,
    ng.TmplAstVariable, ng.TmplAstDeferredTrigger,
  ].filter((c) => typeof c === 'function');

  // An EXPRESSION is not a child either, and it also has a `visit()` method: `BoundText.value` is an
  // `ASTWithSource`, so a shape-only test pulled interpolations and property reads into the node tree
  // (thousands of rows too many). `ng.AST` is the compiler's own base class for every expression.
  const isChildNode = (v) => isTmplNode(v) && !(typeof ng.AST === 'function' && v instanceof ng.AST)
    && !ATTRIBUTE_KINDS.some((c) => v instanceof c);

  const kindOf = (node) => {
    for (const [name, cls] of NODE_KINDS) if (node instanceof cls) return name;
    const n = node?.constructor?.name ?? 'Unknown';
    const i = n.indexOf('$');
    return i > 0 ? n.slice(0, i) : n;
  };

  function readTemplate(comp) {
    if (comp.inline_template !== null && comp.inline_template !== undefined) {
      return { source: comp.inline_template, url: `${comp.file_path}#inline` };
    }
    if (comp.template_abs) {
      try {
        return { source: readFileSync(comp.template_abs, 'utf8'), url: comp.template_abs };
      } catch {
        diag.note('template_read_failed');
        return null;
      }
    }
    return null;
  }

  /** The v17 block nodes (`@if` / `@switch` / `@for` / `@defer`) plus any node type this version added. */
  function emitBlock(kind, node, row, tplId, comp, span) {
    const expr = node.expression;
    const src = expr?.source ?? null;
    if (kind === 'IfBlockBranch') {
      row.alias = node.expressionAlias?.name ?? null;
      row.else_branch = !expr;
    } else if (kind === 'SwitchBlockCase') row.default_case = !expr;
    else if (kind === 'ForLoopBlock') {
      row.item = node.item?.name ?? null;
      row.context_vars = Object.keys(node.contextVariables ?? {});
    } else if (kind === 'DeferredBlock') {
      row.triggers = Object.keys(node.triggers ?? {});
      row.prefetch_triggers = Object.keys(node.prefetchTriggers ?? {});
    } else if (typeof node.name === 'string') row.tag = node.name;

    const nodeId = store.add('template_nodes', 'n', row);
    // THE BLOCK'S KIND IS ITS OWN LABEL - no kind-to-field table. A `@switch` node is `SwitchBlock`, its
    // expression goes in one generic field, and a consumer reads `kind` to know what the expression means.
    // Whether a block's expression GATES content is structural too: an `@if` branch, a `@switch` and a
    // `@case` decide what renders; a `@for` iterates.
    const iterating = kind === 'ForLoopBlock';
    if (expr) {
      emitExpr({
        tplId, nodeId, comp, kind: `block_${kind}`, name: kind,
        norm: normalizeExpr(expr), src, gate: !iterating, gateKind: iterating ? null : 'block', span,
      });
    }
    if (iterating && node.trackBy) {
      emitExpr({
        tplId, nodeId, comp, kind: 'block_track', name: 'track',
        norm: normalizeExpr(node.trackBy), src: node.trackBy.source ?? null,
      });
    }
    return nodeId;
  }

  function extract(comp) {
    const t = readTemplate(comp);
    if (!t) {
      if (comp.is_component) diag.note('component_without_template');
      return null;
    }
    let parsed;
    try {
      parsed = ng.parseTemplate(t.source, t.url, { preserveWhitespaces: false, collectCommentNodes: false });
    } catch {
      diag.note('template_parse_threw');
      return null;
    }
    const errors = (parsed.errors ?? []).map((e) => String(e.msg));
    if (errors.length) diag.note('template_parse_errors');

    /**
     * Line offset from the parsed template's coordinates to the REAL file's.
     *
     * `parseTemplate` receives only the template TEXT, so its spans start at line 1 of that text. For an
     * external `.html` that IS the file, offset 0. For an INLINE template the text starts on the line the
     * backtick sits on, so every span must shift - without it, nearly all inline binding rows and
     * node rows resolved to the wrong `.ts` line.
     */
    const lineOffset = comp.inline_template !== null && comp.inline_template !== undefined && comp.inline_line
      ? comp.inline_line - 1 : 0;
    const shift = (s) => {
      if (!lineOffset) return s;
      const out = { ...s };
      if (out.line !== undefined) out.line += lineOffset;
      if (out.end_line !== undefined) out.end_line += lineOffset;
      return out;
    };

    const tplId = store.add('templates', 't', {
      component: comp.id, class: comp.class, name: comp.name,
      file: comp.template_file ?? comp.file, path: comp.template_abs ? relativeTo(feRoot, comp.template_abs) : null,
      inline: comp.inline_template !== null && comp.inline_template !== undefined,
      chars: t.source.length, lines: t.source.split('\n').length,
      line_offset: lineOffset,
      ng_content_selectors: parsed.ngContentSelectors ?? [],
      style_urls: parsed.styleUrls ?? [], parse_errors: errors,
    });

    let order = 0;
    const walk = (node, parentId, depth) => {
      if (!node || typeof node !== 'object') return;
      const kind = kindOf(node);
      const span = shift(spanOf(node));
      const row = {
        template: tplId, component: comp.id, parent: parentId, order: order++, depth, kind, ...span,
      };
      let nodeId = null;
      if (kind === 'Element' || kind === 'Template') nodeId = element(node, kind, row, tplId, comp, shift, span);
      else if (kind === 'Content') {
        row.selector = node.selector ?? null;
        row.attributes = (node.attributes ?? []).map((a) => ({ name: a.name, value: a.value }));
        nodeId = store.add('template_nodes', 'n', row);
      } else if (kind === 'Text') {
        // Full text, never truncated: this IS what the user reads.
        row.text = String(node.value ?? '');
        nodeId = store.add('template_nodes', 'n', row);
      } else if (kind === 'BoundText') {
        const src = node.value.source ?? '';
        row.text = String(src);
        nodeId = store.add('template_nodes', 'n', row);
        emitExpr({
          tplId, nodeId, comp, kind: 'interpolation', name: null,
          norm: normalizeExpr(node.value), src: String(src),
        });
      } else if (kind === 'Icu') {
        row.icu_vars = Object.keys(node.vars ?? {});
        row.icu_placeholders = Object.keys(node.placeholders ?? {});
        nodeId = store.add('template_nodes', 'n', row);
      } else nodeId = emitBlock(kind, node, row, tplId, comp, span);

      // CHILDREN ARE FOUND BY SHAPE: every own property that holds template nodes. Bindings and attributes
      // are not template nodes, so they are skipped without being excluded by name.
      for (const v of Object.values(node)) {
        if (Array.isArray(v)) {
          for (const c of v) if (isChildNode(c)) walk(c, nodeId, depth + 1);
        } else if (isChildNode(v)) walk(v, nodeId, depth + 1);
      }
    };

    for (const n of parsed.nodes ?? []) walk(n, null, 0);
    return { tplId };
  }

  /** An element or a desugared `Template` wrapper: the node row, what it matches, and every binding on it. */
  function element(node, kind, row, tplId, comp, shift, span) {
    const isTpl = kind === 'Template';
    const tag = isTpl ? (node.tagName ?? 'ng-template') : node.name;
    const references = (node.references ?? []).map((r) => ({ name: r.name, value: r.value }));
    const variables = isTpl ? node.variables.map((v) => ({ name: v.name, value: v.value })) : [];
    const matched = matchNode(ng, matcher, node);
    row.tag = tag;
    row.references = references;
    row.variables = variables;
    row.directives = matched.map((m) => ({
      id: m.id, name: m.name, selector: m.selector, is_component: m.is_component,
    }));
    const nodeId = store.add('template_nodes', 'n', row);
    if (!matched.length && isCustomElement(tag) && !isBuiltinTag(ng, tag)) {
      // No project selector matched: a third-party component, whose selector lives in node_modules and is
      // not extracted, or a real dangling element.
      diag.note('element_unmatched',
        { component: comp.name, tag, line: row.line, col: row.col, end_line: row.end_line });
    }
    for (const m of matched) {
      // A STRUCTURAL DIRECTIVE DESUGARS INTO A SYNTHETIC WRAPPER THAT KEEPS THE PRE-DESUGAR TAG, so a
      // component selector matches the wrapper as well as the real element inside it. The wrapper is an
      // `<ng-template>` and not an instance of that component, so a render edge from it is a phantom:
      // over a tenth of the render rows sat on a wrapper. Structural DIRECTIVE matches on the wrapper are
      // real and stay - that is how NgIf and NgFor attach.
      if (isTpl && m.is_component) continue;
      emitRender(m, comp, tplId, nodeId, tag, span);
    }

    // THE DESUGARED STRUCTURAL FORM IS A GATE TOO. `*directive="x"` puts the binding in `templateAttrs`, but
    // `<ng-template [directive]="x">` - what Angular desugars that INTO - puts it in `inputs`, where the
    // gate test never looked. The test is the DIRECTIVE, resolved by the matcher and structural because its
    // constructor takes `TemplateRef` - never the attribute's name, and never every input on an
    // `<ng-template>`, which would file an attribute directive's binding as a gate over content it does not
    // control.
    const structuralInputs = new Set();
    if (isTpl) {
      for (const m of matched) {
        const sel = m.selector ?? '';
        if (!m.is_component && m.isStructural && sel.startsWith('[') && sel.endsWith(']')
          && !sel.includes(',')) {
          structuralInputs.add(sel.slice(1, -1));
        }
      }
    }

    const carrier = carriers.has(tag);
    // Skip what the child element already owns (the same objects) - see `childOwnedBindings`.
    const shared = isTpl ? childOwnedBindings(node) : new Set();
    // The key-bearing input is NAMED BY THE CALLER and must also be a declared input of the matched
    // directive, so a typo in the config cannot invent a binding. Accepting the directive's whole input
    // surface instead recorded `[classes]="'title'"` as a translation key.
    const keyInput = carriers.get(tag) ?? null;
    const carrierDirectives = matched.filter((m) => carriers.has(m.selector ?? ''));
    const isKeyName = (name) => keyInput !== null && name === keyInput
      && carrierDirectives.some((m) => m.inputs.hasBindingPropertyName(name));

    for (const a of (node.attributes ?? []).filter((x) => !shared.has(x))) {
      const bindingId = store.add('bindings', 'b', {
        template: tplId, node: nodeId, component: comp.id, kind: 'attribute',
        name: a.name, value: a.value ?? '', ...shift(spanOf(a)),
      });
      // A STATIC ATTRIBUTE NAMED LIKE THE CARRIER IS NOT A KEY-BEARING INPUT UNLESS THE ELEMENT IS THE
      // CARRIER. HTML has its own `translate` attribute - `<html translate="no">` opts out of machine
      // translation - and it collides with a common carrier name, so `no` was published as a
      // referenced translation key in every bootstrap index.html.
      if (carrier && (isKeyName(a.name) || carriers.has(a.name)) && a.value) {
        store.add('i18n_refs', 'k', {
          key: a.value, kind: 'translate_attribute', source: 'html',
          file: comp.template_file ?? comp.file, template: tplId, node: nodeId,
          binding: bindingId, attribute: a.name, ...shift(spanOf(a)),
        });
      }
    }
    for (const i of (node.inputs ?? []).filter((x) => !shared.has(x))) {
      emitBound(i, 'input', nodeId, tplId, comp, carrier && isKeyName(i.name), shift,
        structuralInputs.has(i.name));
    }
    for (const o of (node.outputs ?? []).filter((x) => !shared.has(x))) {
      emitBound(o, 'output', nodeId, tplId, comp, false, shift);
    }
    if (isTpl) {
      for (const a of node.templateAttrs) emitBound(a, 'template_attr', nodeId, tplId, comp, false, shift);
    }
    return nodeId;
  }

  return { extract };
}
