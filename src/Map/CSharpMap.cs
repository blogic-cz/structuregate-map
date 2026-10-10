using System.Security.Cryptography;
using System.Text;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using Microsoft.CodeAnalysis.CSharp.Syntax;

namespace StructureGate;

/// <summary>
/// THE C# HALF OF THE MAP, in this process and off the same Roslyn parse the line rule already uses.
///
/// WHAT AN EDGE IS HERE, and why it is not the `using` list. A `using` names a NAMESPACE, and a namespace is
/// usually every file in the project — in this repo it is exactly one namespace, so a graph drawn from
/// `using` lines would say either "everything imports everything" or, because same-namespace access needs no
/// `using` at all, "nothing imports anything". Both are useless. So an edge is a TYPE REFERENCE: a simple
/// name - generic ones included, `Pipeline<,>` - that exactly matches a type another file declares.
///
/// THE TWO BLIND SPOTS, stated because an unqualified edge gets acted on. A name declared in two files is
/// AMBIGUOUS and yields no edge (reported instead) — this file cannot tell which one a use binds to without
/// a compilation, and a guessed edge is worse than a missing one. And a local or a member that happens to
/// carry a type's exact name produces a false edge; C# naming conventions make that rare, and the member
/// half of it is already excluded — `x.Foo` never counts `Foo`.
/// </summary>
public static class CSharpMap
{
    /// <summary>A body this short is not a duplication worth reporting: two wrappers that both return the
    /// same field are alike by coincidence. A repeated body starts carrying a DECISION someone would have to
    /// remember to change in both places at around three statements.</summary>
    private const int MinBodyStatements = 3;

    /// <summary>An expression smaller than this is shared vocabulary, not a copy. Counted in TOKENS so a
    /// long variable name cannot make a trivial expression look substantial.</summary>
    private const int MinExpressionTokens = 20;

    /// <summary>A summary line is a headline, not the paragraph under it.</summary>
    private const int MaxSummary = 160;

    /// <summary>
    /// THE CALLS THAT NAME A TYPE WITHOUT REFERENCING IT. Everything else in this half is an identifier the
    /// compiler would bind, so a type reached only through one of these is invisible to the identifier walk
    /// and looks unreferenced - which is the one C# blind spot the other three halves did not have.
    ///
    /// The qualifier is checked as well as the method, and both are STATIC entry points on purpose: `GetType`
    /// on its own is the instance method every object has, and counting that would put an edge on every file
    /// in the tree.
    /// </summary>
    private static readonly (string Owner, string Method)[] Reflective =
    [
        ("Type", "GetType"),
        ("Assembly", "Load"), ("Assembly", "LoadFrom"), ("Assembly", "LoadFile"),
        ("Activator", "CreateInstance"),
        ("AppDomain", "CreateInstanceAndUnwrap"),
    ];

    public static void Read(MapFile file, SyntaxTree tree, MapCollector into)
    {
        var root = (CompilationUnitSyntax)tree.GetRoot();

        // A PARSE ERROR IS NOT A SKIP. Roslyn error-recovers and hands back a PARTIAL tree, so every edge
        // below silently stops covering the rest of the file — the failure this whole tool is built to
        // avoid. Named, and counted as a state that cannot be legitimate.
        foreach (var diagnostic in tree.GetDiagnostics().Where(d => d.Severity == DiagnosticSeverity.Error).Take(3))
        {
            var at = tree.GetLineSpan(diagnostic.Location.SourceSpan).StartLinePosition.Line + 1;
            into.Errors.Add($"UNPARSED  {file.Rel}:{at}: does not parse as C# ({diagnostic.Id}: "
                + $"{diagnostic.GetMessage()}) — every edge read out of this file is from a PARTIAL tree");
        }

        file.Generated = CsFacts.Generated(root, file.Rel);
        file.Summary = Summary(root);
        file.Entry = root.Members.OfType<GlobalStatementSyntax>().Any();

        foreach (var node in root.DescendantNodes())
        {
            switch (node)
            {
                case BaseTypeDeclarationSyntax type:
                    file.Declares.Add(type.Identifier.ValueText);
                    if (type is ClassDeclarationSyntax activated) Registered(file, activated);
                    break;
                case DelegateDeclarationSyntax handler:
                    file.Declares.Add(handler.Identifier.ValueText);
                    break;
                case MethodDeclarationSyntax { Identifier.ValueText: "Main" }:
                    file.Entry = true;
                    break;
            }

            // A GENERIC NAME TOO: a type reached only through `typeof(Pipeline<,>)` or a field of `Holder<int>` read as
            // unused while a bare identifier was the only use.
            if (node is SimpleNameSyntax name && !IsMemberName(name) && !InUsing(name))
                file.Uses.Add(name.Identifier.ValueText);
        }

        Reflection(file, tree, root, into);

        // A GENERATED FILE IS JOINED, NEVER FINGERPRINTED. What it declares and uses is real — hand-written
        // code calls into an NSwag client, and an edge to it is an edge — so the graph keeps it. Its BODIES
        // are another matter: every EF model snapshot resembles every other one, so the duplicate detector
        // reports groups nobody can act on, and fingerprinting them is most of the time this pass spends on
        // a real solution.
        if (file.Generated) return;
        Bodies(file.Rel, tree, root, into);
        Expressions(file.Rel, tree, root, into);
    }

