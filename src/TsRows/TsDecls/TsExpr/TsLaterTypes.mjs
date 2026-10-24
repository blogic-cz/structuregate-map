/**
 * THE TYPE A VALUE TAKES LATER, where the checker gives it no contextual type of its own: an object in an un-annotated const, pushed into an array, put in an
 * untyped member, returned from a function with no return type or from a callback - what `fieldTarget` (`TsReadRefs.mjs`) looks a property's member up on.
 *
 * Moved out of `TsReadRefs.mjs`, which had reached this repo's line ceiling. Imports are flat: see `TsTypeRef.mjs`.
 */

export function makeLaterTypes(ts, checker) {
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
    // A MEMBER DECLARED WITHOUT A TYPE (rows 105): what it holds takes what its reads take.
    const member = untypedMember(top);
    if (member !== null) return memberLaterOf(member, levels);
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
    if (fn === null) return [];
    const called = callersOf(fn, levels);
    return called.length ? called : down(foldedReturn(fn), levels);
  };
  /** WHAT A FUNCTION WITH NO RETURN TYPE RETURNS WHEN ITS RETURNS FOLD INTO A NAMED TYPE (rows 106): `if (on) return this.typed(); return { .. };` - the checker infers
   *  the union of every return reduced by subtype, so a literal assignable to the other return's `Rule` is absorbed into it and the function returns `Rule`. Only when no
   *  arm is an object literal's own type: one left standing was not absorbed, and its members are its own. Empty otherwise. */
  const foldedReturn = (holder) => {
    const fn = ts.isFunctionLike(holder) ? holder : holder.initializer;
    const signature = fn === undefined ? undefined : checker.getSignatureFromDeclaration(fn);
    if (signature === undefined) return [];
    const result = checker.getReturnTypeOfSignature(signature);
    const arms = result.isUnion() ? result.types : [result];
    // By its SYMBOL: widening rebuilds a literal's type without `ObjectFlags.ObjectLiteral`.
    const literal = (t) => ((t.symbol?.flags ?? 0) & ts.SymbolFlags.ObjectLiteral) !== 0;
    return arms.some(literal) || !arms.some((t) => (t.flags & ts.TypeFlags.Object) !== 0) ? [] : [result];
  };
  /** THE CLASS MEMBER A VALUE IS PUT IN WHEN THE MEMBER DECLARES NO TYPE (rows 105): `private rules = [{ .. }]` (its initializer) or `this.rules = x` (an
   *  assignment) - its type is only what the checker inferred from that value, and the type it takes is where its reads go (`memberLaterOf`). Null otherwise. */
  const untypedMember = (n) => {
    const p = n.parent;
    let decl;
    if (p !== undefined && ts.isPropertyDeclaration(p) && p.initializer === n) decl = p;
    else if (p !== undefined && ts.isBinaryExpression(p) && p.right === n && p.operatorToken.kind === ts.SyntaxKind.EqualsToken
      && ts.isPropertyAccessExpression(p.left) && p.left.expression.kind === ts.SyntaxKind.ThisKeyword) {
      decl = checker.getSymbolAtLocation(p.left.name)?.valueDeclaration;
    }
    return decl !== undefined && ts.isPropertyDeclaration(decl) && decl.type === undefined && ts.isIdentifier(decl.name)
      && decl.parent !== undefined && ts.isClassLike(decl.parent) ? decl : null;
  };
  /** WHAT A MEMBER'S READS TAKE: every `this.<member>` in its class that is no assignment target, climbed and asked as a value is, `levels` down. Memoized per
   *  member and depth, empty while asked. */
  const memberLaterOf = (decl, levels) => {
    const memo = laterMemo.get(decl) ?? new Map();
    laterMemo.set(decl, memo);
    if (memo.has(levels)) return memo.get(levels);
    memo.set(levels, []);
    const symbol = checker.getSymbolAtLocation(decl.name);
    const found = [];
    const stack = symbol === undefined ? [] : [decl.parent];
    while (stack.length) {
      const node = stack.pop();
      if (ts.isPropertyAccessExpression(node) && node.expression.kind === ts.SyntaxKind.ThisKeyword && node.name.text === decl.name.text
        && checker.getSymbolAtLocation(node.name) === symbol
        && !(node.parent !== undefined && ts.isBinaryExpression(node.parent) && node.parent.left === node && node.parent.operatorToken.kind === ts.SyntaxKind.EqualsToken)) {
        const { n: top, levels: inside } = climb(node);
        found.push(...typesAt(top, levels + inside, null));
      }
      ts.forEachChild(node, (c) => { stack.push(c); });
    }
    memo.set(levels, found);
    return found;
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
  return { passes, climb, pushedInto, settledBy, mappedBy, laterTypes, returnedFrom };
}
