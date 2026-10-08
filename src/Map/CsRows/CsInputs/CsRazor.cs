using System.Text;
using System.Xml.Linq;
using Microsoft.AspNetCore.Razor.Language;
using Microsoft.AspNetCore.Razor.Language.Intermediate;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;

namespace StructureGate;

/// <summary>
/// `.razor` AND `.cshtml`, turned into the C# the build compiles - by the RAZOR COMPILER ITSELF
/// (`Microsoft.AspNetCore.Razor.Language`), in this process. A source generator cannot be loaded by a NativeAOT
/// exe; its engine can be called.
///
/// THE GENERATED C# CARRIES `#line`: every expression written in markup maps back to its `.razor`/`.cshtml`
/// line, and the plumbing between them is `#line hidden`. So the tree is compiled with the project and WALKED
/// as that file - its calls bind, its rows name the markup line (`CsRows` reads the mapped span and skips
/// hidden code). A component that C# names (`Layout.Width`) binds for the same reason.
///
/// COMPONENTS IN TWO PASSES, as the SDK generator does: declarations first, compiled with the project so the
/// components can be DISCOVERED as tag helpers, then the full generation with them - without that pass
/// `&lt;Layout /&gt;` is markup, not a component, and `renders` would be empty.
///
/// A `.cshtml` VIEW takes its base type and namespaces from the MVC `Web.config` beside it
/// (`&lt;pages pageBaseType&gt;` and `&lt;namespaces&gt;`) - what RazorEngine and MVC 5 read at run time and a parse
/// tree cannot. They are appended as directives at the END of the text, so no line moves.
/// </summary>
internal static class CsRazor
{
    public sealed record Output(string Abs, string Kind, SyntaxTree Tree, List<Render> Renders);

    /// <summary>A component a component renders: `&lt;Tag Attr="..."&gt;`, at its markup line.</summary>
    public sealed record Render(int Line, string Component, string Tag, List<string> Attributes);

    /// <summary>What Razor reads as IMPORTS, not classes. Only these: a partial named `_Header.cshtml` is a view.</summary>
    private static readonly string[] Imports = ["_Imports.razor", "_ViewImports.cshtml", "_ViewStart.cshtml"];

    public static bool Owns(string path) =>
        path.EndsWith(".razor", StringComparison.OrdinalIgnoreCase) || path.EndsWith(".cshtml", StringComparison.OrdinalIgnoreCase);

    /// <summary>What discovery had to leave out since the last batch - notes, drained by `DeepMap.Batch`.</summary>
    public static readonly List<string> Skipped = [];

    /// <summary>The generated tree of every `.razor` and `.cshtml` the project owns; empty when it has none.</summary>
    public static List<Output> Generate(string csproj, string name, IReadOnlyList<SyntaxTree> parsed,
        IReadOnlyList<MetadataReference> references, CSharpParseOptions options)
    {
        var folder = Path.GetDirectoryName(csproj)!;
        var files = Files(folder, csproj);
        var outputs = new List<Output>();
        if (files.Count == 0) return outputs;
        var root = RootNamespace(csproj, name);
        // WHAT AN EARLIER RUN GENERATED FROM EXACTLY THESE INPUTS - see CsRazorCache.
        var key = CsRazorCache.Key(folder, root, files, parsed, references, options);
        if (CsRazorCache.Read(folder, key, options) is { } kept) return kept;
        var system = RazorProjectFileSystem.Create(folder);
        var skippedBefore = Skipped.Count;

        var components = files.Where(f => f.EndsWith(".razor", StringComparison.OrdinalIgnoreCase)).ToList();
        if (components.Count > 0)
        {
            var declaring = RazorProjectEngine.Create(RazorConfiguration.Default, system, b =>
            {
                b.SetRootNamespace(root);
                Microsoft.CodeAnalysis.Razor.CompilerFeatures.Register(b);
            });
            var declarations = components
                .Select(f => Tree(declaring.ProcessDeclarationOnly(Item(system, folder, f, null)), f + ".declaration.g.cs", options))
                .Where(t => t is not null).Select(t => t!).ToList();
            var discovery = CSharpCompilation.Create(name + ".razor", parsed.Concat(declarations), references,
                new CSharpCompilationOptions(OutputKind.DynamicallyLinkedLibrary, allowUnsafe: true));
            var context = TagHelperDescriptorProviderContext.Create();
            Microsoft.CodeAnalysis.Razor.TagHelperDescriptorProviderContextExtensions.SetCompilation(context, discovery);
            foreach (var provider in declaring.Engine.Features.OfType<ITagHelperDescriptorProvider>().OrderBy(p => p.Order))
            {
                // ONE PROVIDER THAT THROWS COSTS ITS OWN TAG HELPERS, never the project's components: Razor 6's
                // `EventHandlerTagHelperDescriptorProvider` dereferenced a null reading the `[EventHandler]` attributes
                // of .NET 10's Components on Windows, and every file of the project went UNPARSED with it.
                // `STRUCTUREGATE_TEST_THROW_PROVIDER` names one to throw, for the black-box case.
                try
                {
                    if (provider.GetType().Name == Environment.GetEnvironmentVariable("STRUCTUREGATE_TEST_THROW_PROVIDER"))
                        throw new NullReferenceException("thrown by STRUCTUREGATE_TEST_THROW_PROVIDER");
                    provider.Execute(context);
                }
                catch (Exception e) when (e is not OutOfMemoryException)
                {
                    Skipped.Add($"razor {name}: the {provider.GetType().Name} tag-helper discovery threw {DeepMap.Thrown(e)} "
                        + "- the tag helpers it finds are left out, so their attributes read as plain markup");
                }
            }
            var helpers = context.Results.ToList();

            var generating = RazorProjectEngine.Create(RazorConfiguration.Default, system, b =>
            {
                b.SetRootNamespace(root);
                Microsoft.CodeAnalysis.Razor.CompilerFeatures.Register(b);
                b.Features.Add(new Helpers(helpers));
            });
            foreach (var file in components)
            {
                var document = generating.Process(Item(system, folder, file, null));
                var tree = Tree(document, file + ".g.cs", options);
                if (tree is null) continue;
                var renders = new Renders();
                renders.Visit(document.GetDocumentIntermediateNode());
                outputs.Add(new Output(file, "component", tree, renders.Found));
            }
        }

        var views = files.Where(f => f.EndsWith(".cshtml", StringComparison.OrdinalIgnoreCase)).ToList();
        if (views.Count > 0)
        {
            var engine = RazorProjectEngine.Create(RazorConfiguration.Default, system, b =>
            {
                b.SetRootNamespace(root);
                Microsoft.AspNetCore.Mvc.Razor.Extensions.RazorExtensions.Register(b);
                Microsoft.CodeAnalysis.Razor.CompilerFeatures.Register(b);
            });
            foreach (var file in views)
            {
                var tree = Tree(engine.Process(Item(system, folder, file, Appended(folder, file))), file + ".g.cs", options);
                if (tree is not null) outputs.Add(new Output(file, "view", tree, []));
            }
        }
        // A GENERATION MISSING A PROVIDER'S TAG HELPERS IS NOT KEPT: answered from the cache, the next run would repeat
        // it without a word, and a fixed reference set would never get them back.
        if (Skipped.Count == skippedBefore) CsRazorCache.Write(folder, key, outputs);
        return outputs;
    }

