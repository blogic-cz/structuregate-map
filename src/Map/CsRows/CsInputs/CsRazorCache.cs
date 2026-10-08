using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;

namespace StructureGate;

/// <summary>
/// WHAT THE RAZOR COMPILER GENERATED, KEPT until something it read changes. Generating a project's `.razor` and
/// `.cshtml` is the Razor engine run twice over the whole project - declarations, component discovery across
/// every reference, then the full generation - and a one-file C# edit on a large consumer spent much of its time
/// regenerating template projects nobody had touched, because they are references of the edited one.
///
/// THE KEY IS EVERYTHING THE GENERATOR READS: the markup files, the `_Imports`/`_ViewImports`/`_ViewStart`/
/// `Web.config` beside them, the root namespace, the parse options - and, for COMPONENTS, which discover their
/// tag helpers from compiled code, every syntax tree of the project and every reference (a built assembly by
/// path, size and time; a project compiled here from source by the content of its own trees). A hit is the same
/// generated C#, parsed at the same path with the same options: the same trees, the same rows.
///
/// Kept in the project's own `obj/structuregate.razor/`, a build folder, the newest few keys at a time - a
/// multi-targeted project has one per framework its consumers pick.
/// </summary>
internal static class CsRazorCache
{
    /// <summary>BUMP IT WHEN WHAT `CsRazor.Generate` PRODUCES CHANGES: an old entry would answer for it otherwise.</summary>
    private const string Version = "1";

    private const int Kept = 4;

    /// <summary>What a referenced compilation contributes to a key, worked out once per compilation.</summary>
    private static readonly Dictionary<Compilation, string> Compiled = new(ReferenceEqualityComparer.Instance);

    private static readonly string[] Inputs = ["Web.config", "_Imports.razor", "_ViewImports.cshtml", "_ViewStart.cshtml"];

    public static string Key(string folder, string root, IReadOnlyList<string> files, IReadOnlyList<SyntaxTree> parsed,
        IReadOnlyList<MetadataReference> references, CSharpParseOptions options)
    {
        using var hash = IncrementalHash.CreateHash(HashAlgorithmName.SHA256);
        void Add(string text) => hash.AppendData(Encoding.UTF8.GetBytes(text + "\n"));
        void AddFile(string path)
        {
            Add(path);
            try { hash.AppendData(SHA256.HashData(File.ReadAllBytes(path))); }
            catch (Exception e) when (e is IOException or UnauthorizedAccessException) { Add("?"); }
        }
        Add(Version);
        Add(root);
        Add(options.LanguageVersion.ToString());
        Add(string.Join(";", options.PreprocessorSymbolNames));
        foreach (var file in files) AddFile(file);
        foreach (var input in Around(folder, files)) AddFile(input);
        // ONLY A COMPONENT READS COMPILED CODE: a view is generated from its own text and its config alone.
        if (files.Any(f => f.EndsWith(".razor", StringComparison.OrdinalIgnoreCase)))
        {
            foreach (var tree in parsed) Add(tree.FilePath + "|" + Convert.ToHexString(tree.GetText().GetChecksum().AsSpan()));
            foreach (var reference in references) Add(Of(reference));
        }
        return Convert.ToHexString(hash.GetHashAndReset())[..32].ToLowerInvariant();
    }

    /// <summary>A reference as a key sees it: a file by path, size and time; a compilation by what it compiles.</summary>
    private static string Of(MetadataReference reference)
    {
        switch (reference)
        {
            case PortableExecutableReference { FilePath: { } path }:
                var info = new FileInfo(path);
                return info.Exists ? $"{path}|{info.Length}|{info.LastWriteTimeUtc.Ticks}" : $"{path}|-";
            case CompilationReference { Compilation: var compilation }:
                if (Compiled.TryGetValue(compilation, out var known)) return known;
                using (var hash = IncrementalHash.CreateHash(HashAlgorithmName.SHA256))
                {
                    hash.AppendData(Encoding.UTF8.GetBytes(compilation.AssemblyName ?? ""));
                    foreach (var tree in compilation.SyntaxTrees) hash.AppendData(tree.GetText().GetChecksum().AsSpan());
                    foreach (var inner in compilation.References) hash.AppendData(Encoding.UTF8.GetBytes(Of(inner)));
                    return Compiled[compilation] = compilation.AssemblyName + "|" + Convert.ToHexString(hash.GetHashAndReset());
                }
            default:
                return reference.Display ?? "";
        }
    }