    /// <summary>The bases a framework discovers its classes by, and the attributes that mark one.</summary>
    private static readonly string[] FrameworkBases = ["ControllerBase", "Controller", "PageModel", "Hub"];
    private static readonly string[] FrameworkAttributes = ["ApiController", "Controller", "TestClass", "TestFixture"];
    /// <summary>The attributes a test framework finds a test METHOD by - MSTest, NUnit, xUnit. A class holding one is run,
    /// though nothing names it: NUnit and xUnit need no attribute on the class at all.</summary>
    private static readonly string[] TestAttributes = ["TestMethod", "DataTestMethod", "Test", "TestCase", "TestCaseSource", "Fact", "Theory"];

    /// <summary>
    /// A CLASS A FRAMEWORK ACTIVATES BY CONVENTION, read off the syntax - a test framework's too: MVC finds a controller by its `Controller`
    /// suffix - which is why `ThingsController : BaseController : ControllerBase` is found without resolving the
    /// chain - or by `[ApiController]`/`[Controller]`, and a Razor page model or a SignalR hub by its base. Nothing in
    /// the tree names one, so every controller read as NO READER. An ABSTRACT class is activated by nothing.
    /// </summary>
    private static void Registered(MapFile file, ClassDeclarationSyntax type)
    {
        if (type.Modifiers.Any(SyntaxKind.AbstractKeyword)) return;
        if (type.Identifier.ValueText.EndsWith("Controller", StringComparison.Ordinal)) file.Registered.Add("Controller");
        foreach (var listed in type.BaseList?.Types ?? default)
        {
            var named = listed.Type switch
            {
                SimpleNameSyntax simple => simple.Identifier.ValueText,
                QualifiedNameSyntax qualified => qualified.Right.Identifier.ValueText,
                _ => "",
            };
            if (FrameworkBases.Contains(named)) file.Registered.Add(named);
        }
        foreach (var named in AttributeNames(type.AttributeLists))
        {
            if (FrameworkAttributes.Contains(named)) file.Registered.Add(named);
        }
        foreach (var method in type.Members.OfType<MethodDeclarationSyntax>())
        {
            foreach (var named in AttributeNames(method.AttributeLists))
            {
                if (TestAttributes.Contains(named)) file.Registered.Add(named);
            }
        }
    }

    /// <summary>Each attribute's simple name, without its `Attribute` suffix: `[Fact]`, `[Xunit.Fact]`, `[FactAttribute]`.</summary>
    private static IEnumerable<string> AttributeNames(SyntaxList<AttributeListSyntax> lists)
    {
        foreach (var attribute in lists.SelectMany(list => list.Attributes))
        {
            var named = attribute.Name switch
            {
                SimpleNameSyntax simple => simple.Identifier.ValueText,
                QualifiedNameSyntax qualified => qualified.Right.Identifier.ValueText,
                _ => "",
            };
            yield return named.EndsWith("Attribute", StringComparison.Ordinal) ? named[..^"Attribute".Length] : named;
        }
    }

