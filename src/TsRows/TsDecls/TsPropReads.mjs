/**
 * WHICH PROPERTY NAMES A FILE READS, AND HOW OFTEN - `files.prop_reads` (`{name: count}`) and `files.this_indexed`.
 *
 * The key closure drops a component's `literal` site of a key held by a field initializer only when NOTHING can
 * read the field (`rust/fbtcore/src/rows/ts/key/routes/key_unread.rs`): no template, no string, and no TypeScript
 * read of its name. The last is this list - every name a property access (`x.name`, `x?.name`), a destructuring
 * (`const { name } = x`) or a shorthand property (`{ name }`) spells, by NAME and never resolved, so it can only
 * over-count: a read of another class's `name` keeps the field read too. A string (`x['name']`) is a
 * `string_literals` row already. The COUNT says whether every read of a private method is a call the map holds
 * (`key/key_called.rs`): `setTimeout(this.open)` is a read and no call.
 *
 * `this_indexed` is a file that hands its own instance to something that could read it by a name no source spells:
 * `this` anywhere but the receiver of `this.x` / `this['x']` or the argument of `.bind` / `.call` / `.apply` -
 * `this[k]`, `...this`, `const self = this`, `Object.values(this)`, `register(this)`. Then every field of its
 * classes may be read, and none is dropped. `untilDestroyed(this)` of `@ngneat/until-destroy` is none: it reads the destroy method alone.
 *
 * Imports are flat: see `TsTypeRef.mjs`.
 */
export function propReads(ts, sf) {
  const names = new Map();
  const add = (name) => names.set(name, (names.get(name) ?? 0) + 1);
  let indexed = false;
  // `(this as any)`, `this!` and `(this)` are `this`: the wrapper changes the type, not what is read.
  const wraps = (n) => ts.isParenthesizedExpression(n) || ts.isAsExpression(n) || ts.isNonNullExpression(n)
    || ts.isTypeAssertionExpression(n) || (ts.isSatisfiesExpression && ts.isSatisfiesExpression(n));
  const bound = (call) => ts.isPropertyAccessExpression(call.expression)
    && ['bind', 'call', 'apply'].includes(call.expression.name.text);
  // `untilDestroyed(this)` OF `@ngneat/until-destroy` (rows 124) reads ONE member of the instance - its destroy method, `ngOnDestroy` or the name
  // its second argument spells, counted below as a read of that name - so it hands nothing away. Known by the file's own import, never the name alone.
  const untilDestroyed = new Set();
  for (const st of sf.statements) {
    if (!ts.isImportDeclaration(st) || !ts.isStringLiteral(st.moduleSpecifier) || st.moduleSpecifier.text !== '@ngneat/until-destroy') continue;
    for (const el of st.importClause?.namedBindings && ts.isNamedImports(st.importClause.namedBindings) ? st.importClause.namedBindings.elements : []) {
      if ((el.propertyName ?? el.name).text === 'untilDestroyed') untilDestroyed.add(el.name.text);
    }
  }
  const destroyedBy = (call, at) => ts.isIdentifier(call.expression) && untilDestroyed.has(call.expression.text) && call.arguments[0] === at;
  const escapes = (self) => {
    let at = self;
    while (at.parent && wraps(at.parent)) at = at.parent;
    const p = at.parent;
    if (!p) return true;
    if (ts.isPropertyAccessExpression(p) && p.expression === at) return false;
    if (ts.isElementAccessExpression(p) && p.expression === at) {
      return !(p.argumentExpression && ts.isStringLiteralLike(p.argumentExpression));
    }
    if (ts.isCallExpression(p) && destroyedBy(p, at)) {
      const named = p.arguments[1];
      add(named && ts.isStringLiteralLike(named) ? named.text : 'ngOnDestroy');
      return false;
    }
    return !(ts.isCallExpression(p) && p.arguments.includes(at) && bound(p));
  };
  const walk = (n) => {
    if (ts.isPropertyAccessExpression(n)) add(n.name.text);
    else if (ts.isBindingElement(n)) {
      const named = n.propertyName ?? n.name;
      if (ts.isIdentifier(named) || ts.isStringLiteral(named)) add(named.text);
    } else if (ts.isShorthandPropertyAssignment(n)) add(n.name.text);
    else if (n.kind === ts.SyntaxKind.ThisKeyword && !indexed && escapes(n)) indexed = true;
    ts.forEachChild(n, walk);
  };
  walk(sf);
  return { prop_reads: Object.fromEntries([...names].sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))), this_indexed: indexed ? 1 : 0 };
}
