/**
 * WHAT A LOOP STATEMENT SAYS, as plain facts about its node - `TsBody.mjs` writes the row and keeps the walk's state.
 *
 * A LOOP IS ITS OWN TABLE (`loops`), NEVER A `branches` ROW. The plain half and C# record a loop as a `branches` row
 * with a `kind`; here every `branches` row is an if-arm (`sense`) the rust closure reads as a CONDITION
 * (`key/key_branches.rs`), so a key assigned inside a loop written as one would turn conditional and a release
 * compare would report a restriction lost. A row inside a loop body carries `loop` beside `branch` instead.
 *
 * Imports are flat: see `TsTypeRef.mjs`.
 */

/** Every name a binding declares, a destructuring pattern's included (`for (const [k, { v }] of m)`). */
function namesOf(ts, name, out) {
  if (ts.isIdentifier(name)) { out.push(name.text); return; }
  for (const el of name.elements) if (ts.isBindingElement(el)) namesOf(ts, el.name, out);
}

/** The kind of a loop statement, or null for any other node. From node KINDS, never the keyword text. */
export function loopKind(ts, node) {
  if (ts.isForOfStatement(node)) return 'for_of';
  if (ts.isForInStatement(node)) return 'for_in';
  if (ts.isForStatement(node)) return 'for';
  if (ts.isWhileStatement(node)) return 'while';
  if (ts.isDoStatement(node)) return 'do';
  return null;
}

/**
 * The facts of a loop: its kind, `await`, the names its initializer declares, and the header expression `source`
 * is read from (the iterable of a for-of/in, the condition of the others; null when a `for` has none).
 */
export function loopFacts(ts, node) {
  const kind = loopKind(ts, node);
  const variables = [];
  const init = node.initializer;
  if (init && ts.isVariableDeclarationList(init)) for (const d of init.declarations) namesOf(ts, d.name, variables);
  const header = ts.isForOfStatement(node) || ts.isForInStatement(node) ? node.expression : (node.condition ?? node.expression ?? null);
  return {
    kind, variables, header,
    await: ts.isForOfStatement(node) && node.awaitModifier !== undefined,
    incrementor: ts.isForStatement(node) ? (node.incrementor ?? null) : null,
  };
}
