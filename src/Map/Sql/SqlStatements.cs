using Microsoft.SqlServer.TransactSql.ScriptDom;

namespace StructureGate;

/// <summary>
/// THE STATEMENTS OF ONE SCRIPT, each with its batch, its place, its nesting and its normalized tree (`SqlTree`).
/// A statement that touches nothing (`SET NOCOUNT ON`, `DECLARE`, `PRINT`) is still a row: `sql_refs` alone left it
/// invisible, and a multi-line statement could not be told from two that share a line.
///
/// NESTING IS READ OFF TOKEN RANGES. ScriptDom's visitor has no exit hook, so every fragment is listed in the order
/// it is visited (each one before its children, a subtree contiguous) and an explicit stack pops what no longer
/// holds the next fragment's tokens: what is left on top is its parent. That order is not the source's - a SELECT's
/// query is visited before its WITH clause - but containment does not care, and `SqlTree` numbers nodes by token.
/// </summary>
internal sealed class SqlStatements
{
    public sealed record Statement(int Batch, int Ordinal, int Parent, int First, int Last, int Line, int EndLine, int Col,
        string Kind, string Source)
    {
        /// <summary>No statement inside it: only a leaf keeps its text, a compound one its line range.</summary>
        public bool Leaf { get; set; } = true;
    }

    public sealed record Node(int Statement, int N, int Parent, int Ord, string Kind, string Value, int Line);

    public List<Statement> Statements { get; } = [];
    public List<Node> Nodes { get; } = [];

    private sealed class Preorder : TSqlFragmentVisitor
    {
        public List<TSqlFragment> Fragments { get; } = [];
        public override void Visit(TSqlFragment node) => Fragments.Add(node);
    }

    private readonly record struct Open(TSqlFragment Fragment, int Statement, int Node);

    /// <param name="text">The file as it is on disk - a leaf's `source` - at the offsets of the text ScriptDom parsed.</param>
    public static SqlStatements Read(TSqlFragment? fragment, string text)
    {
        var read = new SqlStatements();
        if (fragment is not TSqlScript script) return read;
        var trees = new List<SqlTree>();
        for (var batch = 0; batch < script.Batches.Count; batch++)
        {
            var order = new Preorder();
            script.Batches[batch].Accept(order);
            var open = new List<Open>();
            var ordinal = 0;
            foreach (var f in order.Fragments)
            {
                if (f is TSqlBatch || f.FirstTokenIndex < 0 || f.LastTokenIndex < f.FirstTokenIndex) continue;
                while (open.Count > 0 && !Holds(open[^1].Fragment, f)) open.RemoveAt(open.Count - 1);
                Open? up = open.Count > 0 ? open[^1] : null;
                if (f is TSqlStatement statement)
                {
                    var parent = up?.Statement ?? -1;
                    if (parent >= 0) read.Statements[parent].Leaf = false;
                    read.Statements.Add(new Statement(batch, ordinal++, parent, f.FirstTokenIndex, f.LastTokenIndex, f.StartLine,
                        SqlVisitor.EndLine(f), f.StartColumn, SqlTree.KindOf(statement), Slice(text, f)));
                    var tree = new SqlTree();
                    trees.Add(tree);
                    open.Add(new Open(f, read.Statements.Count - 1, tree.Add(-1, SqlTree.KindOf(statement), SqlTree.RootValue(statement), f.StartLine, f.FirstTokenIndex)));
                    continue;
                }
                if (up is not { Statement: >= 0 } held)
                {
                    open.Add(new Open(f, -1, -1));
                    continue;
                }
                var into = trees[held.Statement];
                var node = held.Node;
                if (SqlTree.Wrapper(held.Fragment, f) is { } wrapper) node = into.Add(node, wrapper, "", f.StartLine, f.FirstTokenIndex);
                if (SqlTree.Map(held.Fragment, f) is { } mapped)
                {
                    node = into.Add(node, mapped.Kind, mapped.Value, f.StartLine, f.FirstTokenIndex);
                    into.Synthesized(f, node);
                }
                open.Add(new Open(f, held.Statement, node));
            }
        }
        for (var s = 0; s < trees.Count; s++)
        {
            foreach (var (n, parent, ord, kind, value, line) in trees[s].Numbered())
                read.Nodes.Add(new Node(s, n, parent, ord, kind, value, line));
        }
        return read;
    }

    private static bool Holds(TSqlFragment outer, TSqlFragment inner) =>
        outer.FirstTokenIndex <= inner.FirstTokenIndex && inner.LastTokenIndex <= outer.LastTokenIndex;

    private static string Slice(string text, TSqlFragment f) =>
        f.StartOffset < 0 || f.StartOffset + f.FragmentLength > text.Length ? "" : text.Substring(f.StartOffset, f.FragmentLength);

    /// <summary>The innermost statement whose tokens hold this one, or -1: statements nest, so the one starting
    /// last before it that still reaches past it.</summary>
    public int Of(int token)
    {
        if (token < 0) return -1;
        var best = -1;
        for (var s = 0; s < Statements.Count; s++)
        {
            var at = Statements[s];
            if (at.First <= token && token <= at.Last && (best < 0 || at.First >= Statements[best].First)) best = s;
        }
        return best;
    }
}
