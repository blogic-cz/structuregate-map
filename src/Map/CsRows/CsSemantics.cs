using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using Microsoft.CodeAnalysis.CSharp.Syntax;

namespace StructureGate;

/// <summary>
/// WHAT THE COMPILER KNOWS THAT THE TEXT DOES NOT. Every column this file fills is one the syntax pass has
/// to guess at or leave out: which method `Resolve` is, what type an expression has, what a `const` actually
/// holds, and which parameter an argument is being passed to.
///
/// THE DIFFERENCE IS NOT COSMETIC. `Save(true, false)` says nothing; bound, the same call says
/// `overwrite: true, notify: false` — read off the declaration the callee writes, never off position or
/// naming convention. Two `Load` methods in two namespaces are one string to a grep and two rows here.
///
/// A MODEL IS OPTIONAL, ALWAYS. A project that was never restored, a file outside every `.csproj`, a
/// machine with no framework pack: each leaves `model` null, every method here answers "" and the row keeps
/// exactly what the syntax pass gave it. That is why `files.semantic` exists — a reader has to be able to
/// tell a name that resolved to nothing from a file that was never bound.
/// </summary>
internal static class CsSemantics
{
    /// <summary>`Namespace.Type.Member`, with no `global::` on the front. The name a person would write to
    /// name the thing from anywhere, so two members that share a short name are two rows.</summary>
    private static readonly SymbolDisplayFormat Named = new(
        SymbolDisplayGlobalNamespaceStyle.Omitted,
        SymbolDisplayTypeQualificationStyle.NameAndContainingTypesAndNamespaces,
        SymbolDisplayGenericsOptions.IncludeTypeParameters,
        memberOptions: SymbolDisplayMemberOptions.IncludeContainingType);

    /// <summary>The same, plus the parameter types — which is what tells one OVERLOAD from another, and the
    /// question `symbol` alone cannot answer.</summary>
    private static readonly SymbolDisplayFormat Signed = new(
        SymbolDisplayGlobalNamespaceStyle.Omitted,
        SymbolDisplayTypeQualificationStyle.NameAndContainingTypesAndNamespaces,
        SymbolDisplayGenericsOptions.IncludeTypeParameters,
        memberOptions: SymbolDisplayMemberOptions.IncludeContainingType
            | SymbolDisplayMemberOptions.IncludeParameters,
        parameterOptions: SymbolDisplayParameterOptions.IncludeType,
        miscellaneousOptions: SymbolDisplayMiscellaneousOptions.UseSpecialTypes);

    internal static readonly SymbolDisplayFormat Typed = new(
        SymbolDisplayGlobalNamespaceStyle.Omitted,
        SymbolDisplayTypeQualificationStyle.NameAndContainingTypesAndNamespaces,
        SymbolDisplayGenericsOptions.IncludeTypeParameters,
        miscellaneousOptions: SymbolDisplayMiscellaneousOptions.UseSpecialTypes);

    /// <summary>
    /// The symbol a node binds to. A CANDIDATE COUNTS when there is exactly one: an argument that does not
    /// convert leaves the call "wrong" rather than unknown, and the reader still wants to know which method
    /// was meant. Several candidates is an ambiguity, and a guessed one of them would be worse than none.
    /// </summary>
    /// <summary>
    /// What a `nameof` operand names. `nameof(Load)` over an OVERLOADED method binds to no single symbol - the
    /// compiler answers with the member group - and some of a large consumer's unbound calls were that. Every candidate
    /// shares the name and the containing type, which is all the `symbol` column spells, so any one says it.
    /// </summary>
    public static ISymbol? NameOf(SemanticModel? model, SyntaxNode node)
    {
        if (Bound(model, node) is { } bound) return bound;
        if (model is null) return null;
        try
        {
            var candidates = model.GetSymbolInfo(node).CandidateSymbols;
            if (candidates.Length == 0) candidates = model.GetMemberGroup(node);
            if (candidates.Length == 0) return null;
            var first = candidates[0];
            return candidates.All(c => c.Name == first.Name
                && SymbolEqualityComparer.Default.Equals(c.ContainingSymbol, first.ContainingSymbol)) ? first : null;
        }
        catch (ArgumentException) { return null; }
    }