    /// <summary>
    /// The types named through REFLECTION rather than referenced.
    ///
    /// A LITERAL NAME IS A REAL EDGE - `Type.GetType("Reader")` reaches the same file `new Reader()` would,
    /// and the join below treats it the same way, ambiguity rule included. A name built at RUN TIME is not
    /// guessed at: it is COUNTED, because it is exactly the reason a type nothing references may still be
    /// used, and a blind spot nobody is told about gets acted on as if it were not there.
    ///
    /// An argument that is already a type reference - `typeof(X)`, `nameof(X)` - is neither: the identifier
    /// walk has drawn that edge, and calling it computed would inflate the count that qualifies the
    /// dead-file finding.
    /// </summary>
    private static void Reflection(MapFile file, SyntaxTree tree, SyntaxNode root, MapCollector into)
    {
        foreach (var call in root.DescendantNodes().OfType<InvocationExpressionSyntax>())
        {
            if (call.Expression is not MemberAccessExpressionSyntax access) continue;
            // The receiver may be WRITTEN OUT: `System.Type.GetType` is a member access whose own receiver
            // is another one, so taking only a bare identifier missed every fully qualified call - which is
            // how most reflection is actually written.
            var owner = access.Expression switch
            {
                IdentifierNameSyntax bare => bare.Identifier.ValueText,
                MemberAccessExpressionSyntax qualified => qualified.Name.Identifier.ValueText,
                _ => "",
            };
            var method = access.Name.Identifier.ValueText;
            if (!Reflective.Any(r => r.Owner == owner && r.Method == method)) continue;

            var argument = call.ArgumentList.Arguments.FirstOrDefault()?.Expression;
            if (argument is null or TypeOfExpressionSyntax) continue;
            if (argument is InvocationExpressionSyntax { Expression: IdentifierNameSyntax { Identifier.ValueText: "nameof" } })
                continue;

            if (argument is LiteralExpressionSyntax { Token.Value: string named })
            {
                var simple = SimpleName(named);
                if (simple.Length > 0) file.Uses.Add(simple);
                continue;
            }
            into.Computed.Add(new ComputedImport(file.Rel, Line(tree, call).ToString(),
                $"{owner}.{method}() with a name built at run time"));
        }
    }

    /// <summary>
    /// The type name inside a reflection string. Three things are stripped, because a runtime type name is
    /// not a source one: the assembly that follows a comma (`N.T, Asm`), the namespace in front of it, and
    /// the arity a generic carries (`List`1`). What is left is what a file DECLARES.
    /// </summary>
    private static string SimpleName(string runtimeName)
    {
        var name = runtimeName;
        var comma = name.IndexOf(',');
        if (comma >= 0) name = name[..comma];
        var tick = name.IndexOf('`');
        if (tick >= 0) name = name[..tick];
        var dot = name.LastIndexOf('.');
        if (dot >= 0) name = name[(dot + 1)..];
        var nested = name.LastIndexOf('+');
        if (nested >= 0) name = name[(nested + 1)..];
        return name.Trim();
    }

    /// <summary>
    /// The file's headline: the first `&lt;summary&gt;` on the first member that carries one. A C# file's
    /// intent is written there the way a python module's is written in its docstring, so the map reads the
    /// same thing from both and a reader can scan one list.
    /// </summary>
    private static string Summary(CompilationUnitSyntax root)
    {
        foreach (var member in root.DescendantNodes().OfType<MemberDeclarationSyntax>())
        {
            foreach (var trivia in member.GetLeadingTrivia())
            {
                if (trivia.GetStructure() is not DocumentationCommentTriviaSyntax doc) continue;
                foreach (var element in doc.Content.OfType<XmlElementSyntax>())
                {
                    if (element.StartTag.Name.LocalName.ValueText != "summary") continue;
                    var line = FirstLine(element.Content.ToFullString());
                    if (line.Length > 0) return line;
                }
            }
        }
        return "";
    }

    /// <summary>The first line with words on it, with the `///` exterior the raw text still carries removed.
    /// Read off the tree rather than matched for, so a `///` inside the prose is not mistaken for one.</summary>
    private static string FirstLine(string content)
    {
        foreach (var raw in content.Split('\n'))
        {
            var line = raw.Trim();
            if (line.StartsWith("///", StringComparison.Ordinal)) line = line[3..].Trim();
            if (line.Length == 0) continue;
            return line.Length > MaxSummary ? line[..MaxSummary] + " …" : line;
        }
        return "";
    }

    /// <summary>The `Foo` in `x.Foo` and in `A.B.Foo` — a MEMBER, never a file another file imports.</summary>
    private static bool IsMemberName(SimpleNameSyntax name) => name.Parent switch
    {
        MemberAccessExpressionSyntax access => access.Name == name,
        QualifiedNameSyntax qualified => qualified.Right == name,
        MemberBindingExpressionSyntax binding => binding.Name == name,
        NameColonSyntax or NameEqualsSyntax => true,
        _ => false,
    };

