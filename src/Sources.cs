using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;

namespace StructureGate;

/// <summary>
/// HOW MANY LINES a C# file has - the one count that needs a .NET parser. Which files there are, and how many
/// lines every other file has, are rust's (`rust/fbtcore/src/sources/`, `rust/fbtcore/src/count/`), and the gate
/// and the map both ask for this one through a callback.
/// </summary>
public static class Sources
{
    /// <summary>C# BY ROSLYN: a source line is a line a TOKEN sits on. Comments and whitespace are trivia,
    /// so this is exact where a scanner only approximates.</summary>
    public static int CSharpLines(string text) => CSharpLines(CSharpSyntaxTree.ParseText(text));

    /// <summary>The same count over a tree already parsed: the map reads the file's edges off the same tree.</summary>
    public static int CSharpLines(SyntaxTree tree)
    {
        var lines = new HashSet<int>();
        foreach (var token in tree.GetRoot().DescendantTokens())
        {
            if (token.IsKind(SyntaxKind.EndOfFileToken)) continue;
            var span = tree.GetLineSpan(token.Span);
            for (var line = span.StartLinePosition.Line; line <= span.EndLinePosition.Line; line++) lines.Add(line);
        }
        return lines.Count;
    }
}
