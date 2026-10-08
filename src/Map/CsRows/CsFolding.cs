using System.Globalization;
using System.Runtime.CompilerServices;
using System.Text;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using Microsoft.CodeAnalysis.CSharp.Syntax;

namespace StructureGate;

/// <summary>
/// WHAT AN EXPRESSION ACTUALLY IS, one step past what the compiler will fold for you.
///
/// `GetConstantValue` answers only for compile-time constants, and the values people search a codebase for
/// mostly are not: a path is built by interpolation out of `static readonly` fields, and a `static readonly`
/// is by definition not `const`. So `$"{BaseUrl}/{Folder}/readme.pdf"` came back empty — the one row
/// somebody looking for that path would have found.
///
/// THIS FOLLOWS THE OPERANDS. Each piece of an interpolation or a concatenation is resolved to its
/// declaration and folded in turn, across files and across projects in the same compilation, until the whole
/// string is known or one piece is not. WHO DID THE FOLDING IS RECORDED: `const` means the compiler folded
/// it and `folded` means this pass did, because the second is an inference about what the code will produce
/// and a reader is entitled to know which one they are looking at.
///
/// AN ENUM IS A NAME, NOT A NUMBER. `Region.US` folded to `2` is a row nobody can read back; it is recorded
/// as `Region.US`, or as `Region=6` when the number names no single member (a flags combination, a cast) —
/// the two forms differ on sight, so a number can never be mistaken for a member name.
///
/// AND WHAT IS NOT CONSTANT AT ALL still says what it BINDS to. A parameter, a field read, a property: the
/// value is unknown, the symbol is not, and "which call passes this field" is a question about the symbol.
/// </summary>
internal static class CsFolding
{
    /// <summary>
    /// What one expression is: the value where it can be known, WHO worked it out, and the symbol it binds
    /// to when it binds to one. The kinds are `const`, `folded`, `enum`, `null`, `default`, `typeof`,
    /// `symbol`, and "" for an expression that is none of those.
    /// </summary>
    public sealed record Value(string Text, string Kind, string Symbol)
    {
        public static readonly Value None = new("", "", "");
    }

    /// <summary>How far an operand chain is followed. A constant built out of constants is a few links deep;
    /// a limit is what stops a cycle in a tree that does not compile from being an infinite walk.</summary>
    private const int MaxDepth = 6;

    public static Value Of(SemanticModel? model, SyntaxNode? node)
    {
        if (model is null || node is not ExpressionSyntax expression) return Value.None;
        try { return Fold(model, expression, 0); }
        catch (ArgumentException) { return Value.None; }
    }

    private static Value Fold(SemanticModel model, ExpressionSyntax expression, int depth)
    {
        if (depth > MaxDepth) return Value.None;

        var constant = model.GetConstantValue(expression);
        if (constant.HasValue)
        {
            var type = model.GetTypeInfo(expression).Type ?? model.GetTypeInfo(expression).ConvertedType;
            return Literal(constant.Value, type);
        }

        switch (expression)
        {
            case ParenthesizedExpressionSyntax inner:
                return Fold(model, inner.Expression, depth);
            case TypeOfExpressionSyntax typed:
                return new Value(CsSemantics.Type(model, typed.Type) is { Length: > 0 } name
                    ? name : typed.Type.ToString(), "typeof", "");
            case DefaultExpressionSyntax:
                return new Value("", "default", "");
            case InterpolatedStringExpressionSyntax interpolated:
                return Text(model, interpolated, depth);
            case BinaryExpressionSyntax binary when binary.IsKind(SyntaxKind.AddExpression):
                return Joined(model, binary, depth);
            case IdentifierNameSyntax or MemberAccessExpressionSyntax:
                return Named(model, expression, depth);
            // `string.Format("DELETE FROM T")` - a format whose every `{n}` folds is the string it builds.
            case InvocationExpressionSyntax invocation:
                return CsTemplate.Format(model, invocation, e => Fold(model, e, depth + 1)) is { } pieces && !pieces.Any(p => p.Hole)
                    ? new Value(string.Concat(pieces.Select(p => p.Text)), "folded", "") : Value.None;
            default:
                return Value.None;
        }
    }

    /// <summary>A compiler constant, in the spelling of the language it came from.</summary>
    private static Value Literal(object? value, ITypeSymbol? type)
    {
        if (value is null) return new Value("", "null", "");
        if (type is { TypeKind: TypeKind.Enum }) return new Value(Member(type, value), "enum", Name(type));
        return value switch
        {
            string text => new Value(text, "const", ""),
            bool yes => new Value(yes ? "true" : "false", "const", ""),
            _ => new Value(Convert.ToString(value, CultureInfo.InvariantCulture) ?? "",
                "const", ""),
        };
    }

