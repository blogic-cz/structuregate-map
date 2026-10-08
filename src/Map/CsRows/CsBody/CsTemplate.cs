using System.Text;
using System.Text.Json;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using Microsoft.CodeAnalysis.CSharp.Syntax;

namespace StructureGate;

/// <summary>
/// A STRING THAT DOES NOT FOLD WHOLE, AS ITS PIECES: `arguments.template`, a JSON list of the strings the
/// argument can be - each a list of literal text and `{"hole": "&lt;C# source&gt;"}` - for the SQL links
/// (`src/Map/Sql/SqlHoles.cs`) to fill and parse.
///
/// `const` STAYS EMPTY for such a string: half a path looks like a path (`CsFolding`). The template is the
/// other answer, and says which half is unknown. What folds is folded INTO it - `nameof(...)`, a `const`
/// field, a `static readonly` nothing writes - so `$"SELECT {nameof(P.Id)} FROM {Tables.Products}"` has no
/// hole at all; a hole is what only the run time knows (`{count}`, `{date:yyyy-MM-dd}`, an enum's name).
///
/// FOLLOWED like a fold: an interpolation, a string `+`, `string.Format` (each `{n}` a hole unless its
/// argument folds), a local or a readonly field to its initializer, and the variable of a `foreach` over a
/// collection written out in the code - one string per element, so a loop running five literal DELETEs is
/// five strings.
/// </summary>
internal static class CsTemplate
{
    /// <summary>Literal text, or - when `Hole` - the C# source of a value only the run time knows.</summary>
    /// <summary>A piece of a string: literal text, or a hole's C# source - with, where the hole is a field, the
    /// field's symbol, so the links can read its value off the `consts` row a referenced project wrote.</summary>
    public sealed record Piece(string Text, bool Hole, string Symbol = "");

    /// <summary>How deep a template is followed, and how many strings one argument may stand for.</summary>
    private const int MaxDepth = 6;
    private const int MaxStrings = 32;

    /// <summary>The template of an argument whose value did not fold, as JSON; "" when there is nothing to say:
    /// a value that folded, no model, or no literal text around the holes.</summary>
    public static string Of(SemanticModel? model, ExpressionSyntax expression, CsFolding.Value value)
    {
        if (model is null || value.Kind is "const" or "folded") return "";
        List<List<Piece>>? strings;
        try { strings = Strings(model, expression, 0); }
        catch (ArgumentException) { return ""; }
        if (strings is null || !strings.Any(s => s.Any(p => !p.Hole && p.Text.Trim().Length > 0))) return "";
        using var stream = new MemoryStream();
        using (var json = new Utf8JsonWriter(stream))
        {
            json.WriteStartArray();
            foreach (var pieces in strings)
            {
                json.WriteStartArray();
                foreach (var piece in pieces)
                {
                    if (!piece.Hole) { json.WriteStringValue(piece.Text); continue; }
                    json.WriteStartObject();
                    json.WriteString("hole", piece.Text);
                    if (piece.Symbol.Length > 0) json.WriteString("symbol", piece.Symbol);
                    json.WriteEndObject();
                }
                json.WriteEndArray();
            }
            json.WriteEndArray();
        }
        return Encoding.UTF8.GetString(stream.ToArray());
    }

    /// <summary>Every string an expression can be, or null when it is one value only the run time knows.</summary>
    private static List<List<Piece>>? Strings(SemanticModel model, ExpressionSyntax expression, int depth)
    {
        if (depth > MaxDepth) return null;
        var folded = CsFolding.Of(model, expression);
        if (folded.Kind is "const" or "folded") return [[new Piece(folded.Text, false)]];
        switch (expression)
        {
            case ParenthesizedExpressionSyntax inner:
                return Strings(model, inner.Expression, depth);
            case InterpolatedStringExpressionSyntax interpolated:
                return Interpolated(model, interpolated, depth);
            case BinaryExpressionSyntax binary when binary.IsKind(SyntaxKind.AddExpression)
                    && model.GetTypeInfo(binary).Type?.SpecialType == SpecialType.System_String:
                return Product(Strings(model, binary.Left, depth + 1) ?? [[Hole(binary.Left, model)]],
                    Strings(model, binary.Right, depth + 1) ?? [[Hole(binary.Right, model)]]);
            case InvocationExpressionSyntax invocation:
                return Format(model, invocation, e => CsFolding.Of(model, e)) is { } pieces ? [pieces] : null;
            case IdentifierNameSyntax or MemberAccessExpressionSyntax:
                return Named(model, expression, depth);
            default:
                return null;
        }
    }

