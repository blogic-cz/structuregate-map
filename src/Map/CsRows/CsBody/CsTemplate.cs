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
    /// field's symbol, so the links can read its value off the `consts` row a referenced project wrote.
    /// `Format` marks a parameter that is the FORMAT of a `string.Format` - its `{n}` are holes only the caller's text shows.</summary>
    public sealed record Piece(string Text, bool Hole, string Symbol = "", bool Parameter = false, bool Format = false, IParameterSymbol? Of = null);

    /// <summary>The template of an argument whose value did not fold, as a STRUCTURE the store keeps as node rows
    /// (`arguments__template`): a list of the strings the argument can be, each a list of pieces - a string, or a
    /// hole `{hole, symbol}`. "" when there is nothing to say: a value that folded, no model, or no literal text
    /// around the holes.</summary>
    public static object Of(SemanticModel? model, ExpressionSyntax expression, CsFolding.Value value)
    {
        if (model is null || value.Kind is "const" or "folded") return "";
        List<List<Piece>>? strings;
        try { strings = Strings(model, expression, new HashSet<ISymbol>(SymbolEqualityComparer.Default)); }
        catch (ArgumentException) { return ""; }
        // HOLES ALONE SAY NOTHING - except where each string ends in the method's own PARAMETER (`_cfg.Url + url`): a base the run time
        // knows and the url the caller hands in, which is what makes the method a wrapper of the call (`rows/http/cscalls.rs`).
        var handed = strings is not null && strings.All(s => s.Count > 0 && s.All(p => p.Hole) && s[^1].Parameter);
        if (strings is null || (!handed && !strings.Any(s => s.Any(p => !p.Hole && p.Text.Trim().Length > 0)))) return "";
        return strings.Select(pieces => (object)pieces.Select(piece => piece.Hole ? (object)HoleNode(piece) : piece.Text).ToList()).ToList();
    }

    /// <summary>A hole as its node: its source, the symbol it reads, and `format` where it is a `string.Format`'s format.</summary>
    private static (string Key, object? Value)[] HoleNode(Piece piece)
    {
        var node = new List<(string Key, object? Value)> { ("hole", piece.Text) };
        if (piece.Symbol.Length > 0) node.Add(("symbol", piece.Symbol));
        if (piece.Format) node.Add(("format", "string.Format"));
        return [.. node];
    }

    /// <summary>Every string an expression can be, or null when it is one value only the run time knows.</summary>
    private static List<List<Piece>>? Strings(SemanticModel model, ExpressionSyntax expression, HashSet<ISymbol> path)
    {
        var folded = CsFolding.Of(model, expression);
        if (folded.Kind is "const" or "folded") return [[new Piece(folded.Text, false)]];
        switch (expression)
        {
            case ParenthesizedExpressionSyntax inner:
                return Strings(model, inner.Expression, path);
            case InterpolatedStringExpressionSyntax interpolated:
                return Interpolated(model, interpolated, path);
            case BinaryExpressionSyntax binary when CsFolding.IsConcatenation(model, binary):
                return Chain(model, binary, path);
            case InvocationExpressionSyntax invocation when Concatenated(model, invocation) is { } parts:
                return parts.Aggregate(new List<List<Piece>> { new() }, (strings, part) => Product(strings, Strings(model, part, path) ?? [[Hole(part, model)]]));
            case InvocationExpressionSyntax invocation when Format(model, invocation, e => CsFolding.Of(model, e)) is { } pieces:
                return [pieces];
            // A FORMAT THE CALLER HANDS IN (`string.Format(endpoint, args)`): one hole, the parameter, marked - the http pass reads
            // the caller's text as the url, each `{n}` in it a hole (`rows/http/cscalls.rs`).
            case InvocationExpressionSyntax invocation when HandedFormat(model, invocation) is { } format:
                return [[Hole(format, model) with { Format = true }]];
            // `QueryHelpers.AddQueryString(url, ..)`: the url, then a query.
            case InvocationExpressionSyntax invocation when QueryAdded(model, invocation) is { } url:
                return [.. (Strings(model, url, path) ?? [[Hole(url, model)]]).Select(s => Joined([.. s, new Piece("?", false), Hole(invocation, model)]))];
            case InvocationExpressionSyntax invocation:
                return Returned(model, invocation, path);
            // A TERNARY IS EITHER ARM: `c ? "/api/files" : $"/api/files?basePath={p}"` is the strings of both.
            case ConditionalExpressionSyntax conditional:
                return Strings(model, conditional.WhenTrue, path) is { } whenTrue && Strings(model, conditional.WhenFalse, path) is { } whenFalse
                    ? [.. whenTrue, .. whenFalse] : null;
            case IdentifierNameSyntax or MemberAccessExpressionSyntax:
                return Named(model, expression, path);
            default:
                return null;
        }
    }

    private static List<List<Piece>>? Interpolated(SemanticModel model, InterpolatedStringExpressionSyntax node, HashSet<ISymbol> path)
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
                        ? Strings(model, hole.Expression, path) : null;
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
    private static List<List<Piece>>? Named(SemanticModel model, ExpressionSyntax expression, HashSet<ISymbol> path)
    {
        var symbol = CsSemantics.Bound(model, expression);
        if (symbol is ILocalSymbol { IsForEach: true })
        {
            foreach (var reference in symbol.DeclaringSyntaxReferences)
            {
                if (reference.GetSyntax() is ForEachStatementSyntax loop && Owner(model, loop) is { } loopModel)
                    return Following(path, symbol, () => Elements(loopModel, loop.Expression, path));
            }
            return null;
        }
        if (symbol is null || CsFolding.Initializer(symbol) is not { } initializer) return null;
        if (Owner(model, initializer) is not { } owner) return null;
        if (!CsFolding.Written(symbol, model.Compilation)) return Following(path, symbol, () => Strings(owner, initializer, path));
        // A LOCAL ONLY EVER APPENDED TO (`url += $"&page={p}"`) is its initializer, or its initializer and something after it: what
        // the appends add is a hole, each string it can start with is kept whole.
        if (symbol is not ILocalSymbol || Appends(symbol, model) is not { } append) return null;
        return Following(path, symbol, () => Strings(owner, initializer, path) is { } start
            ? [.. start, .. start.Select(s => Joined([.. s, Hole(append, model)]))] : null);
    }

    /// <summary>The value of the first `+=` to a local, when every write to it after its declaration is a `+=` - no `=`, no
    /// `++`, no `ref`/`out` - or null.</summary>
    private static ExpressionSyntax? Appends(ISymbol symbol, SemanticModel model)
    {
        ExpressionSyntax? first = null;
        foreach (var reference in symbol.DeclaringSyntaxReferences)
        {
            var scope = reference.GetSyntax().FirstAncestorOrSelf<MemberDeclarationSyntax>();
            if (scope is null || Owner(model, scope) is not { } owner) return null;
            foreach (var node in scope.DescendantNodes())
            {
                ExpressionSyntax? target = node switch
                {
                    AssignmentExpressionSyntax assignment => assignment.Left,
                    PrefixUnaryExpressionSyntax or PostfixUnaryExpressionSyntax => null,
                    ArgumentSyntax { RefKindKeyword.RawKind: not 0 } argument => argument.Expression,
                    _ => null,
                };
                if (node is PrefixUnaryExpressionSyntax prefix && Binds(owner, prefix.Operand, symbol)) return null;
                if (node is PostfixUnaryExpressionSyntax postfix && Binds(owner, postfix.Operand, symbol)) return null;
                if (target is null || !Binds(owner, target, symbol)) continue;
                if (node is not AssignmentExpressionSyntax add || !add.IsKind(SyntaxKind.AddAssignmentExpression)) return null;
                first ??= add.Right;
            }
        }
        return first;
    }

    private static bool Binds(SemanticModel model, ExpressionSyntax expression, ISymbol symbol) =>
        expression is IdentifierNameSyntax name && name.Identifier.Text == symbol.Name
        && SymbolEqualityComparer.Default.Equals(CsSemantics.Bound(model, expression), symbol);

    /// <summary>The strings of a collection written out in the code - an array, a collection expression -
    /// directly or through a name that is never written again.</summary>
    private static List<List<Piece>>? Elements(SemanticModel model, ExpressionSyntax expression, HashSet<ISymbol> path)
    {
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
            return Owner(model, initializer) is { } owner ? Following(path, symbol, () => Elements(owner, initializer, path)) : null;
        }
        var all = new List<List<Piece>>();
        foreach (var item in items)
        {
            all.AddRange(Strings(model, item, path) ?? [[Hole(item, model)]]);
        }
        return all.Count > 0 ? all : null;
    }

    /// <summary>
    /// A walk into a symbol's declaration, unless that symbol is already being walked: `A = B + x` and
    /// `B = A + y` is a CYCLE, and the name that closes it is a value only the run time knows (null), however
    /// long a chain without one is followed.
    /// </summary>
    private static List<List<Piece>>? Following(HashSet<ISymbol> path, ISymbol symbol, Func<List<List<Piece>>?> walk)
    {
        if (!path.Add(symbol)) return null;
        try { return walk(); }
        finally { path.Remove(symbol); }
    }

    /// <summary>
    /// A string `+` chain as every string it can be. WALKED DOWN ITS LEFT SPINE IN A LOOP: `a + b + ... + z`
    /// is a left-leaning tree as deep as it is long. EVERY string it can be is kept: `a + b` of 8 and 8 is 64
    /// strings, never a hole standing in for a product that grew.
    /// </summary>
    private static List<List<Piece>>? Chain(SemanticModel model, BinaryExpressionSyntax node, HashSet<ISymbol> path)
    {
        var spine = new List<BinaryExpressionSyntax>();
        ExpressionSyntax head = node;
        while (head is BinaryExpressionSyntax link && CsFolding.IsConcatenation(model, link))
        {
            spine.Add(link);
            head = link.Left;
        }
        var strings = Strings(model, head, path) ?? [[Hole(head, model)]];
        for (var i = spine.Count - 1; i >= 0; i--)
        {
            var right = spine[i].Right;
            strings = Product(strings, Strings(model, right, path) ?? [[Hole(right, model)]]);
        }
        return strings;
    }

    /// <summary>`string.Concat(a, b, ..)` over strings, as its arguments in order - as a `+` chain of them is (`Chain`); null for any other
    /// call, or one handed an array or a named argument.</summary>
    private static List<ExpressionSyntax>? Concatenated(SemanticModel model, InvocationExpressionSyntax invocation) =>
        model.GetSymbolInfo(invocation).Symbol is IMethodSymbol { Name: "Concat", ContainingType.SpecialType: SpecialType.System_String } method
        && method.Parameters.All(p => p.Type.SpecialType == SpecialType.System_String)
        && invocation.ArgumentList.Arguments.All(a => a.NameColon is null)
            ? [.. invocation.ArgumentList.Arguments.Select(a => a.Expression)] : null;

    /// <summary>The format of a `string.Format` when it is a parameter of the method it is written in, or null.</summary>
    private static ExpressionSyntax? HandedFormat(SemanticModel model, InvocationExpressionSyntax invocation) =>
        model.GetSymbolInfo(invocation).Symbol is IMethodSymbol { Name: "Format", ContainingType.SpecialType: SpecialType.System_String } method
        && method.Parameters.Length > 0 && method.Parameters[0].Type.SpecialType == SpecialType.System_String
        && invocation.ArgumentList.Arguments is [{ NameColon: null } first, ..]
        && CsSemantics.Bound(model, first.Expression) is IParameterSymbol
            ? first.Expression : null;

    /// <summary>`Microsoft.AspNetCore.WebUtilities.QueryHelpers.AddQueryString`'s url, or null for any other call.</summary>
    private static ExpressionSyntax? QueryAdded(SemanticModel model, InvocationExpressionSyntax invocation) =>
        model.GetSymbolInfo(invocation).Symbol is IMethodSymbol { Name: "AddQueryString" } method
        && CsSemantics.Name(method.ContainingType) == "Microsoft.AspNetCore.WebUtilities.QueryHelpers"
        && invocation.ArgumentList.Arguments is [{ NameColon: null } first, ..]
            ? first.Expression : null;

    /// <summary>
    /// THE STRINGS A METHOD OF THE TREE RETURNS FOR ONE CALL: each `return` of its body (or its `=>`), folded where it is written, with
    /// each of its parameters there the strings this call hands it - `BuildUri("/orders")` over `Prefix + uri` is `/api/orders`, and a
    /// caller's own parameter handed on stays a parameter. Null for a call that may run another body (virtual, abstract, an interface's,
    /// an override a subclass may override again), a method with no body in the tree, one that writes a parameter or takes one by `ref`.
    /// </summary>
    private static List<List<Piece>>? Returned(SemanticModel model, InvocationExpressionSyntax invocation, HashSet<ISymbol> path)
    {
        if (model.GetSymbolInfo(invocation).Symbol is not IMethodSymbol { ReturnType.SpecialType: SpecialType.System_String } called) return null;
        var method = (called.ReducedFrom ?? called).OriginalDefinition;
        if (method.IsVirtual || method.IsAbstract || method.IsExtern || method.ContainingType.TypeKind == TypeKind.Interface
            || (method.IsOverride && !method.IsSealed && !method.ContainingType.IsSealed)) return null;
        if (method.DeclaringSyntaxReferences is not [var reference] || reference.GetSyntax() is not MethodDeclarationSyntax declaration) return null;
        if (Owner(model, declaration) is not { } owner) return null;
        if (method.Parameters.Any(p => p.RefKind != RefKind.None || p.IsParams || CsFolding.Written(p, model.Compilation))) return null;
        List<ExpressionSyntax?> returns = declaration.ExpressionBody is { } arrow ? [arrow.Expression]
            : [.. declaration.Body?.DescendantNodes(n => n is not (LocalFunctionStatementSyntax or AnonymousFunctionExpressionSyntax))
                .OfType<ReturnStatementSyntax>().Select(r => r.Expression) ?? []];
        if (returns.Count == 0 || returns.Any(r => r is null)) return null;
        // WHAT THE CALL HANDS EACH PARAMETER: an extension's receiver, then each argument by its place or its name; one left out, its default.
        var handed = new ExpressionSyntax?[method.Parameters.Length];
        var place = 0;
        if (called.ReducedFrom is not null)
        {
            if (invocation.Expression is not MemberAccessExpressionSyntax access) return null;
            handed[place++] = access.Expression;
        }
        foreach (var argument in invocation.ArgumentList.Arguments)
        {
            var at = argument.NameColon is { } named ? method.Parameters.IndexOf(method.Parameters.FirstOrDefault(p => p.Name == named.Name.Identifier.Text)!) : place++;
            if (at < 0 || at >= handed.Length) return null;
            handed[at] = argument.Expression;
        }
        List<List<Piece>> Given(IParameterSymbol parameter) => handed[parameter.Ordinal] is { } given
            ? Strings(model, given, path) ?? [[Hole(given, model)]]
            : parameter.HasExplicitDefaultValue && parameter.ExplicitDefaultValue is string text ? [[new Piece(text, false)]] : [[new Piece(parameter.Name, true)]];
        return Following(path, method, () =>
        {
            var all = new List<List<Piece>>();
            foreach (var expression in returns)
            {
                var strings = Strings(owner, expression!, path) ?? [[Hole(expression!, owner)]];
                foreach (var pieces in strings)
                    all.AddRange(pieces.Aggregate(new List<List<Piece>> { new() }, (done, piece) => Product(done,
                        // A FORMAT handed in stays a hole: what the caller hands is not yet the string the method returns.
                        piece is { Of: { } own, Format: false } && SymbolEqualityComparer.Default.Equals(own.ContainingSymbol.OriginalDefinition, method)
                            ? Given(method.Parameters[own.Ordinal])
                            : piece.Of is not null && piece.Format ? [[new Piece(piece.Text, true)]] : [[piece]])));
            }
            return all;
        });
    }

    /// <summary>The model for the tree a node lives in - a model answers about its own tree only.</summary>
    private static SemanticModel? Owner(SemanticModel model, SyntaxNode node) =>
        node.SyntaxTree == model.SyntaxTree ? model
            : model.Compilation.ContainsSyntaxTree(node.SyntaxTree) ? model.Compilation.GetSemanticModel(node.SyntaxTree) : null;

    private static List<List<Piece>> Product(List<List<Piece>> left, List<List<Piece>> right) =>
        [.. left.SelectMany(l => right.Select(r => Joined([.. l, .. r])))];

    private static Piece Hole(ExpressionSyntax expression, SemanticModel model) => CsSemantics.Bound(model, expression) switch
    {
        IFieldSymbol field => new(CsFacts.Text(expression), true, CsSemantics.Name(field)),
        // A PARAMETER by its name, as `parameters.name` spells it - one the method writes is no longer what its caller handed it.
        IParameterSymbol parameter when !CsFolding.Written(parameter, model.Compilation) => new(CsFacts.Text(expression), true, parameter.Name, true, Of: parameter),
        _ => new(CsFacts.Text(expression), true),
    };

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
