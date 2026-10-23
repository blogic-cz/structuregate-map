/**
 * WHERE A READ IN A TYPESCRIPT EXPRESSION IS DECLARED - the `target` every `Read` node carries.
 *
 * Moved out of `TsMap.mjs`, which is held in the structure baseline and may only shrink. Imports are flat:
 * see `TsTypeRef.mjs`.
 */

export function makeReadRefs(ts, checker, canonicalPath) {
  /**
   * WHERE A DECLARATION IS: its NAME's line, as a row's own `line` is (`TsNodes.mjs::locationOf`). `getStart()` of a decorated member is its FIRST DECORATOR (`@Input()`
   * above `label = 'x'`), so a target anchored there named a line no `members` row sits at, and every reader that finds a row by (file, line) missed
   * the member (rows 75). A declaration with no name (a call signature, a constructor) starts where it starts.
   */
  const at = (d) => {
    const sf = d.getSourceFile();
    const pos = d.name && typeof d.name.getStart === 'function' ? d.name.getStart() : d.getStart();
    return {
      file: canonicalPath(sf.fileName),
      line: sf.getLineAndCharacterOfPosition(pos).line + 1,
    };
  };

  /**
   * THE PROPERTY A READ LANDS ON, resolved to where it is DECLARED. A path published as a name alone
   * leaves the consumer matching it against candidate interfaces by hand. Declarations inside
   * dependencies are kept too - a library's own member is a fact, and dropping externals would make
   * "unresolved" mean two different things at once. EVERY declaration, not the first: a merged
   * interface or an overload set gives the same symbol several.
   */
  function resolveRead(node) {
    const decls = checker.getSymbolAtLocation(node)?.declarations ?? [];
    // THE CHECKER'S ORDER IS KEPT, AND THAT IS NOT AN OVERSIGHT. Its declaration order is not a fact
    // about the tree - a full run and a partial one disagree about a few `expressions`,
    // `assignments` and `functions` on nothing else - and sorting it makes the two agree. It was
    // tried, and a comparison refused it: sorting the declarations moved many rows away from
    // what the tool this map reproduces publishes, many of them in `members`. The reproduction is the
    // contract, so the instability stays and the partial-run comparison records it.
    const [first, ...rest] = decls.map(at);
    if (first === undefined) return null;
    return { name: node.getText(), ...first, ...(rest.length ? { also: rest } : {}) };
  }

  /**
   * A MEMBER NAMED BY A STRING, resolved through the TYPE of what it is defined on: `Object.defineProperty(
   * Array.prototype, 'm', ...)` names `m` in a literal, which no symbol sits on, so the member is the property
   * the prototype's type declares - the same declarations a call `xs.m(..)` resolves to.
   */
  function resolveMember(objectNode, name) {
    const decls = checker.getTypeAtLocation(objectNode).getProperty(name)?.declarations ?? [];
    const [first, ...rest] = decls.map(at);
    if (first === undefined) return null;
    return { name, ...first, ...(rest.length ? { also: rest } : {}) };
  }

  /** A declaration a module makes: a top-level `const`, a function, an enum, a CLASS. Never a local or a parameter. */
  const moduleLevel = (d) => {
    if (ts.isFunctionDeclaration(d) || ts.isEnumDeclaration(d) || ts.isClassDeclaration(d)) return ts.isSourceFile(d.parent);
    // A NAME A TOP-LEVEL DESTRUCTURING BINDS IS A MODULE DECLARATION TOO (rows 76): `export const { DemoActions } = makeSlice()` declares `DemoActions` as a BINDING ELEMENT, inside
    // patterns nested to any depth, whose top VariableDeclaration is the top-level one. Admitting only the VariableDeclaration left every read of such a name (an action
    // creator, a selector set) with no `target`, `local` or `param`. `constRows` writes the row each target joins, one per bound name.
    while (ts.isBindingElement(d)) d = d.parent?.parent;
    if (d === undefined || !ts.isVariableDeclaration(d)) return false;
    const statement = d.parent?.parent;
    return statement !== undefined && ts.isVariableStatement(statement) && ts.isSourceFile(statement.parent);
  };

  /**
   * THE SYMBOL A BARE NAME READS. A SHORTHAND PROPERTY (`{ client }`) IS A READ OF THE VARIABLE IT NAMES, and `getSymbolAtLocation` on its name answers
   * with the PROPERTY's own symbol - so the read bound to nothing, or, when the checker could type the literal, to a property declared elsewhere. The
   * value symbol is the local, parameter or module declaration the shorthand stands for.
   */
  const symbolOf = (node) => {
    const parent = node.parent;
    if (parent !== undefined && ts.isShorthandPropertyAssignment(parent) && parent.name === node) {
      return checker.getShorthandAssignmentValueSymbol(parent);
    }
    return checker.getSymbolAtLocation(node);
  };

  /**
   * A BARE NAME, resolved only when it names what a module declares.
   *
   * An imported name is an ALIAS, and its own declaration is the import line in the file that reads it -
   * which is where nothing is declared. The aliased symbol is where the value lives.
   *
   * A CLASS IS ONE (`JsonSerializer.parse(x)`, `new Foo()`, `Foo.Member`): it gets a target marked `kind: 'class'`, anchored at its NAME line - the
   * `classes` row's own line - never the decorator's. The mark is what keeps the closure's readers from taking a class for a constant, a member or a
   * local: they look a target up by line in tables a class is not in, and treat a bare name with no target as a name nothing declares.
   */
  function resolveName(node) {
    let symbol = symbolOf(node);
    if (symbol !== undefined && (symbol.flags & ts.SymbolFlags.Alias) !== 0) {
      symbol = checker.getAliasedSymbol(symbol);
    }
    const all = symbol?.declarations ?? [];
    // A LIBRARY NAME THAT IS NOT A TOP-LEVEL DECLARATION is still declared (rows 75): `import { cloneDeep } from 'lodash'` binds the METHOD of an interface the package
    // types its export with (a `MethodSignature`), a namespace's member, a module. Those keep their place in the library's file, as the link `_.cloneDeep` always did -
    // never a name nothing declares. ONLY when every declaration is in a declaration file under `node_modules`: an in-tree declaration stays what it was.
    const inLibrary = (d) => d.getSourceFile().isDeclarationFile && d.getSourceFile().fileName.includes('/node_modules/');
    const top = all.filter(moduleLevel);
    const decls = top.length ? top : (all.length && all.every(inLibrary) ? all : []);
    const [first, ...rest] = decls.map(at);
    if (first === undefined) return null;
    const klass = ts.isClassDeclaration(decls[0]);
    return { name: node.text, ...first, ...(klass ? { kind: 'class' } : {}), ...(rest.length ? { also: rest } : {}) };
  }

  /**
   * A BARE NAME DECLARED IN A BODY, as where its `locals` row sits: `{file, line, col}` of the declaration's name, the row's own `line` and `col`.
   * Not a `target` - every reader takes a `Read` without one for a local.
   *
   * EVERY DECLARATION HAS A ROW, WITH A VALUE OR WITHOUT (rows 75): a loop's `for (const key of keys)`, a `catch (e)` and `let t;` are `locals` rows the body walk
   * writes (`TsBody.mjs::emitLocal`), and a name a destructuring binds (`const { a } = f()`, `for (const [k, v] of m)`) is its DECLARATION's row, so the place is the
   * pattern's. Requiring an initializer left every read of a loop variable and of a destructured name bound to nothing - the largest share of the reads with no
   * `target`, `local` or `param`. A declaration file has no body walk, so a library's own local has no row.
   */
  function resolveLocal(node) {
    const decls = symbolOf(node)?.declarations ?? [];
    if (decls.length !== 1) return null;
    const [d] = decls;
    if (!ts.isIdentifier(d.name ?? node)) return null;
    let holder = d;
    while (holder !== undefined && ts.isBindingElement(holder)) holder = holder.parent?.parent;
    if (holder === undefined || !ts.isVariableDeclaration(holder) || moduleLevel(holder) || holder.getSourceFile().isDeclarationFile) return null;
    return placeOf(holder.name);
  }

  /**
   * A BARE NAME THAT IS A PARAMETER, as where the parameter's name sits: `{file, line, col}` - the `line` and `col` its `params` item carries when the name is the
   * whole parameter, and one of the item's `bindings` when it is destructured (`({ a, b: c }) =>`, `([x]) =>`, the binding element's name). Only for a function
   * that has a body - a declaration, method, accessor, constructor, arrow or function expression - and each of those has a row.
   */
  function resolveParam(node) {
    const decls = symbolOf(node)?.declarations ?? [];
    if (decls.length !== 1) return null;
    const [d] = decls;
    if (!ts.isParameter(d) && !ts.isBindingElement(d)) return null;
    if (!ts.isIdentifier(d.name)) return null;
    let holder = d;
    while (holder !== undefined && ts.isBindingElement(holder)) holder = holder.parent?.parent;
    if (holder === undefined || !ts.isParameter(holder)) return null;
    const fn = holder.parent;
    if (fn === undefined || !ts.isFunctionLike(fn) || fn.body === undefined) return null;
    return placeOf(d.name);
  }

  /** `{file, line, col}` of a name node: the place a row is keyed by. */
  function placeOf(name) {
    const sf = name.getSourceFile();
    const pos = sf.getLineAndCharacterOfPosition(name.getStart());
    return { file: canonicalPath(sf.fileName), line: pos.line + 1, col: pos.character + 1 };
  }

  /**
   * THE VARIABLE A CALL ARGUMENT NAMES (rows 79, #103): `convert(HueIDs, hueLabels)` evaluates `hueLabels` to the const's value, and the value alone cannot be tied to
   * the declaration that holds it. A bare name (through parentheses, `as`, `!`, `satisfies`) that resolves, through import aliases, to a `const`/`let`/`var` - a local, a
   * module's or an imported one - or to a name a TOP-LEVEL destructuring binds is `{name, $ref, file, line, col, kind}`, the place of the declaration's NAME (the `consts`
   * row, or the `locals` row, that holds it). A class, an enum and a function already carry `$ref` in the value, and a library's own declaration has no row.
   */
  function resolveArg(arg) {
    let at = arg;
    while (ts.isParenthesizedExpression(at) || ts.isAsExpression(at) || ts.isNonNullExpression(at) || ts.isTypeAssertionExpression(at)
      || (ts.isSatisfiesExpression && ts.isSatisfiesExpression(at))) at = at.expression;
    if (!ts.isIdentifier(at)) return null;
    let symbol = checker.getSymbolAtLocation(at);
    if (symbol !== undefined && (symbol.flags & ts.SymbolFlags.Alias) !== 0) symbol = checker.getAliasedSymbol(symbol);
    const decl = (symbol?.declarations ?? []).find((d) => {
      if (d.getSourceFile().isDeclarationFile) return false;
      if (ts.isBindingElement(d)) return ts.isIdentifier(d.name) && moduleLevel(d);
      return ts.isVariableDeclaration(d) && ts.isIdentifier(d.name);
    });
    if (decl === undefined) return null;
    const binding = ts.isBindingElement(decl);
    let holder = decl;
    while (ts.isBindingElement(holder)) holder = holder.parent?.parent;
    const flags = holder.parent?.flags ?? 0;
    const kind = binding ? 'binding' : (flags & ts.NodeFlags.Const) !== 0 ? 'const' : (flags & ts.NodeFlags.Let) !== 0 ? 'let' : 'var';
    return { name: at.text, $ref: decl.name.text, ...placeOf(decl.name), kind };
  }

  /**
   * THE PLACE OF THE DECLARATION A CALL NAMES (rows 80, 81): a call's `target` is a file and a name, and a name names several things in one file - the property
   * `getAge` of `Alpha = { getAge: (..) => .. }` and the `function getAge` beside it, a class's `getAmounts` method and a function's own `const getAmounts` - so the
   * rollup (`TsRefs.mjs::rollupRefIds`) never binds by name: EVERY in-tree target carries `line`, the NAME's line of the declaration the checker reached (an alias followed,
   * as a read's `target` is anchored), and the row AT that file, line and name is the call's `target_id`. A library's declaration carries none. An OVERLOAD SET
   * names its IMPLEMENTATION (the signature that has a body, as the class's last row of that name always was), never the first signature the checker lists.
   * A PROPERTY THAT ONLY NAMES A VALUE (`{ x }`, `{ y: x }`) is itself the declaration, so `follow` is where `x` is declared - the callable the call runs (`callee_id`).
   */
  function targetLine(node) {
    const declarationsOf = (sym) => {
      if (sym !== undefined && (sym.flags & ts.SymbolFlags.Alias) !== 0) {
        try { sym = checker.getAliasedSymbol(sym); } catch { return []; }
      }
      return sym?.declarations ?? [];
    };
    const placeOfDecl = (decls) => {
      const [first] = decls;
      if (first === undefined || first.getSourceFile().fileName.includes('/node_modules/')) return null;
      const impl = ts.isFunctionLike(first) && first.body === undefined
        ? decls.find((x) => x.getSourceFile() === first.getSourceFile() && ts.isFunctionLike(x) && x.body !== undefined) : undefined;
      return impl ?? first;
    };
    const d = placeOfDecl(declarationsOf(checker.getSymbolAtLocation(node)));
    if (d === null) return null;
    const value = ts.isShorthandPropertyAssignment(d) ? checker.getShorthandAssignmentValueSymbol(d)
      : ts.isPropertyAssignment(d) && ts.isIdentifier(d.initializer) ? checker.getSymbolAtLocation(d.initializer) : undefined;
    const named = value === undefined ? null : placeOfDecl(declarationsOf(value));
    return { line: at(d).line, ...(named === null ? {} : { follow: { name: value.getName(), ...at(named) } }) };
  }

  /**
   * THE MEMBER A LITERAL IN AN OBJECT LITERAL FILLS (rows 81, #104): `{ Field: 'alpha' }` typed `HueRule[]` (an element of a typed array, a typed variable, member, parameter or return, a
   * call argument - whatever the CHECKER gives the object as its contextual type) puts 'alpha' in the member `Field` of `HueRule`, and a consumer should join that place, never match
   * the property's SPELLING. `{name, file, line}` is the declaration's NAME line, as every read's `target` is (a library's file kept); `target_id` is the row there (`TsRefs.mjs::rollupRefIds`).
   * A union contextual type asks the arms that DECLARE the name: ONE declaration place is the target, several name none (the template rule). An object with no contextual type, and a property the
   * type does not declare (an excess one, a computed name), have none. A sign in front of a number is stepped through.
   * A FUNCTION FILLS A MEMBER TOO (rows 83, #105): an arrow or function expression that is the property's value, or a method written in the object
   * (`{ Message() { .. } }`, which is its own property), names the member as a literal does - `TsWalk.mjs::declareInline` stamps it on the `functions` row.
   * Parentheses, `as` and `satisfies` around the value are stepped through: `(x)`, `'a' as const` and `fn satisfies F` fill the member `x`, `'a'` and `fn` do.
   */
  const wraps = (n) => ts.isPrefixUnaryExpression(n) || ts.isParenthesizedExpression(n) || ts.isAsExpression(n)
    || (ts.isSatisfiesExpression !== undefined && ts.isSatisfiesExpression(n));
  function fieldTarget(node) {
    let child = node;
    while (child.parent !== undefined && wraps(child.parent)) child = child.parent;
    const method = ts.isMethodDeclaration(node) && node.parent !== undefined && ts.isObjectLiteralExpression(node.parent);
    const prop = method ? node : child.parent;
    if (prop === undefined || !(method || (ts.isPropertyAssignment(prop) && prop.initializer === child))) return null;
    if (!(ts.isIdentifier(prop.name) || ts.isStringLiteral(prop.name) || ts.isNumericLiteral(prop.name))) return null;
    // PUSHED INTO AN UN-ANNOTATED ARRAY (rows 88): the checker's contextual type is only that array's inferred element type; the type the array takes later answers instead.
    const contextual = checker.getContextualType(prop.parent);
    // (rows 95) so is a value a `then`/`catch` or `map`/`flatMap` callback returns: the checker's type there is inferred from the value itself.
    const top = climb(prop.parent).n;
    const type = pushedInto(top) === null && settledBy(top) === null && mappedBy(top) === null ? contextual : undefined;
    const later = type === undefined ? laterTypes(prop.parent) : [];
    if (type === undefined && !later.length) return null;
    const name = prop.name.text;
    const places = new Map();
    for (const arm of (type === undefined ? later : [type]).flatMap((t) => (t.isUnion() ? t.types : [t]))) {
      const [first] = checker.getPropertyOfType(arm, name)?.declarations ?? [];
      if (first === undefined) continue;
      // A TYPE INFERRED FROM ANOTHER LITERAL is no declared member (an un-annotated array nothing types later): its property is that literal's, and names nothing here.
      if (type === undefined && first.parent !== undefined && ts.isObjectLiteralExpression(first.parent)) continue;
      const place = at(first);
      places.set(`${place.file}#${place.line}`, place);
    }
    return places.size === 1 ? { name, ...[...places.values()][0] } : null;
  }

  /**
   * THE TYPE AN UN-ANNOTATED CONST'S LITERAL TAKES LATER (rows 87, #106): `const list = [{ Field: 'alpha' }]; return list;` in a method typed `HueRule[]` gives the object no
   * contextual type where it is written, yet the only type the value ever takes is `HueRule[]` at the `return`. The object is climbed out of its array literals (one level each,
   * parentheses and `!` stepped through) to the initializer of a `const` with no annotation; every use of that const in its scope is asked for its contextual type - a typed
   * `return`, a parameter, a typed variable or member - indexed down as many levels; a use that initializes another un-annotated `const` is followed into it (a visited set ends a
   * cycle). Uses with no contextual type (`list.length`, `list.push(..)`) say nothing. The types found are what the object's property is looked up on.
   */
  const passes = (n) => ts.isParenthesizedExpression(n) || ts.isNonNullExpression(n);
  const constOf = (n) => {
    const decl = n.parent;
    return decl !== undefined && ts.isVariableDeclaration(decl) && decl.initializer === n && decl.type === undefined && ts.isIdentifier(decl.name)
      && ts.isVariableDeclarationList(decl.parent) && (decl.parent.flags & ts.NodeFlags.Const) !== 0 ? decl : null;
  };
  const scopeOf = (decl) => {
    let n = decl.parent;
    while (n !== undefined && !(ts.isBlock(n) || ts.isSourceFile(n) || ts.isModuleBlock(n) || ts.isCaseClause(n) || ts.isDefaultClause(n))) n = n.parent;
    return n;
  };
  const down = (types, levels) => {
    let out = types;
    for (let i = 0; i < levels; i++) {
      out = out.flatMap((t) => (t.isUnion() ? t.types : [t])).flatMap((t) => {
        const element = checker.getIndexTypeOfType(t, ts.IndexKind.Number);
        return element === undefined ? [] : [element];
      });
    }
    return out;
  };
  /** `Array`'s own method of that name - checked by the declaration the checker names, never by the name alone. */
  const arrayMethod = (access, names) => names.includes(access.name.text)
    && (checker.getSymbolAtLocation(access.name)?.declarations ?? []).some((d) => d.parent !== undefined && ts.isInterfaceDeclaration(d.parent) && d.parent.name.text === 'Array');
  // THE ARRAY METHODS WHOSE RESULT HOLDS THE RECEIVER'S ELEMENTS (rows 90, #107): `[{..}].filter(Boolean)` is typed where its RESULT stands.
  const SAME_ELEMENTS = ['filter', 'slice', 'sort', 'reverse', 'concat'];
  /** A node climbed out of what keeps it an element: parentheses and `!`, array literals (one level each) and the receiver of a `SAME_ELEMENTS` call - the outermost
   *  node `n` and how many arrays down the node sits in it. */
  const climb = (node) => {
    let n = node;
    let levels = 0;
    for (;;) {
      const p = n.parent;
      if (p !== undefined && passes(p)) n = p;
      else if (p !== undefined && ts.isArrayLiteralExpression(p)) { n = p; levels++; }
      else if (p !== undefined && ts.isPropertyAccessExpression(p) && p.expression === n && p.parent !== undefined && ts.isCallExpression(p.parent)
        && p.parent.expression === p && arrayMethod(p, SAME_ELEMENTS)) n = p.parent;
      // A VALUE A CONDITION PASSES ON (rows 91): `c && { .. }`, `a || { .. }`, `a ?? { .. }` and a ternary's arm are the value of the whole where they win.
      else if (p !== undefined && ts.isBinaryExpression(p) && (p.right === n || (p.left === n && p.operatorToken.kind !== ts.SyntaxKind.AmpersandAmpersandToken))
        && [ts.SyntaxKind.AmpersandAmpersandToken, ts.SyntaxKind.BarBarToken, ts.SyntaxKind.QuestionQuestionToken].includes(p.operatorToken.kind)) n = p;
      else if (p !== undefined && ts.isConditionalExpression(p) && (p.whenTrue === n || p.whenFalse === n)) n = p;
      // A SPREAD IN AN ARRAY LITERAL (rows 95): `[...a, ...[{ .. }]]` makes the spread array's elements the outer one's - the level already counted stays.
      else if (p !== undefined && ts.isSpreadElement(p) && p.expression === n && p.parent !== undefined && ts.isArrayLiteralExpression(p.parent)) n = p.parent;
      else break;
    }
    return { n, levels };
  };
  /** THE UN-ANNOTATED CONST AN ARGUMENT IS PUSHED INTO (rows 88): `list.push(x)` / `list.unshift(x)` - the method `Array` declares, checked by its declaration - makes `x` an
   *  ELEMENT of `list`, so `x` takes what `list` takes later, one level down. Null for any other call, or a receiver that is annotated (its type is then the contextual one). */
  const pushedInto = (n) => {
    const call = n.parent;
    if (call === undefined || !ts.isCallExpression(call) || !call.arguments.includes(n)) return null;
    const callee = call.expression;
    if (!ts.isPropertyAccessExpression(callee) || !arrayMethod(callee, ['push', 'unshift'])) return null;
    let receiver = callee.expression;
    while (passes(receiver)) receiver = receiver.expression;
    const held = ts.isIdentifier(receiver) ? checker.getSymbolAtLocation(receiver)?.valueDeclaration : undefined;
    return held !== undefined && ts.isVariableDeclaration(held) && held.initializer !== undefined ? constOf(held.initializer) : null;
  };
  /** A PROPERTY VALUE'S TYPE IS WHAT ITS OBJECT'S TYPES GIVE THE PROPERTY (rows 90, #107): `Children: [{..}].filter(..)` in an object that is itself a `HueRule`
   *  (by its contextual type, or the type it takes later) is `HueRule['Children']`. Null when `n` is no property value of an object literal. */
  const propertyTypes = (n) => {
    const prop = n.parent;
    if (prop === undefined || !ts.isPropertyAssignment(prop) || prop.initializer !== n || !ts.isObjectLiteralExpression(prop.parent)) return null;
    if (!(ts.isIdentifier(prop.name) || ts.isStringLiteral(prop.name) || ts.isNumericLiteral(prop.name))) return [];
    const outer = prop.parent;
    const contextual = checker.getContextualType(outer);
    const holders = contextual !== undefined && pushedInto(climb(outer).n) === null ? [contextual] : laterTypes(outer);
    return holders.flatMap((t) => (t.isUnion() ? t.types : [t])).flatMap((t) => {
      const member = checker.getPropertyOfType(t, prop.name.text);
      return member === undefined ? [] : [checker.getTypeOfSymbolAtLocation(member, outer)];
    });
  };
  /** WHAT A CLIMBED NODE TAKES, `levels` arrays down: an un-annotated const's or a pushed-into array's later types (asked of their uses, queued on `todo` when one is
   *  given, else read through `laterOf`), a property value's type in its object, else the checker's contextual type. */
  const typesAt = (top, levels, todo) => {
    const next = constOf(top);
    const into = next === null ? pushedInto(top) : null;
    if (next !== null || into !== null) {
      const job = { decl: next ?? into, levels: next !== null ? levels : levels + 1 };
      if (todo !== null) { todo.push(job); return []; }
      return laterOf(job.decl, job.levels);
    }
    const held = propertyTypes(top);
    if (held !== null) return down(held, levels);
    // ASKED BEFORE THE CHECKER: inside a `then` callback its contextual type is the one it INFERRED from this very value, which names no member.
    const settled = settledBy(top);
    if (settled !== null) return down(awaited(typesAt(climb(settled).n, 0, null)), levels);
    const mapped = mappedBy(top);
    if (mapped !== null) {
      const result = typesAt(climb(mapped.call).n, 0, null);
      return down(mapped.flat ? [...result, ...down(result, 1)] : down(result, 1), levels);
    }
    const t = checker.getContextualType(top);
    if (t !== undefined) return down([t], levels);
    const fn = returnedFrom(top);
    return fn === null ? [] : callersOf(fn, levels);
  };
  /** A VALUE A `then`/`catch` CALLBACK RETURNS settles the promise that call makes (rows 94): `p.then((r) => this.rule())` in a method typed `Promise<Rule>` hands
   *  `this.rule()` the `Rule` the call's own type awaits. `Promise`'s own method (checked by its declaration), the callback an anonymous function among its
   *  arguments; the call is what is asked. Null otherwise. */
  const settledBy = (n) => {
    const fn = callbackOf(n);
    if (fn === null) return null;
    const call = fn.parent;
    if (call === undefined || !ts.isCallExpression(call) || !call.arguments.includes(fn) || !ts.isPropertyAccessExpression(call.expression)) return null;
    const access = call.expression;
    const promised = (access.name.text === 'then' || access.name.text === 'catch')
      && (checker.getSymbolAtLocation(access.name)?.declarations ?? []).some((d) => d.parent !== undefined && ts.isInterfaceDeclaration(d.parent) && d.parent.name.text === 'Promise');
    return promised ? call : null;
  };
  /** A VALUE AN `Array` `map`/`flatMap` CALLBACK RETURNS is an element of what the call makes (rows 95): `list.map((e) => this.rule(e))` typed `Rule[]` makes `rule()` a
   *  `Rule`; a `flatMap` callback may return an element or an array of them, so both are offered (`flat`) and the property lookup keeps the one that declares it.
   *  `Array`'s own method (checked by its declaration), the callback an anonymous function among the arguments. Null otherwise. */
  const mappedBy = (n) => {
    const fn = callbackOf(n);
    const call = fn?.parent;
    if (call === undefined || !ts.isCallExpression(call) || !call.arguments.includes(fn) || !ts.isPropertyAccessExpression(call.expression)) return null;
    if (!arrayMethod(call.expression, ['map', 'flatMap'])) return null;
    return { call, flat: call.expression.name.text === 'flatMap' };
  };
  /** The anonymous function a value is returned from - a `return` in its body, or an arrow's concise body - or null. */
  const callbackOf = (n) => {
    let fn = null;
    if (n.parent !== undefined && ts.isReturnStatement(n.parent)) {
      fn = n.parent.parent;
      while (fn !== undefined && !ts.isFunctionLike(fn)) fn = fn.parent;
    } else if (n.parent !== undefined && ts.isArrowFunction(n.parent) && n.parent.body === n) fn = n.parent;
    return fn !== undefined && fn !== null && (ts.isArrowFunction(fn) || ts.isFunctionExpression(fn)) ? fn : null;
  };
  /** What a promise type settles to: each arm's awaited type, the checker's own `getAwaitedType`. */
  const awaited = (types) => types.flatMap((t) => (t.isUnion() ? t.types : [t])).flatMap((t) => {
    const settled = checker.getAwaitedType(t);
    return settled === undefined || settled === t ? [] : [settled];
  });
  /** THE FUNCTION A VALUE IS RETURNED FROM when it declares no return type (rows 91): `return x` in its body, or an arrow's concise body - named by a declaration,
   *  a method, or the variable or property an arrow or function expression is held in. Null otherwise: an annotated return is the contextual type already. */
  const returnedFrom = (n) => {
    let fn = null;
    if (n.parent !== undefined && ts.isReturnStatement(n.parent)) {
      fn = n.parent.parent;
      while (fn !== undefined && !ts.isFunctionLike(fn)) fn = fn.parent;
    } else if (n.parent !== undefined && ts.isArrowFunction(n.parent) && n.parent.body === n) fn = n.parent;
    if (fn === undefined || fn === null || fn.type !== undefined) return null;
    const holder = (ts.isArrowFunction(fn) || ts.isFunctionExpression(fn)) && fn.parent !== undefined
      && (ts.isVariableDeclaration(fn.parent) || ts.isPropertyDeclaration(fn.parent)) && fn.parent.initializer === fn ? fn.parent : fn;
    return holder.name !== undefined && ts.isIdentifier(holder.name) ? holder : null;
  };
  /** WHAT A FUNCTION'S CALLS TAKE (rows 91): every call of it in its own FILE - a private method's callers are all there - climbed and asked as a value is,
   *  `levels` down. Callers in other files are not searched: that is the whole program per function. Memoized per function and depth, empty while asked. */
  const callersOf = (fn, levels) => {
    const memo = laterMemo.get(fn) ?? new Map();
    laterMemo.set(fn, memo);
    if (memo.has(levels)) return memo.get(levels);
    memo.set(levels, []);
    const symbol = checker.getSymbolAtLocation(fn.name);
    const found = [];
    const stack = symbol === undefined ? [] : [fn.getSourceFile()];
    while (stack.length) {
      const node = stack.pop();
      if (ts.isCallExpression(node)) {
        const callee = ts.isPropertyAccessExpression(node.expression) ? node.expression.name : node.expression;
        if (ts.isIdentifier(callee) && callee.text === fn.name.text && checker.getSymbolAtLocation(callee) === symbol) {
          const { n: top, levels: inside } = climb(node);
          found.push(...typesAt(top, levels + inside, null));
        }
      }
      ts.forEachChild(node, (c) => { stack.push(c); });
    }
    memo.set(levels, found);
    return found;
  };
  function laterTypes(object) {
    // Asked of an object the checker gives no contextual type (or only a pushed-into array's inferred one): what its climbed node takes, `typesAt` in order.
    const { n, levels } = climb(object);
    return typesAt(n, levels, null);
  }
  /** EVERY LITERAL OF ONE ARRAY asks the same question: answered once per const and depth, the answer held as nothing while it is being asked (a cycle). */
  const laterOf = (decl, levels) => {
    const memo = laterMemo.get(decl) ?? new Map();
    laterMemo.set(decl, memo);
    if (!memo.has(levels)) {
      memo.set(levels, []);
      memo.set(levels, usesOf(decl, levels));
    }
    return memo.get(levels);
  };
  const laterMemo = new Map();
  function usesOf(start, depth) {
    const found = [];
    const seen = new Map();
    const todo = [{ decl: start, levels: depth }];
    while (todo.length) {
      const { decl, levels } = todo.pop();
      const asked = seen.get(decl) ?? new Set();
      if (asked.has(levels)) continue;
      seen.set(decl, asked.add(levels));
      const symbol = checker.getSymbolAtLocation(decl.name);
      const scope = scopeOf(decl);
      if (symbol === undefined || scope === undefined) continue;
      const stack = [scope];
      while (stack.length) {
        const node = stack.pop();
        if (ts.isIdentifier(node) && node !== decl.name && node.text === decl.name.text && checker.getSymbolAtLocation(node) === symbol) {
          // A USE CLIMBS AS AN OBJECT DOES (rows 90, #107): `const list = [first, ..]` makes `first` an element of `list`, one level down.
          const { n: top, levels: inside } = climb(node);
          found.push(...typesAt(top, levels + inside, todo));
        }
        ts.forEachChild(node, (c) => { stack.push(c); });
      }
    }
    return found;
  }

  /**
   * A HOLE THAT NAMES AN ENUM MEMBER (rows 97, #108): `${Hue[this.hue]}` indexes the enum by a VALUE - TypeScript's reverse mapping - so it yields one of the
   * member NAMES, and only a numeric member has one. `{name, file, line, kind: 'enum_name', names}` - the enum's declaration (its name line, as every target) and
   * the names it can yield, in declaration order across a merged enum - or null: a string index (`Hue['Red']`) is the member itself, no name lookup.
   */
  function enumNameHole(expr) {
    let e = expr;
    while (passes(e)) e = e.expression;
    if (!ts.isElementAccessExpression(e)) return null;
    let symbol = checker.getSymbolAtLocation(e.expression);
    if (symbol !== undefined && (symbol.flags & ts.SymbolFlags.Alias) !== 0) symbol = checker.getAliasedSymbol(symbol);
    if (symbol === undefined || (symbol.flags & ts.SymbolFlags.Enum) === 0) return null;
    // A STRING KEY - every arm of its type string-like - reads the member itself, no name lookup.
    const key = checker.getTypeAtLocation(e.argumentExpression);
    if ((key.isUnion() ? key.types : [key]).every((t) => (t.flags & ts.TypeFlags.StringLike) !== 0)) return null;
    const decls = (symbol.declarations ?? []).filter((d) => ts.isEnumDeclaration(d));
    if (!decls.length) return null;
    const names = decls.flatMap((d) => d.members.filter((m) => typeof checker.getConstantValue(m) === 'number').map((m) => (ts.isIdentifier(m.name) || ts.isStringLiteral(m.name) ? m.name.text : m.name.getText())));
    // No numeric member, no reverse mapping: a string enum yields no names, so the hole names no enum.
    return names.length ? { name: symbol.getName(), ...at(decls[0]), kind: 'enum_name', names } : null;
  }

  return { resolveRead, resolveName, resolveLocal, resolveParam, resolveMember, resolveArg, targetLine, fieldTarget, enumNameHole };
}
