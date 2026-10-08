using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;

namespace StructureGate;

/// <summary>
/// ONE COMPILATION AT A TIME, and it is the project's, not the file's.
///
/// A SEMANTIC MODEL IS A PROJECT-SIZED THING. `Load` resolves to a method because the compilation holds the
/// type that declares it, the partial half written in another file, the base class two projects away and
/// the package that declares its return type. So a file cannot be bound on its own: the unit is the nearest
/// `.csproj`, exactly as the build has it.
///
/// THE FILES ARRIVE SORTED BY PROJECT, so exactly one compilation is alive at any moment — built when the
/// first file of a project comes through, dropped when the first file of the next one does. Holding all
/// dozens of projects of a solution would be the memory this half already ran out of once; rebuilding one per
/// file would re-bind an N-file project N times.
///
/// EVERY FILE OF THE PROJECT IS PARSED, not only the ones being re-read. A compilation missing the other
/// half of a partial class binds what that half declares to nothing, so an incremental run would quietly
/// answer differently from a cold one — the same rows, with the resolved columns empty.
/// </summary>
internal sealed class CsModels
{
    /// <summary>What one file needs to become rows: its tree (the one the model knows about — a model
    /// rejects a tree from another compilation), the model if the project bound, and which project it
    /// belongs to. `Declared` is the compilation's declaration diagnostics that have a location in this tree.</summary>
    public readonly record struct Ready(SyntaxTree? Tree, SemanticModel? Model, string Project, bool Compiled,
        CsRazor.Output? Razor = null, IReadOnlyList<Diagnostic>? Declared = null);

    private readonly Dictionary<string, List<(string Rel, string Abs)>> byProject = new(StringComparer.OrdinalIgnoreCase);
    private readonly Dictionary<string, string?> owner = new(StringComparer.Ordinal);
    private readonly Dictionary<string, SyntaxTree> trees = new(StringComparer.Ordinal);
    private readonly HashSet<string> excluded = new(StringComparer.Ordinal);
    // THE RAZOR OUTPUT of each compilation, by the same key, and of each mapped `.razor`/`.cshtml` by its rel.
    private readonly Dictionary<string, List<CsRazor.Output>> razors = new(StringComparer.OrdinalIgnoreCase);
    private readonly Dictionary<string, CsRazor.Output> razorOf = new(StringComparer.Ordinal);
    /// <summary>Every project already compiled in this run, referenced or mapped. A solution's projects
    /// reference each other many times over, and compiling one twice is the whole project twice.</summary>
    private readonly Dictionary<string, CSharpCompilation?> built = new(StringComparer.OrdinalIgnoreCase);
    private readonly HashSet<string> building = new(StringComparer.OrdinalIgnoreCase);

    /// <summary>The referenced projects `--skip` keeps out, and their built assemblies - see CsSkipped.</summary>
    public CsSkipped? Skipped { get; init; }
    private string? current;

    private CSharpCompilation? compilation;
    private string name = "";
    /// <summary>THE DECLARATION DIAGNOSTICS OF THE CURRENT COMPILATION, by every tree each has a location in -
    /// asked ONCE. Asked per file (`model.GetDiagnostics()`), Roslyn completed the declarations again and walked
    /// the whole declaration bag for each tree: a visible share of a large consumer's run.</summary>
    private Dictionary<SyntaxTree, List<Diagnostic>>? declared;

    /// <summary>
    /// Which project each mapped C# file belongs to, worked out once. It is also the ORDER the caller
    /// should send files in — see <see cref="Order"/>.
    /// </summary>
    public CsModels(SortedDictionary<string, string> paths)
    {
        foreach (var (rel, abs) in paths)
        {
            var csproj = CsProject.Owner(abs);
            owner[rel] = csproj;
            if (csproj is null) continue;
            if (!byProject.TryGetValue(csproj, out var listed)) byProject[csproj] = listed = [];
            listed.Add((rel, abs));
        }
    }

