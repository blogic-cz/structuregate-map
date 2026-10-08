using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using Microsoft.CodeAnalysis.CSharp.Syntax;

namespace StructureGate;

/// <summary>
/// THE THIRD PART OF <see cref="CsRows"/>: a value built out of PARTS, and what each part is.
///
/// A call has a row and its arguments have rows, so `Save(path, retry: true)` is fully answerable. Nothing
/// answered for the other half of how data is written down in C#: `new Options { Region = "US", Retry = 2 }`
/// was one assignment row per line with no sense of the object they belong to, a tuple was an expression
/// with a shape, and an array of route strings was a handful of unattached literals. "What is this thing
/// configured with" is a question about the CONSTRUCT, and the construct had no row.
///
/// `new Foo(x)` IS NOT HERE, on purpose. It is already a `calls` row — a constructor is called, which is
/// the vocabulary the python half shares, and `who constructs this` is one query across both languages. Its
/// INITIALIZER is here, because `new Foo(x) { Name = "a" }` carries two different things and only the first
/// is a call.
///
/// THE SLOTS GO IN `arguments`, beside a call's, because they are the same row: a position, a name where
/// there is one, the text, and what the value turned out to be. A second table of the same shape would be a
/// second place to look for the same fact.
/// </summary>
internal sealed partial class CsRows
{
    /// <summary>Every construct row of this file, by the node that made it — so a construct nested inside
    /// another can name its parent. Ancestors are always visited first, which is what makes the lookup a
    /// walk up the tree rather than a second pass.</summary>
    private readonly Dictionary<SyntaxNode, string> constructs = [];

    /// <summary>
    /// The construct this one sits inside, or "". `new (string, string)[] { (path, name), ... }` is an
    /// array whose ELEMENTS ARE TUPLES, and without the link each tuple is a row floating free of the array
    /// it belongs to — a reader can see both and cannot say which array a pair came from.
    /// </summary>
    private string ParentOf(SyntaxNode node)
    {
        for (var at = node.Parent; at is not null; at = at.Parent)
        {
            if (constructs.TryGetValue(at, out var owner)) return owner;
            if (at is StatementSyntax or MemberDeclarationSyntax) break;
        }
        return "";
    }

    /// <summary>A construct whose slots are worth rows. `new` is excluded — see the class comment.</summary>
    private void Construct(ExpressionSyntax node, string kind, SyntaxNode? typeSource,
        IEnumerable<SyntaxNode> slots)
    {
        var listed = slots.ToList();
        var owner = Row("objects", "o",
            ("line", Line(node)),
            ("kind", kind),
            ("parent", ParentOf(node)),
            // THE NAME THIS CONSTRUCT IS STORED UNDER, where it has one. `_items = new (string,string)[]
            // {…}` is the array a type's entries live in, and a reader that had to find it by LINE —
            // matching the field's line against the construct's — lost entries the moment an
            // initializer wrapped onto the next line.
            ("target", Stored(node)),
            // The TYPE the construct produces, resolved: `[..]` says nothing in the text, and `new[]` says
            // only that it is an array of whatever the elements turned out to be. The written type is asked
            // first and the construct itself second — an initializer's `Options` is a type NAME, which the
            // model answers about with nothing, while the creation around it resolves.
            ("type", CsSemantics.Type(model, typeSource) is { Length: > 0 } written
                ? written : CsSemantics.Type(model, node)),
            ("source", CsFacts.Text(node)),
            ("slots", listed.Count));
        if (owner.Length == 0) return;
        constructs[node] = owner;

        var position = 0;
        foreach (var slot in listed) Slot(owner, slot, position++);
    }

