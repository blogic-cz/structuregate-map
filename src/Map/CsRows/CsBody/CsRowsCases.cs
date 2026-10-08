using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using Microsoft.CodeAnalysis.CSharp.Syntax;

namespace StructureGate;

/// <summary>
/// `enums` and `switch_cases`: the two tables a backend value and a frontend value are JOINED through, written in
/// the shape the TypeScript half writes them, so one query reads both languages. Each row carries `owner_file` as
/// the TypeScript rows do, and `file` as every C# row does - the C# store drops a re-read file's rows by `file`,
/// the TypeScript half drops only rows stamped with its `half`, so neither touches the other's.
///
/// AN ENUM MEMBER IS STILL A `consts` ROW (cls = the enum) as it always was; the `enums` row adds what that row
/// cannot carry - every member at once, with its NUMBER. The number is the compiler's when a model bound the
/// file. Without one it is read off the source only where C# defines it without a lookup - a literal, or the
/// previous member's number plus one - and a member whose number would need a lookup is null, never a guess.
///
/// A SWITCH IS STILL ONE `branches` ROW (kind = switch). Each `case` section of a statement and each arm of a
/// switch expression is a `switch_cases` row beside it (`branch` = that row's id): its labels as written, the
/// symbol and the constant each label binds to, its `when` guard - so "which switch handles `Kinds.Acme`"
/// is a query, not a read of the source.
/// </summary>
internal sealed partial class CsRows
{
    /// <summary>
    /// THE CASE A ROW SITS IN, stamped on it as `case` - as the TypeScript half does. `case Acme: return 1;` recorded
    /// as a bare `returns` row says the method returns 1, full stop, when it returns 1 for ONE kind: a conditional
    /// fact published as an unconditional one. Nested switches nest: an inner switch's case rows carry the outer case.
    /// </summary>
    private string? currentCase;

    /// <summary>The `switch_cases` row each section and arm was given when its switch was read.</summary>
    private readonly Dictionary<SyntaxNode, string> caseOf = [];

    public override void VisitSwitchSection(SwitchSectionSyntax node) => InCase(node, () => base.VisitSwitchSection(node));

    public override void VisitSwitchExpressionArm(SwitchExpressionArmSyntax node) => InCase(node, () => base.VisitSwitchExpressionArm(node));

    private void InCase(SyntaxNode node, Action visit)
    {
        var outer = currentCase;
        if (caseOf.TryGetValue(node, out var id) && id.Length > 0) currentCase = id;
        try { visit(); }
        finally { currentCase = outer; }
    }

    /// <summary>One `enums` row, with every member and its number.</summary>
    private void Enum(EnumDeclarationSyntax node)
    {
        var members = new List<(string Key, object? Value)[]>();
        long? previous = -1;
        foreach (var member in node.Members)
        {
            var value = MemberValue(member, previous);
            previous = value;
            members.Add([("name", member.Identifier.ValueText), ("value", Number(value))]);
        }
        Row("enums", "e",
            ("owner_file", file),
            ("line", Line(node)),
            ("end_line", EndLine(node)),
            ("name", node.Identifier.ValueText),
            ("symbol", CsSemantics.DeclaredName(model, node)),
            ("exported", node.Modifiers.Any(SyntaxKind.PublicKeyword) ? 1 : 0),
            ("const", 0),
            ("underlying", node.BaseList?.Types.FirstOrDefault()?.Type.ToString() ?? "int"),
            ("members", members));
    }

    /// <summary>The number a member stands for: the compiler's, else what the source alone defines.</summary>
    private long? MemberValue(EnumMemberDeclarationSyntax member, long? previous)
    {
        if (model is not null)
        {
            try
            {
                if (model.GetDeclaredSymbol(member) is IFieldSymbol { HasConstantValue: true } field)
                    return Integral(field.ConstantValue);
            }
            catch (ArgumentException) { }
        }
        return member.EqualsValue?.Value switch
        {
            null => previous + 1,
            LiteralExpressionSyntax literal => Integral(literal.Token.Value),
            PrefixUnaryExpressionSyntax { RawKind: (int)SyntaxKind.UnaryMinusExpression, Operand: LiteralExpressionSyntax literal }
                => -Integral(literal.Token.Value),
            _ => null,
        };
    }

