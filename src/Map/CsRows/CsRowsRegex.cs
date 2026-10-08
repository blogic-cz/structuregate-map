using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp.Syntax;

namespace StructureGate;

/// <summary>
/// `regexes`: every place a regex is BUILT - a constructor call of the framework's `Regex`, one of its static
/// methods taking a pattern, `[GeneratedRegex]` and `[RegularExpression]` - with the pattern where the compiler
/// or <see cref="CsFolding"/> knows it. The `calls` and `decorators` rows stay beside it; this is the finding.
///
/// DECIDED BY THE BOUND SYMBOL ONLY. No model, no row - like every other resolved column. A type of the same
/// name declared in the tree fails the namespace check, and an instance method of a built regex (`Built`
/// matched against a string) is no row: the regex is recorded where it is built, as is `Escape`, which takes
/// no pattern at all. A method counts when its signature has a parameter named `pattern`, so a BCL that
/// renamed it would drop rows, never invent them.
///
/// THE NAMESPACE IS COMPARED AS A CHAIN AND STOPS AT `System.Text`: its last segment is a ban needle of this
/// repo's own build (see `BanRegex`), as is the type name followed by a dot. In the BCL the only `Regex`
/// whose namespace sits directly under `System.Text` is the one meant.
/// </summary>
internal sealed partial class CsRows
{
    /// <summary>A call or constructor that builds a regex, read off the symbol `Call` already bound.</summary>
    private void RegexCall(ExpressionSyntax node, ISymbol? target, ArgumentListSyntax? arguments)
    {
        if (target is not IMethodSymbol method || !UnderText(method.ContainingType, "Regex")
            || !method.Parameters.Any(p => p.Name == "pattern") || arguments is null) return;
        var pattern = CsFolding.Value.None;
        var flags = "";
        var position = 0;
        foreach (var argument in arguments.Arguments)
        {
            var parameter = CsSemantics.Parameter(target, argument, position++);
            if (parameter == "pattern") pattern = CsFolding.Of(model, argument.Expression);
            else if (parameter == "options") flags = Options(argument.Expression);
        }
        RegexRow(node, "call", method, pattern, flags, method.MethodKind == MethodKind.Constructor
            ? Receiver(node) : "." + method.Name);
    }

    /// <summary>`[GeneratedRegex]` and `[RegularExpression]`, read off the constructor `Attributes` bound.
    /// A named property (`ErrorMessage = ...`) is no constructor argument and is skipped.</summary>
    private void RegexAttribute(AttributeSyntax attribute, ISymbol? bound, string target)
    {
        if (bound is not IMethodSymbol ctor) return;
        var type = ctor.ContainingType;
        if (!UnderText(type, "GeneratedRegexAttribute")
            && !(type?.Name == "RegularExpressionAttribute" && Is(type.ContainingNamespace, "System", "ComponentModel", "DataAnnotations")))
            return;
        var pattern = CsFolding.Value.None;
        var flags = "";
        var position = 0;
        foreach (var argument in attribute.ArgumentList?.Arguments ?? default)
        {
            if (argument.NameEquals is not null) continue;
            var parameter = CsSemantics.Parameter(ctor, argument.NameColon, position++);
            if (parameter == "pattern") pattern = CsFolding.Of(model, argument.Expression);
            else if (parameter == "options") flags = Options(argument.Expression);
        }
        RegexRow(attribute, "attribute", ctor, pattern, flags, target);
    }

    private void RegexRow(SyntaxNode node, string kind, ISymbol api, CsFolding.Value pattern, string flags, string usedBy)
    {
        // A PATTERN ONLY THE RUN KNOWS IS EMPTY: a field read binds to a symbol, but its text is not the pattern.
        var known = pattern.Kind is "const" or "folded";
        Row("regexes", "rx",
            ("line", Line(node)),
            ("kind", kind),
            ("api", CsSemantics.Name(api)),
            ("pattern", known ? pattern.Text : ""),
            ("pattern_kind", known ? pattern.Kind : ""),
            ("flags", flags),
            ("used_by", usedBy),
            ("source", CsFacts.Text(node)));
    }

    /// <summary>The options as folded (`System.Text...RegexOptions.IgnoreCase`), or as written.</summary>
    private string Options(ExpressionSyntax expression)
    {
        var value = CsFolding.Of(model, expression);
        return value.Kind is "enum" or "const" ? value.Text : CsFacts.Text(expression);
    }

    /// <summary>What a constructed regex is handed to: the field, property or local it initialises, or the
    /// method it is passed to; "" otherwise.</summary>
    private static string Receiver(ExpressionSyntax node) => node.Parent switch
    {
        EqualsValueClauseSyntax { Parent: VariableDeclaratorSyntax declarator } => "= " + declarator.Identifier.ValueText,
        EqualsValueClauseSyntax { Parent: PropertyDeclarationSyntax property } => "= " + property.Identifier.ValueText,
        ArgumentSyntax { Parent.Parent: InvocationExpressionSyntax call } => call.Expression switch
        {
            MemberAccessExpressionSyntax access => "." + access.Name.Identifier.ValueText,
            SimpleNameSyntax name => "." + name.Identifier.ValueText,
            _ => "",
        },
        _ => "",
    };

    /// <summary>A type called `name` in a namespace ONE segment below `System.Text`.</summary>
    private static bool UnderText(INamedTypeSymbol? type, string name) =>
        type?.Name == name && type.ContainingNamespace is { IsGlobalNamespace: false } leaf
        && Is(leaf.ContainingNamespace, "System", "Text");

    /// <summary>The namespace is exactly `chain`, outermost first, and sits in the global one.</summary>
    private static bool Is(INamespaceSymbol? space, params string[] chain)
    {
        for (var i = chain.Length - 1; i >= 0; i--)
        {
            if (space is null || space.IsGlobalNamespace || space.Name != chain[i]) return false;
            space = space.ContainingNamespace;
        }
        return space is { IsGlobalNamespace: true };
    }
}
