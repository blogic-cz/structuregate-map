/**
 * ANGULAR DECORATOR METADATA as its own tables - what a component IS, not what its decorator looks like.
 *
 * Ported from the original Angular extractor. The decorator arguments were already EVALUATED
 * by the value evaluator, so `declarations`, `imports` and `providers` arrive as resolved `$ref`/`$call`
 * structures rather than as source text. Imports are flat: see `TsTypeRef.mjs`.
 */
import { existsSync } from 'node:fs';
import path from 'node:path';

import { canonicalPath, relativeTo, slash } from './TsPaths.mjs';

const asArray = (v) => (Array.isArray(v) ? v : v === undefined ? [] : [v]);
const asString = (v) => (typeof v === 'string' ? v : null);

/**
 * NgModule metadata arrays are FLATTENED BY ANGULAR, at any nesting depth.
 *
 * A module writes `declarations: [..._components]` (spread - already flat) and `exports: [_components]`
 * (nested - one element that IS an array), and both mean the same thing to the compiler. Read without
 * flattening, the second becomes a single unrecognisable entry and the module reads as exporting NOTHING:
 * render rows across several modules were stamped out-of-scope for components that genuinely render.
 */
const flatRefs = (v) => {
  const out = [];
  const walk = (x) => {
    if (Array.isArray(x)) {
      for (const y of x) walk(y);
      return;
    }
    if (x !== undefined) out.push(x);
  };
  walk(v);
  return out;
};

/** `input` / `input.required` / `ns.output` -> 'input' | 'output' | null, by dotted-segment EQUALITY and
 *  never by pattern. */
export function signalKind(callText) {
  const segments = callText.split('.');
  const last = segments[segments.length - 1] ?? '';
  const base = last === 'required' && segments.length > 1 ? segments[segments.length - 2] ?? '' : last;
  return base === 'input' || base === 'output' ? base : null;
}

/** The `$call` text of an evaluated call literal, or null. */
export function callTextOf(v) {
  if (v === null || v === undefined || typeof v !== 'object' || Array.isArray(v)) return null;
  return typeof v.$call === 'string' ? v.$call : null;
}

/** An evaluated metadata entry as a reference: a resolved `$ref`, a `$call`, an expression, or a value. */
function refName(v) {
  if (v === undefined || v === null) return null;
  if (typeof v === 'string') return { name: v };
  if (typeof v !== 'object' || Array.isArray(v)) return { value: v };
  if (typeof v.$ref === 'string') return { name: v.$ref, file: typeof v.file === 'string' ? v.file : null };
  if (typeof v.$call === 'string') return { call: v.$call, args: Array.isArray(v.$args) ? v.$args : [] };
  if (typeof v.$expr === 'string') return { expr: v.$expr };
  return { value: v };
}

/** The decorator's first argument as an object literal, or `{}` when it has none. */
function metaOf(d) {
  const first = d.args[0];
  return first !== undefined && first !== null && typeof first === 'object' && !Array.isArray(first)
    ? first : {};
}

