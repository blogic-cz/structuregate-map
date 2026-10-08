using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using Microsoft.CodeAnalysis.CSharp.Syntax;

namespace StructureGate;

/// <summary>
/// THE OTHER HALF OF <see cref="CsRows"/>: what happens INSIDE a declaration — every call, branch,
/// assignment, throw, string and expression as a row. Split out of the file beside it because that file
/// crossed the 500-line ceiling this repo holds every hand-written file to, and the split is the one the
/// rows themselves make: a declaration is a name someone can reach, a statement is what the code does.
///
/// EVERY ROW IS EMITTED WHILE THE SCOPE STACK IS STANDING. That is the whole reason this is a walk and not
/// a second pass: the python half learned it the hard way, where a pass run after the visit had finished
/// wrote expression rows that all said `cls=""` and `func=""`, and the one query the table exists
/// for — which functions read this constant — could then only ever answer with a file.
/// </summary>
internal sealed partial class CsRows
{
    /// <summary>
    /// An expression smaller than this is a name or a literal, and a row for each would multiply the table
    /// while answering nothing the other columns already answer. Counted in NODES, so a long identifier
    /// cannot make a trivial expression look substantial.
    /// </summary>
    private const int MinExpressionNodes = 3;

    /// <summary>What a `refs` row is worth recording for. A LOCAL AND A PARAMETER ARE NOT: they are named
    /// once, read three lines later, and gone — every one of them would be a row, and the table would be
    /// mostly noise about names nothing outside one method can see.</summary>
    private static readonly SymbolKind[] Referable =
        [SymbolKind.Field, SymbolKind.Property, SymbolKind.Method, SymbolKind.NamedType, SymbolKind.Event];

    /// <summary>One row per symbol per LINE. `Store.Path` written twice on one line is one reference to
    /// follow, and the chain `a.b.c` would otherwise report its own prefixes as separate rows.</summary>
    private readonly HashSet<string> referenced = [];