    /// <summary>The files, PROJECT BY PROJECT. Sent in any other order, each file would rebuild the
    /// compilation its project needs and the run would never finish.</summary>
    public List<string> Order(IEnumerable<string> files) =>
        [.. files.OrderBy(rel => owner.GetValueOrDefault(rel) ?? "", StringComparer.OrdinalIgnoreCase)
            .ThenBy(rel => rel, StringComparer.Ordinal)];

    /// <summary>The mapped files of a project, then every other `.cs` the project owns, each once.</summary>
    private static List<(string Rel, string Abs)> Whole(List<(string Rel, string Abs)> mapped, string folder, string csproj)
    {
        var seen = new HashSet<string>(mapped.Select(f => f.Abs), StringComparer.OrdinalIgnoreCase);
        var all = new List<(string Rel, string Abs)>(mapped);
        foreach (var file in Walk(folder, csproj))
        {
            if (seen.Add(file.Abs)) all.Add(file);
        }
        return all;
    }

    /// <summary>The `.cs` a project owns on disk: its folder, minus its build folders, minus every file a
    /// NESTED project owns - the nearest `.csproj` decides, as in the build.</summary>
    private static IEnumerable<(string Rel, string Abs)> Walk(string folder, string csproj)
    {
        if (folder.Length == 0 || !Directory.Exists(folder)) yield break;
        string[] found;
        try { found = Directory.GetFiles(folder, "*.cs", SearchOption.AllDirectories); }
        catch (Exception e) when (e is IOException or UnauthorizedAccessException) { yield break; }
        foreach (var abs in found)
        {
            var rel = Path.GetRelativePath(folder, abs).Replace(Path.DirectorySeparatorChar, '/');
            if (rel.StartsWith("bin/", StringComparison.OrdinalIgnoreCase)
                || rel.StartsWith("obj/", StringComparison.OrdinalIgnoreCase)) continue;
            if (!string.Equals(CsProject.Owner(abs), csproj, StringComparison.OrdinalIgnoreCase)) continue;
            yield return (rel, abs);
        }
    }

    public Ready For(string rel)
    {
        var csproj = owner.GetValueOrDefault(rel);
        // A .cs file under no .csproj is a file the build does not compile either — a script, a sample, a
        // leftover. It still gets every syntax row; there is simply nothing to bind it against.
        if (csproj is null) return new Ready(null, null, "", true);
        if (!string.Equals(csproj, current, StringComparison.OrdinalIgnoreCase)) Build(csproj);
        var tree = trees.GetValueOrDefault(rel);
        // NOT IN THE COMPILATION MEANS NOT COMPILED. The project removed it, so the build never sees it and
        // neither does this map — the file is still listed, and says why it has no rows.
        if (tree is null && excluded.Contains(rel)) return new Ready(null, null, name, false);
        if (tree is null || compilation is null) return new Ready(tree, null, name, true);
        if (declared is null)
        {
            var completing = System.Diagnostics.Stopwatch.StartNew();
            declared = Declarations(compilation);
            CsTimings.Add(csproj, 3, completing.ElapsedMilliseconds);
        }
        return new Ready(tree, compilation.GetSemanticModel(tree), name, true, razorOf.GetValueOrDefault(rel),
            declared.GetValueOrDefault(tree) ?? []);
    }

    /// <summary>
    /// Every declaration diagnostic, under each tree it has a location in - the main one and every additional
    /// one, once per tree: what Roslyn's own per-tree filter answers (`HasIntersectingLocation`). A duplicate
    /// type is an error in BOTH files.
    /// </summary>
    private static Dictionary<SyntaxTree, List<Diagnostic>> Declarations(CSharpCompilation compilation)
    {
        var grouped = new Dictionary<SyntaxTree, List<Diagnostic>>();
        foreach (var diagnostic in compilation.GetDeclarationDiagnostics())
        {
            var seen = new HashSet<SyntaxTree>();
            foreach (var location in diagnostic.AdditionalLocations.Prepend(diagnostic.Location))
            {
                if (location.SourceTree is not { } tree || !seen.Add(tree)) continue;
                if (!grouped.TryGetValue(tree, out var list)) grouped[tree] = list = [];
                list.Add(diagnostic);
            }
        }
        return grouped;
    }