export function makeAngularRows({ store, feRoot, diag }) {
  function templateOf(meta, filePath, className) {
    const templateUrl = asString(meta.templateUrl);
    if (templateUrl === null) return { templateFile: null, templateAbs: null };
    const resolved = path.resolve(path.dirname(filePath), templateUrl);
    if (!existsSync(resolved)) {
      diag.note('template_url_missing');
      return { templateFile: null, templateAbs: null };
    }
    // CANONICAL CASING, like every other file key: a `templateUrl` is resolved against the .ts file's
    // directory, which carries the import specifier's spelling. Keyed raw, the same .html got a SECOND row
    // and was then re-parsed as a phantom orphan template - render rows with no owner.
    const abs = canonicalPath(resolved);
    // `path` is derived from `abs` and not from `resolved`: canonicalising only the key left one row with a
    // lowercase `path` and a correct `abs`, so the identity was right and the field a consumer opens was
    // still the import specifier's spelling.
    const templateFile = store.intern('files', 'f', abs, () => ({
      path: relativeTo(feRoot, abs), abs, ext: 'html', project: null,
    }));
    return { templateFile, templateAbs: abs };
  }

  /** The decorators of one class -> its `components`/`directives`/`pipes`/`injectables`/`ng_modules` rows. */
  function registerAngular({ classId, fid, filePath, className, decorators }) {
    const out = [];
    for (const d of decorators) {
      const meta = metaOf(d);
      if (d.name === 'Component' || d.name === 'Directive') {
        const { templateFile, templateAbs } = templateOf(meta, filePath, className);
        const inline = asString(meta.template);
        // The line the inline template's backtick sits on: template spans are relative to the template
        // TEXT, so this is what turns a template row back into a real .ts file:line.
        const inlineLine = inline === null ? null : (d.argLines?.template ?? null);
        const id = store.add(d.name === 'Component' ? 'components' : 'directives', 'ng', {
          class: classId, file: fid, name: className,
          selector: asString(meta.selector),
          selector_dynamic: typeof meta.selector === 'object' ? meta.selector : undefined,
          standalone: meta.standalone === true,
          template_file: templateFile,
          template_abs: templateAbs ? slash(templateAbs) : null,
          inline_template: inline,
          inline_template_line: inlineLine,
          // BOTH SPELLINGS. Angular 17 added the singular `styleUrl` beside `styleUrls`, and reading only
          // the plural dropped the stylesheet of every component written the new way - dozens of files, each
          // publishing an empty list while naming a real .scss one line above. The two are mutually
          // exclusive in a decorator, so concatenating them cannot double-count.
          style_urls: [...asArray(meta.styleUrls), meta.styleUrl].filter((s) => typeof s === 'string'),
          change_detection: meta.changeDetection ?? null,
          encapsulation: meta.encapsulation ?? null,
          export_as: meta.exportAs ?? null,
          host: meta.host ?? null,
          inputs_meta: asArray(meta.inputs),
          outputs_meta: asArray(meta.outputs),
          imports: flatRefs(meta.imports).map(refName),
          providers: flatRefs(meta.providers).map(refName),
          view_providers: flatRefs(meta.viewProviders).map(refName),
          animations_present: meta.animations !== undefined,
          is_component: d.name === 'Component',
        });
        out.push({
          id, kind: d.name === 'Component' ? 'component' : 'directive',
          selector: asString(meta.selector), templateAbs, inline, inlineLine, classId, className, fid,
        });
      } else if (d.name === 'Pipe') {
        store.add('pipes', 'ng', {
          class: classId, file: fid, name: className,
          pipe_name: asString(meta.name), pure: meta.pure ?? true, standalone: meta.standalone === true,
        });
      } else if (d.name === 'Injectable') {
        store.add('injectables', 'ng', {
          class: classId, file: fid, name: className, provided_in: meta.providedIn ?? null,
        });
      } else if (d.name === 'NgModule') {
        store.add('ng_modules', 'ng', {
          class: classId, file: fid, name: className,
          declarations: flatRefs(meta.declarations).map(refName),
          imports: flatRefs(meta.imports).map(refName),
          exports: flatRefs(meta.exports).map(refName),
          providers: flatRefs(meta.providers).map(refName),
          bootstrap: flatRefs(meta.bootstrap).map(refName),
          schemas: flatRefs(meta.schemas).map(refName),
        });
      }
    }
    return out;
  }

  /** `@Input()` / `@Output()` members - and the signal API that replaced them - as the `io` table, which is
   *  what selector matching and binding validation are both built on. */
  function registerIo({ classId, className, fid, members }) {
    const io = { inputs: [], outputs: [] };
    for (const m of members) {
      for (const d of m.decorators) {
        if (d.name !== 'Input' && d.name !== 'Output') continue;
        const first = d.args[0];
        const opts = first !== null && typeof first === 'object' && !Array.isArray(first) ? first : null;
        const alias = typeof first === 'string' ? first : asString(opts?.alias);
        const bindingName = alias ?? m.name;
        store.add('io', 'io', {
          class: classId, component: className, file: fid, member: m.name,
          binding_name: bindingName, alias: alias ?? null, type: m.type,
          required: opts?.required === true, kind: d.name.toLowerCase(),
        });
        (d.name === 'Input' ? io.inputs : io.outputs).push(bindingName);
      }
      // The SIGNAL API: `foo = input<T>()` / `output<T>()`. The call name is an already-extracted
      // identifier, compared by dotted-segment equality - no pattern matching.
      const call = callTextOf(m.value);
      const kind = call === null ? null : signalKind(call);
      if (kind !== null) {
        store.add('io', 'io', {
          class: classId, component: className, file: fid, member: m.name,
          binding_name: m.name, alias: null, type: m.type, kind, signal: true,
        });
        (kind === 'input' ? io.inputs : io.outputs).push(m.name);
      }
    }
    return io;
  }

  return { registerAngular, registerIo };
}
