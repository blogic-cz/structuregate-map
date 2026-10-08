/**
 * THE TEXT IN A FILE THAT IS NOT CODE: every comment, every string literal, and what a file EXPORTS.
 *
 * Ported from the original Angular extractor.
 * All three exist because the alternative is a consumer pattern-matching source lines, which is the one
 * thing this tool forbids in every language it maps.
 */
import { locationOf, parentKind, propertyContext } from './TsNodes.mjs';
import { literalUse, numberOf } from './TsLiterals.mjs';
import { canonicalPath, relativeTo } from './TsPaths.mjs';

/**
 * EVERY comment in the file, through the compiler's own scanner run with trivia enabled.
 *
 * Leading trivia of top-level statements is not enough: the comments that explain business rules sit
 * inside method bodies and next to class members, and a statement-level pass found ZERO of them across
 * hundreds of files of a real frontend. The scanner sees all trivia, so nothing is left behind.
 */
export function collectComments(ts, store, sf, fileId) {
  const scanner = ts.createScanner(ts.ScriptTarget.Latest, false, ts.LanguageVariant.Standard, sf.text);
  let kind = scanner.scan();
  while (kind !== ts.SyntaxKind.EndOfFileToken) {
    if (kind === ts.SyntaxKind.SingleLineCommentTrivia || kind === ts.SyntaxKind.MultiLineCommentTrivia) {
      const pos = scanner.getTokenStart();
      const start = sf.getLineAndCharacterOfPosition(pos);
      store.add('comments', 'cm', {
        file: fileId,
        kind: kind === ts.SyntaxKind.SingleLineCommentTrivia ? 'line' : 'block',
        line: start.line + 1,
        col: start.character + 1,
        context: enclosingLabel(ts, sf, pos),
        text: scanner.getTokenText(),
      });
    }
    kind = scanner.scan();
  }
}

/** The innermost NAMED declaration containing `pos` - "which class, which method is this comment in".
 *  Walked by POSITION over the AST; no text scanning. */
function enclosingLabel(ts, sf, pos) {
  const parts = [];
  const descend = (node) => {
    ts.forEachChild(node, (child) => {
      if (child.pos <= pos && pos < child.end) {
        if (child.name && typeof child.name.getText === 'function') {
          try {
            parts.push(child.name.getText());
          } catch { /* unnamed after all */ }
        }
        descend(child);
      }
    });
  };
  descend(sf);
  return parts.length ? parts.join('.') : null;
}

/**
 * Every string literal in the file, with WHAT IT SITS IN.
 *
 * A bare `'en'` says nothing. `'en'` as the `locale` property of a call to `setLanguage` says what it is,
 * and `context`/`property` are how that is published without anyone having to re-read the source.
 */
export function collectStrings(ts, store, node, owner, propName, evalNode) {
  const text = (n) => n.getText();
  const walk = (n) => {
    // `use`/`callee`/`target` are the vocabulary every half shares (TsLiterals.mjs); `context` stays this
    // half's own - the parent's SyntaxKind - for the readers that already ask for it.
    if (ts.isStringLiteral(n) || ts.isNoSubstitutionTemplateLiteral(n)) {
      store.add('string_literals', 'sl', {
        ...owner, value: n.text, context: parentKind(ts, n),
        property: propertyContext(ts, n, propName), ...literalUse(ts, n, text), ...locationOf(n),
      });
    } else if (ts.isNumericLiteral(n)) {
      store.add('number_literals', 'nl', { ...owner, ...numberOf(ts, n, text), ...literalUse(ts, n, text), ...locationOf(n) });
    }
    // A TEMPLATE LITERAL WITH SUBSTITUTIONS IS NOT A STRING, and it is not nothing either: its literal
    // PARTS are the stable half of a key or a path built at run time, and its HOLES say where the rest
    // comes from. Published as its own table, so a consumer never has to re-parse the text to find them.
    if (ts.isTemplateExpression(n) && evalNode) {
      const v = evalNode(n);
      store.add('template_literals', 'tl', {
        ...owner, parts: v.$template ?? [], holes: v.$holes ?? [],
        context: parentKind(ts, n), property: propertyContext(ts, n, propName), ...locationOf(n),
      });
    }
    const regex = regexOf(ts, n);
    if (regex) store.add('regexes', 'rx', { ...owner, ...regex, context: parentKind(ts, n), ...locationOf(n) });
    ts.forEachChild(n, walk);
  };
  walk(node);
}

