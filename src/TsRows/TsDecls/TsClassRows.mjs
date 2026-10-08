/**
 * A CLASS, ITS DECORATORS, ITS MEMBERS AND WHAT ITS CONSTRUCTOR IS HANDED.
 *
 * Ported from the original Angular extractor. Imports are flat: see `TsTypeRef.mjs`.
 *
 * The member bodies are NOT walked here - that is one walk producing calls, assignments, locals and
 * returns, and it arrives with those tables.
 */
import { createHash } from 'node:crypto';

import { canonicalPath, relativeTo, slash } from './TsPaths.mjs';
import { locationOf } from './TsNodes.mjs';

/** The package a decorator must come FROM to be an Angular one. ONE identity resolved through the type
 *  checker, instead of a list of the five names - which says nothing about origin and would count a
 *  locally-defined `Component` decorator as Angular's. */
const ANGULAR_CORE = '@angular/core';

export function makeClassRows({ ts, checker, store, feRoot, modifiersOf, typeRefAt, symbolAt,
  evalNode, propName, jsdocOf, collectBody, angular }) {
  /** A type as TEXT: the annotation if there is one, otherwise what the checker computed.
   *
   *  NoTruncation, and then no truncation of our own either: a 400-character union type is what the member's
   *  type IS, and cutting it at an invented limit made the longest types - the ones a consumer most needs
   *  spelled out - the only ones the map lied about. */
  const typeText = (node, typeNode) => {
    if (typeNode) return typeNode.getText();
    try {
      // AS THE CHECKER PRINTS IT, union order included. It interns a union's members in the order the
      // types are first asked for, so a full run and a partial one print some of these differently -
      // and putting them in one order moved this column away from the tool being reproduced. See the
      // note in `TsMap`'s describer.
      return checker.typeToString(checker.getTypeAtLocation(node), node, ts.TypeFormatFlags.NoTruncation);
    } catch {
      return null;
    }
  };

  /**
   * WHERE A NAME IS DECLARED, as the map spells paths.
   *
   * ONE SYMBOL, POSSIBLY SEVERAL DECLARATIONS - merged interfaces, an overload set, an ambient
   * augmentation. The first is the primary answer so joins are unchanged, and the others are listed rather
   * than dropped: "declared here" and "declared here and nowhere else" are different claims.
   */
  const declFileOf = (node) => {
    const sym = symbolAt(node);
    const decls = sym?.declarations ?? [];
    if (!decls.length || !sym) return null;
    // The checker's order is kept - see the note in `TsMap`'s describer.
    const paths = [...new Set(decls.map((d) => canonicalPath(d.getSourceFile().fileName)))];
    const [first, ...rest] = paths;
    return {
      name: sym.getName(), file: relativeTo(feRoot, first), external: first.includes('node_modules'),
      ...(rest.length ? { also: rest.map((p) => relativeTo(feRoot, p)) } : {}),
    };
  };

  /** Does this decorator identifier RESOLVE into @angular/core? By SYMBOL, so a same-named local decorator
   *  is never mistaken for Angular's - and ANY declaration, not the first: a re-exported or merged symbol
   *  lists several, and testing only the first answers "no" for a name the framework declares one entry on. */
  const fromAngularCore = (node) => {
    const decls = symbolAt(node)?.declarations ?? [];
    return decls.some((d) => slash(d.getSourceFile().fileName).includes(`node_modules/${ANGULAR_CORE}/`));
  };

  /** The 1-based line of each property VALUE in a decorator's object-literal argument. Evaluation throws
   *  positions away, and an inline `template:` string needs its line so its spans can be placed in the .ts
   *  file: nearly all inline-template binding rows once resolved to the wrong source line. */
  const argLinesOf = (arg) => {
    if (!arg || !ts.isObjectLiteralExpression(arg)) return undefined;
    const sf = arg.getSourceFile();
    const out = {};
    for (const p of arg.properties) {
      if (!ts.isPropertyAssignment(p)) continue;
      out[propName(p.name)] = sf.getLineAndCharacterOfPosition(p.initializer.getStart()).line + 1;
    }
    return Object.keys(out).length ? out : undefined;
  };

  const decoratorsOf = (node) => {
    const list = ts.canHaveDecorators(node) ? (ts.getDecorators(node) ?? []) : [];
    return list.map((d) => {
      const expr = d.expression;
      if (ts.isCallExpression(expr)) {
        return {
          name: expr.expression.getText(),
          args: expr.arguments.map((a) => evalNode(a)),
          argLines: argLinesOf(expr.arguments[0]),
          fromAngular: fromAngularCore(expr.expression),
        };
      }
      return { name: expr.getText(), args: [], fromAngular: fromAngularCore(expr) };
    });
  };

  const typeIdentifier = (typeNode) => (ts.isTypeReferenceNode(typeNode) ? typeNode.typeName : null);

  function paramsOf(node) {
    return [...(node.parameters ?? [])].map((p) => ({
      name: p.name.getText(),
      type: typeText(p, p.type),
      optional: !!p.questionToken || !!p.initializer,
      rest: !!p.dotDotDotToken,
      default: p.initializer ? evalNode(p.initializer) : undefined,
      decorators: decoratorsOf(p),
      resolved: p.type ? declFileOf(typeIdentifier(p.type) ?? p.type) : null,
      type_ref: p.type ? typeRefAt(p.type) : null,
    }));
  }

  function heritageOf(node) {
    let extendsInfo = null;
    const implementsInfo = [];
    for (const h of node.heritageClauses ?? []) {
      for (const t of h.types) {
        const info = { text: t.expression.getText(), ...(declFileOf(t.expression) ?? {}) };
        if (h.token === ts.SyntaxKind.ExtendsKeyword) extendsInfo = info;
        else implementsInfo.push(info);
      }
    }
    return { extendsInfo, implementsInfo };
  }

  const kindOfMember = (m) => (ts.isPropertyDeclaration(m) ? 'property'
    : ts.isMethodDeclaration(m) ? 'method'
      : ts.isGetAccessor(m) ? 'getter'
        : ts.isSetAccessor(m) ? 'setter'
          : ts.isConstructorDeclaration(m) ? 'constructor'
            : (ts.SyntaxKind[m.kind] ?? 'unknown'));

  /** What the Angular pass needs to know about a member: its name, its decorators, its declared type and
   *  its evaluated value - the last two are how the SIGNAL input/output API is recognised. */
  function memberSummary(ts_, m, name, decorators, memberType, value) {
    return { name, decorators, type: memberType, value };
  }

  function memberRow(m, classId, fid) {
    const mods = modifiersOf(m);
    const decorators = decoratorsOf(m);
    const initializer = ts.isPropertyDeclaration(m) && m.initializer ? m.initializer : null;
    const value = initializer ? evalNode(initializer) : undefined;
    const memberType = m.type ? m.type.getText() : null;
    const kind = kindOfMember(m);
    const name = m.name ? propName(m.name) : kind;
    // A FUNCTION-VALUED PROPERTY TAKES PARAMETERS like a method does: `check = (id: ItemId) => ...` is
    // called exactly as `check(id: ItemId) {...}` is, and without its parameters every reader that
    // follows a call into its callee's returns - the gate's list and predicate readers - passed it by.
    const fnValued = initializer && (ts.isArrowFunction(initializer) || ts.isFunctionExpression(initializer));
    const isSignature = ts.isMethodDeclaration(m) || ts.isConstructorDeclaration(m) || ts.isSetAccessor(m);
    return store.add('members', 'm', {
      class: classId, file: fid, kind, name,
      static: mods.static, readonly: mods.readonly, visibility: mods.visibility,
      optional: !!m.questionToken,
      type: kind === 'property' ? typeText(m, m.type) : memberType,
      // AN ANNOTATION IF THERE IS ONE, OTHERWISE THE PROPERTY'S OWN TYPE - but never a METHOD's. Asking the
      // checker what type a method declaration is answers with its own function type whose symbol is the
      // method: every un-annotated method resolved to itself. For a PROPERTY the same question
      // is exactly right - `items$ = new BehaviorSubject<Item[]>([])` has no annotation and a fully
      // known type, and refusing it left every RxJS-style member unresolvable.
      type_ref: m.type ? typeRefAt(m.type) : kind === 'property' ? typeRefAt(m) : null,
      value,
      params: isSignature ? paramsOf(m) : fnValued ? paramsOf(initializer) : undefined,
      decorators,
      jsdoc: jsdocOf(m),
      ...locationOf(m),
    });
  }

  function extractClass(node, fid) {
    let injectsTemplateRef = false;
    const mods = modifiersOf(node);
    const decorators = decoratorsOf(node);
    const { extendsInfo, implementsInfo } = heritageOf(node);
    const className = node.name?.getText() ?? '(anonymous)';
    const classId = store.add('classes', 'c', {
      file: fid, name: className,
      // WHAT THIS ROW DESCRIBES, HASHED. The file's digest changes when anything in the file changes; a
      // class's changes only when THIS declaration does. `getText()` starts at the node's own start - its
      // first DECORATOR - so `@Component({...})` metadata is inside the hash exactly as it is in the row.
      content_hash: createHash('sha256').update(node.getText(), 'utf8').digest('hex'),
      abstract: mods.abstract, exported: mods.exported,
      extends: extendsInfo, implements: implementsInfo,
      decorators: decorators.map((d) => d.name),
      angular: decorators.filter((d) => d.fromAngular).map((d) => d.name),
      jsdoc: jsdocOf(node),
      ...locationOf(node),
    });
    for (const d of decorators) {
      store.add('class_decorators', 'cd', { class: classId, file: fid, name: d.name, args: d.args });
    }

    const members = [];
    const summaries = [];
    for (const m of node.members) {
      const memberId = memberRow(m, classId, fid);
      members.push({ id: memberId, node: m });
      const mInit = ts.isPropertyDeclaration(m) && m.initializer ? m.initializer : null;
      summaries.push(memberSummary(ts, m,
        m.name ? propName(m.name) : kindOfMember(m), decoratorsOf(m),
        m.type ? m.type.getText() : null, mInit ? evalNode(mInit) : undefined));
      // A MEMBER CAN BE A FUNCTION: `handler = () => {...}` deserves the same walk as a method's body. ONE
      // walk for calls, assignments, locals and returns, and the rows carry `class` as well as `member` - a
      // gate names a property of the COMPONENT and the assignment explaining it usually sits in a different
      // member, so a consumer joining from a gate has only the class to join on.
      const initializer = ts.isPropertyDeclaration(m) && m.initializer ? m.initializer : null;
      const propertyFn = initializer
        && (ts.isArrowFunction(initializer) || ts.isFunctionExpression(initializer))
        ? initializer.body : undefined;
      const body = m.body ?? propertyFn;
      if (body) collectBody(body, memberId, classId);
      if (ts.isConstructorDeclaration(m)) {
        for (const p of paramsOf(m)) {
          // A DIRECTIVE IS STRUCTURAL WHEN IT TAKES THE TEMPLATE, and that is a DECLARATION, not a naming
          // convention: `TemplateRef` resolved to Angular's own `.d.ts`. It is what separates a desugared
          // `<ng-template [demoIf]>` - a real gate - from an attribute directive that happens to
          // be bound there.
          const ref = p.type_ref;
          if (ref && ref.name === 'TemplateRef' && String(ref.file || '').includes('@angular/core')) {
            injectsTemplateRef = true;
          }
          store.add('di', 'd', {
            class: classId, file: fid, param: p.name, type: p.type,
            resolved: p.resolved, type_ref: p.type_ref,
            decorators: p.decorators.map((x) => x.name), optional: p.optional,
          });
        }
      }
    }
    // THE ANGULAR TABLES, from the same decorators and the same member summaries. `io` first, because it is
    // what selector matching and binding validation are both built on.
    let io = { inputs: [], outputs: [] };
    let angularRows = [];
    if (angular) {
      io = angular.registerIo({ classId, className, fid, members: summaries });
      angularRows = angular.registerAngular({
        classId, fid, filePath: node.getSourceFile().fileName, className, decorators,
      });
    }
    return { classId, decorators, members, name: className, io, angularRows, injectsTemplateRef };
  }

  return { extractClass, paramsOf, typeText, declFileOf, decoratorsOf };
}