    private static List<List<Piece>>? Interpolated(SemanticModel model, InterpolatedStringExpressionSyntax node, int depth)
    {
        var pieces = new List<Piece>();
        foreach (var content in node.Contents)
        {
            switch (content)
            {
                case InterpolatedStringTextSyntax literal:
                    pieces.Add(new Piece(literal.TextToken.ValueText, false));
                    break;
                // A FORMATTED HOLE (`{date:yyyy-MM-dd}`, `{x,8}`) is a value whatever its operand folds to.
                case InterpolationSyntax hole when hole.AlignmentClause is not null || hole.FormatClause is not null:
                    pieces.Add(Hole(hole.Expression, model));
                    break;
                case InterpolationSyntax hole:
                    var inner = model.GetTypeInfo(hole.Expression).Type?.SpecialType == SpecialType.System_String
                        ? Strings(model, hole.Expression, depth + 1) : null;
                    if (inner is { Count: 1 }) pieces.AddRange(inner[0]);
                    else pieces.Add(Hole(hole.Expression, model));
                    break;
                default:
                    return null;
            }
        }
        return [Joined(pieces)];
    }

    /// <summary>
    /// `string.Format(format, args...)` as pieces: each `{n}` is the argument's folded text, or a hole when it
    /// does not fold or carries an alignment or a format. Null when the call is no `string.Format` over a
    /// format that folds - `CsFolding` folds the call whole when no hole is left (`Format("DELETE FROM T")`).
    /// </summary>
    public static List<Piece>? Format(SemanticModel model, InvocationExpressionSyntax invocation, Func<ExpressionSyntax, CsFolding.Value> fold)
    {
        if (model.GetSymbolInfo(invocation).Symbol is not IMethodSymbol { Name: "Format" } method
            || method.ContainingType?.SpecialType != SpecialType.System_String
            || method.Parameters.Length == 0 || method.Parameters[0].Type.SpecialType != SpecialType.System_String) return null;
        var arguments = invocation.ArgumentList.Arguments;
        if (arguments.Count == 0 || arguments.Any(a => a.NameColon is not null)) return null;
        // `params object[]` passed as ONE array is a list this does not open.
        if (arguments.Count == 2 && model.GetTypeInfo(arguments[1].Expression).Type is IArrayTypeSymbol) return null;
        var format = fold(arguments[0].Expression);
        if (format.Kind is not ("const" or "folded")) return null;
        var text = format.Text;
        var pieces = new List<Piece>();
        var literal = new StringBuilder();
        for (var i = 0; i < text.Length; i++)
        {
            var c = text[i];
            if ((c == '{' || c == '}') && i + 1 < text.Length && text[i + 1] == c) { literal.Append(c); i++; continue; }
            if (c == '}') return null;
            if (c != '{') { literal.Append(c); continue; }
            var close = text.IndexOf('}', i);
            if (close < 0) return null;
            var item = text[(i + 1)..close];
            var digits = item.TakeWhile(char.IsAsciiDigit).Count();
            if (digits == 0 || !int.TryParse(item[..digits], out var index) || index + 1 >= arguments.Count) return null;
            pieces.Add(new Piece(literal.ToString(), false));
            literal.Clear();
            var argument = arguments[index + 1].Expression;
            var value = digits == item.Length ? fold(argument) : CsFolding.Value.None;
            pieces.Add(value.Kind is "const" or "folded" ? new Piece(value.Text, false) : Hole(argument, model));
            i = close;
        }
        pieces.Add(new Piece(literal.ToString(), false));
        return Joined(pieces);
    }