    /// <summary>
    /// Everything that is not a declaration. Roslyn's walker routes every node it has no override for
    /// through here, so this is where the tree is read — and the switch falls through to
    /// <c>base.DefaultVisit</c> in every case, because a row is never a reason to stop descending.
    /// </summary>
    public override void DefaultVisit(SyntaxNode node)
    {
        // WHAT A TYPE HOLDS AND A BODY DECLARES, beside whatever else the node is - see CsMembers.
        if (node is BasePropertyDeclarationSyntax or BaseFieldDeclarationSyntax) Members((MemberDeclarationSyntax)node);
        if (node is VariableDeclaratorSyntax or ForEachStatementSyntax or SingleVariableDesignationSyntax) Local(node);
        switch (node)
        {
            case IfStatementSyntax branch: Branch(branch, "if", branch.Condition); break;
            case WhileStatementSyntax branch: Branch(branch, "while", branch.Condition); break;
            case DoStatementSyntax branch: Branch(branch, "do", branch.Condition); break;
            case ForStatementSyntax branch: Branch(branch, "for", branch.Condition); break;
            case ForEachStatementSyntax branch: Branch(branch, "foreach", branch.Expression); break;
            case SwitchStatementSyntax branch: Cases(branch, Branch(branch, "switch", branch.Expression)); break;
            case SwitchExpressionSyntax branch: Arms(branch, Branch(branch, "switch", branch.GoverningExpression)); break;
            case UsingStatementSyntax branch:
                Branch(branch, "using", (SyntaxNode?)branch.Expression ?? branch.Declaration);
                break;
            case LockStatementSyntax branch: Branch(branch, "lock", branch.Expression); break;
            case TryStatementSyntax branch: Try(branch); break;

            case ReturnStatementSyntax statement: Returns(statement, statement.Expression); break;
            case YieldStatementSyntax statement: Returns(statement, statement.Expression); break;
            // `=> expression` IS a return, and a tree written in expression-bodied members would otherwise
            // have an empty `returns` table — which reads as a codebase whose methods return nothing.
            case ArrowExpressionClauseSyntax arrow: Returns(arrow, arrow.Expression); break;

            case ThrowStatementSyntax statement: Raises(statement, statement.Expression); break;
            case ThrowExpressionSyntax expression: Raises(expression, expression.Expression); break;

            case EventFieldDeclarationSyntax field: Variables(field.Declaration, "event"); break;
            case LocalDeclarationStatementSyntax local: Variables(local.Declaration, "local"); break;
            case EnumMemberDeclarationSyntax member: EnumMember(member); break;
            case PropertyDeclarationSyntax { Initializer: not null } property:
                Assignment(property, property.Identifier.ValueText, "property", property.Initializer.Value);
                break;
            // An assignment INSIDE an initializer is a slot of that construct and has its row there; a row
            // here as well would be the same fact under two names.
            case AssignmentExpressionSyntax assignment
                    when assignment.Parent is not InitializerExpressionSyntax:
                Assigned(assignment);
                break;

            case TupleExpressionSyntax or ArrayCreationExpressionSyntax
                or ImplicitArrayCreationExpressionSyntax or CollectionExpressionSyntax:
                Constructs(node);
                break;

            case MemberAccessExpressionSyntax access: Reference(access); break;
            case IdentifierNameSyntax name when !CsFacts.IsTail(name): Reference(name); break;
            // A GENERIC OR QUALIFIED TYPE NAME is a reference too: `typeof(Pipeline<,>)`, a field of `Holder<int>`, a
            // `Demo.Other.Thing` written out. Only bare identifiers were, so a type registered with DI through an open
            // generic had no row and read as unused. A qualified name that binds to a namespace is dropped below.
            case GenericNameSyntax generic when !CsFacts.IsTail(generic): Reference(generic); break;
            case QualifiedNameSyntax qualified: Reference(qualified); break;

            case InvocationExpressionSyntax call: Call(call, call.Expression, call.ArgumentList); break;
            // A creation is BOTH: a call of the constructor, and — when it carries one — a construct whose
            // initializer says what the object was filled with.
            case ObjectCreationExpressionSyntax creation:
                Call(creation, creation.Type, creation.ArgumentList);
                Constructs(creation);
                break;
            case BaseObjectCreationExpressionSyntax implicitly:
                Call(implicitly, null, implicitly.ArgumentList);
                Constructs(implicitly);
                break;
            // Strings, chars and numbers, each with what it is DOING there - see CsLiterals.cs.
            case LiteralExpressionSyntax literal: Literal(literal); break;
        }

        if (node is ExpressionSyntax expressionNode) Expression(expressionNode);
        base.DefaultVisit(node);
    }

    /// <summary>Every mapped C# file, absolute path to the path it is mapped under - the files a bound symbol can be
    /// declared in. Null for a syntax-only read.</summary>
    private IReadOnlyDictionary<string, string>? rels;
    /// <summary>Where each `using` the compiler calls unnecessary starts; null when it cannot say (see `Diagnostics`).</summary>
    private HashSet<int>? unneeded;
    /// <summary>The mapped files this one's bound symbols are declared in - its `file_refs` rows.</summary>
    private readonly SortedSet<string> reached = new(StringComparer.Ordinal);

    /// <summary>
    /// THE FILE A BOUND SYMBOL IS DECLARED IN, as the compiler says - every file of a partial type. The file map
    /// joins C# by NAME, and a name declared in several files gave no edge at all, though Roslyn had bound every use
    /// to one of them; these rows are what lets it choose.
    /// </summary>
    private void Reach(ISymbol? symbol)
    {
        if (symbol is null || rels is null) return;
        if (symbol is INamedTypeSymbol named) symbol = named.OriginalDefinition;
        if (!Referable.Contains(symbol.Kind)) return;
        foreach (var location in symbol.Locations)
        {
            // ITS OWN FILE TOO: a type naming itself binds to itself, and the join leaves that edge out.
            if (location.IsInSource && location.SourceTree?.FilePath is { } path && rels.TryGetValue(path, out var target))
                reached.Add(target);
        }
    }

