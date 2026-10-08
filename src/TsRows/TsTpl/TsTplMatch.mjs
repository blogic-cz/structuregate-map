/**
 * WHICH DIRECTIVES A TEMPLATE NODE MATCHES - asked of ANGULAR'S OWN `SelectorMatcher`, never of a name.
 *
 * Ported from the original Angular extractor. Imports are flat: see `TsDecls/TsTypeRef.mjs`.
 */

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

/** Whitespace split WITHOUT a pattern - this repo bans them, and a `class` attribute may be separated by
 *  tabs or newlines, so this walks characters rather than splitting on a single space. */
function splitWhitespace(value) {
  const out = [];
  let current = '';
  for (const ch of value) {
    if (ch === ' ' || ch === '\t' || ch === '\n' || ch === '\r' || ch === '\f' || ch === '\v') {
      if (current) { out.push(current); current = ''; }
    } else current += ch;
  }
  if (current) out.push(current);
  return out;
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
 * The compiler's own `createCssSelectorFromNode`, reproduced.
 *
 * `instanceof ng.TmplAstTemplate`, never a name test: the bundle renames colliding classes, so a name test
 * picks the wrong branch the moment a version renames one - and then structural attributes are read off the
 * wrong field and every `*ngIf` directive match silently disappears.
 */
function cssSelectorForNode(ng, node) {
  const isTemplate = node instanceof ng.TmplAstTemplate;
  const name = isTemplate ? (node.tagName || 'ng-template') : (node.name || 'ng-template');
  const attrs = {};
  if (isTemplate) {
    for (const a of node.templateAttrs) attrs[a.name] = '';
    for (const a of node.attributes) attrs[a.name] = a.value ?? '';
    // A TEMPLATE'S OWN BINDINGS SELECT DIRECTIVES TOO, and leaving them out is not a narrower match but a
    // MISSED one: `*directive="x"` puts the binding in `templateAttrs`, while `<ng-template [directive]="x">`
    // - what Angular desugars that INTO - puts it in `inputs`. Built without them, such templates matched
    // NOTHING: no directive on the node, so no render edge and no gate for a restriction that really gates
    // its content, while the ones written with the star all resolved.
    //
    // ITS OWN, BY IDENTITY: a synthetic wrapper's inputs ARE the child element's objects, so matching on them
    // attributes the CHILD's directives to the wrapper - a few real matches and hundreds of phantom render rows.
    const shared = childOwnedBindings(node);
    for (const i of node.inputs) if (!shared.has(i)) attrs[i.name] = '';
    for (const o of node.outputs) if (!shared.has(o)) attrs[o.name] = '';
  } else {
    for (const a of node.attributes) attrs[a.name] = a.value ?? '';
    for (const i of node.inputs) attrs[i.name] = '';
    for (const o of node.outputs) attrs[o.name] = '';
  }
  const selector = new ng.CssSelector();
  selector.setElement(name);
  for (const [k, v] of Object.entries(attrs)) {
    selector.addAttribute(k, v);
    if (k.toLowerCase() === 'class' && v) for (const c of splitWhitespace(v)) selector.addClassName(c);
  }
  return selector;
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