    public static ISymbol? Bound(SemanticModel? model, SyntaxNode node)
    {
        if (model is null) return null;
        try
        {
            var info = model.GetSymbolInfo(node);
            if (info.Symbol is not null) return info.Symbol;
            return info.CandidateSymbols.Length == 1 ? info.CandidateSymbols[0] : null;
        }
        catch (ArgumentException)
        {
            // The node belongs to another tree than this model's. Nothing to say about it.
            return null;
        }
    }

    /// <summary>
    /// The name of what a symbol IS, as a person would write it to reach the declaration.
    ///
    /// AN EXTENSION METHOD IS NAMED WHERE IT IS DECLARED. Roslyn hands back the REDUCED form of
    /// `21.Twice()` — `System.Int32.Twice`, the method as the call site sees it — and a map built on that
    /// answers "who calls `Probe.Ext.Twice`" with nothing, for a method this codebase declares and calls.
    /// </summary>
    public static string Name(ISymbol? symbol) =>
        Declaring(symbol) is { } named ? named.ToDisplayString(Named) : "";

    public static string Signature(ISymbol? symbol) =>
        Declaring(symbol) is IMethodSymbol method ? method.ToDisplayString(Signed) : "";

    private static ISymbol? Declaring(ISymbol? symbol) =>
        symbol is IMethodSymbol { ReducedFrom: not null } reduced ? reduced.ReducedFrom : symbol;

    /// <summary>The declared type of an expression — `var` resolved, an inferred lambda resolved, an
    /// interface named where the text says `this`.</summary>
    public static string Type(SemanticModel? model, SyntaxNode? node)
    {
        if (model is null || node is not ExpressionSyntax expression) return "";
        try
        {
            var info = model.GetTypeInfo(expression);
            var type = info.Type ?? info.ConvertedType;
            return type is null or IErrorTypeSymbol ? "" : type.ToDisplayString(Typed);
        }
        catch (ArgumentException) { return ""; }
    }

    /// <summary>The type a declaration DECLARES, for a row that is about the declaration rather than about
    /// an expression: a method's return type, a field's type, a class itself.</summary>
    public static string Declared(SemanticModel? model, SyntaxNode declaration)
    {
        if (model is null) return "";
        try
        {
            var symbol = model.GetDeclaredSymbol(declaration);
            return symbol switch
            {
                IMethodSymbol method => method.ReturnType.ToDisplayString(Typed),
                IPropertySymbol property => property.Type.ToDisplayString(Typed),
                INamedTypeSymbol type => type.ToDisplayString(Named),
                _ => "",
            };
        }
        catch (ArgumentException) { return ""; }
    }

    /// <summary>The fully qualified name of what a declaration declares — `Demo.Core.Reader.Load`, as the
    /// compiler resolves it rather than as the file happens to be nested.</summary>
    public static string DeclaredName(SemanticModel? model, SyntaxNode declaration)
    {
        if (model is null) return "";
        try { return Name(model.GetDeclaredSymbol(declaration)); }
        catch (ArgumentException) { return ""; }
    }

    /// <summary>
    /// THE PARAMETER AN ARGUMENT IS BEING PASSED TO, named by the CALLEE's declaration.
    ///
    /// This is the one fact no parse tree carries and the reason the semantic pass is worth its cost:
    /// `Send(id, true, false)` is three values with no meaning, and the same call bound says which of them
    /// is `retry` and which is `silent`. A named argument says so itself; everything else is position
    /// against the resolved signature, and `params` swallows every argument past its own.
    /// </summary>
    public static string Parameter(ISymbol? callee, ArgumentSyntax argument, int position) =>
        Parameter(callee, argument.NameColon, position);

    /// <summary>The same, for an argument of any shape - an attribute's argument is no `ArgumentSyntax`.</summary>
    public static string Parameter(ISymbol? callee, NameColonSyntax? named, int position)
    {
        if (named is not null) return named.Name.Identifier.ValueText;
        var parameters = callee switch
        {
            IMethodSymbol method => method.Parameters,
            IPropertySymbol indexer => indexer.Parameters,
            _ => default,
        };
        if (parameters.IsDefaultOrEmpty) return "";
        if (position < parameters.Length) return parameters[position].Name;
        var last = parameters[^1];
        return last.IsParams ? last.Name : "";
    }
}
