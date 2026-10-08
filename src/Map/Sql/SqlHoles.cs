using System.Text;
using Microsoft.SqlServer.TransactSql.ScriptDom;

namespace StructureGate;

/// <summary>
/// SQL BUILT AROUND HOLES (`arguments.template`, `CsTemplate`): which holes are VALUES and which name what
/// the SQL touches, decided by the T-SQL parser - never by reading the text around a hole.
///
/// EACH HOLE IS FIRST FILLED WITH `0`. Where that parses, a hole whose token is a number, a string or a
/// variable sits where a value goes - a comparison, `IN (...)`, `TOP`, a quoted date - and the tables around
/// it are what the SQL touches whatever the run time puts there: `Demo_Clear` deletes from several
/// tables, whatever `{count}` is. A `0` that lands inside a NAME (`[dbo].[0_Logs]`, `Trig_0`) is a hole in an
/// identifier, filled again with a marker so the touch it is part of can be told from the ones that are known.
/// Where `0` does not parse (`FROM {schema}.T`, `FROM T {condition}`) the holes are filled with the marker
/// alone, then with nothing; a hole that is no value there is SQL of its own, and the link is `partial`.
/// </summary>
internal static class SqlHoles
{
    /// <summary>A hole: its C# source, and whether the parser held it as a value.</summary>
    public sealed record Hole(string Source, bool Value);

    /// <summary>A template read: the visitor over the filled SQL (null with `Failed`), and its holes.</summary>
    public sealed record Reading(SqlVisitor? Visitor, string Failed, List<Hole> Holes);

    /// <summary>The marker a hole in a name is filled with - a plain identifier, so the name still parses.</summary>
    public static string Marker(int hole) => $"zzhole{hole}zz";

    private static readonly HashSet<TSqlTokenType> Values =
    [
        TSqlTokenType.Integer, TSqlTokenType.Numeric, TSqlTokenType.Real, TSqlTokenType.Money,
        TSqlTokenType.AsciiStringLiteral, TSqlTokenType.UnicodeStringLiteral, TSqlTokenType.Variable,
    ];

    /// <summary>One string of a template: its pieces, literal text or (`Hole`) the C# source of a hole.</summary>
    public static Reading Read(List<(string Text, bool Hole)> pieces)
    {
        var sources = pieces.Where(p => p.Hole).Select(p => p.Text).ToList();
        var zeroed = Parse(pieces, _ => "0");
        if (zeroed.Errors.Count == 0)
        {
            var value = sources.Select((_, i) => Values.Contains(TokenAt(zeroed.Tokens, zeroed.Offsets[i]))).ToList();
            var holes = sources.Select((s, i) => new Hole(s, value[i])).ToList();
            if (value.All(v => v)) return new Reading(zeroed.Visitor, "", holes);
            var marked = Parse(pieces, i => value[i] ? "0" : Marker(i));
            if (marked.Errors.Count == 0) return new Reading(marked.Visitor, "", holes);
        }
        var named = Parse(pieces, Marker);
        if (named.Errors.Count == 0)
        {
            var strings = sources.Select((_, i) => TokenAt(named.Tokens, named.Offsets[i]) is TSqlTokenType.AsciiStringLiteral
                or TSqlTokenType.UnicodeStringLiteral).ToList();
            return new Reading(named.Visitor, "", [.. sources.Select((s, i) => new Hole(s, strings[i]))]);
        }
        var emptied = Parse(pieces, _ => "");
        if (emptied.Errors.Count == 0) return new Reading(emptied.Visitor, "", [.. sources.Select(s => new Hole(s, false))]);
        var error = zeroed.Errors.Count > 0 ? zeroed.Errors[0] : named.Errors[0];
        return new Reading(null, $"line {error.Line}: {error.Message}", [.. sources.Select(s => new Hole(s, false))]);
    }

    private sealed record Parsed(SqlVisitor Visitor, IList<ParseError> Errors, IList<TSqlParserToken> Tokens, List<int> Offsets);

    /// <summary>The pieces with each hole filled by `fill(index)`, parsed. Dapper's `IN @ids` is rewritten per
    /// literal piece, so the offset of every hole is known in the text the parser reads.</summary>
    private static Parsed Parse(List<(string Text, bool Hole)> pieces, Func<int, string> fill)
    {
        var built = new StringBuilder();
        var offsets = new List<int>();
        foreach (var (text, hole) in pieces)
        {
            if (!hole) { built.Append(SqlLinks.Lists(text)); continue; }
            offsets.Add(built.Length);
            built.Append(fill(offsets.Count - 1));
        }
        var prepared = SqlVisitor.Prepared(built.ToString(), []);
        var parser = new TSql170Parser(initialQuotedIdentifiers: true);
        using var reader = new StringReader(prepared);
        var fragment = parser.Parse(reader, out var errors);
        var visitor = new SqlVisitor(prepared);
        if (errors.Count == 0) fragment?.Accept(visitor);
        return new Parsed(visitor, errors, fragment?.ScriptTokenStream ?? [], offsets);
    }

    /// <summary>The type of the token an offset falls in; `None` when it falls in none.</summary>
    private static TSqlTokenType TokenAt(IList<TSqlParserToken> tokens, int offset)
    {
        foreach (var token in tokens)
        {
            if (offset >= token.Offset && offset < token.Offset + (token.Text?.Length ?? 0)) return token.TokenType;
        }
        return TSqlTokenType.None;
    }
}