    private void Build(string csproj)
    {
        current = csproj;
        trees.Clear();
        excluded.Clear();
        razorOf.Clear();
        declared = null;
        compilation = Compile(csproj, register: true);
        name = CsProject.Load(csproj)?.Name ?? Path.GetFileNameWithoutExtension(csproj);
        // A MAPPED `.razor`/`.cshtml` IS WALKED AS ITS GENERATED TREE, whichever pass generated it.
        var generated = razors.GetValueOrDefault(csproj + "|" + (CsProject.Load(csproj)?.Framework ?? "")) ?? [];
        foreach (var (rel, abs) in byProject.GetValueOrDefault(csproj) ?? [])
        {
            if (generated.FirstOrDefault(o => string.Equals(o.Abs, abs, StringComparison.OrdinalIgnoreCase)) is not { } output) continue;
            trees[rel] = output.Tree;
            razorOf[rel] = output;
        }
        // A PROJECT REACHED AS A REFERENCE FIRST is already compiled, and its trees were not kept by path
        // because nothing was going to be mapped out of it. When its own turn comes the cache answers with
        // that compilation and this map would be empty: most of the files of one solution silently
        // lost their model and produced syntax-only rows. The trees are in the compilation; they are put
        // back by path rather than parsed a second time.
        // Razor trees DO NOT COUNT here: they were just put in above, and counting them skipped the C# ones -
        // a project's .cs files lost their model whenever its Razor trees had been compiled first.
        if (trees.Keys.Any(k => !CsRazor.Owns(k)) || compilation is null) return;
        var wanted = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase);
        foreach (var (rel, abs) in byProject.GetValueOrDefault(csproj) ?? []) wanted[abs] = rel;
        foreach (var tree in compilation.SyntaxTrees)
        {
            if (wanted.TryGetValue(tree.FilePath, out var rel)) trees[rel] = tree;
        }
        // AND WHAT IT REMOVED STAYS REMOVED. Compiled as a reference, the project recorded no exclusions -
        // only the mapped pass does - so a `<Compile Remove>` file was neither a tree nor excluded, and read
        // as compiled with no model: syntax rows, `compiled = 1`, and every call in it unbound, for a file
        // the build never sees.
        var files = CsProject.Load(csproj)?.Files ?? Compiled.Everything;
        var folder = Path.GetDirectoryName(csproj) ?? "";
        foreach (var (rel, abs) in byProject.GetValueOrDefault(csproj) ?? [])
        {
            if (!trees.ContainsKey(rel) && !files.Covers(Path.GetRelativePath(folder, abs))) excluded.Add(rel);
        }
    }


    /// <summary>
    /// One project's compilation, cached.
    ///
    /// A PROJECT REFERENCE IS FOLLOWED TO ITS SOURCE, not to its assembly. The assembly exists only if
    /// somebody built the tree: on a freshly cloned or cleaned checkout there is none, and every type from
    /// a sibling project then resolves to nothing — thousands of names of one project, which is not a
    /// fact about that code. The referenced project is compiled here instead and handed over as a
    /// reference, which is what a real workspace does with a project graph.
    ///
    /// <paramref name="register"/> marks the project being MAPPED: only its trees are kept by path, because
    /// only its files produce rows.
    /// </summary>
    /// <param name="consumer">The framework of the project referencing this one - a multi-targeted project
    /// is compiled once per framework its consumers pick (`framework::pick` in `rust/fbtcore/src/csproj/`).</param>
    private CSharpCompilation? Compile(string csproj, bool register, string? consumer = null)
    {
        var key = csproj + "|" + (CsProject.Load(csproj, consumer)?.Framework ?? "");
        if (built.TryGetValue(key, out var ready)) return ready;
        // A cycle in the project graph is illegal in MSBuild and still possible in a half-edited tree. The
        // guard makes it a missing reference rather than a stack overflow.
        if (!building.Add(key)) return null;

        var compiled = Sources(csproj, register, consumer);
        building.Remove(key);
        built[key] = compiled;
        return compiled;
    }

    private CSharpCompilation? Sources(string csproj, bool register, string? consumer)
    {
        var project = CsProject.Load(csproj, consumer);
        var folder = Path.GetDirectoryName(csproj) ?? "";
        var compiled = project?.Files ?? Compiled.Everything;
        // THE PROJECT'S OWN `#if` SYMBOLS. Without them a region behind `#if DEBUG` is disabled text: it
        // binds to nothing and contributes no row, in many files of one real solution.
        // AND THE PROJECT'S OWN C# VERSION, not the parser's newest - see CsProject.LangVersion. A value
        // this Roslyn cannot read (a version newer than it knows) falls back to its default.
        var language = LanguageVersionFacts.TryParse(project?.LangVersion ?? "", out var parsedVersion)
            ? parsedVersion : LanguageVersion.Default;
        var options = new CSharpParseOptions(language, preprocessorSymbols: project?.Defines ?? []);

        // THE WHOLE PROJECT IS COMPILED, whatever part of it is mapped. A root inside a project - one
        // folder of a core project - gave a compilation of that folder alone, so every
        // type declared elsewhere in the same project was "not found" and its calls stayed unbound. The files
        // outside the map are parsed and bound against, never registered: only mapped files produce rows.
        var reading = System.Diagnostics.Stopwatch.StartNew();
        var mapped = byProject.GetValueOrDefault(csproj);
        var files = mapped is null ? [.. Walk(folder, csproj)] : Whole(mapped, folder, csproj);
        var rows = new HashSet<string>(mapped?.Select(f => f.Abs) ?? [], StringComparer.OrdinalIgnoreCase);
        var wanted = new List<(string Rel, string Abs, bool Mine)>(files.Count);
        foreach (var (rel, abs) in files)
        {
            var mine = register && rows.Contains(abs);
            // MARKUP IS NOT C#: a `.razor`/`.cshtml` enters the compilation as the tree the Razor compiler writes.
            if (CsRazor.Owns(abs)) continue;
            // A FILE THE PROJECT REMOVES IS NOT IN THE COMPILATION, so it must not be in this one either:
            // binding a file against a compilation the build never put it in answers with names that do not
            // exist for it.
            if (!compiled.Covers(Path.GetRelativePath(folder, abs)))
            {
                if (mine) excluded.Add(rel);
                continue;
            }
            wanted.Add((rel, abs, mine));
        }
        // READ AND PARSED ON EVERY CORE, each tree into its own slot: the ORDER of a compilation's trees is
        // part of what it means (partial members, diagnostics), so it stays the order the files were listed in.
        // The PATH is given to the tree, because a diagnostic or a symbol location that names no file is one
        // nobody can follow back.
        var slots = new SyntaxTree?[wanted.Count];
        Parallel.For(0, wanted.Count, i =>
        {
            try { slots[i] = CSharpSyntaxTree.ParseText(File.ReadAllText(wanted[i].Abs), options, wanted[i].Abs); }
            catch (Exception e) when (e is IOException or UnauthorizedAccessException) { slots[i] = null; }
        });
        var parsed = new List<SyntaxTree>(files.Count);
        for (var i = 0; i < wanted.Count; i++)
        {
            if (slots[i] is not { } tree) continue;
            if (wanted[i].Mine) trees[wanted[i].Rel] = tree;
            parsed.Add(tree);
        }
        CsTimings.Add(csproj, 1, reading.ElapsedMilliseconds, wanted.Count);
        if (project is null || !project.Bound) return null;

        // WHAT THE BUILD WROTE FOR THE COMPILER, compiled and never mapped: `GlobalUsings.g.cs` is where
        // every `using` an implicit-usings project relies on lives, and it is not a file anyone authored.
        // ONCE: a tree that does not skip `obj` walked a generated file in already, and a second copy declares it twice.
        var walked = new HashSet<string>(wanted.Select(w => w.Abs), StringComparer.OrdinalIgnoreCase);
        foreach (var generated in project.Generated.Where(g => !walked.Contains(g)))
        {
            try { parsed.Add(CSharpSyntaxTree.ParseText(File.ReadAllText(generated), options, generated)); }
            catch (Exception e) when (e is IOException or UnauthorizedAccessException) { /* not authored */ }
        }
        // The global usings a BUILD would have written, when none did - see CsProject.Usings.
        if (project.Usings.Length > 0)
        {
            parsed.Add(CSharpSyntaxTree.ParseText(project.Usings, options,
                System.IO.Path.Combine(System.IO.Path.GetDirectoryName(project.Path)!, "obj", "structuregate.GlobalUsings.g.cs")));
        }
        // And the assembly attributes it would have written - see CsProject.Attributes.
        if (project.Attributes.Length > 0)
        {
            parsed.Add(CSharpSyntaxTree.ParseText(project.Attributes, options,
                System.IO.Path.Combine(System.IO.Path.GetDirectoryName(project.Path)!, "obj", "structuregate.AssemblyInfo.g.cs")));
        }
        var references = new List<MetadataReference>(project.References);
        foreach (var referenced in project.Projects)
        {
            var load = CsProject.Load(referenced, project.Framework);
            if (load is not null && Skipped?.Built(referenced, load.Name, load.Framework) is { } dll)
            {
                references.Add(dll);
                continue;
            }
            var other = Compile(referenced, register: false, project.Framework);
            if (other is not null) references.Add(other.ToMetadataReference());
        }

        // THE RAZOR THE BUILD WOULD GENERATE, when the build left none on disk - see CsRazor. After the
        // references, because discovering a component needs a compilation that can bind it.
        var generating = System.Diagnostics.Stopwatch.StartNew();
        var razor = project.Generated.Any(g => g.EndsWith(".razor.g.cs", StringComparison.OrdinalIgnoreCase)) ? []
            : CsRazor.Generate(csproj, project.Name, parsed, references, options);
        CsTimings.Add(csproj, 2, generating.ElapsedMilliseconds);
        parsed.AddRange(razor.Select(o => o.Tree));
        razors[csproj + "|" + project.Framework] = razor;

        return CSharpCompilation.Create(project.Name, parsed, references,
            // A LIBRARY, whatever the project really is: an exe kind makes Roslyn look for an entry point
            // and report its absence, and this compilation is never emitted — only asked questions.
            new CSharpCompilationOptions(OutputKind.DynamicallyLinkedLibrary, allowUnsafe: true));
    }
}

