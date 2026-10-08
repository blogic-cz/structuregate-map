/**
 * WHAT A FILE DECLARES, AS A HASH - the shape a partial run compares before it lets an edit to a file a template
 * resolves through go the short way.
 *
 * A file that declares a selector, a pipe, a directive or an NgModule is in the plan's `scope`, and an edit to it
 * once read the whole workspace again: a changed selector can make a template match something it never matched.
 * Most edits to such a file do not touch anything a template can resolve - a comment, a method body. So the
 * shape is every TOKEN of the file except the ones inside a function's body: decorators, imports, consts, member
 * signatures and initialisers move it; comments and whitespace are trivia and never do. It is one-sided, like the
 * surface: node computes it on both runs, and nothing else has to agree with it.
 *
 * A value a decorator reaches in ANOTHER file is not in this file's shape - and was not in the scope either.
 */
import { createHash } from 'node:crypto';

/** The shape of a parsed file, as a hash. */
export function shapeOf(ts, sf) {
  const hash = createHash('sha256');
  // THE PARENT IS THE WALK'S OWN: a file parsed without a program has no parent pointers until it is bound.
  const visit = (n, parent) => {
    if (n.kind >= ts.SyntaxKind.FirstJSDocNode && n.kind <= ts.SyntaxKind.LastJSDocNode) return;
    if (ts.isBlock(n) && parent && isFunctionLike(ts, parent)) return;
    const kids = n.getChildren(sf);
    if (!kids.length) {
      hash.update(n.getText(sf));
      hash.update('\u0000');
      return;
    }
    for (const c of kids) visit(c, n);
  };
  visit(sf, null);
  return hash.digest('hex').slice(0, 32);
}

/** The shape of a file as it is on disk now, parsed without a program - what the plan run asks. Null when unreadable. */
export function shapeOfText(ts, abs, text) {
  if (typeof text !== 'string') return null;
  return shapeOf(ts, ts.createSourceFile(abs, text, ts.ScriptTarget.Latest, true));
}

function isFunctionLike(ts, n) {
  return ts.isMethodDeclaration(n) || ts.isConstructorDeclaration(n) || ts.isGetAccessorDeclaration(n)
    || ts.isSetAccessorDeclaration(n) || ts.isFunctionDeclaration(n) || ts.isFunctionExpression(n)
    || ts.isArrowFunction(n);
}