    /// <summary>A name: the string its declaration gives it where nothing writes it again (`CsFolding.Initializer`),
    /// or each element of what a `foreach` runs over.</summary>
    private static List<List<Piece>>? Named(SemanticModel model, ExpressionSyntax expression, int depth)
    {
        var symbol = CsSemantics.Bound(model, expression);
        if (symbol is ILocalSymbol { IsForEach: true })
        {
            foreach (var reference in symbol.DeclaringSyntaxReferences)
            {
                if (reference.GetSyntax() is ForEachStatementSyntax loop && Owner(model, loop) is { } loopModel)
                    return Elements(loopModel, loop.Expression, depth + 1);
            }
            return null;
        }
        if (symbol is null || CsFolding.Initializer(symbol) is not { } initializer || CsFolding.Written(symbol, model.Compilation)) return null;
        return Owner(model, initializer) is { } owner ? Strings(owner, initializer, depth + 1) : null;
    }

    /// <summary>The strings of a collection written out in the code - an array, a collection expression -
    /// directly or through a name that is never written again.</summary>
    private static List<List<Piece>>? Elements(SemanticModel model, ExpressionSyntax expression, int depth)
    {
        if (depth > MaxDepth) return null;
        IEnumerable<ExpressionSyntax>? items = expression switch
        {
            ArrayCreationExpressionSyntax { Initializer: { } init } => init.Expressions,
            ImplicitArrayCreationExpressionSyntax created => created.Initializer.Expressions,
            // `new List<string> { "DELETE ...", ... }`: a collection initializer holds the strings as an array's does.
            BaseObjectCreationExpressionSyntax { Initializer: { } init } when init.IsKind(SyntaxKind.CollectionInitializerExpression)
                && init.Expressions.All(e => e is not InitializerExpressionSyntax) => init.Expressions,
            InitializerExpressionSyntax init => init.Expressions,
            CollectionExpressionSyntax collection => collection.Elements.All(e => e is ExpressionElementSyntax)
                ? collection.Elements.Cast<ExpressionElementSyntax>().Select(e => e.Expression) : null,
            _ => null,
        };
        if (items is null)
        {
            if (expression is not (IdentifierNameSyntax or MemberAccessExpressionSyntax)) return null;
            var symbol = CsSemantics.Bound(model, expression);
            if (symbol is null || CsFolding.Initializer(symbol) is not { } initializer || CsFolding.Written(symbol, model.Compilation)) return null;
            return Owner(model, initializer) is { } owner ? Elements(owner, initializer, depth + 1) : null;
        }
        var all = new List<List<Piece>>();
        foreach (var item in items)
        {
            all.AddRange(Strings(model, item, depth + 1) ?? [[Hole(item, model)]]);
            if (all.Count > MaxStrings) return null;
        }
        return all.Count > 0 ? all : null;
    }

    /// <summary>The model for the tree a node lives in - a model answers about its own tree only.</summary>
    private static SemanticModel? Owner(SemanticModel model, SyntaxNode node) =>
        node.SyntaxTree == model.SyntaxTree ? model
            : model.Compilation.ContainsSyntaxTree(node.SyntaxTree) ? model.Compilation.GetSemanticModel(node.SyntaxTree) : null;

    private static List<List<Piece>>? Product(List<List<Piece>> left, List<List<Piece>> right)
    {
        if (left.Count * right.Count > MaxStrings) return null;
        return [.. left.SelectMany(l => right.Select(r => Joined([.. l, .. r])))];
    }

    private static Piece Hole(ExpressionSyntax expression, SemanticModel model) =>
        new(CsFacts.Text(expression), true, CsSemantics.Bound(model, expression) is IFieldSymbol field ? CsSemantics.Name(field) : "");

    /// <summary>Adjacent literal text as one piece, and no empty text.</summary>
    private static List<Piece> Joined(List<Piece> pieces)
    {
        var joined = new List<Piece>();
        foreach (var piece in pieces)
        {
            if (!piece.Hole && piece.Text.Length == 0) continue;
            if (!piece.Hole && joined.Count > 0 && !joined[^1].Hole) joined[^1] = new Piece(joined[^1].Text + piece.Text, false);
            else joined.Add(piece);
        }
        return joined;
    }
}
