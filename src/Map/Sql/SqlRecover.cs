using Microsoft.SqlServer.TransactSql.ScriptDom;

namespace StructureGate;

/// <summary>
/// ONE BAD STATEMENT MUST NOT ERASE ITS BATCH'S GOOD ONES. ScriptDom gives no usable tree for a text with any
/// error, so a file with one broken statement lost every row of the others; PostgreSQL's half already recovers
/// per statement (`rust/fbtcore/src/sqlpg/mod.rs`) and T-SQL does the same here.
///
/// SPANS come from ScriptDom's own tokenizer, never a text scan: a span ends at a `;` or `GO` outside every
/// BEGIN..END / CASE..END block (`BEGIN TRAN`, `END CONVERSATION` and the like open or close nothing). The span
/// holding an error is BLANKED - every character a space except line breaks - so every line and offset the rows
/// carry still points into the file, and the text is parsed again. One diagnostic per blanked span: its own first.
/// EVERY BLANK IS CONFIRMED BY AN ISOLATED PARSE: ScriptDom reports a statement that fails at its first token on the
/// PREVIOUS statement's terminator, so the offset alone would blank the good statement; parsing a candidate alone
/// proves it is the bad one and gives its own error line.
/// </summary>
internal static class SqlRecover
{
    private static readonly HashSet<string> NotBlock = new(StringComparer.Ordinal)
        { "TRAN", "TRANSACTION", "DISTRIBUTED", "DIALOG", "CONVERSATION" };

    public static (TSqlFragment? Fragment, string Text, List<ParseError> Errors) Parse(string prepared)
    {
        var parser = new TSql170Parser(initialQuotedIdentifiers: true);
        var (fragment, errors) = ParseOnce(parser, prepared);
        if (errors.Count == 0) return (fragment, prepared, errors);

        var (firstFragment, firstErrors) = (fragment, errors);
        // A RECOVERY BUG COSTS THE STATEMENTS OF THAT FILE, NEVER THE FILE OR THE BATCH: any failure falls back to the first pass.
        try
        {
            var spans = Spans(parser, prepared);
            var blanked = new HashSet<int>();
            var kept = new List<ParseError>();
            var text = prepared;
            while (true)
            {
                var chars = text.ToCharArray();
                var lost = new List<ParseError>();
                var confirmed = false;
                foreach (var error in errors)
                {
                    var k = spans.FindIndex(s => s.Start <= error.Offset && error.Offset < s.End);
                    if (k < 0) { lost.Add(error); continue; }
                    var candidates = error.Offset >= spans[k].Term && spans[k].Term >= 0 && k + 1 < spans.Count ? new[] { k + 1, k } : [k];
                    if (candidates.All(c => blanked.Contains(spans[c].Start))) continue;
                    var found = false;
                    foreach (var c in candidates.Where(c => !blanked.Contains(spans[c].Start)))
                    {
                        var alone = Blank(text, spans, c);
                        var own = ParseOnce(new TSql170Parser(initialQuotedIdentifiers: true), alone).Errors;
                        if (own.Count == 0) continue;
                        blanked.Add(spans[c].Start);
                        kept.Add(own[0]);
                        Blank(chars, spans[c].Start, spans[c].End);
                        found = confirmed = true;
                        break;
                    }
                    if (!found) lost.Add(error);
                }
                if (!confirmed) return (fragment, text, [.. kept, .. lost]);
                text = new string(chars);
                (fragment, errors) = ParseOnce(parser, text);
                if (errors.Count == 0) return (fragment, text, kept);
            }
        }
        catch (Exception e) when (e is not OutOfMemoryException) { return (firstFragment, prepared, firstErrors); }
    }

    // EVERY CHARACTER OF A RANGE A SPACE BUT THE LINE BREAKS, so lines and offsets stay put.
    private static void Blank(char[] chars, int from, int to)
    {
        for (var i = from; i < to && i < chars.Length; i++)
        {
            if (chars[i] != '\r' && chars[i] != '\n') chars[i] = ' ';
        }
    }

    // The text with everything OUTSIDE span `keep` blanked.
    private static string Blank(string text, List<(int Start, int End, int Term)> spans, int keep)
    {
        var chars = text.ToCharArray();
        Blank(chars, 0, spans[keep].Start);
        Blank(chars, spans[keep].End, chars.Length);
        return new string(chars);
    }

    private static (TSqlFragment? Fragment, List<ParseError> Errors) ParseOnce(TSqlParser parser, string text)
    {
        TSqlFragment? fragment;
        IList<ParseError> errors;
        using (var reader = new StringReader(text)) fragment = parser.Parse(reader, out errors);
        return (fragment, [.. errors]);
    }

    private static List<(int Start, int End, int Term)> Spans(TSqlParser parser, string text)
    {
        IList<TSqlParserToken> tokens;
        using (var reader = new StringReader(text)) tokens = parser.GetTokenStream(reader, out _);
        var spans = new List<(int Start, int End, int Term)>();
        var open = 0;
        var start = -1;
        var end = 0;
        for (var i = 0; i < tokens.Count; i++)
        {
            var token = tokens[i];
            if (Blank(token)) continue;
            if (start < 0) start = token.Offset;
            end = token.Offset + token.Text.Length;
            var next = Next(tokens, i);
            switch (token.TokenType)
            {
                case TSqlTokenType.Begin when !NotBlock.Contains(next): open++; break;
                case TSqlTokenType.Case: open++; break;
                case TSqlTokenType.End when next != "CONVERSATION": open = Math.Max(0, open - 1); break;
                case TSqlTokenType.Semicolon or TSqlTokenType.Go when open == 0:
                    spans.Add((start, token.Offset + token.Text.Length, token.Offset));
                    start = -1;
                    break;
            }
        }
        if (start >= 0) spans.Add((start, end, -1));
        return spans;
    }

    private static bool Blank(TSqlParserToken t) =>
        t.TokenType is TSqlTokenType.EndOfFile or TSqlTokenType.WhiteSpace or TSqlTokenType.SingleLineComment or TSqlTokenType.MultilineComment;

    private static string Next(IList<TSqlParserToken> tokens, int from)
    {
        for (var i = from + 1; i < tokens.Count; i++)
        {
            if (!Blank(tokens[i])) return (tokens[i].Text ?? "").ToUpperInvariant();
        }
        return "";
    }
}