    /// <summary>
    /// The name a construct is assigned to: a field, a local, a property, or the left side of an
    /// assignment. "" for one that is passed straight into a call or returned.
    /// </summary>
    private string Stored(SyntaxNode node)
    {
        // A CONSTRUCT INSIDE A CONSTRUCT IS NOT THE ONE BEING STORED. `_filesLocal = new (…)[] { (a, b) }`
        // stores the ARRAY; the tuple inside it is an element. Letting the tuple claim the name too made
        // the later row win a name lookup, and the file it held then belonged to nothing.
        if (ParentOf(node).Length > 0) return "";
        for (var at = node.Parent; at is not null; at = at.Parent)
        {
            switch (at)
            {
                case VariableDeclaratorSyntax declarator:
                    return declarator.Identifier.ValueText;
                case PropertyDeclarationSyntax property:
                    return property.Identifier.ValueText;
                case AssignmentExpressionSyntax assigned:
                    return CsFacts.Dotted(assigned.Left) is { Length: > 0 } written
                        ? written : assigned.Left.ToString();
                case StatementSyntax or MemberDeclarationSyntax:
                    return "";
            }
        }
        return "";
    }

    /// <summary>
    /// One slot of a construct. A NAME where the language gives one — the member in an object initializer,
    /// the element name of a tuple — and a position always, because that is what the other kind of slot has.
    /// </summary>
    private void Slot(string owner, SyntaxNode slot, int position)
    {
        var (name, value) = slot switch
        {
            // `new Options { Region = "US" }` — the member is the name, the right-hand side is the value.
            AssignmentExpressionSyntax assigned =>
                (CsFacts.Dotted(assigned.Left) is { Length: > 0 } written ? written : assigned.Left.ToString(),
                 (ExpressionSyntax?)assigned.Right),
            // `(code: "EU", name: x)` — a tuple element may be named too.
            ArgumentSyntax argument => (argument.NameColon?.Name.Identifier.ValueText ?? "", argument.Expression),
            ExpressionElementSyntax element => ("", element.Expression),
            SpreadElementSyntax spread => ("..", spread.Expression),
            ExpressionSyntax expression => ("", expression),
            _ => ("", null),
        };

        var folded = CsFolding.Of(model, value);
        Row("arguments", "arg",
            ("line", Line(slot)),
            // The OWNER column a call's argument does not fill, and the other way round: one row shape, two
            // kinds of owner, and a query that joins the wrong one gets nothing rather than the wrong rows.
            ("object", owner),
            ("call", ""),
            ("position", position),
            ("name", name),
            ("source", CsFacts.Text(value)),
            ("const", folded.Text),
            ("const_kind", folded.Kind),
            ("symbol", folded.Symbol),
            ("type", CsSemantics.Type(model, value)));
    }

    /// <summary>
    /// The constructs, read off the tree. An initializer is taken from the creation that owns it rather than
    /// on its own, because `{ ... }` means something different after `new Foo` than after `new[]`.
    /// </summary>
    private void Constructs(SyntaxNode node)
    {
        switch (node)
        {
            case TupleExpressionSyntax tuple:
                Construct(tuple, "tuple", tuple, tuple.Arguments);
                break;
            case ArrayCreationExpressionSyntax array:
                Construct(array, "array", array.Type,
                    array.Initializer?.Expressions.Cast<SyntaxNode>() ?? []);
                break;
            case ImplicitArrayCreationExpressionSyntax implicitArray:
                Construct(implicitArray, "array", implicitArray, implicitArray.Initializer.Expressions);
                break;
            case CollectionExpressionSyntax collection:
                Construct(collection, "collection", collection, collection.Elements);
                break;
            case BaseObjectCreationExpressionSyntax { Initializer: not null } created:
                // The creation itself is a `calls` row; this is what it was filled with.
                Construct(created, Named(created.Initializer) ? "initializer" : "collection",
                    created is ObjectCreationExpressionSyntax typed ? typed.Type : created,
                    created.Initializer.Expressions);
                break;
        }
    }

    /// <summary>Whether an initializer names members (`{ A = 1 }`) or lists values (`{ 1, 2 }`). A
    /// collection initializer on a `new` is the second, and its slots have no names to record.</summary>
    private static bool Named(InitializerExpressionSyntax initializer) =>
        initializer.Expressions.Any(e => e is AssignmentExpressionSyntax);
}