/// <summary>
/// WHERE A PROJECT'S COMPILE TIME WENT, per project and phase - [load references, parse sources, razor, declarations,
/// files parsed, assemblies opened, assemblies already open] - handed to rust with each batch for the trace. A large consumer's first run after an update spent minutes in
/// "compile" for a few hundred files and nothing said whether it was reading sources, picking reference assemblies, generating
/// Razor or completing declarations.
/// </summary>
internal static class CsTimings
{
    public const int Opened = 5, Shared = 6;

    private static readonly Dictionary<string, long[]> ByProject = new(StringComparer.OrdinalIgnoreCase);

    public static void Add(string csproj, int phase, long ms, int files = 0)
    {
        lock (ByProject)
        {
            var spent = Of(csproj);
            spent[phase] += ms;
            spent[4] += files;
        }
    }

    /// <summary>One more reference this project took: an assembly it <see cref="Opened"/>, or one <see cref="Shared"/>
    /// with a project before it.</summary>
    public static void Count(string csproj, int slot)
    {
        lock (ByProject) Of(csproj)[slot]++;
    }

    private static long[] Of(string csproj)
    {
        if (!ByProject.TryGetValue(csproj, out var spent)) ByProject[csproj] = spent = new long[7];
        return spent;
    }

    public static List<(string Project, long[] Spent)> Drain()
    {
        lock (ByProject)
        {
            var all = ByProject.Select(p => (p.Key, p.Value)).ToList();
            ByProject.Clear();
            return all;
        }
    }
}