    /// <summary>A `using` directive's segments are namespace parts. Counting one that happens to spell a
    /// type's name would draw an edge from an import list to an unrelated file.</summary>
    private static bool InUsing(SyntaxNode node)
    {
        for (var parent = node.Parent; parent is not null; parent = parent.Parent)
        {
            if (parent is UsingDirectiveSyntax) return true;
            if (parent is MemberDeclarationSyntax) return false;
        }
        return false;
    }

    private static void Bodies(string rel, SyntaxTree tree, SyntaxNode root, MapCollector into)
    {
        foreach (var node in root.DescendantNodes())
        {
            var (body, name) = node switch
            {
                MethodDeclarationSyntax method => (method.Body, method.Identifier.ValueText),
                ConstructorDeclarationSyntax ctor => (ctor.Body, ctor.Identifier.ValueText),
                LocalFunctionStatementSyntax local => (local.Body, local.Identifier.ValueText),
                AccessorDeclarationSyntax accessor => (accessor.Body, AccessorName(accessor)),
                _ => (null, ""),
            };
            if (body is null || body.Statements.Count < MinBodyStatements) continue;
            var (digest, _) = Fingerprint(body);
            Record(into.Bodies, digest, $"{rel}:{Line(tree, body)}:{name}", body.Span.Length);
        }
    }

    private static string AccessorName(AccessorDeclarationSyntax accessor)
    {
        var owner = accessor.Parent?.Parent switch
        {
            PropertyDeclarationSyntax property => property.Identifier.ValueText,
            IndexerDeclarationSyntax => "this[]",
            EventDeclarationSyntax handler => handler.Identifier.ValueText,
            _ => "?",
        };
        return $"{owner}.{accessor.Keyword.ValueText}";
    }

    private static void Expressions(string rel, SyntaxTree tree, SyntaxNode root, MapCollector into)
    {
        foreach (var node in root.DescendantNodes().OfType<ExpressionSyntax>())
        {
            var (digest, tokens) = Fingerprint(node);
            if (tokens < MinExpressionTokens) continue;
            // TOKENS DECIDE WHETHER TO REPORT IT, CHARACTERS SAY HOW BIG IT IS. A token count is the right
            // filter — a long variable name cannot make a trivial expression look substantial — but it is
            // not comparable with what the other three halves count, and the groups are ranked in ONE list.
            // A source span is the same unit in every language.
            Record(into.Expressions, digest, $"{rel}:{Line(tree, node)}", node.Span.Length);
        }
    }

    private static void Record(Dictionary<string, List<(string Where, int Size)>> found,
        string digest, string where, int size)
    {
        if (!found.TryGetValue(digest, out var list)) found[digest] = list = [];
        list.Add((where, size));
    }

    private static int Line(SyntaxTree tree, SyntaxNode node) =>
        tree.GetLineSpan(node.Span).StartLinePosition.Line + 1;

    /// <summary>
    /// The shape of a piece of code, with LOCAL NAMES BLANKED so two spellings of one idiom fingerprint
    /// alike. Without that the detector misses what it is for: the same helper pasted into several files under
    /// different variable names lands in groups of one and the scan reports nothing.
    ///
    /// MEMBER NAMES ARE KEPT. `string.Normalize` is what makes that expression that expression, while
    /// whether its input is called `s` or `word` is not. Comments never enter: this walks TOKENS, and a
    /// comment is trivia — so two functions match when they DO the same thing however differently they are
    /// described, which is exactly what a copied body looks like.
    /// </summary>
    private static (string Digest, int Size) Fingerprint(SyntaxNode node)
    {
        using var hash = IncrementalHash.CreateHash(HashAlgorithmName.SHA256);
        var tokens = 0;
        foreach (var token in node.DescendantTokens())
        {
            tokens++;
            var text = token.IsKind(SyntaxKind.IdentifierToken)
                && !(token.Parent is SimpleNameSyntax simple && IsMemberName(simple))
                ? "_"
                : token.ValueText;
            hash.AppendData(Encoding.UTF8.GetBytes($"{token.RawKind}:{text}"));
        }
        return (Convert.ToHexString(hash.GetHashAndReset())[..16].ToLowerInvariant(), tokens);
    }
}