    /// <summary>
    /// A NAME, RESOLVED — every reference to something this codebase declares that no call row already
    /// carries.
    ///
    /// `reads` on the rows beside this one is what the text SAYS: `Config.Path`, as written, joined to
    /// nothing. This is what it MEANS — `Demo.Core.Config.Path`, the one declared in this solution — so
    /// "who touches this property" is a query rather than a grep across two spellings of the same name.
    ///
    /// ONLY WHAT IS DECLARED IN SOURCE. A reference to `string.Length` or to a package type is a fact about
    /// the framework, not about this codebase, and there are hundreds of thousands of them.
    /// </summary>
    private void Reference(ExpressionSyntax node)
    {
        if (model is null) return;
        // The callee of a call already has a row of its own, with its arguments and its overload.
        if (node.Parent is InvocationExpressionSyntax invoked && invoked.Expression == node) return;
        // `var` binds to whatever the initializer turned out to be. A row saying the file references
        // `Demo.Region` under the name `var` is true and useless, and there is one per local in the tree.
        if (node is IdentifierNameSyntax { IsVar: true }) return;
        var symbol = CsSemantics.Bound(model, node);
        // A MEMBER ACCESS THE COMPILER COULD NOT BIND is a MAYBE, as an unbound call is: a row with `symbol = ''` and
        // `kind = 'unbound'`, found by the name it is written as. Left out, "who uses this property" undercounted with
        // nothing to say so - a large share on a large consumer, where a lambda's type could not be inferred.
        if (symbol is null)
        {
            if (node is MemberAccessExpressionSyntax && referenced.Add($"{Line(node)}:?{CsFacts.Dotted(node)}"))
            {
                Row("refs", "ref",
                    ("line", Line(node)),
                    ("name", CsFacts.Dotted(node) is { Length: > 0 } spelled ? spelled : CsFacts.Text(node)),
                    ("symbol", ""),
                    ("kind", "unbound"));
            }
            return;
        }
        if (!Referable.Contains(symbol.Kind)) return;
        if (!symbol.Locations.Any(location => location.IsInSource)) return;
        Reach(symbol);

        var line = Line(node);
        // A TYPE IS NAMED BY ITS DEFINITION - `Demo.Pipeline<TReq, TRes>` for `Pipeline<,>` and `Pipeline<int, string>`
        // alike - because that is what `classes.symbol` holds, so "who uses this type" is a join.
        var full = CsSemantics.Name(symbol is INamedTypeSymbol named ? named.OriginalDefinition : symbol);
        if (!referenced.Add($"{line}:{full}")) return;
        Row("refs", "ref",
            ("line", line),
            ("name", CsFacts.Dotted(node) is { Length: > 0 } written ? written : CsFacts.Text(node)),
            ("symbol", full),
            ("kind", symbol.Kind.ToString().ToLowerInvariant()));
    }

    /// <summary>
    /// A test, with what it reads. `if (Store.Ready)` is a read of `Store.Ready` and produces no expression
    /// row of its own — a bare member access is below the size a row is worth — so without this column the
    /// one branch in the tree that reads a flag would not answer a query about that flag.
    /// </summary>
    private string Branch(SyntaxNode node, string kind, SyntaxNode? tested)
    {
        var touched = CsFacts.Of(tested);
        return Row("branches", "br",
            ("line", Line(node)),
            ("kind", kind),
            ("test", CsFacts.Text(tested)),
            ("end_line", EndLine(node)),
            ("reads", touched.Reads),
            ("calls", touched.Calls));
    }