    /// <summary>The `.razor`/`.cshtml` a project owns - not its build folders, not a nested project's, and
    /// not the `_Imports`/`_ViewImports`/`_ViewStart` files, which are imports rather than classes.</summary>
    private static List<string> Files(string folder, string csproj)
    {
        var found = new List<string>();
        foreach (var pattern in new[] { "*.razor", "*.cshtml" })
        {
            try { found.AddRange(Directory.GetFiles(folder, pattern, SearchOption.AllDirectories)); }
            catch (Exception e) when (e is IOException or UnauthorizedAccessException) { }
        }
        return [.. found.Where(f =>
        {
            var relative = Path.GetRelativePath(folder, f).Replace('\\', '/');
            return !relative.StartsWith("bin/", StringComparison.OrdinalIgnoreCase)
                && !relative.StartsWith("obj/", StringComparison.OrdinalIgnoreCase)
                && !Imports.Contains(Path.GetFileName(f), StringComparer.OrdinalIgnoreCase)
                && string.Equals(CsProject.Owner(f), csproj, StringComparison.OrdinalIgnoreCase);
        }).Order(StringComparer.Ordinal)];
    }

    private static SyntaxTree? Tree(RazorCodeDocument document, string path, CSharpParseOptions options)
    {
        var code = document.GetCSharpDocument()?.GeneratedCode;
        return string.IsNullOrEmpty(code) ? null : CSharpSyntaxTree.ParseText(code, options, path, Encoding.UTF8);
    }

    private static RazorProjectItem Item(RazorProjectFileSystem system, string folder, string file, string? appended)
    {
        var item = system.GetItem("/" + Path.GetRelativePath(folder, file).Replace('\\', '/'),
            file.EndsWith(".razor", StringComparison.OrdinalIgnoreCase) ? FileKinds.Component : FileKinds.Legacy);
        return appended is null ? item : new Text(item, appended);
    }

