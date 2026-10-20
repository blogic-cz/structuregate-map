using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using Microsoft.CodeAnalysis.CSharp.Syntax;

namespace StructureGate;

/// <summary>
/// THE TEST OF A BRANCH AS A TREE (`branches.condition`, stored as `branches__condition` node rows), in ROSLYN'S OWN
/// SHAPE: each node's `kind` is the syntax kind (`EqualsExpression`, `LogicalAndExpression`, `IsPatternExpression`),
/// `source` its text, and its operands sit under `left`/`right`, `operand`, `pattern`, `condition`/`then`/`else`,
/// `receiver`/`args`. This is NOT the TypeScript evaluator: nothing is decided here, so `condition_expr` is a
/// TypeScript column only. A leaf says what it BINDS to (`symbol`) and what the compiler or `CsFolding` knows of it
/// (`const`, `const_kind`) - `tier == Tier.Gold` carries `Demo.Tier.Gold` as an enum, which a text search of `test`
/// cannot tell from a local named the same.
///
/// A DOTTED MEMBER ACCESS IS A LEAF (`Store.Ready` is one fact, `CsFacts.Dotted` its name), parentheses are no node, and
/// only a member WITH a value is written: an empty one would be a row of nothing. Followed to the end, no depth.
/// "" when nothing was tested (a `try`, a `using` declaration), so a join finds no node.
/// </summary>
internal static class CsCondition
{
    public static object Of(SemanticModel? model, SyntaxNode? tested) =>
        tested is ExpressionSyntax expression ? Node(model, expression) : "";

    private static object Node(SemanticModel? model, ExpressionSyntax raw)
    {
        var node = raw;
        while (node is ParenthesizedExpressionSyntax inner) node = inner.Expression;
        var members = Start(node);
        switch (node)
        {
            case BinaryExpressionSyntax binary:
                members.Add(("op", binary.OperatorToken.Text));
                members.Add(("left", Node(model, binary.Left)));
                members.Add(("right", Node(model, binary.Right)));
                return members.ToArray();
            case PrefixUnaryExpressionSyntax prefix:
                members.Add(("op", prefix.OperatorToken.Text));
                members.Add(("operand", Node(model, prefix.Operand)));
                return members.ToArray();
            case PostfixUnaryExpressionSyntax postfix:
                members.Add(("op", postfix.OperatorToken.Text));
                members.Add(("operand", Node(model, postfix.Operand)));
                return members.ToArray();
            case IsPatternExpressionSyntax test:
                members.Add(("op", "is"));
                members.Add(("left", Node(model, test.Expression)));
                members.Add(("pattern", Pattern(model, test.Pattern)));
                return members.ToArray();
            case ConditionalExpressionSyntax choice:
                members.Add(("condition", Node(model, choice.Condition)));
                members.Add(("then", Node(model, choice.WhenTrue)));
                members.Add(("else", Node(model, choice.WhenFalse)));
                return members.ToArray();
            case InvocationExpressionSyntax call:
                Leaf(model, node, members);
                if (call.Expression is MemberAccessExpressionSyntax access)
                    members.Add(("receiver", Node(model, access.Expression)));
                members.Add(("args", call.ArgumentList.Arguments.Select(a => Node(model, a.Expression)).ToList()));
                return members.ToArray();
            default:
                Leaf(model, node, members);
                return members.ToArray();
        }
    }

    private static object Pattern(SemanticModel? model, PatternSyntax pattern)
    {
        var members = Start(pattern);
        switch (pattern)
        {
            case ConstantPatternSyntax constant:
                var value = Node(model, constant.Expression);
                foreach (var (key, member) in (ValueTuple<string, object?>[])value)
                    if (key is "symbol" or "const" or "const_kind") members.Add((key, member));
                break;
            case UnaryPatternSyntax unary:
                members.Add(("op", unary.OperatorToken.Text));
                members.Add(("operand", Pattern(model, unary.Pattern)));
                break;
            case BinaryPatternSyntax binary:
                members.Add(("op", binary.OperatorToken.Text));
                members.Add(("left", Pattern(model, binary.Left)));
                members.Add(("right", Pattern(model, binary.Right)));
                break;
            case RelationalPatternSyntax relational:
                members.Add(("op", relational.OperatorToken.Text));
                members.Add(("operand", Node(model, relational.Expression)));
                break;
            case DeclarationPatternSyntax declaration:
                Typed(model, declaration.Type, members);
                break;
            case TypePatternSyntax typed:
                Typed(model, typed.Type, members);
                break;
        }
        return members.ToArray();
    }

    private static void Typed(SemanticModel? model, TypeSyntax type, List<(string Key, object? Value)> members)
    {
        var name = CsSemantics.Name(CsSemantics.Bound(model, type));
        if (name.Length > 0) members.Add(("type", name));
    }

    private static List<(string Key, object? Value)> Start(SyntaxNode node) =>
        [("kind", node.Kind().ToString()), ("source", CsFacts.Text(node))];

    /// <summary>What a name, a member access or a literal binds to and folds to. A dotted access is not descended into.</summary>
    private static void Leaf(SemanticModel? model, ExpressionSyntax node, List<(string Key, object? Value)> members)
    {
        var symbol = CsSemantics.Name(CsSemantics.Bound(model, node));
        var folded = CsFolding.Of(model, node);
        if (symbol.Length == 0 && folded.Symbol.Length > 0) symbol = folded.Symbol;
        if (symbol.Length > 0) members.Add(("symbol", symbol));
        if (folded.Kind.Length == 0) return;
        if (folded.Text.Length > 0) members.Add(("const", folded.Text));
        members.Add(("const_kind", folded.Kind));
    }
}
