using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using Microsoft.CodeAnalysis.CSharp.Syntax;

namespace StructureGate;

/// <summary>
/// ASYNC DISCIPLINE (`--async-discipline`), a fourth rule for C#: inside code that already is <c>async</c>,
/// nothing may block, and no call may use a synchronous API that has an awaitable twin.
///
/// Why a gate and not a review note: sync-over-async does not fail, it stalls. A single <c>.Result</c> on a
/// watcher's tick delays every unit of work queued behind it, and a <c>Thread.Sleep</c> inside an async method
/// holds a pool thread for the duration. Both read as ordinary code and neither shows up in a test.
///
/// SYNTAX ONLY, deliberately. This gate has the Roslyn parser and no compilation, so the rule keys on shapes
/// that are unambiguous in source (`.GetAwaiter().GetResult()`, `Task.WaitAll`, `File.ReadAllText` inside an
/// async body) and stays silent where only a symbol table could decide. A rule that guesses gets switched off.
///
/// OPT OUT WITH A REASON: a line carrying <c>// async-ok</c> (or the line above it) is skipped. The blocking
/// call that genuinely cannot be awaited exists; the comment is where its reason lives.
/// </summary>
public static class AsyncRules
{
    private const string Waiver = "async-ok";

    /// <summary>Sync APIs whose awaitable twin is the one to use inside an async body. Name-based on purpose:
    /// the receiver is not resolvable without a compilation, and these names are not overloaded across the BCL
    /// in a way that would make an await version wrong.</summary>
    private static readonly Dictionary<string, string> SyncWithAsyncTwin = new(StringComparer.Ordinal)
    {
        ["ReadAllText"] = "ReadAllTextAsync",
        ["WriteAllText"] = "WriteAllTextAsync",
        ["AppendAllText"] = "AppendAllTextAsync",
        ["ReadAllLines"] = "ReadAllLinesAsync",
        ["WriteAllLines"] = "WriteAllLinesAsync",
        ["ReadAllBytes"] = "ReadAllBytesAsync",
        ["WriteAllBytes"] = "WriteAllBytesAsync",
        ["ReadToEnd"] = "ReadToEndAsync",
        ["ReadLine"] = "ReadLineAsync",
        ["WaitForExit"] = "WaitForExitAsync",
        ["CopyTo"] = "CopyToAsync",
        ["FlushAsync"] = "FlushAsync",
    };

    public static IEnumerable<string> Inspect(string rel, string text)
    {
        var tree = CSharpSyntaxTree.ParseText(text);
        var root = tree.GetRoot();
        var problems = new List<string>();

        foreach (var node in root.DescendantNodes())
        {
            switch (node)
            {
                case InvocationExpressionSyntax invocation:
                    Invocation(problems, tree, rel, invocation);
                    break;
                case MemberAccessExpressionSyntax access when access.Name.Identifier.ValueText == "Result":
                    BlockingResult(problems, tree, rel, access);
                    break;
                case MethodDeclarationSyntax method:
                    AsyncVoid(problems, tree, rel, method);
                    break;
            }
        }

        return problems;
    }

    private static void Invocation(List<string> problems, SyntaxTree tree, string rel, InvocationExpressionSyntax invocation)
    {
        if (invocation.Expression is not MemberAccessExpressionSyntax access) return;
        var name = access.Name.Identifier.ValueText;
        var receiver = access.Expression.ToString();

        // `.GetAwaiter().GetResult()` is sync-over-async wherever it appears — including a sync method, which is
        // exactly where it hides a call chain that should have been async to the top.
        if (name == "GetResult" && receiver.EndsWith("GetAwaiter()", StringComparison.Ordinal))
        {
            Add(problems, tree, rel, invocation, "blocks on a task with .GetAwaiter().GetResult()",
                "make the caller async and await it");
            return;
        }

        if (name is "WaitAll" or "WaitAny" && receiver is "Task" or "System.Threading.Tasks.Task")
        {
            Add(problems, tree, rel, invocation, $"blocks on Task.{name}", $"await Task.When{name[4..]} instead");
            return;
        }

        if (!InAsyncBody(invocation)) return;

        if (name == "Wait" && invocation.ArgumentList.Arguments.Count == 0)
        {
            Add(problems, tree, rel, invocation, "blocks on .Wait() inside an async body", "await the task instead");
            return;
        }

        if (name == "Sleep" && receiver.EndsWith("Thread", StringComparison.Ordinal))
        {
            Add(problems, tree, rel, invocation, "Thread.Sleep inside an async body holds a pool thread",
                "await Task.Delay instead");
            return;
        }

        // A no-argument WaitForExit is the blocking one; WaitForExit(int) is a bounded wait and has no direct
        // awaitable twin, so it is left alone.
        if (name == "WaitForExit" && invocation.ArgumentList.Arguments.Count > 0) return;

        if (SyncWithAsyncTwin.TryGetValue(name, out var twin) && twin != name)
        {
            Add(problems, tree, rel, invocation, $"synchronous {name}() inside an async body",
                $"await {twin}() instead");
        }
    }