    /// <summary>
    /// A `try`, with WHAT IT CATCHES in the same column a python `try` puts its handler types in. An empty
    /// `catch` catches everything, and that is worth being able to find.
    /// </summary>
    private void Try(TryStatementSyntax node)
    {
        var caught = node.Catches.Select(c => c.Declaration is null
            ? "*"
            : CsFacts.Dotted(c.Declaration.Type) is { Length: > 0 } written ? written : c.Declaration.Type.ToString());
        var test = string.Join(", ", caught);
        foreach (var clause in node.Catches) Handler(node, clause);
        Row("branches", "br",
            ("line", Line(node)),
            ("kind", "try"),
            ("test", test),
            ("end_line", EndLine(node)),
            ("reads", new List<string>()),
            ("calls", new List<string>()));
    }

    /// <summary>
    /// What a body hands back. A row WITHOUT the columns is not the same row: the table takes its shape
    /// from the first rows written, so a bare `return;` in the first file would decide that `returns` has
    /// no `reads` column at all and every later row would lose it.
    /// </summary>
    private void Returns(SyntaxNode node, ExpressionSyntax? value)
    {
        var touched = CsFacts.Of(value);
        Row("returns", "r",
            ("line", Line(node)),
            ("source", CsFacts.Text(value)),
            ("reads", touched.Reads),
            ("calls", touched.Calls));
    }

    private void Raises(SyntaxNode node, ExpressionSyntax? thrown)
    {
        var touched = CsFacts.Of(thrown);
        // `throw;` rethrows whatever was caught, and naming it "" is the honest answer: the type is decided
        // by the `catch` above it, not here.
        var name = thrown is ObjectCreationExpressionSyntax creation ? CsFacts.Dotted(creation.Type)
            : CsFacts.Dotted(thrown);
        Row("raises", "rs",
            ("line", Line(node)),
            ("name", name),
            ("symbol", CsSemantics.Type(model, thrown)),
            ("source", CsFacts.Text(thrown)),
            ("reads", touched.Reads),
            ("calls", touched.Calls));
    }

    /// <summary>
    /// A field: an assignment row for what it is initialised to, and a CONST row when it is one.
    ///
    /// WHAT COUNTS AS A CONSTANT HERE is `const` and `static readonly` — the two ways C# writes the thing
    /// python writes as a module-level capital name. A `static readonly` holding a built value is exactly
    /// the row somebody is looking for when they ask where a path or a route prefix is decided.
    /// </summary>
    private void Field(FieldDeclarationSyntax field)
    {
        var constant = field.Modifiers.Any(m => m.IsKind(SyntaxKind.ConstKeyword))
            || (field.Modifiers.Any(m => m.IsKind(SyntaxKind.StaticKeyword))
                && field.Modifiers.Any(m => m.IsKind(SyntaxKind.ReadOnlyKeyword)));
        Variables(field.Declaration, "field");
        if (!constant) return;
        foreach (var declarator in field.Declaration.Variables)
        {
            var touched = CsFacts.Of(declarator.Initializer?.Value);
            var folded = CsFolding.Of(model, declarator.Initializer?.Value);
            Row("consts", "k",
                ("line", Line(declarator)),
                ("name", declarator.Identifier.ValueText),
                ("source", CsFacts.Text(declarator.Initializer?.Value)),
                // WHAT IT ACTUALLY HOLDS. `Prefix + "/root"` is two names and a slash in the text and one
                // string here — which is the difference between finding a path and reading an expression.
                ("value", folded.Text),
                ("value_kind", folded.Kind),
                ("reads", touched.Reads),
                ("calls", touched.Calls));
        }
    }

    private void EnumMember(EnumMemberDeclarationSyntax member)
    {
        var touched = CsFacts.Of(member.EqualsValue?.Value);

        var folded = CsFolding.Of(model, member.EqualsValue?.Value);
        Row("consts", "k",
            ("line", Line(member)),
            ("name", member.Identifier.ValueText),
            ("source", CsFacts.Text(member.EqualsValue?.Value)),
            ("value", folded.Text),
            ("value_kind", folded.Kind),
            ("reads", touched.Reads),
            ("calls", touched.Calls));
    }

