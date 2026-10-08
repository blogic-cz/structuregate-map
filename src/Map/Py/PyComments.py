"""PyComments.py - the COMMENTS of a python file, for the `comments` rows of `PyRows.py`.

WHY. `ast` drops every comment, so a rule written as one - `except Exception:  # noqa: BLE001` - could not be
audited from the map: the handler was a row and the waiver beside it was not. `file_text` holds the words,
but a full-text hit is a line of text, never a fact joined to the statement it sits on.

READ BY THE TOKENIZER, never by looking for `#` in the text. A `#` inside a string is not a comment, and only
python's own tokenizer knows where a string ends - the same reason everything else here is read off `ast`.
A file `ast` parsed can still stop the tokenizer short (a form feed, a stray dedent); what was read up to
there is kept, and the rest of the file simply has no comment rows.

THE SAME TABLE AS THE TYPESCRIPT HALF'S: `kind`, `line`, `col`, `context` (the innermost named def or class,
dotted, or null at module level) and `text`, so one query reads the comments of a tree that holds both. A
python row adds `inline`, `noqa` and `codes`.

It is IMPORTED, never launched, and staged and embedded with `PyRows.py` - see PyBind.py.
"""
from __future__ import annotations

import ast
import io
import tokenize

# The marker flake8 and ruff read, compared lowercased.
NOQA = "noqa"


def comments_of(text: str) -> list:
    """`(line, col, text, inline)` per comment, in file order; `inline` is 1 when code precedes it on its
    line, and `col` is 1-based, as the TypeScript half writes it."""
    found = []
    code_on = set()
    try:
        for token in tokenize.generate_tokens(io.StringIO(text).readline):
            if token.type == tokenize.COMMENT:
                found.append((token.start[0], token.start[1] + 1, token.string))
            elif token.type not in (tokenize.NL, tokenize.NEWLINE, tokenize.INDENT, tokenize.DEDENT,
                                    tokenize.ENDMARKER, tokenize.ENCODING):
                code_on.update(range(token.start[0], token.end[0] + 1))
    except (tokenize.TokenError, SyntaxError):
        pass
    return [(line, col, comment, 1 if line in code_on else 0) for line, col, comment in found]


def spans_of(tree: ast.Module) -> list:
    """`(first, last, dotted name)` of every def and class, outer before inner."""
    spans = []

    def walk(node, prefix: str) -> None:
        for child in ast.iter_child_nodes(node):
            if isinstance(child, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef)):
                name = prefix + child.name
                first = min([d.lineno for d in child.decorator_list] + [child.lineno])
                spans.append((first, getattr(child, "end_lineno", child.lineno), name))
                walk(child, name + ".")
            else:
                walk(child, prefix)

    walk(tree, "")
    return spans


def context_of(spans: list, line: int):
    """The innermost def or class `line` sits in, or None at module level. Inner spans come later."""
    found = None
    for first, last, name in spans:
        if first <= line <= last:
            found = name
    return found


def record_comments(rows, file_id: str, tree: ast.Module, text: str) -> dict:
    """One `comments` row per comment of the file; `{line -> the first comment on it}` back, for the
    `handlers` row whose `except` line it is."""
    spans = spans_of(tree)
    on_line = {}
    for line, col, comment, inline in comments_of(text):
        on_line.setdefault(line, comment)
        noqa, codes = noqa_of(comment)
        rows.add("comments", "cm", {"file": file_id, "kind": "line", "line": line, "col": col,
                                    "context": context_of(spans, line), "text": comment, "inline": inline,
                                    "noqa": noqa, "codes": codes})
    return on_line


def noqa_of(comment: str) -> tuple:
    """`(noqa, codes)`: whether the comment carries a `noqa`, and the codes it names - empty for a bare one,
    which waives every code. `# type: ignore  # noqa: E501` is ONE comment token, so each `#` part is read."""
    for part in comment.split("#")[1:]:
        words = part.strip()
        if not words.lower().startswith(NOQA):
            continue
        rest = words[len(NOQA):]
        if not rest.startswith(":"):
            return 1, []
        codes = []
        for chunk in rest[1:].replace(",", " ").split():
            if not chunk[:1].isalpha():
                break
            codes.append(chunk)
        return 1, codes
    return 0, []
