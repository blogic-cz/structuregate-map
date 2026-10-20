/**
 * WHICH DIRECTIVES A TEMPLATE NODE MATCHES - asked of ANGULAR'S OWN `SelectorMatcher`, never of a name.
 *
 * Ported from the original Angular extractor. Imports are flat: see `TsDecls/TsTypeRef.mjs`.
 */

import { probeBindingTypes } from './TsTplEmit.mjs';

export function buildMatcher(ng, directives, diag) {
  const matcher = new ng.SelectorMatcher();
  let added = 0;
  for (const d of directives) {
    if (!d.selector) {
      diag?.note('directive_without_selector', { name: d.name, file: d.file });
      continue;
    }
    try {
      matcher.addSelectables(ng.CssSelector.parse(d.selector), d);
      added += 1;
    } catch {
      diag?.note('selector_unparseable', { name: d.name, selector: d.selector });
    }
  }
  return { matcher, added };
}

/**
 * Bindings a node's CHILDREN already own, by object IDENTITY.
 *
 * Desugaring `<div *ngIf="x" class="a">` produces a synthetic `Template` wrapper that carries the structural
 * attribute AND THE SAME binding OBJECTS as the real child element - `wrapper.inputs[0] === child.inputs[0]`
 * is literally true. Walking both emitted every such binding twice: about a quarter of the binding rows,
 * cascading into expressions, gates and i18n refs. Identity is the exact test: it needs no rule about which
 * node "should" own a binding, and it leaves an EXPLICIT `<ng-template [foo]="x">` fully intact.
 */
export function childOwnedBindings(node) {
  const owned = new Set();
  for (const child of node.children ?? []) {
    for (const a of child.attributes ?? []) owned.add(a);
    for (const i of child.inputs ?? []) owned.add(i);
    for (const o of child.outputs ?? []) owned.add(o);
  }
  return owned;
}

/**
 * WHAT ANGULAR MATCHES A NODE AGAINST: the compiler's OWN `createCssSelectorFromNode`, never a reproduction (rows 70).
 *
 * A desugared `*dir` wrapper is a `Template` whose `tagName` is the pre-desugar tag, and Angular matches it by its
 * `templateAttrs` ALONE, as `ng-template`: its attributes, inputs and outputs are the child element's own objects,
 * and the child is matched on its own. A reproduction that gave the wrapper the tag and the child's attributes made
 * `<button ripple *ngIf="on">` instantiate `RippleDirective` on the wrapper AND the button, and a component selector
 * match the wrapper - thousands of phantom directive cells on a real tree. An element takes only its Property and
 * TwoWay inputs (`[class.x]`, `[attr.x]` and `[style.x]` select nothing), and no i18n attribute.
 * `[demoIf]` on an explicit `<ng-template>` is its own input and still matches.
 */
function cssSelectorForNode(ng, node) {
  if (typeof ng.createCssSelectorFromNode === 'function') return ng.createCssSelectorFromNode(node);
  // A COMPILER THAT DOES NOT EXPORT IT is matched by the same rule, written out from Angular's
  // `getAttrsForDirectiveMatching`: `matchNode` swallows a throw, so a missing export would match NOTHING, silently.
  const attrs = {};
  if (node instanceof ng.TmplAstTemplate && node.tagName !== 'ng-template') {
    for (const a of node.templateAttrs) attrs[a.name] = '';
  } else {
    for (const a of node.attributes) if (a.name !== 'i18n' && !a.name.startsWith('i18n-')) attrs[a.name] = a.value ?? '';
    // `BindingType` is not exported: the property type is read off a parsed probe. A compiler this old has no
    // TwoWay type - `[(x)]` is a property input and an output there.
    const { propertyType } = probedOnce(ng);
    for (const i of node.inputs) if (i.type === propertyType) attrs[i.name] = '';
    for (const o of node.outputs) attrs[o.name] = '';
  }
  const selector = new ng.CssSelector();
  selector.setElement(withoutNamespace(node instanceof ng.TmplAstElement ? node.name : 'ng-template'));
  for (const [k, v] of Object.entries(attrs)) {
    selector.addAttribute(withoutNamespace(k), v);
    if (k.toLowerCase() === 'class') for (const c of words(v)) selector.addClassName(c);
  }
  return selector;
}

const probed = new WeakMap();
/** The binding types, probed once per compiler rather than once per node. */
function probedOnce(ng) {
  if (!probed.has(ng)) probed.set(ng, probeBindingTypes(ng));
  return probed.get(ng);
}

/** `:svg:rect` -> `rect`, as Angular's `splitNsName`. */
const withoutNamespace = (name) => (name.startsWith(':') ? name.slice(name.indexOf(':', 1) + 1) : name);

/** The whitespace-separated words of a class list. */
function words(text) {
  const out = [];
  let word = '';
  for (const ch of text) {
    if (ch === ' ' || ch === '\t' || ch === '\n' || ch === '\r' || ch === '\f') { if (word) out.push(word); word = ''; } else word += ch;
  }
  if (word) out.push(word);
  // Angular splits the TRIMMED text, and an empty one splits into one empty name: kept, so both build one selector.
  return out.length ? out : [''];
}

/**
 * Matched directives for a node, DEDUPED BY id.
 *
 * `addSelectables` registers one entry per comma-separated selector, so a directive declared
 * `selector: '[demo], demo'` fires the callback once per matching alternative and the same class
 * arrives twice. Left unmerged it multiplied render rows and made an element look like it instantiated the
 * same component several times.
 */
export function matchNode(ng, matcher, node) {
  const byId = new Map();
  try {
    const selector = cssSelectorForNode(ng, node);
    // BOUND, NOT CALLED IN PLACE. This is Angular's `SelectorMatcher`, and its method is spelled exactly
    // like the pattern call this repo's build bans over these folders - a literal `findstr` cannot tell the
    // compiler's selector API from a regex, and would fail the build on this line. Binding it keeps the ban
    // literal, which is the property that makes it worth having.
    const matchSelector = matcher.match.bind(matcher);
    matchSelector(selector, (_s, meta) => {
      if (!byId.has(meta.id)) byId.set(meta.id, meta);
    });
  } catch { /* an unmatchable element is reported by the caller as unmatched, never silently dropped */ }
  return [...byId.values()];
}