    private void Variables(VariableDeclarationSyntax declaration, string kind)
    {
        foreach (var declarator in declaration.Variables)
        {
            if (declarator.Initializer is null) continue;
            Assignment(declarator, declarator.Identifier.ValueText, kind, declarator.Initializer.Value);
        }
    }

    private void Assigned(AssignmentExpressionSyntax node)
    {
        var target = CsFacts.Dotted(node.Left) is { Length: > 0 } written ? written : CsFacts.Text(node.Left);
        // `+=` and `=` are not the same statement: one REPLACES a value and the other reads it first, which
        // is the difference between a handler being registered and a handler list being thrown away.
        var kind = node.IsKind(SyntaxKind.SimpleAssignmentExpression) ? "assign" : "compound";
        Assignment(node, target, kind, node.Right);
    }

    /// <summary>
    /// One assignment, with WHAT THE RIGHT-HAND SIDE READS resolved here rather than left in `source` for a
    /// query to re-parse. A name bound by assignment — `var handler = Registry.Resolve;` — records a read
    /// of `Registry.Resolve`, so a name used only that way does not look unused.
    /// </summary>
    private void Assignment(SyntaxNode node, string target, string kind, ExpressionSyntax? value)
    {
        var touched = CsFacts.Of(value);
        var folded = CsFolding.Of(model, value);
        Row("assignments", "a",
            ("line", Line(node)),
            ("target", target),
            ("kind", kind),
            ("source", CsFacts.Text(value)),
            ("type", CsSemantics.Type(model, value)),
            ("const", folded.Text),
            ("const_kind", folded.Kind),
            ("symbol", folded.Symbol),
            ("reads", touched.Reads),
            ("calls", touched.Calls));
    }

    /// <summary>
    /// A call, by the name it is WRITTEN as. `new Reader()` is a call of `Reader`, the way a python class
    /// call is, so the two languages answer "who constructs this" in one query.
    ///
    /// `kwargs` counts the arguments passed BY NAME, which is what the python column counts too.
    /// </summary>
    private void Call(ExpressionSyntax node, SyntaxNode? callee, ArgumentListSyntax? arguments)
    {
        // `new(...)` names no type in the text at all; the type it resolves to is the callee a reader
        // means. Written name first, resolved name only where there is no written one.
        var name = CsFacts.Dotted(callee);
        // A RECEIVER THAT IS ITSELF A CALL cannot be written as a dotted name, and the whole chain's text is
        // not what anybody means by "the callee": `Get().Where(...)` calls `Where`. Falling back to the text
        // put five call sites of one file under a name nothing can join to.
        if (name.Length == 0 && callee is MemberAccessExpressionSyntax written)
            name = written.Name.Identifier.ValueText;
        if (name.Length == 0 && callee is null) name = CsSemantics.Type(model, node);
        // THE RAZOR GENERATOR'S OWN CALLS are not the author's - see CsRowsRazor.
        if (IsPlumbing(name)) return;
        // BOUND OFF THE CALL, not off the name: `Resolve` is a string, and which `Resolve` it is depends on
        // the receiver's type, the argument types and every `using` above it.
        var target = CsSemantics.Bound(model, node) ?? CsSemantics.Bound(model, callee ?? node);
        // `nameof(x)` IS NO CALL, and binds to no method: nearly all of one tree's unbound calls were it. What it
        // names is the fact worth a row - the member or type its operand binds to.
        if (target is null && name == "nameof" && arguments?.Arguments.Count == 1)
            target = CsSemantics.NameOf(model, arguments.Arguments[0].Expression);
        Reach(target);
        var call = Row("calls", "call",
            // ANCHORED AT THE NAME CALLED, not at the start of the expression. A chain written down the
            // page - `GetFiles(...)` newline `.Where(...)` newline `.ToList()` - starts on one line and
            // calls three methods on three others, and a row that reported the first for all of them puts
            // every call of the chain on a line that calls nothing.
            ("line", Line(callee is MemberAccessExpressionSyntax access ? access.Name : callee ?? node)),
            ("end_line", EndLine(node)),
            ("callee", name.Length > 0 ? name : CsFacts.Text(callee)),
            ("symbol", CsSemantics.Name(target)),
            ("signature", CsSemantics.Signature(target)),
            ("type", CsSemantics.Type(model, node)),
            ("args", arguments?.Arguments.Count ?? 0),
            ("kwargs", arguments?.Arguments.Count(a => a.NameColon is not null) ?? 0),
            ("source", CsFacts.Text(node)));
        Arguments(call, target, arguments);
        RegexCall(node, target, arguments);
    }

