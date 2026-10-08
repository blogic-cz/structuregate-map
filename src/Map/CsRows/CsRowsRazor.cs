using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp.Syntax;

namespace StructureGate;

/// <summary>
/// A `.razor`/`.cshtml` walked as the tree the Razor compiler generated for it - see CsRazor.
///
/// A ROW NAMES THE MARKUP LINE: the generated C# carries `#line` for every expression written in markup, and
/// the mapped span is the line a reader opens. A row anchored in `#line hidden` code - the render plumbing
/// between expressions, the members the generator adds - is not something anybody wrote and is DROPPED
/// (`Row` drops line 0); the component's own class is kept, at line 1, because C# names it.
/// </summary>
internal sealed partial class CsRows
{
    private bool razor;

    /// <summary>The calls the generator writes around each markup expression - `__builder.AddContent`,
    /// MVC's `Write` - which call nothing the author wrote.</summary>
    private static readonly string[] Plumbing =
        ["Write", "WriteLiteral", "BeginWriteAttribute", "WriteAttributeValue", "EndWriteAttribute", "DefineSection"];

    private int Mapped(SyntaxNode node, bool end)
    {
        var span = tree.GetMappedLineSpan(node.Span);
        if (tree.GetLineVisibility(node.SpanStart) != LineVisibility.Visible || !span.HasMappedPath) return node is BaseTypeDeclarationSyntax ? 1 : 0;
        return (end ? span.EndLinePosition.Line : span.StartLinePosition.Line) + 1;
    }

    private bool IsPlumbing(string callee) => razor && IsPlumbingName(callee);

    private static bool IsPlumbingName(string name) =>
        name.StartsWith("__", StringComparison.Ordinal) || Plumbing.Contains(name, StringComparer.Ordinal);

    /// <summary>`razor_renders`: each component a component renders, with the attributes it sets.</summary>
    private static void Renders(CsTables rows, string file, string space, string rel, CsRazor.Output razor)
    {
        var cls = Path.GetFileNameWithoutExtension(rel);
        foreach (var render in razor.Renders)
        {
            rows.Add("razor_renders", "rr", ("file", file), ("cls", cls), ("func", ""), ("line", render.Line),
                ("component", render.Component), ("tag", render.Tag), ("attributes", render.Attributes),
                ("namespace", space));
        }
    }
}