    /// <summary>
    /// <c>Foo(…).Result</c> and <c>task.Result</c>. Only shapes that cannot be anything else are flagged: the
    /// property of a task-returning CALL, or of something named like a task. A bare <c>x.Result</c> where <c>x</c>
    /// is a record with a Result member is not this rule's business.
    /// </summary>
    private static void BlockingResult(List<string> problems, SyntaxTree tree, string rel, MemberAccessExpressionSyntax access)
    {
        if (access.Parent is InvocationExpressionSyntax) return;      // a method called Result, not the property
        var receiver = access.Expression;
        var text = receiver.ToString();
        var taskShaped = receiver is InvocationExpressionSyntax invocation
                         && (invocation.Expression.ToString().EndsWith("Async", StringComparison.Ordinal)
                             || invocation.Expression.ToString().Contains("Task.", StringComparison.Ordinal))
                         || text.EndsWith("Task", StringComparison.Ordinal)
                         || text.EndsWith("task", StringComparison.Ordinal);
        if (!taskShaped) return;

        Add(problems, tree, rel, access, "blocks on a task with .Result", "make the caller async and await it");
    }

    /// <summary>
    /// <c>async void</c> cannot be awaited and its exceptions cross no boundary — they reach the thread's
    /// unhandled handler and take the process down. The ONE legitimate shape is an event handler, whose signature
    /// the framework fixes, so that shape is allowed and everything else is not.
    /// </summary>
    private static void AsyncVoid(List<string> problems, SyntaxTree tree, string rel, MethodDeclarationSyntax method)
    {
        if (!method.Modifiers.Any(SyntaxKind.AsyncKeyword)) return;
        if (method.ReturnType is not PredefinedTypeSyntax { Keyword.RawKind: (int)SyntaxKind.VoidKeyword }) return;
        if (IsEventHandlerShape(method)) return;

        Add(problems, tree, rel, method.Identifier, $"async void {method.Identifier.ValueText}(): "
            + "the caller cannot await it and its exceptions escape to the thread",
            "return Task, or keep async void only for an (object sender, EventArgs e) handler");
    }

    private static bool IsEventHandlerShape(MethodDeclarationSyntax method)
    {
        var parameters = method.ParameterList.Parameters;
        if (parameters.Count != 2) return false;
        var sender = parameters[0].Type?.ToString().TrimEnd('?');
        var args = parameters[1].Type?.ToString() ?? string.Empty;
        return sender is "object" && (args.EndsWith("EventArgs", StringComparison.Ordinal)
                                      || args.EndsWith("EventArgs?", StringComparison.Ordinal));
    }

    /// <summary>
    /// Is this call lexically inside something declared async? The nearest enclosing function wins, so a blocking
    /// call inside a sync local function or a sync lambda nested in an async method is NOT reported — that
    /// function is where the author chose to be synchronous.
    /// </summary>
    private static bool InAsyncBody(SyntaxNode node)
    {
        foreach (var ancestor in node.Ancestors())
        {
            switch (ancestor)
            {
                case MethodDeclarationSyntax method:
                    return method.Modifiers.Any(SyntaxKind.AsyncKeyword);
                case LocalFunctionStatementSyntax local:
                    return local.Modifiers.Any(SyntaxKind.AsyncKeyword);
                case AnonymousFunctionExpressionSyntax lambda:
                    return lambda.AsyncKeyword.RawKind == (int)SyntaxKind.AsyncKeyword;
                case AccessorDeclarationSyntax accessor:
                    return accessor.Modifiers.Any(SyntaxKind.AsyncKeyword);
                case ConstructorDeclarationSyntax or DestructorDeclarationSyntax:
                    return false;
            }
        }
        return false;
    }

    private static void Add(List<string> problems, SyntaxTree tree, string rel, SyntaxNode node, string what, string remedy) =>
        Add(problems, tree, rel, node.GetLocation(), node.ToString(), what, remedy);

    private static void Add(List<string> problems, SyntaxTree tree, string rel, SyntaxToken token, string what, string remedy) =>
        Add(problems, tree, rel, token.GetLocation(), token.ValueText, what, remedy);

    private static void Add(List<string> problems, SyntaxTree tree, string rel, Location location, string snippet, string what, string remedy)
    {
        var line = location.GetLineSpan().StartLinePosition.Line;
        if (Waived(tree, line)) return;
        var first = snippet.Split('\n')[0].Trim();
        if (first.Length > 60) first = first[..60] + " …";
        problems.Add($"{rel}:{line + 1}: {what} — {remedy} (`{first}`)");
    }

    /// <summary>A <c>// async-ok</c> on the line itself or the line above waives it. Line-based rather than
    /// trivia-based so the comment can sit on either, which is how people actually write them.</summary>
    private static bool Waived(SyntaxTree tree, int line)
    {
        var lines = tree.GetText().Lines;
        for (var candidate = Math.Max(0, line - 1); candidate <= line && candidate < lines.Count; candidate++)
        {
            if (lines[candidate].ToString().Contains(Waiver, StringComparison.Ordinal)) return true;
        }
        return false;
    }
}