    /// <summary>
    /// The enum member a number names — `Region.US` — or `Region=6` when it names none. The two forms are
    /// distinguishable on sight, which is the whole point: a flags combination and a cast produce numbers
    /// that are not members, and a row that printed one as a member name would be a lie a query acts on.
    /// </summary>
    private static string Member(ITypeSymbol type, object value)
    {
        var wanted = Number(value);
        if (wanted is not null)
        {
            foreach (var field in type.GetMembers().OfType<IFieldSymbol>())
            {
                if (field is { HasConstantValue: true } && Number(field.ConstantValue) == wanted)
                    return $"{Name(type)}.{field.Name}";
            }
        }
        return $"{Name(type)}={Convert.ToString(value, CultureInfo.InvariantCulture)}";
    }

    private static long? Number(object? value)
    {
        try { return value is null ? null : Convert.ToInt64(value, CultureInfo.InvariantCulture); }
        catch (Exception e) when (e is OverflowException or InvalidCastException or FormatException)
        {
            return null;
        }
    }

    private static string Name(ITypeSymbol type) => CsSemantics.Name(type);

    /// <summary>
    /// An interpolated string, folded piece by piece. EVERY piece has to be known: a string that is half
    /// resolved is not the string the code produces, and half a path is worse than none because it looks
    /// like a path.
    /// </summary>
    private static Value Text(SemanticModel model, InterpolatedStringExpressionSyntax node, int depth)
    {
        var built = new StringBuilder();
        foreach (var content in node.Contents)
        {
            switch (content)
            {
                case InterpolatedStringTextSyntax literal:
                    built.Append(literal.TextToken.ValueText);
                    break;
                case InterpolationSyntax hole:
                    // A hole with an alignment or a format (`{x,-8:N2}`) is formatting this pass does not
                    // reproduce, and a value that ignored it would not be the string that is produced.
                    if (hole.AlignmentClause is not null || hole.FormatClause is not null) return Value.None;
                    var piece = Fold(model, hole.Expression, depth + 1);
                    if (piece.Kind is not ("const" or "folded" or "enum")) return Value.None;
                    built.Append(piece.Text);
                    break;
                default:
                    return Value.None;
            }
        }
        return new Value(built.ToString(), "folded", "");
    }

    /// <summary>
    /// `a + b` as the string it builds. ONLY A STRING `+` IS A CONCATENATION: `i + 1` over a loop variable
    /// was recorded as "01" and `head + 18` as "-118". A numeric sum the compiler could not fold is not known
    /// here either, so it stays unknown rather than being added by hand.
    /// </summary>
    private static Value Joined(SemanticModel model, BinaryExpressionSyntax node, int depth)
    {
        if (model.GetTypeInfo(node).Type?.SpecialType != SpecialType.System_String) return Value.None;
        var left = Fold(model, node.Left, depth + 1);
        var right = Fold(model, node.Right, depth + 1);
        if (left.Kind is not ("const" or "folded" or "enum")) return Value.None;
        if (right.Kind is not ("const" or "folded" or "enum")) return Value.None;
        return new Value(left.Text + right.Text, "folded", "");
    }

    /// <summary>
    /// A name: followed to its declaration and folded there, or reported as the symbol it binds to.
    ///
    /// FOLLOWED ACROSS FILES, because the declaration is hardly ever in the file doing the building. It is
    /// followed only into SOURCE this compilation holds — a constant from a package is already a compiler
    /// constant and was folded above, and a field of a package type has no initializer to read.
    /// </summary>
    private static Value Named(SemanticModel model, ExpressionSyntax expression, int depth)
    {
        var symbol = CsSemantics.Bound(model, expression);
        if (symbol is null) return Value.None;
        var name = CsSemantics.Name(symbol);

        var initializer = Initializer(symbol);
        if (initializer is not null && !Written(symbol, model.Compilation))
        {
            var tree = initializer.SyntaxTree;
            // The model has to be the one for the tree the initializer lives in; a model answers about its
            // own tree and throws for any other.
            var owner = tree == model.SyntaxTree ? model
                : model.Compilation.ContainsSyntaxTree(tree) ? model.Compilation.GetSemanticModel(tree) : null;
            if (owner is not null)
            {
                var folded = Fold(owner, initializer, depth + 1);
                if (folded.Kind is "const" or "folded" or "enum")
                    return folded with { Kind = folded.Kind == "enum" ? "enum" : "folded", Symbol = name };
            }
        }
        return new Value("", "symbol", name);
    }