    /// <summary>
    /// What a view's `Web.config` says RazorEngine or MVC 5 compiles it with, as directives: `@inherits Base&lt;TModel&gt;`
    /// (the `@model` of the file fills `TModel`) and an `@using` per `&lt;add namespace&gt;`. The nearest
    /// `Web.config` holding each wins, walking up to the project.
    /// </summary>
    private static string Appended(string folder, string file)
    {
        string? baseType = null;
        var namespaces = new List<string>();
        for (var at = Path.GetDirectoryName(file); at is not null; at = Path.GetDirectoryName(at))
        {
            var config = Path.Combine(at, "Web.config");
            if (File.Exists(config))
            {
                try
                {
                    var document = XDocument.Load(config);
                    var pages = document.Descendants().Where(e => e.Name.LocalName == "pages").ToList();
                    baseType ??= pages.Select(p => p.Attribute("pageBaseType")?.Value).FirstOrDefault(v => !string.IsNullOrEmpty(v));
                    namespaces.AddRange(pages.SelectMany(p => p.Descendants()).Where(e => e.Name.LocalName == "add")
                        .Select(e => e.Attribute("namespace")?.Value ?? "").Where(n => n.Length > 0));
                }
                catch (Exception e) when (e is IOException or UnauthorizedAccessException or System.Xml.XmlException) { }
            }
            if (string.Equals(Path.GetFullPath(at), Path.GetFullPath(folder), StringComparison.OrdinalIgnoreCase)) break;
        }
        var text = new StringBuilder("\n");
        foreach (var name in namespaces.Distinct(StringComparer.Ordinal)) text.Append($"@using {name}\n");
        if (baseType is not null) text.Append($"@inherits {baseType.Split('`')[0]}<TModel>\n");
        return text.ToString();
    }

    /// <summary>
    /// MVC 5's `@helper Name(args) { ... }`, which RazorEngine templates use and the ASP.NET Core compiler does not
    /// know, as the local function Core Razor DOES compile with markup inside: `@{ object Name(args) { ... return
    /// null; } }`. Without it every helper's name and parameter is "not in the current context" in such a template.
    /// The rewrite stays on the same lines, so no row moves; `object`, so `@Name(x)` still
    /// writes something.
    /// </summary>
    public static string Rewritten(string text)
    {
        var built = new StringBuilder(text.Length + 64);
        var at = 0;
        while (at < text.Length)
        {
            var found = text.IndexOf("@helper ", at, StringComparison.Ordinal);
            if (found < 0 || !LineStart(text, found)) { built.Append(text, at, (found < 0 ? text.Length : found + 1) - at); at = found < 0 ? text.Length : found + 1; continue; }
            var open = text.IndexOf('{', found);
            var close = open < 0 ? -1 : Matching(text, open);
            if (close < 0) { built.Append(text, at, text.Length - at); break; }
            // A bare `return;` leaves a helper early; in an `object` function it has to return something.
            var body = text[(found + 8)..close].Replace("return;", "return null;", StringComparison.Ordinal);
            built.Append(text, at, found - at).Append("@{ object ").Append(body).Append("return null; } }");
            at = close + 1;
        }
        return built.ToString();
    }

    private static bool LineStart(string text, int index)
    {
        for (var i = index - 1; i >= 0 && text[i] != '\n'; i--) if (!char.IsWhiteSpace(text[i])) return false;
        return true;
    }

    /// <summary>The `}` closing the `{` at `open`, braces inside double quotes not counted.</summary>
    private static int Matching(string text, int open)
    {
        var depth = 0;
        var quoted = false;
        for (var i = open; i < text.Length; i++)
        {
            if (text[i] == '"') quoted = !quoted;
            if (quoted) continue;
            if (text[i] == '{') depth++;
            else if (text[i] == '}' && --depth == 0) return i;
        }
        return -1;
    }

    private static string RootNamespace(string csproj, string name)
    {
        try
        {
            var declared = XDocument.Load(csproj).Root?.Descendants().FirstOrDefault(e => e.Name.LocalName == "RootNamespace")?.Value.Trim();
            if (!string.IsNullOrEmpty(declared)) return declared;
        }
        catch (Exception e) when (e is IOException or UnauthorizedAccessException or System.Xml.XmlException) { }
        return name;
    }

    /// <summary>The components discovered in the declaration pass, handed to the full one.</summary>
    private sealed class Helpers(IReadOnlyList<TagHelperDescriptor> found) : RazorEngineFeatureBase, ITagHelperFeature
    {
        public IReadOnlyList<TagHelperDescriptor> GetDescriptors() => found;
    }

    /// <summary>A project item whose text has directives appended at its end.</summary>
    private sealed class Text(RazorProjectItem inner, string appended) : RazorProjectItem
    {
        public override string BasePath => inner.BasePath;
        public override string FilePath => inner.FilePath;
        public override string PhysicalPath => inner.PhysicalPath;
        public override string RelativePhysicalPath => inner.RelativePhysicalPath;
        public override string FileKind => inner.FileKind;
        public override bool Exists => inner.Exists;

        public override Stream Read()
        {
            using var reader = new StreamReader(inner.Read(), Encoding.UTF8, detectEncodingFromByteOrderMarks: true);
            return new MemoryStream(Encoding.UTF8.GetBytes(Rewritten(reader.ReadToEnd()) + appended));
        }
    }

    private sealed class Renders : IntermediateNodeWalker
    {
        public List<Render> Found { get; } = [];

        public override void VisitComponent(ComponentIntermediateNode node)
        {
            Found.Add(new Render((node.Source?.LineIndex ?? -1) + 1, node.TypeName ?? "", node.TagName ?? "",
                [.. node.Attributes.Select(a => a.AttributeName).Distinct(StringComparer.Ordinal)]));
            base.VisitComponent(node);
        }
    }
}
