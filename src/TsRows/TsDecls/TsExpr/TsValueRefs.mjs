/**
 * THE HALF OF THE VALUE EVALUATOR THAT RESOLVES NAMES: which declaration a name reaches, and what that
 * declaration says the value is.
 *
 * Ported from the original Angular extractor, split out of `TsValue.mjs` because the two
 * halves together are past this repo's 500-line ceiling. Imports are flat: see `TsTypeRef.mjs`.
 */
import { canonicalPath } from './TsPaths.mjs';

export function makeValueRefs({ ts, checker, symbolAt, api }) {
  /**
   * The declaration that actually CARRIES the value, not whichever the checker lists first.
   *
   * When a symbol is declared more than once, only one of them tends to have an initializer - a `declare`
   * ahead of a definition, an overload ahead of an implementation - and starting at `[0]` gave up on a
   * value that was present one declaration later.
   */
  function valueDecl(node) {
    const decls = symbolAt(node)?.declarations ?? [];
    const withInit = decls.find((d) => (ts.isVariableDeclaration(d) && d.initializer !== undefined)
      || (ts.isEnumMember(d) && d.initializer !== undefined)
      || (ts.isPropertyDeclaration(d) && d.initializer !== undefined)
      || (ts.isPropertyAssignment(d) && d.initializer !== undefined));
    return withInit ?? decls[0];
  }

  /** A resolved target, carrying EVERY declaration of the symbol rather than the first one: a symbol
   *  routinely has several - merged interfaces, an overload set, an ambient augmentation. */
  function declTarget(name, node) {
    const decls = symbolAt(node)?.declarations ?? [];
    // The checker's order is kept - see the note in `TsMap`'s describer.
    const files = decls.map((d) => canonicalPath(d.getSourceFile().fileName));
    const [first, ...rest] = files;
    if (first === undefined) return null;
    const unique = [...new Set(rest.filter((f) => f !== first))];
    return { name, file: first, ...(unique.length ? { also: unique } : {}) };
  }

  /** `Enum.Member` / const-object member -> its resolved constant value, PLUS the provenance. */
  function evalMemberAccess(node) {
    const constant = checker.getConstantValue(node);
    if (constant !== undefined) return { $enum: node.getText(), value: constant };
    const decl = valueDecl(node);
    if (decl && ts.isEnumMember(decl) && decl.initializer) {
      // KEEP THE PROVENANCE. `getConstantValue` returns undefined for the CROSS-FILE enum-member accesses a
      // real codebase uses, so this fallback runs - and returning a bare number turned `[Demo.A,
      // Demo.B]` into `[1, 2]` with no record of WHICH members those were.
      return { $enum: node.getText(), value: api.follow(decl, decl.initializer, node.getText()) };
    }
    if (decl && ts.isPropertyAssignment(decl) && decl.initializer) {
      // A CONST OBJECT IS A CONST MAP. `Flags` is a plain object in some applications, so the
      // access resolved to the bare string and the map held no record of which member produced it - silently
      // lossy anywhere a map's value differs from its key.
      return { $member: node.getText(), value: api.follow(decl, decl.initializer, node.getText()) };
    }
    if (decl && ts.isPropertyDeclaration(decl) && decl.initializer
      && (ts.getCombinedModifierFlags(decl) & ts.ModifierFlags.Static) !== 0) {
      // A CLASS OF STATIC FIELDS IS A CONST MAP TOO - `export class Flags { static IsX = 'IsX'; }`,
      // and any class of the same shape, however many members it has.
      //
      // STATIC ONLY, and that restriction is the whole safety of this branch. An INSTANCE field's initializer
      // is its STARTING value, not its value: resolving `this.isOnline` to `false` would assert runtime state
      // the map cannot know.
      return { $member: node.getText(), value: api.follow(decl, decl.initializer, node.getText()) };
    }
    // A STATIC INDEX INTO A KNOWN COLLECTION IS KNOWABLE. `MAP[Enum.X]` / `arr[0]` where both sides evaluate
    // is a value, not an unresolvable expression - and the access itself stays visible rather than being
    // replaced by the bare value.
    if (ts.isElementAccessExpression(node)) {
      const receiver = api.evalNode(node.expression);
      const rawKey = api.evalNode(node.argumentExpression);
      const key = rawKey !== null && typeof rawKey === 'object' && !Array.isArray(rawKey)
        ? rawKey.value : rawKey;
      if (Array.isArray(receiver) && typeof key === 'number' && Number.isInteger(key)
        && key >= 0 && key < receiver.length) {
        return { $index: node.getText(), value: receiver[key] };
      }
      const plainObject = receiver !== null && typeof receiver === 'object' && !Array.isArray(receiver)
        && receiver.$expr === undefined && receiver.$call === undefined;
      if (plainObject && typeof key === 'string'
        && Object.prototype.hasOwnProperty.call(receiver, key)) {
        return { $index: node.getText(), value: receiver[key] };
      }
    }
    return api.unresolved(node);
  }

  /** An identifier reference -> the literal it was initialized with, when that is knowable. */
  function evalIdentifier(node) {
    const decl = valueDecl(node);
    if (!decl) return api.unresolved(node);
    if (ts.isVariableDeclaration(decl) && decl.initializer) {
      return api.follow(decl, decl.initializer, node.getText());
    }
    if (ts.isEnumMember(decl)) {
      const value = checker.getConstantValue(decl);
      if (value !== undefined) return { $enum: node.getText(), value };
      return decl.initializer
        ? { $enum: node.getText(), value: api.follow(decl, decl.initializer, node.getText()) }
        : api.unresolved(node);
    }
    // A class / enum / interface / function reference: name it and say where it is declared. CANONICAL
    // casing, because this `file` is a JOIN KEY and a spelling `files.abs` does not carry drops the ref
    // back into the duplicate-name failure an id-keyed join exists to prevent.
    if (ts.isClassDeclaration(decl) || ts.isEnumDeclaration(decl) || ts.isInterfaceDeclaration(decl)
      || ts.isFunctionDeclaration(decl)) {
      return { $ref: node.getText(), file: canonicalPath(decl.getSourceFile().fileName) };
    }
    return api.unresolved(node);
  }

  /**
   * A DYNAMICALLY IMPORTED DECLARATION IS A REFERENCE, NOT A STRING.
   *
   * `import(<specifier>).then((m) => m.Thing)` otherwise publishes its subject as two halves a consumer
   * cannot join: a module specifier among the strings and a dotted read among the reads. Reassembling them
   * means resolving a relative path by hand and then matching a class BY NAME. The checker resolves both.
   *
   * The export is chosen by the TYPE of the receiver and not by which identifier the callback names: the
   * parameter of a `.then` over a dynamic import HAS the module's type, which also holds for an export
   * re-exported from elsewhere, where comparing declaration files alone would refuse.
   *
   * An `import()` inside a REJECTION handler is a recovery, not the answer - published with
   * `recovery: true`, so a consumer cannot mistake it for the normal destination. More than one dynamic
   * import publishes nothing at all: which one runs is a branch this join cannot pick.
   */
  function lazyTarget(fn) {
    const specifiers = [];
    let viaRecovery = false;
    const findImports = (n, recovery) => {
      if (ts.isCallExpression(n) && n.expression.kind === ts.SyntaxKind.ImportKeyword) {
        const arg = n.arguments[0];
        if (arg !== undefined) {
          specifiers.push(arg);
          viaRecovery = recovery;
        }
      }
      if (ts.isCallExpression(n) && ts.isPropertyAccessExpression(n.expression)
        && n.expression.name.text === 'catch') {
        findImports(n.expression, recovery);
        for (const a of n.arguments) findImports(a, true);
        return;
      }
      ts.forEachChild(n, (c) => findImports(c, recovery));
    };
    findImports(fn, false);
    if (specifiers.length !== 1) return null;
    const moduleFiles = new Set((symbolAt(specifiers[0])?.declarations ?? [])
      .map((d) => canonicalPath(d.getSourceFile().fileName)));
    if (moduleFiles.size === 0) return null;
    let picked = null;
    const findPick = (n) => {
      if (picked !== null) return;
      if (ts.isPropertyAccessExpression(n)) {
        const owner = checker.getTypeAtLocation(n.expression).getSymbol();
        const declaredIn = (owner?.declarations ?? [])
          .map((d) => canonicalPath(d.getSourceFile().fileName));
        if (declaredIn.some((f) => moduleFiles.has(f))) {
          picked = declTarget(n.name.text, n.name);
          if (picked !== null) return;
        }
      }
      ts.forEachChild(n, findPick);
    };
    findPick(fn);
    if (picked === null || !viaRecovery) return picked;
    return { ...picked, recovery: true };
  }

  return { valueDecl, declTarget, evalMemberAccess, evalIdentifier, lazyTarget };
}