    /// <summary>
    /// The expression a field, property or local was declared with, when it is in source AND IS STILL ITS
    /// VALUE WHERE IT IS READ.
    ///
    /// A DECLARATION IS NOT A VALUE ONCE THE NAME IS WRITTEN AGAIN. `var enabled = false;` followed by
    /// `enabled = true;` was folded to `false`, and so was every loop counter and every `ref` argument: many of
    /// the folded arguments checked on a large consumer were wrong. So a local folds only when nothing in its member writes
    /// it after the declaration, and a field or property only when it is readonly or get-only AND nothing in
    /// its type assigns it outside the initializer. What is left is a value the code cannot change.
    /// </summary>
    internal static ExpressionSyntax? Initializer(ISymbol symbol)
    {
        if (symbol is IFieldSymbol { IsReadOnly: false, IsConst: false }) return null;
        if (symbol is IPropertySymbol { SetMethod: { IsInitOnly: false } }) return null;
        if (symbol is not (IFieldSymbol or IPropertySymbol or ILocalSymbol)) return null;
        foreach (var reference in symbol.DeclaringSyntaxReferences)
        {
            switch (reference.GetSyntax())
            {
                case VariableDeclaratorSyntax { Initializer: not null } declarator:
                    return declarator.Initializer.Value;
                case PropertyDeclarationSyntax { Initializer: not null } property:
                    return property.Initializer.Value;
                case PropertyDeclarationSyntax { ExpressionBody: not null } property:
                    return property.ExpressionBody.Expression;
                case EnumMemberDeclarationSyntax { EqualsValue: not null } member:
                    return member.EqualsValue.Value;
            }
        }
        return null;
    }

    /// <summary>Whether `symbol` is written anywhere outside its own declaration - see `Initializer`.</summary>
    internal static bool Written(ISymbol symbol, Compilation compilation)
    {
        var known = WrittenCache.GetOrCreateValue(compilation);
        lock (known)
        {
            if (known.TryGetValue(symbol, out var cached)) return cached;
        }
        var written = false;
        foreach (var scope in Scopes(symbol))
        {
            if (!compilation.ContainsSyntaxTree(scope.SyntaxTree)) { written = true; break; }
            var model = compilation.GetSemanticModel(scope.SyntaxTree);
            foreach (var target in Targets(scope))
            {
                // THE NAME FIRST, THE BINDING SECOND: binding every write in a type to ask about one field is
                // the cost of a compilation, and almost none of them spell the name.
                if (Last(target) != symbol.Name) continue;
                if (SymbolEqualityComparer.Default.Equals(CsSemantics.Bound(model, target), symbol)) { written = true; break; }
            }
            if (written) break;
        }
        lock (known) known[symbol] = written;
        return written;
    }

    private static readonly ConditionalWeakTable<Compilation, Dictionary<ISymbol, bool>> WrittenCache = new();

    /// <summary>Where a write to `symbol` could be: the member holding a local, every part of the type
    /// holding a field or property.</summary>
    private static IEnumerable<SyntaxNode> Scopes(ISymbol symbol)
    {
        if (symbol is ILocalSymbol)
        {
            foreach (var reference in symbol.DeclaringSyntaxReferences)
            {
                var declaration = reference.GetSyntax();
                yield return (SyntaxNode?)declaration.FirstAncestorOrSelf<MemberDeclarationSyntax>()
                    ?? declaration.SyntaxTree.GetRoot();
            }
            yield break;
        }
        foreach (var part in symbol.ContainingType?.DeclaringSyntaxReferences ?? [])
            yield return part.GetSyntax();
    }

    /// <summary>Every expression a statement in `scope` writes to: an assignment's left side, `++`/`--`, and a
    /// `ref` or `out` argument.</summary>
    private static IEnumerable<ExpressionSyntax> Targets(SyntaxNode scope)
    {
        foreach (var node in scope.DescendantNodes())
        {
            switch (node)
            {
                case AssignmentExpressionSyntax assignment:
                    yield return assignment.Left;
                    break;
                case PrefixUnaryExpressionSyntax prefix when prefix.IsKind(SyntaxKind.PreIncrementExpression)
                        || prefix.IsKind(SyntaxKind.PreDecrementExpression):
                    yield return prefix.Operand;
                    break;
                case PostfixUnaryExpressionSyntax postfix when postfix.IsKind(SyntaxKind.PostIncrementExpression)
                        || postfix.IsKind(SyntaxKind.PostDecrementExpression):
                    yield return postfix.Operand;
                    break;
                case ArgumentSyntax { RefKindKeyword.RawKind: not 0 } argument
                        when !argument.RefKindKeyword.IsKind(SyntaxKind.InKeyword):
                    yield return argument.Expression;
                    break;
            }
        }
    }

    private static string Last(ExpressionSyntax expression) => expression switch
    {
        IdentifierNameSyntax name => name.Identifier.ValueText,
        MemberAccessExpressionSyntax access => access.Name.Identifier.ValueText,
        _ => "",
    };
}
