/**
 * WHAT ONE TEMPLATE EXPRESSION PRODUCES: its own row, the gate it is when it decides what renders, every
 * string in it, and the translation keys the AST PROVES.
 *
 * Ported from the emit half of the original Angular extractor, split out of
 * `TsTplExtract.mjs` because the two together are past this repo's 500-line ceiling. Imports are flat: see
 * `TsDecls/TsTypeRef.mjs`.
 */
import { dedupeSummary, summarizeExpr } from './TsExprSummary.mjs';
import { collectLiteralStrings, dynamicKeyParts, pipeArgStrings, spanOf, transformedByPipe } from './TsTplKeys.mjs';

/**
 * WHICH BINDING TYPES ARE CLASS AND STYLE - asked of the compiler, never written as 2 and 3.
 *
 * `BoundAttribute.type` is a `BindingType`, and @angular/compiler does not export that enum, so the numbers
 * cannot be imported. A one-line template is parsed at startup and the values are READ OFF the result:
 * whatever number Angular gives `[class.x]` is the class type here. A version that renumbers the enum is
 * followed automatically, and there is no literal to defend.
 */
export function probeBindingTypes(ng) {
  try {
    const probe = ng.parseTemplate('<i [class.a]="v" [style.b]="v"></i>', 'binding-type-probe.html', {});
    const element = probe.nodes[0] ?? null;
    const byName = new Map((element?.inputs ?? []).map((i) => [i.name, i.type]));
    return { classType: byName.get('a'), styleType: byName.get('b') };
  } catch {
    return { classType: undefined, styleType: undefined };
  }
}

export function makeEmitters({ store, normalizeExpr, carriers, gateInputs, classType, styleType }) {
  const carrierNames = () => new Set(carriers.keys());

  function emitRender(m, comp, tplId, nodeId, tag, span) {
    store.add('renders', 'rd', {
      from_component: comp.id, from_class: comp.class, to: m.id, to_class: m.classId,
      to_name: m.name, kind: m.is_component ? 'component' : 'directive',
      // HOW THE EDGE WAS PROVED, on every row rather than only the ones that need explaining: a component
      // created in code is a render too, and a field present on one half of a table makes its absence on the
      // other half unreadable - "static, or written before the field existed?".
      via: 'element',
      template: tplId, node: nodeId, tag, ...span,
    });
  }

  function emitExpr({ tplId, nodeId, comp, kind, name, norm, src, gate = false, gateKind = null,
    binding = null, carrier = false, span = {} }) {
    const summary = dedupeSummary(summarizeExpr(norm));
    const exprId = store.add('expressions', 'x', {
      template: tplId, node: nodeId, component: comp.id, binding, kind, name,
      source: src ?? null, ast: norm, ...summary,
    });
    if (gate) {
      store.add('gates', 'g', {
        template: tplId, node: nodeId, component: comp.id, expression: exprId,
        kind, gate_kind: gateKind, name, source: src ?? null, reads: summary.reads,
        identifiers: summary.identifiers, calls: summary.calls, strings: summary.strings, ...span,
      });
    }
    // Every string literal in the expression - unfiltered, exactly as on the TypeScript side.
    for (const s of summary.strings) {
      store.add('template_strings', 'sg', {
        value: s, template: tplId, node: nodeId, component: comp.id, expression: exprId,
        // The INPUT or ATTRIBUTE NAME the string came through - NOT a bindings id. Published in one column
        // called `binding`, the derived join spec advertised a foreign key that resolved for a handful of rows.
        input: name, kind, pipes: summary.pipes,
      });
    }

    // An i18n reference is claimed only where the AST proves it: a translate pipe's argument, or a binding
    // on an element that IS the carrier.
    const keys = pipeArgStrings(carrierNames(), norm);
    // The carrier element's own strings, collected the SAME way as a pipe's: the flat summary lists every
    // literal including the halves of a `+`, so `[text]="base + '.title'"` published `.title` as a key.
    if (carrier && !transformedByPipe(norm, carrierNames())) {
      collectLiteralStrings(norm, (v) => keys.push({ value: v, pipe: null, via: 'carrier_element' }),
        new Set(), true);
    }
    for (const p of keys) {
      store.add('i18n_refs', 'k', {
        key: p.value, kind: p.pipe ? 'translate_pipe' : (p.via ?? 'translate_binding'),
        pipe: p.pipe ?? null, source: 'html', file: comp.template_file ?? comp.file, template: tplId,
        node: nodeId, expression: exprId, input: name,
      });
    }

    // A key built at run time: the literal parts and the expression, never a fabricated key.
    //
    // ONLY WHERE THE BINDING IS A CARRIER. Run on EVERY binding, a `[height]="'calc(' + n + 'px)'"` and an
    // `[href]="'mailto:' + x"` were filed as dynamic translation keys - a thousand rows, mostly `id`, `for` and
    // CSS. They carried a null key so no count was wrong, but the TABLE claimed an i18n reference the AST
    // never proved, which is the one rule this map is built on broken by the row's own existence.
    const piped = (carrier || keys.length) && transformedByPipe(norm, carrierNames());
    const parts = [];
    if (piped) {
      collectLiteralStrings(norm, (v) => parts.push(v), new Set(), true);
      parts.push(null);
    }
    const dynamic = piped ? parts : (carrier || keys.length) ? dynamicKeyParts(norm) : null;
    if (dynamic) {
      store.add('i18n_refs', 'k', {
        key: null, literal_parts: dynamic, kind: 'dynamic_key', source: 'html',
        file: comp.template_file ?? comp.file, template: tplId, node: nodeId, expression: exprId,
      });
    }
    return { exprId, keys: keys.map((p) => p.value) };
  }

  /**
   * One bound attribute, event or structural attribute.
   *
   * WHY a binding is a gate, not just whether - four reasons of different strength, so they are labelled
   * rather than merged. `structural` and `class`/`style` are proved by the compiler; `declared_input` is the
   * caller's judgement, supplied through the config.
   *
   * The old test was `name.startsWith('class.')`, which never matched anything: Angular strips the prefix
   * into `type`, so the binding's name is `is-active`. Class and style bindings were therefore never
   * recorded as gates at all - hundreds of rows the map silently lacked.
   */
  function emitBound(attr, kind, nodeId, tplId, comp, carrier, shift, structural = false) {
    // An input carries `value`, an output carries `handler` - take whichever this binding actually has.
    const expression = attr.value ?? attr.handler;
    const norm = expression ? normalizeExpr(expression) : null;
    const source = expression?.source ?? null;
    const span = shift(spanOf(attr));
    const bindingId = store.add('bindings', 'b', {
      template: tplId, node: nodeId, component: comp.id, kind,
      name: attr.name, binding_type: attr.type ?? null, source, ...span,
    });
    if (!norm) return { source, exprId: null, keys: [], bindingId };
    const gateKind = (kind === 'template_attr' || structural) ? 'structural'
      : (classType !== undefined && attr.type === classType) ? 'class'
        : (styleType !== undefined && attr.type === styleType) ? 'style'
          : gateInputs.has(attr.name) ? 'declared_input'
            : null;
    const e = emitExpr({
      tplId, nodeId, comp, kind, name: attr.name, norm, src: source, gate: gateKind !== null, gateKind,
      binding: bindingId, carrier, span,
    });
    return { source, exprId: e.exprId, keys: e.keys, bindingId };
  }

  return { emitRender, emitExpr, emitBound };
}
