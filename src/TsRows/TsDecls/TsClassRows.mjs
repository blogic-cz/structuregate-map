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
import { makeLibPipes } from './TsLibPipes.mjs';

/** The package a decorator must come FROM to be an Angular one. ONE identity resolved through the type
 *  checker, instead of a list of the five names - which says nothing about origin and would count a
 *  locally-defined `Component` decorator as Angular's. */
const ANGULAR_CORE = '@angular/core';

export function makeClassRows({ ts, checker, store, feRoot, modifiersOf, typeRefAt, symbolAt,
  evalNode, evalCell, propName, jsdocOf, collectBody, angular }) {
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
  const declFileOf = (node) => declFileOfSymbol(symbolAt(node));
  const libPipesOf = makeLibPipes({ ts, checker, symbolAt, feRoot, propName });
  const declFileOfSymbol = (sym) => {
    const decls = sym?.declarations ?? [];
    if (!decls.length || !sym) return null;
    // The checker's order is kept - see the note in `TsMap`'s describer.
    const paths = [...new Set(decls.map((d) => canonicalPath(d.getSourceFile().fileName)))];
    const [first, ...rest] = paths;
    // THE CLASS OR INTERFACE A MEMBER IS DECLARED IN: `get` on `HttpClient` and `get` on `HttpHeaders` are one name in
    // one `.d.ts`, and only the owner tells a request from a header read (`rows/http/calls.rs`).
    const holder = decls[0].parent;
    const owner = holder && (ts.isClassDeclaration(holder) || ts.isInterfaceDeclaration(holder)) && holder.name ? holder.name.text : '';
    return {
      name: sym.getName(), file: relativeTo(feRoot, first), external: first.includes('node_modules'),
      ...(owner ? { owner } : {}),
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

  /** Does this identifier RESOLVE into @ngrx? By SYMBOL, as `fromAngularCore` is, so a same-named local helper is never
   *  mistaken for the library's `createEffect`. */
  const fromNgrx = (node) => (symbolAt(node)?.declarations ?? [])
    .some((d) => slash(d.getSourceFile().fileName).includes('/node_modules/@ngrx/'));

  /** The 1-based line of each property VALUE in a decorator's object-literal argument. Evaluation throws
   *  positions away, and an inline `template:` string needs its line so its spans can be placed in the .ts
   *  file: nearly all inline-template binding rows once resolved to the wrong source line.
   *
   *  A VALUE HELD IN A CONST (`template: tpl`, or the shorthand `template`) IS PLACED AT THE CONST'S INITIALIZER when
   *  that sits in this file: its text is there, and the decorator's line put every span of it on the wrong line. An
   *  IMPORTED const keeps the decorator's line - its text is in another file, which a span here cannot point into. */
  const argLinesOf = (arg) => {
    if (!arg || !ts.isObjectLiteralExpression(arg)) return undefined;
    const sf = arg.getSourceFile();
    const out = {};
    for (const p of arg.properties) {
      const at = ts.isPropertyAssignment(p) ? p.initializer : ts.isShorthandPropertyAssignment(p) ? p.name : null;
      if (!at) continue;
      const sym = ts.isShorthandPropertyAssignment(p) ? checker.getShorthandAssignmentValueSymbol(p)
        : ts.isIdentifier(at) ? checker.getSymbolAtLocation(at) : undefined;
      const decl = (sym?.declarations ?? []).find((d) => ts.isVariableDeclaration(d) && d.initializer);
      const node = decl && decl.getSourceFile() === sf ? decl.initializer : at;
      out[propName(p.name)] = sf.getLineAndCharacterOfPosition(node.getStart()).line + 1;
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

  /** THE `@Inject(TOKEN)` ARGUMENT of a constructor parameter, or null: the decorator must RESOLVE into
   *  @angular/core, so a local decorator that is also called `Inject` is never read as Angular's. */
  const injectTokenOf = (param) => {
    for (const d of ts.getDecorators(param) ?? []) {
      const e = d.expression;
      if (ts.isCallExpression(e) && e.arguments.length && ts.isIdentifier(e.expression)
        && e.expression.text === 'Inject' && fromAngularCore(e.expression)) return e.arguments[0];
    }
    return null;
  };

  /** EVERY `inject(...)` OF A PROPERTY INITIALIZER, in source order. A function-valued initializer is a
   *  deferred body, run in whatever context calls it, so it is not walked: `lazy = () => inject(X)` is not a
   *  dependency of the class. */
  const injectCallsOf = (init) => {
    const out = [];
    const visit = (n) => {
      if (ts.isArrowFunction(n) || ts.isFunctionExpression(n) || ts.isClassExpression(n)) return;
      if (ts.isCallExpression(n) && ts.isIdentifier(n.expression) && n.expression.text === 'inject'
        && fromAngularCore(n.expression)) out.push(n);
      ts.forEachChild(n, visit);
    };
    visit(init);
    return out;
  };

  const typeIdentifier = (typeNode) => (ts.isTypeReferenceNode(typeNode) ? typeNode.typeName : null);

  /** Every member of a union as WRITTEN, `(A | B) | null` flattened, or the type node alone when it is no union. */
  const unionParts = (typeNode) => {
    if (ts.isParenthesizedTypeNode(typeNode)) return unionParts(typeNode.type);
    return ts.isUnionTypeNode(typeNode) ? typeNode.types.flatMap(unionParts) : [typeNode];
  };

  /**
   * WHERE THE NAME A PARAMETER'S TYPE IS WRITTEN WITH IS DECLARED. A union (`Codes | string`, `Codes | null`) has no
   * name of its own, so it asked the symbol of the union node and resolved nothing (#88): its members are asked, and
   * the ONE declaration they name is the answer. Several (`A | B`) are no one answer - `type_refs` lists them.
   */
  const resolvedOf = (typeNode) => {
    const parts = unionParts(typeNode);
    if (parts.length === 1) return declFileOf(typeIdentifier(typeNode) ?? typeNode);
    const named = new Map();
    for (const part of parts) {
      const ref = typeIdentifier(part) ? declFileOf(typeIdentifier(part)) : null;
      if (ref) named.set(`${ref.file}\u0000${ref.name}`, ref);
    }
    return named.size === 1 ? [...named.values()][0] : null;
  };

  function paramsOf(node) {
    return [...(node.parameters ?? [])].map((p) => ({
      name: p.name.getText(),
      type: typeText(p, p.type),
      optional: !!p.questionToken || !!p.initializer,
      rest: !!p.dotDotDotToken,
      default: p.initializer ? evalNode(p.initializer) : undefined,
      decorators: decoratorsOf(p),
      resolved: p.type ? resolvedOf(p.type) : null,
      ...(p.type ? typeRefAt(p.type) : { type_ref: null }),
    }));
  }

  /**
   * WHERE THE CLASS A NAME RESOLVES TO IS DECLARED - its name's line, the line its `classes` row carries - so the refs
   * rollup joins it by the DECLARATION the checker reached (`TsRefs.mjs::rollupRefIds`, #87), never by the name as
   * written: an alias or a barrel is followed by `symbolAt`, and two classes of one name in one file stay two.
   */
  const classLineOf = (sym, file) => {
    const decl = (sym?.declarations ?? []).find((d) => ts.isClassDeclaration(d) || ts.isClassExpression(d));
    if (!decl || relativeTo(feRoot, canonicalPath(decl.getSourceFile().fileName)) !== file) return {};
    return { line: locationOf(decl).line };
  };
  /** A heritage clause's base, or one of its type arguments: what the checker resolves it to, and each type argument
   *  of its own the same way, every level of them (`extends Base<Page<Order>>`). */
  const heritageRef = (nameNode, typeArgs, text) => {
    const sym = nameNode ? symbolAt(nameNode) : undefined;
    const declared = nameNode ? declFileOfSymbol(sym) : null;
    const args = [...(typeArgs ?? [])].map((a) => (ts.isTypeReferenceNode(a)
      ? heritageRef(ts.isQualifiedName(a.typeName) ? a.typeName.right : a.typeName, a.typeArguments, a.getText())
      : { text: a.getText() }));
    return {
      text, ...(declared ?? {}), ...(declared ? classLineOf(sym, declared.file) : {}),
      ...(args.length ? { type_args: args } : {}),
    };
  };

  function heritageOf(node) {
    let extendsInfo = null;
    const implementsInfo = [];
    for (const h of node.heritageClauses ?? []) {
      for (const t of h.types) {
        if (h.token === ts.SyntaxKind.ExtendsKeyword) {
          extendsInfo = heritageRef(t.expression, t.typeArguments, t.expression.getText());
        } else implementsInfo.push({ text: t.expression.getText(), ...(declFileOf(t.expression) ?? {}) });
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
    const value = initializer ? evalCell(initializer) : undefined;
    const memberType = m.type ? m.type.getText() : null;
    const kind = kindOfMember(m);
    const name = m.name ? propName(m.name) : kind;
    // A FUNCTION-VALUED PROPERTY TAKES PARAMETERS like a method does: `check = (id: ItemId) => ...` is
    // called exactly as `check(id: ItemId) {...}` is, and without its parameters every reader that
    // follows a call into its callee's returns - the gate's list and predicate readers - passed it by.
    const fnValued = initializer && (ts.isArrowFunction(initializer) || ts.isFunctionExpression(initializer));
    const isSignature = ts.isMethodDeclaration(m) || ts.isConstructorDeclaration(m) || ts.isSetAccessor(m);
    // AN ACCESSOR IS TYPED AS A PROPERTY IS (#95): its symbol IS the property's, so the checker answers the property's
    // type, never a function's. Typed only by its annotation, `get vm() { return this.load<Outer>(); }` had none, and
    // every template read chained off it stopped there.
    const typedLikeProperty = kind === 'property' || kind === 'getter' || kind === 'setter';
    return store.add('members', 'm', {
      class: classId, file: fid, kind, name,
      static: mods.static, readonly: mods.readonly, visibility: mods.visibility,
      optional: !!m.questionToken,
      type: typedLikeProperty ? typeText(m, m.type) : memberType,
      // AN ANNOTATION IF THERE IS ONE, OTHERWISE THE PROPERTY'S OWN TYPE - but never a METHOD's. Asking the
      // checker what type a method declaration is answers with its own function type whose symbol is the
      // method: every un-annotated method resolved to itself. For a PROPERTY the same question
      // is exactly right - `items$ = new BehaviorSubject<Item[]>([])` has no annotation and a fully
      // known type, and refusing it left every RxJS-style member unresolvable.
      ...(m.type ? typeRefAt(m.type) : typedLikeProperty ? typeRefAt(m) : { type_ref: null }),
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
      // ITS TYPE PARAMETERS, in order: what a subclass's `extends Base<Payload>` binds them to (#98, `TsTplReads.mjs`).
      ...(node.typeParameters?.length ? { type_params: node.typeParameters.map((p) => p.name.text) } : {}),
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
      // AN NGRX EFFECT'S CALLBACK IS WALKED: `loadX$ = createEffect(() => actions$.pipe(ofType(a, b), ...))` holds
      // its whole logic in the arrow handed to the library, and a property walks only when it IS the function -
      // so the `ofType` calls existed as `$fn` text and `state_effects` could not name the actions an effect hears.
      const effectFn = initializer && ts.isCallExpression(initializer) && fromNgrx(initializer.expression)
        ? initializer.arguments[0] : undefined;
      if (effectFn && (ts.isArrowFunction(effectFn) || ts.isFunctionExpression(effectFn))) {
        collectBody(effectFn.body, memberId, classId);
      }
      if (ts.isConstructorDeclaration(m)) {
        const paramNodes = [...(m.parameters ?? [])];
        for (const [pi, p] of paramsOf(m).entries()) {
          const tokenArg = injectTokenOf(paramNodes[pi]);
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
            // THE TOKEN `@Inject(TOKEN)` NAMES - the parameter's type is the interface, and the interface
            // says nothing about which provider answers.
            ...(tokenArg ? { token: tokenArg.getText(), token_ref: declFileOf(tokenArg) } : {}),
          });
        }
      }
      // `inject()` IN A PROPERTY INITIALIZER IS CONSTRUCTOR INJECTION by another spelling: without a row
      // here a class that takes every dependency through `inject()` looked as if it had none.
      if (mInit) {
        for (const call of injectCallsOf(mInit)) {
          const t = checker.getTypeAtLocation(call);
          const opts = call.arguments[1];
          const optional = !!opts && ts.isObjectLiteralExpression(opts) && opts.properties.some((pr) =>
            ts.isPropertyAssignment(pr) && propName(pr.name) === 'optional'
            && pr.initializer.kind === ts.SyntaxKind.TrueKeyword);
          const tokenArg = call.arguments[0];
          store.add('di', 'd', {
            class: classId, file: fid, param: propName(m.name), member: memberId, inject: true,
            type: typeText(call, null), resolved: declFileOfSymbol(t.aliasSymbol ?? t.getSymbol()),
            type_ref: null, decorators: [], optional,
            ...(tokenArg ? { token: tokenArg.getText(), token_ref: declFileOf(tokenArg) } : {}),
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
        externalPipes: libPipesOf(node, decorators),
      });
    }
    return { classId, decorators, members, name: className, io, angularRows, injectsTemplateRef };
  }

  return { extractClass, paramsOf, typeText, declFileOf, decoratorsOf };
}
