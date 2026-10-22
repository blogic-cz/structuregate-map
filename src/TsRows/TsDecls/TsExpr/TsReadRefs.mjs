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
   * THE LINE OF A MEMBER THAT NO NAME KEY CAN TELL (rows 80): a call's `target` is a file and a name, and `getAge` names the property of
   * `Alpha = { getAge: (..) => .. }` as well as a `function getAge` beside it - the rollup bound the call to the function. A member of an OBJECT LITERAL
   * that is a value of its own (a method, an accessor, a property whose value is not a bare name - `{ x }` and `{ x: x }` still follow the
   * declaration they name) and a SIGNATURE of an interface or type literal carry `line`, the place of the declaration the checker named, and
   * `TsRefs.mjs::rollupRefIds` finds the `shape_members` or `type_members` row there. A library's declaration carries none.
   */
  function targetLine(node) {
    const d = checker.getSymbolAtLocation(node)?.declarations?.[0];
    if (d === undefined || d.getSourceFile().fileName.includes('/node_modules/')) return null;
    const own = d.parent !== undefined && ts.isObjectLiteralExpression(d.parent)
      && ((ts.isPropertyAssignment(d) && !ts.isIdentifier(d.initializer)) || ts.isMethodDeclaration(d) || ts.isGetAccessor(d) || ts.isSetAccessor(d));
    return own || ts.isPropertySignature(d) || ts.isMethodSignature(d) ? { line: at(d).line } : null;
  }

  return { resolveRead, resolveName, resolveLocal, resolveParam, resolveMember, resolveArg, targetLine };
}
