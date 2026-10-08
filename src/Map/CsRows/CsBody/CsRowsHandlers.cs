using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using Microsoft.CodeAnalysis.CSharp.Syntax;

namespace StructureGate;

/// <summary>
/// `handlers`: one row per `catch` clause - what it catches, what it binds, and what its body DOES with it -
/// in the columns the python half writes for an `except`. The `try` row in `branches` stays beside it; this is
/// the clause, one by one.
///
/// THE BODY IS THE CLAUSE'S OWN NODES. A lambda, an anonymous method, a local function or a type declared in
/// the block runs later, or never: a `throw` there is not the handler's, as python leaves out a nested `def`.
///
/// THREE COLUMNS ARE C#'S. `guard` is the `when (...)` filter as written, `finally` says the try has one, and
/// `symbol` is the BOUND type - "" without a model, never a guess. The python-only columns (`star`,
/// `exc_info`, `noqa`, `codes`) are written as 0 / `[]`, so a filter on them reads the same in every half.
/// </summary>
internal sealed partial class CsRows
{
    /// <summary>One `catch` clause of <paramref name="tryNode"/>, as a row.</summary>
    private void Handler(TryStatementSyntax tryNode, CatchClauseSyntax clause)
    {
        var type = clause.Declaration?.Type;
        var name = clause.Declaration?.Identifier.ValueText ?? "";
        var calls = new SortedSet<string>(StringComparer.Ordinal);
        var raises = 0;
        var reraises = 0;
        var nameRead = 0;
        foreach (var node in OwnNodes(clause.Block))
        {
            switch (node)
            {
                // `throw;`, or `throw ex` of the name this clause bound: the exception goes on as it came.
                case ThrowStatementSyntax thrown:
                    raises++;
                    if (thrown.Expression is null || IsName(thrown.Expression, name)) reraises = 1;
                    break;
                case ThrowExpressionSyntax:
                    raises++;
                    break;
                case IdentifierNameSyntax read when !CsFacts.IsTail(read) && IsName(read, name):
                    nameRead = 1;
                    break;
                case InvocationExpressionSyntax call when CsFacts.Dotted(call.Expression) is { Length: > 0 } callee:
                    calls.Add(callee);
                    break;
                case ObjectCreationExpressionSyntax creation when CsFacts.Dotted(creation.Type) is { Length: > 0 } built:
                    calls.Add(built);
                    break;
            }
        }
        var written = type is null ? ""
            : CsFacts.Dotted(type) is { Length: > 0 } dotted ? dotted : type.ToString();
        // WHAT A HANDLER CATCHES IS READ, every prefix of the chain too - the python half's rule.
        var reads = new SortedSet<string>(CsFacts.Of(type).Reads, StringComparer.Ordinal);
        if (written.Length > 0) reads.Add(written);
        Row("handlers", "h",
            ("line", Line(clause)),
            ("end_line", EndLine(clause)),
            ("try_line", Line(tryNode)),
            ("types", type is null ? new List<string>() : [written]),
            ("bare", type is null ? 1 : 0),
            ("star", 0),
            ("name", name),
            ("name_read", nameRead),
            // A COMMENT IS TRIVIA, not a statement: a block holding only one still does nothing.
            ("passes", clause.Block.Statements.All(s => s is EmptyStatementSyntax) ? 1 : 0),
            ("raises", raises),
            ("reraises", reraises),
            ("exc_info", 0),
            ("calls", calls.ToList()),
            ("comment", CommentOn(clause)),
            ("noqa", 0),
            ("codes", new List<string>()),
            ("reads", reads.ToList()),
            ("guard", CsFacts.Text(clause.Filter?.FilterExpression)),
            ("finally", tryNode.Finally is null ? 0 : 1),
            ("symbol", CsSemantics.Type(model, type)));
    }

    /// <summary>Every node of <paramref name="block"/>, a nested scope's children left out.</summary>
    private static IEnumerable<SyntaxNode> OwnNodes(SyntaxNode block) =>
        block.DescendantNodesAndSelf(n => n is not AnonymousFunctionExpressionSyntax
            and not LocalFunctionStatementSyntax and not TypeDeclarationSyntax);

    private static bool IsName(SyntaxNode node, string name) =>
        name.Length > 0 && node is IdentifierNameSyntax read && read.Identifier.ValueText == name;

    /// <summary>
    /// The first comment on the `catch` keyword's line, as written - after the type, after the filter or after
    /// the `{`. Lines are compared on the RAW tree for both, so a razor `#line` mapping cannot split them.
    /// </summary>
    private string CommentOn(CatchClauseSyntax clause)
    {
        var line = tree.GetLineSpan(clause.CatchKeyword.Span).StartLinePosition.Line;
        foreach (var trivia in clause.DescendantTrivia())
        {
            if (!trivia.IsKind(SyntaxKind.SingleLineCommentTrivia)
                && !trivia.IsKind(SyntaxKind.MultiLineCommentTrivia)) continue;
            if (tree.GetLineSpan(trivia.Span).StartLinePosition.Line == line) return trivia.ToString();
        }
        return "";
    }
}