    /// <summary>A `switch` statement: one row per section, its labels and guards.</summary>
    private void Cases(SwitchStatementSyntax node, string branch)
    {
        foreach (var section in node.Sections)
        {
            var labels = new List<ExpressionSyntax>();
            var shown = new List<string>();
            var guards = new List<string>();
            var isDefault = false;
            foreach (var label in section.Labels)
            {
                switch (label)
                {
                    case DefaultSwitchLabelSyntax:
                        isDefault = true;
                        break;
                    case CaseSwitchLabelSyntax value:
                        labels.Add(value.Value);
                        shown.Add(CsFacts.Text(value.Value));
                        break;
                    case CasePatternSwitchLabelSyntax pattern:
                        if (pattern.Pattern is ConstantPatternSyntax constant) labels.Add(constant.Expression);
                        shown.Add(CsFacts.Text(pattern.Pattern));
                        if (pattern.WhenClause is { } when) guards.Add(CsFacts.Text(when.Condition));
                        break;
                }
            }
            Case(section, branch, "case", node.Expression, shown, labels, guards, isDefault, "");
        }
    }

    /// <summary>A switch EXPRESSION: one row per arm, with what the arm evaluates to.</summary>
    private void Arms(SwitchExpressionSyntax node, string branch)
    {
        foreach (var arm in node.Arms)
        {
            var isDefault = arm.Pattern is DiscardPatternSyntax;
            List<ExpressionSyntax> labels = arm.Pattern is ConstantPatternSyntax constant ? [constant.Expression] : [];
            List<string> shown = isDefault ? [] : [CsFacts.Text(arm.Pattern)];
            List<string> guards = arm.WhenClause is { } when ? [CsFacts.Text(when.Condition)] : [];
            Case(arm, branch, "arm", node.GoverningExpression, shown, labels, guards, isDefault, CsFacts.Text(arm.Expression));
        }
    }

    private void Case(SyntaxNode node, string branch, string kind, ExpressionSyntax discriminant, List<string> shown,
        List<ExpressionSyntax> labels, List<string> guards, bool isDefault, string result)
    {
        caseOf[node] = Row("switch_cases", "sc",
            ("owner_file", file),
            ("line", Line(node)),
            ("end_line", EndLine(node)),
            ("branch", branch),
            ("kind", kind),
            ("discriminant_source", CsFacts.Text(discriminant)),
            ("discriminant_type", CsSemantics.Type(model, discriminant)),
            ("labels", shown),
            // WHAT EACH CONSTANT LABEL BINDS TO, as the compiler names it - "" without a model, never a guess.
            ("label_symbols", labels.Select(l => CsSemantics.Name(CsSemantics.NameOf(model, l))).ToList()),
            ("label_values", labels.Select(LabelValue).ToList()),
            ("guard", string.Join(" && ", guards)),
            ("is_default", isDefault ? 1 : 0),
            ("result", result));
    }

    /// <summary>The constant a label compares against - an enum member's NUMBER, not its name.</summary>
    private string LabelValue(ExpressionSyntax label)
    {
        if (model is null) return "";
        try
        {
            var constant = model.GetConstantValue(label);
            return constant.HasValue ? Convert.ToString(constant.Value, System.Globalization.CultureInfo.InvariantCulture) ?? "" : "";
        }
        catch (ArgumentException) { return ""; }
    }

    private static long? Integral(object? value) => value switch
    {
        sbyte or byte or short or ushort or int or uint or long => Convert.ToInt64(value, System.Globalization.CultureInfo.InvariantCulture),
        ulong big when big <= long.MaxValue => (long)big,
        _ => null,
    };

    /// <summary>As the TypeScript rows hold it: a number where one fits, null where none is known.</summary>
    private static object? Number(long? value) => value is null ? null : value is >= int.MinValue and <= int.MaxValue ? (int)value : value;
}