    /// <summary>
    /// ONE ROW PER ARGUMENT, CARRYING THE NAME THE CALLEE DECLARES.
    ///
    /// This is the row that only a bound tree can produce, and the reason the semantic pass is worth its
    /// cost: `Send(id, true, false)` is three values and no meaning, while the same call with the callee's
    /// signature in hand says which of them is `retry` and which is `silent`. The VALUE is carried both as
    /// written and, where the compiler can fold it, as what it actually is.
    ///
    /// NOT WRITTEN WITHOUT A MODEL. Positions alone would be a table of numbers, and a `name` filled in by
    /// counting would be the guess this whole tool refuses to make.
    /// </summary>
    private void Arguments(string call, ISymbol? target, ArgumentListSyntax? arguments)
    {
        if (model is null || arguments is null) return;
        var position = 0;
        foreach (var argument in arguments.Arguments)
        {
            var value = CsFolding.Of(model, argument.Expression);
            Row("arguments", "arg",
                ("line", Line(argument)),
                ("call", call),
                ("object", ""),
                ("position", position),
                ("name", CsSemantics.Parameter(target, argument, position++)),
                ("source", CsFacts.Text(argument.Expression)),
                ("const", value.Text),
                // WHO WORKED THE VALUE OUT, and what it binds to when nothing could: `const` is the
                // compiler's, `folded` is this pass following the operands, `enum` is a member name, and
                // `symbol` says the value is unknown but the name it comes from is not.
                ("const_kind", value.Kind),
                ("symbol", value.Symbol),
                ("type", CsSemantics.Type(model, argument.Expression)),
                // A STRING THAT DID NOT FOLD WHOLE, as its pieces - what the SQL links fill and parse.
                ("template", CsTemplate.Of(model, argument.Expression, value)));
        }
    }

    /// <summary>
    /// One expression worth a row of its own, with its text, its shape and what it touches.
    ///
    /// A TYPE IS NOT AN EXPRESSION HERE. Roslyn models `List&lt;string&gt;` as one, so the table would fill
    /// with rows for type names that say nothing about what the code does — and `shape`, which exists to
    /// group expressions that are built the same way, would group declarations by their type syntax.
    /// </summary>
    private void Expression(ExpressionSyntax node)
    {
        if (node is TypeSyntax or LiteralExpressionSyntax) return;
        // A NAME WRITTEN OUT IS NOT AN EXPRESSION. `config.Store.Path` is three nodes of tree and no
        // decision at all, and it is already carried, whole and in its prefixes, in the `reads` of whatever
        // statement it sits in. Rows for it would be the largest group in the table and the least useful.
        if (node is MemberAccessExpressionSyntax && CsFacts.Dotted(node).Length > 0) return;
        var size = node.DescendantNodesAndSelf().Count();
        if (size < MinExpressionNodes) return;
        var source = CsFacts.Text(node);
        if (source.Length == 0) return;
        var touched = CsFacts.Of(node);
        Row("expressions", "x",
            ("line", Line(node)),
            ("role", node.Kind().ToString()),
            ("source", source),
            // THE STATIC TYPE, which is what `var` hides and what a query about a type cannot do without.
            ("type", CsSemantics.Type(model, node)),
            ("size", size),
            ("shape", CsFacts.Shape(node)),
            ("reads", touched.Reads),
            ("calls", touched.Calls),
            ("strings", touched.Strings));
    }
}
