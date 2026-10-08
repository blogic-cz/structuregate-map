using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using Microsoft.CodeAnalysis.CSharp.Syntax;

namespace StructureGate;

/// <summary>
/// WHAT A C# TYPE HOLDS AND A C# BODY NAMES, in the tables the TypeScript half already fills: `members` (a
/// property, a field, a constant, an event, an indexer), `locals` (every variable a body declares) and `comments`.
/// "On which types is `Reference` declared" had no answer but a text search: a property was a `functions`
/// row and a field without an initializer was no row at all.
///
/// THE SHARED TABLES KEEP THEIR SHAPE, and the C# rows say whose they are by `files.lang`: the TypeScript half
/// reads back only the rows it stamped with `half`, so none of these reaches its closure. A C# member names its
/// type by `cls` and `class_symbol`, never by the TypeScript half's `class` id.
///
/// `symbol` IS THE NAME `refs.symbol` USES - every use of the member joins to its declaration on it.
/// </summary>
internal sealed partial class CsRows
{
    /// <summary>A member's row - one per declarator for a field, so `int a, b;` is two.</summary>
    private void Members(MemberDeclarationSyntax node)
    {
        switch (node)
        {
            case PropertyDeclarationSyntax property:
                Member(property, property.Identifier.ValueText, "property", property.Type, property.Modifiers,
                    Accessors(property.AccessorList, property.ExpressionBody), property.Initializer?.Value);
                break;
            case IndexerDeclarationSyntax indexer:
                Member(indexer, "this[]", "indexer", indexer.Type, indexer.Modifiers,
                    Accessors(indexer.AccessorList, indexer.ExpressionBody), null);
                break;
            case EventDeclarationSyntax e:
                Member(e, e.Identifier.ValueText, "event", e.Type, e.Modifiers, Accessors(e.AccessorList, null), null);
                break;
            case BaseFieldDeclarationSyntax field:
                var kind = field is EventFieldDeclarationSyntax ? "event"
                    : field.Modifiers.Any(m => m.IsKind(SyntaxKind.ConstKeyword)) ? "const" : "field";
                foreach (var declarator in field.Declaration.Variables)
                    Member(declarator, declarator.Identifier.ValueText, kind, field.Declaration.Type, field.Modifiers, [],
                        declarator.Initializer?.Value, field);
                break;
        }
    }

    private void Member(SyntaxNode node, string name, string kind, TypeSyntax type, SyntaxTokenList modifiers,
        List<string> accessors, ExpressionSyntax? initializer, MemberDeclarationSyntax? declaration = null)
    {
        var owner = declaration ?? (MemberDeclarationSyntax)node;
        var declared = Declared(node);
        Row("members", "m",
            ("line", Line(node)),
            ("end_line", EndLine(owner)),
            ("name", name),
            ("kind", kind),
            ("symbol", CsSemantics.Name(declared)),
            ("class_symbol", CsSemantics.Name(declared?.ContainingType)),
            // RESOLVED, and as written: `var`-free here, but an alias or a type from another project reads one way
            // per file and one way everywhere.
            ("type", TypeOf(declared)),
            ("type_text", type.ToString()),
            ("static", modifiers.Any(m => m.IsKind(SyntaxKind.StaticKeyword) || m.IsKind(SyntaxKind.ConstKeyword)) ? 1 : 0),
            ("readonly", modifiers.Any(m => m.IsKind(SyntaxKind.ReadOnlyKeyword) || m.IsKind(SyntaxKind.ConstKeyword))
                || kind is "property" or "indexer" && !accessors.Any(a => a.EndsWith("set") || a.EndsWith("init")) ? 1 : 0),
            ("visibility", Visibility(modifiers, declared)),
            ("accessors", accessors),
            ("attributes", owner.AttributeLists.SelectMany(l => l.Attributes)
                .Select(a => CsFacts.Dotted(a.Name) is { Length: > 0 } written ? written : a.Name.ToString()).ToList()),
            ("initializer", CsFacts.Text(initializer)),
            ("doc", CsFacts.Doc(owner)));
    }

    /// <summary>`get`, `private set`, `init` - as declared; an expression-bodied property is a `get`.</summary>
    private static List<string> Accessors(AccessorListSyntax? list, ArrowExpressionClauseSyntax? body)
    {
        if (list is null) return body is null ? [] : ["get"];
        return [.. list.Accessors.Select(a => string.Join(' ', a.Modifiers.Select(m => m.ValueText).Append(a.Keyword.ValueText)))];
    }

    /// <summary>The accessibility as the compiler settles it - a member with no modifier is `private` in a class and
    /// `public` in an interface - or as written when nothing bound.</summary>
    private static string Visibility(SyntaxTokenList modifiers, ISymbol? declared)
    {
        if (declared is not null)
        {
            return declared.DeclaredAccessibility switch
            {
                Accessibility.Public => "public",
                Accessibility.Private => "private",
                Accessibility.Protected => "protected",
                Accessibility.Internal => "internal",
                Accessibility.ProtectedOrInternal => "protected internal",
                Accessibility.ProtectedAndInternal => "private protected",
                _ => "",
            };
        }
        var written = modifiers.Where(m => m.IsKind(SyntaxKind.PublicKeyword) || m.IsKind(SyntaxKind.PrivateKeyword)
            || m.IsKind(SyntaxKind.ProtectedKeyword) || m.IsKind(SyntaxKind.InternalKeyword)).Select(m => m.ValueText);
        return string.Join(' ', written);
    }