/**
 * A regex the code BUILDS - a literal `/x/g` or a `RegExp` construction - as the columns of a `regexes` row
 * (the table the plain half and the python and C# halves fill too), or null for any other node.
 *
 * THIS WALK HAS NO CHECKER: a `RegExp` the file declares itself is not told apart from the global one. The
 * plain half does better, because it has the file's bindings. A pattern is known only from a literal; one
 * built at run time is '' - never a guess. A literal handed to `RegExp` is its own row, used by `RegExp`.
 */
function regexOf(ts, n) {
  if (ts.isRegularExpressionLiteral(n)) {
    const last = n.text.lastIndexOf('/');
    return { kind: 'literal', api: '', pattern: n.text.slice(1, last), pattern_kind: 'literal',
      flags: n.text.slice(last + 1), source: n.text, used_by: usedBy(ts, n) };
  }
  if (!(ts.isNewExpression(n) || ts.isCallExpression(n)) || !ts.isIdentifier(n.expression)
    || n.expression.text !== 'RegExp') return null;
  const [first, second] = n.arguments ?? [];
  if (first && ts.isRegularExpressionLiteral(first)) return null;
  const literal = (arg) => (arg && (ts.isStringLiteral(arg) || ts.isNoSubstitutionTemplateLiteral(arg)) ? arg.text : null);
  const pattern = literal(first);
  return { kind: 'call', api: 'RegExp', pattern: pattern ?? '', pattern_kind: pattern === null ? '' : 'literal',
    flags: literal(second) ?? '', source: n.getText(), used_by: usedBy(ts, n) };
}

/** WHAT RECEIVES A REGEX: the method it is passed to, the variable or property it initialises, or ''. */
function usedBy(ts, n) {
  const parent = n.parent;
  if (parent && (ts.isCallExpression(parent) || ts.isNewExpression(parent)) && parent.expression !== n) {
    return ts.isPropertyAccessExpression(parent.expression) ? '.' + parent.expression.name.text : parent.expression.getText();
  }
  if (parent && (ts.isVariableDeclaration(parent) || ts.isPropertyDeclaration(parent)) && parent.initializer === n) {
    return '= ' + parent.name.getText();
  }
  return '';
}

/**
 * What a file makes public, with THE NAMES PARSED - not just the statement's text.
 *
 * `text` alone made this table unjoinable: the only way to learn that `export { ProfileComponent };`
 * is what makes that class public was to pattern-match the source line. `names` are the LOCAL names (what
 * a declaration in this file answers to), `exported_as` the outside world's, and `from` is set where the
 * statement re-exports another file's declarations instead of this file's own.
 */
export function exportRow(ts, store, statement, fileId, symbolAt, feRoot) {
  const clause = ts.isExportDeclaration(statement) ? statement.exportClause : undefined;
  const elements = clause && ts.isNamedExports(clause) ? clause.elements : [];
  const spec = ts.isExportDeclaration(statement) ? statement.moduleSpecifier : undefined;
  // WHICH FILE `from` IS - the compiler's answer, as `imports.resolved` is for an import. The specifier
  // alone is text: `./x` means `x.ts`, `x/index.ts` or `x.d.ts`, and only resolution says which.
  const target = spec && symbolAt ? symbolAt(spec)?.declarations?.find((d) => ts.isSourceFile(d)) : undefined;
  store.add('exports', 'ex', {
    file: fileId,
    text: statement.getText(),
    names: elements.map((e) => (e.propertyName ?? e.name).text),
    exported_as: elements.map((e) => e.name.text),
    from: spec && ts.isStringLiteral(spec) ? spec.text : null,
    ...(target ? { resolved: relativeTo(feRoot, canonicalPath(target.fileName)) } : {}),
    ...locationOf(statement),
  });
}
