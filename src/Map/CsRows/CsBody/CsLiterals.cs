using System.Globalization;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using Microsoft.CodeAnalysis.CSharp.Syntax;

namespace StructureGate;

/// <summary>
/// `string_literals` and `number_literals`: every literal, with what it is DOING where it is written - the
/// vocabulary the python, TypeScript and rust halves share, so one `--magic` query reads all of them.
///
/// `use` is `compare` (an operand of `==`/`&lt;`, a `case`, a pattern), `arith`, `index` (an element
/// access), `argument` (with the `callee` it is passed to), `assign`, `return`, `default` (a parameter's),
/// `declared` (the initializer of a `const` or `static readonly` field, a `const` local or an enum member),
/// `format` (an interpolation hole) or `other`. `target` is what an index or a method call applies to:
/// `line.Split(':')[2]` is a ':' passed to `Split` on `line`, and a 2 indexing `line.Split(':')`.
///
/// A `char` IS A STRING HERE: `Split(':')` splits on a char, and a separator is the same fact either way.
/// A SIGN IS PART OF THE NUMBER: `-1` is one literal, as the compiler folds it.
/// </summary>
internal sealed partial class CsRows
{
    private void Literal(LiteralExpressionSyntax literal)
    {
        var context = LiteralContext(literal);
        switch (literal.Token.Value)
        {
            case string spelled when spelled.Trim().Length > 0:
                Row("string_literals", "s", ("line", Line(literal)), ("value", spelled), ("length", spelled.Length),
                    ("use", context.Context), ("callee", context.Callee), ("target", context.Target));
                break;
            case char single when !char.IsWhiteSpace(single):
                Row("string_literals", "s", ("line", Line(literal)), ("value", single.ToString()), ("length", 1),
                    ("use", context.Context), ("callee", context.Callee), ("target", context.Target));
                break;
            case not null when literal.IsKind(SyntaxKind.NumericLiteralExpression):
                var negative = literal.Parent is PrefixUnaryExpressionSyntax sign && sign.IsKind(SyntaxKind.UnaryMinusExpression);
                var number = Convert.ToDouble(literal.Token.Value, CultureInfo.InvariantCulture);
                Row("number_literals", "n", ("line", Line(literal)),
                    ("value", (negative ? "-" : "") + literal.Token.Text), ("number", negative ? -number : number),
                    ("use", context.Context), ("callee", context.Callee), ("target", context.Target));
                break;
        }
    }

    private static (string Context, string Callee, string Target) LiteralContext(ExpressionSyntax literal)
    {
        var found = UseOf(literal, out var contained);
        // AN ELEMENT IS NOT THE ARGUMENT: the strings of `string.Join(",", new[] { a, "x" })` are not separators.
        // Only a constant's value is the collection's - `static readonly int[] Sizes = { 1, 2 }` declares both.
        return contained && found.Context != "declared" ? ("other", "", "") : found;
    }

    private static (string Context, string Callee, string Target) UseOf(ExpressionSyntax literal, out bool contained)
    {
        contained = false;
        SyntaxNode child = literal;
        var parent = literal.Parent;
        // Through a sign, brackets and the collection a literal sits in: `static readonly int[] Sizes = { 1, 2 }`
        // declares both numbers.
        while (parent is PrefixUnaryExpressionSyntax or ParenthesizedExpressionSyntax or InitializerExpressionSyntax
                   { RawKind: (int)SyntaxKind.ArrayInitializerExpression or (int)SyntaxKind.CollectionInitializerExpression }
                   or ExpressionElementSyntax or CollectionExpressionSyntax
                   or ArrayCreationExpressionSyntax or ImplicitArrayCreationExpressionSyntax)
        {
            contained |= parent is not (PrefixUnaryExpressionSyntax or ParenthesizedExpressionSyntax);
            child = parent;
            parent = parent.Parent;
        }
        switch (parent)
        {
            case BinaryExpressionSyntax binary:
                return (binary.Kind() is SyntaxKind.EqualsExpression or SyntaxKind.NotEqualsExpression
                    or SyntaxKind.LessThanExpression or SyntaxKind.LessThanOrEqualExpression
                    or SyntaxKind.GreaterThanExpression or SyntaxKind.GreaterThanOrEqualExpression ? "compare" : "arith", "", "");
            case ConstantPatternSyntax or RelationalPatternSyntax or CaseSwitchLabelSyntax:
                return ("compare", "", "");
            case AssignmentExpressionSyntax assignment when assignment.Right == child:
                return (assignment.IsKind(SyntaxKind.SimpleAssignmentExpression) ? "assign" : "arith", "", "");
            case ArgumentSyntax { Parent: BracketedArgumentListSyntax { Parent: ElementAccessExpressionSyntax access } }:
                return ("index", "", CsFacts.Text(access.Expression));
            case ArgumentSyntax { Parent: ArgumentListSyntax { Parent: InvocationExpressionSyntax call } }:
                return call.Expression switch
                {
                    MemberAccessExpressionSyntax member => ("argument", member.Name.Identifier.ValueText, CsFacts.Text(member.Expression)),
                    SimpleNameSyntax name => ("argument", name.Identifier.ValueText, ""),
                    _ => ("argument", "", ""),
                };
            case ArgumentSyntax { Parent: ArgumentListSyntax { Parent: BaseObjectCreationExpressionSyntax creation } }:
                return ("argument", creation is ObjectCreationExpressionSyntax made ? CsFacts.Text(made.Type) : "", "");
            case EqualsValueClauseSyntax { Parent: EnumMemberDeclarationSyntax }:
                return ("declared", "", "");
            case EqualsValueClauseSyntax { Parent: ParameterSyntax }:
                return ("default", "", "");
            case EqualsValueClauseSyntax { Parent: VariableDeclaratorSyntax { Parent: VariableDeclarationSyntax { Parent: var holder } } }:
                return (Declares(holder) ? "declared" : "assign", "", "");
            case EqualsValueClauseSyntax:
                return ("assign", "", "");
            case ReturnStatementSyntax or ArrowExpressionClauseSyntax or YieldStatementSyntax:
                return ("return", "", "");
            case InterpolationSyntax:
                return ("format", "", "");
            default:
                return ("other", "", "");
        }
    }

    /// <summary>A `const` or `static readonly` field, or a `const` local - what `consts` calls a constant.</summary>
    private static bool Declares(SyntaxNode? holder) => holder switch
    {
        FieldDeclarationSyntax field => field.Modifiers.Any(SyntaxKind.ConstKeyword)
            || (field.Modifiers.Any(SyntaxKind.StaticKeyword) && field.Modifiers.Any(SyntaxKind.ReadOnlyKeyword)),
        LocalDeclarationStatementSyntax local => local.IsConst,
        _ => false,
    };
}