    /// <summary>The imports and configs the markup files are compiled with: those beside each file and in every
    /// folder above it, up to the project - where Razor and `Web.config` are looked for, and nowhere else.</summary>
    private static IEnumerable<string> Around(string folder, IReadOnlyList<string> files)
    {
        var top = Path.GetFullPath(folder);
        var seen = new SortedSet<string>(StringComparer.OrdinalIgnoreCase);
        foreach (var file in files)
        {
            for (var at = Path.GetDirectoryName(file); at is not null; at = Path.GetDirectoryName(at))
            {
                foreach (var name in Inputs)
                {
                    var candidate = Path.Combine(at, name);
                    if (File.Exists(candidate)) seen.Add(candidate);
                }
                if (string.Equals(Path.GetFullPath(at), top, StringComparison.OrdinalIgnoreCase)) break;
            }
        }
        return seen;
    }

    private static string Store(string folder) => Path.Combine(folder, "obj", "structuregate.razor");

    /// <summary>What an earlier run generated under this key, or null.</summary>
    public static List<CsRazor.Output>? Read(string folder, string key, CSharpParseOptions options)
    {
        var path = Path.Combine(Store(folder), key + ".json");
        if (!File.Exists(path)) return null;
        try
        {
            using var document = JsonDocument.Parse(File.ReadAllBytes(path));
            var outputs = new List<CsRazor.Output>();
            foreach (var entry in document.RootElement.GetProperty("outputs").EnumerateArray())
            {
                var abs = entry.GetProperty("abs").GetString()!;
                var tree = CSharpSyntaxTree.ParseText(entry.GetProperty("code").GetString()!, options, abs + ".g.cs", Encoding.UTF8);
                var renders = entry.GetProperty("renders").EnumerateArray().Select(r => new CsRazor.Render(
                    r.GetProperty("line").GetInt32(), r.GetProperty("component").GetString()!, r.GetProperty("tag").GetString()!,
                    [.. r.GetProperty("attributes").EnumerateArray().Select(a => a.GetString()!)])).ToList();
                outputs.Add(new CsRazor.Output(abs, entry.GetProperty("kind").GetString()!, tree, renders));
            }
            File.SetLastWriteTimeUtc(path, DateTime.UtcNow);
            return outputs;
        }
        catch (Exception e) when (e is IOException or UnauthorizedAccessException or JsonException or InvalidOperationException or KeyNotFoundException)
        {
            return null;
        }
    }

    /// <summary>Keep what was generated, and only the newest few keys beside it.</summary>
    public static void Write(string folder, string key, List<CsRazor.Output> outputs)
    {
        try
        {
            var store = Directory.CreateDirectory(Store(folder));
            using (var stream = File.Create(Path.Combine(store.FullName, key + ".json")))
            using (var json = new Utf8JsonWriter(stream))
            {
                json.WriteStartObject();
                json.WriteStartArray("outputs");
                foreach (var output in outputs)
                {
                    json.WriteStartObject();
                    json.WriteString("abs", output.Abs);
                    json.WriteString("kind", output.Kind);
                    json.WriteString("code", output.Tree.GetText().ToString());
                    json.WriteStartArray("renders");
                    foreach (var render in output.Renders)
                    {
                        json.WriteStartObject();
                        json.WriteNumber("line", render.Line);
                        json.WriteString("component", render.Component);
                        json.WriteString("tag", render.Tag);
                        json.WriteStartArray("attributes");
                        foreach (var attribute in render.Attributes) json.WriteStringValue(attribute);
                        json.WriteEndArray();
                        json.WriteEndObject();
                    }
                    json.WriteEndArray();
                    json.WriteEndObject();
                }
                json.WriteEndArray();
                json.WriteEndObject();
            }
            foreach (var old in store.GetFiles("*.json").OrderByDescending(f => f.LastWriteTimeUtc).Skip(Kept)) old.Delete();
        }
        catch (Exception e) when (e is IOException or UnauthorizedAccessException) { /* a cache that cannot be written is only slower */ }
    }
}