    /// <summary>A local a body declares: a declared variable, `using`, `for`, `foreach`, `out var`, a pattern's
    /// variable, a deconstruction's. `declared` says which.</summary>
    private void Local(SyntaxNode node)
    {
        var (name, declared, type, value) = node switch
        {
            VariableDeclaratorSyntax { Parent: VariableDeclarationSyntax { Parent: LocalDeclarationStatementSyntax statement } declaration } v =>
                (v.Identifier.ValueText, statement.IsConst ? "const" : statement.UsingKeyword.ValueText.Length > 0 ? "using" : "local",
                    declaration.Type.ToString(), v.Initializer?.Value),
            VariableDeclaratorSyntax { Parent: VariableDeclarationSyntax { Parent: UsingStatementSyntax } declaration } v =>
                (v.Identifier.ValueText, "using", declaration.Type.ToString(), v.Initializer?.Value),
            VariableDeclaratorSyntax { Parent: VariableDeclarationSyntax { Parent: ForStatementSyntax or FixedStatementSyntax } declaration } v =>
                (v.Identifier.ValueText, "for", declaration.Type.ToString(), v.Initializer?.Value),
            ForEachStatementSyntax f => (f.Identifier.ValueText, "foreach", f.Type.ToString(), (ExpressionSyntax?)f.Expression),
            SingleVariableDesignationSyntax d => (d.Identifier.ValueText, DesignatedBy(d), "", (ExpressionSyntax?)null),
            _ => ("", "", "", null),
        };
        if (name.Length == 0 || name == "_") return;
        var symbol = Declared(node);
        Row("locals", "lo",
            ("line", Line(node)),
            ("name", name),
            ("declared", declared),
            ("type", TypeOf(symbol)),
            ("type_text", type),
            ("source", CsFacts.Text(value)),
            ("reads", CsFacts.Of(value).Reads),
            ("calls", CsFacts.Of(value).Calls));
    }

    private static string DesignatedBy(SingleVariableDesignationSyntax d) => d.Parent switch
    {
        DeclarationExpressionSyntax { Parent: ArgumentSyntax } => "out",
        DeclarationPatternSyntax or RecursivePatternSyntax or VarPatternSyntax => "pattern",
        ParenthesizedVariableDesignationSyntax => "deconstruction",
        DeclarationExpressionSyntax => "deconstruction",
        _ => "designation",
    };

    /// <summary>
    /// Every comment of the file, after the walk: `line`, `block`, or `doc` for an XML doc comment, with the
    /// declaration it sits in or documents (`Account.Reference`). Not for markup: the Razor compiler's tree
    /// carries its own comments, not the ones somebody wrote.
    /// </summary>
    private void Comments(SyntaxNode root)
    {
        foreach (var trivia in root.DescendantTrivia())
        {
            var kind = trivia.Kind() switch
            {
                SyntaxKind.SingleLineCommentTrivia => "line",
                SyntaxKind.MultiLineCommentTrivia => "block",
                SyntaxKind.SingleLineDocumentationCommentTrivia or SyntaxKind.MultiLineDocumentationCommentTrivia => "doc",
                _ => "",
            };
            if (kind.Length == 0) continue;
            var position = tree.GetLineSpan(trivia.Span).StartLinePosition;
            Row("comments", "cm",
                ("line", position.Line + 1),
                ("col", position.Character + 1),
                ("kind", kind),
                ("context", Context(trivia.Token.Parent)),
                ("text", kind == "doc" ? "///" + trivia.ToString().TrimEnd() : trivia.ToString().TrimEnd()));
        }
    }

    /// <summary>The innermost named declarations around a node, outermost first: `Account.Reference`.</summary>
    private static string Context(SyntaxNode? node)
    {
        var names = new List<string>();
        for (var at = node; at is not null; at = at.Parent)
        {
            var name = at switch
            {
                BaseTypeDeclarationSyntax t => t.Identifier.ValueText,
                MethodDeclarationSyntax m => m.Identifier.ValueText,
                PropertyDeclarationSyntax p => p.Identifier.ValueText,
                ConstructorDeclarationSyntax c => c.Identifier.ValueText,
                EventDeclarationSyntax e => e.Identifier.ValueText,
                LocalFunctionStatementSyntax l => l.Identifier.ValueText,
                BaseFieldDeclarationSyntax f => f.Declaration.Variables.FirstOrDefault()?.Identifier.ValueText ?? "",
                EnumMemberDeclarationSyntax e => e.Identifier.ValueText,
                _ => "",
            };
            if (name.Length > 0) names.Insert(0, name);
        }
        return string.Join('.', names);
    }

    private ISymbol? Declared(SyntaxNode node)
    {
        if (model is null) return null;
        try { return model.GetDeclaredSymbol(node); }
        catch (ArgumentException) { return null; }
    }

    private static string TypeOf(ISymbol? symbol) => symbol switch
    {
        IPropertySymbol p => p.Type.ToDisplayString(CsSemantics.Typed),
        IFieldSymbol f => f.Type.ToDisplayString(CsSemantics.Typed),
        IEventSymbol e => e.Type.ToDisplayString(CsSemantics.Typed),
        ILocalSymbol l => l.Type.ToDisplayString(CsSemantics.Typed),
        _ => "",
    };
}
