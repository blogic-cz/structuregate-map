using System.Reflection.PortableExecutable;
using System.Text.Json;
using Microsoft.CodeAnalysis;

namespace StructureGate;

/// <summary>
/// THE REFERENCES OF ONE PROJECT, as a <c>Compilation</c> takes them. What the project compiles against is
/// worked out by rust WITHOUT MSBUILD (`rust/fbtcore/src/csproj/`): the project file, `Directory.Build.props`,
/// `project.assets.json`, the build's `bin`/`obj`, the framework packs, a package's own `build/*.targets`.
/// This side does the two things that need .NET: it picks which copy of an assembly wins - by the assembly
/// VERSION in its metadata - and it opens the winners as Roslyn references.
///
/// A REFERENCE THAT IS MISSING IS NOT A FAILURE. Roslyn binds what it can and leaves the rest unresolved, so
/// a project that was never restored still produces every row, without the resolved columns.
/// </summary>
internal sealed class CsProject
{
    private static readonly Dictionary<string, CsProject?> Loaded = new(StringComparer.OrdinalIgnoreCase);

    /// <summary>Every assembly opened ONCE for the whole run, by its <see cref="Identity"/>. Dozens of projects of one
    /// solution reference the same packages, and a metadata handle per project per package is the memory
    /// this half already ran out of once.</summary>
    private static readonly Dictionary<string, MetadataReference?> Opened = new(StringComparer.Ordinal);

    /// <summary>Each path's <see cref="Identity"/>, asked of the file system once per run.</summary>
    private static readonly Dictionary<string, string> Identities = new(StringComparer.OrdinalIgnoreCase);
    private static readonly Dictionary<string, string?> Owners = new(StringComparer.OrdinalIgnoreCase);

    public required string Path { get; init; }
    public required string Name { get; init; }
    public required string Framework { get; init; }
    public required List<MetadataReference> References { get; init; }

    /// <summary>The `.cs` the BUILD wrote under `obj` and this pass does not map: global usings, assembly
    /// info, a generator's output - compiled, never walked.</summary>
    public required List<string> Generated { get; init; }

    /// <summary>The global usings the SDK WOULD have written, as source, when no build wrote them - "" once
    /// a `*.GlobalUsings.g.cs` is on disk. A restored, never-built tree lost `CancellationToken` hundreds of times.</summary>
    public string Usings { get; init; } = "";

    /// <summary>The assembly attributes the SDK WOULD have written from items (`InternalsVisibleTo`), as source, when
    /// no build wrote a `*.AssemblyInfo.cs` - "" once one is on disk.</summary>
    public string Attributes { get; init; } = "";

    /// <summary>The project's `&lt;LangVersion&gt;`, or its framework's default - NOT the parser's newest: C#
    /// 14's first-class spans turned a net9 `array.Reverse()` into "`.` cannot be applied to void".</summary>
    public string LangVersion { get; init; } = "";

    /// <summary>The `#if` symbols: code in a region this list does not name is disabled text.</summary>
    public required List<string> Defines { get; init; }

    /// <summary>What the project says it compiles, when it says anything at all.</summary>
    public required Compiled Files { get; init; }

    /// <summary>The projects in this one's restored CLOSURE, as absolute `.csproj` paths - compiled from
    /// SOURCE when nobody built them.</summary>
    public required List<string> Projects { get; init; }

    /// <summary>Whether this project resolved enough to bind anything at all.</summary>
    public bool Bound => References.Count > 0;

    /// <summary>The nearest `.csproj` at or above a file - which decides its compilation.</summary>
    public static string? Owner(string fileAbs)
    {
        var folder = System.IO.Path.GetDirectoryName(fileAbs) ?? "";
        if (Owners.TryGetValue(folder, out var known)) return known;
        using var answer = Ask(Question("owner", fileAbs));
        var found = answer?.RootElement.GetProperty("owner").GetString();
        Owners[folder] = found;
        return found;
    }

    /// <param name="consumer">The framework of the project REFERENCING this one, which picks among its
    /// `TargetFrameworks`. Null for the project being mapped: its first.</param>
    public static CsProject? Load(string csproj, string? consumer = null)
    {
        var key = csproj + "|" + (consumer ?? "");
        if (Loaded.TryGetValue(key, out var known)) return known;
        CsProject? project = null;
        var loading = System.Diagnostics.Stopwatch.StartNew();
        using (var answer = Ask(Question("project", csproj, consumer)))
        {
            if (answer is not null && answer.RootElement.ValueKind == JsonValueKind.Object) project = Read(answer.RootElement);
        }
        CsTimings.Add(csproj, 0, loading.ElapsedMilliseconds);
        Loaded[key] = project;
        return project;
    }

    private static CsProject Read(JsonElement root)
    {
        List<string> Strings(JsonElement owner, string name) => [.. owner.GetProperty(name).EnumerateArray().Select(e => e.GetString()!)];
        var references = new List<MetadataReference>();
        var seen = new HashSet<string>(Strings(root, "exclude"), StringComparer.OrdinalIgnoreCase);
        var path = root.GetProperty("path").GetString()!;
        foreach (var assembly in Winners(Strings(root, "packaged"), Strings(root, "platform"))) Add(path, references, seen, assembly);
        var files = root.GetProperty("files");
        return new CsProject
        {
            Path = path,
            Name = root.GetProperty("name").GetString()!,
            Framework = root.GetProperty("framework").GetString()!,
            References = references,
            Generated = Strings(root, "generated"),
            Usings = root.GetProperty("usings").GetString()!,
            Attributes = root.TryGetProperty("attributes", out var attributes) ? attributes.GetString() ?? "" : "",
            LangVersion = root.GetProperty("lang_version").GetString()!,
            Defines = Strings(root, "defines"),
            Files = new Compiled(files.GetProperty("explicit").GetBoolean(), Strings(files, "include"), Strings(files, "remove")),
            Projects = Strings(root, "projects"),
        };
    }

    private static string Question(string ask, string path, string? consumer = null)
    {
        using var buffer = new MemoryStream();
        using (var json = new Utf8JsonWriter(buffer))
        {
            json.WriteStartObject();
            json.WriteString(ask, path);
            if (consumer is not null) json.WriteString("consumer", consumer);
            json.WriteEndObject();
        }
        return System.Text.Encoding.UTF8.GetString(buffer.ToArray());
    }

    private static JsonDocument? Ask(string question) =>
        FbtCore.CsAsk(question) is { } reply ? JsonDocument.Parse(reply) : null;

    /// <summary>
    /// ONE FILE PER ASSEMBLY NAME, chosen the way MSBuild's conflict resolution chooses it: the higher
    /// assembly version wins, and on a tie the FRAMEWORK wins over a package.
    ///
    /// FIRST-NAME-WINS DROPPED THE FRAMEWORK. A restore closure still carries the old netstandard facades -
    /// `System.Runtime/4.3.0` - and added before the framework pack, that file took the name: `DateOnly` and
    /// `Index` were "not defined", and a tree's map carried many compile errors and bound many of its
    /// calls to nothing. A package that really is NEWER than the framework still wins, as it does in a build.
    /// </summary>
    private static IEnumerable<string> Winners(IEnumerable<string> packaged, IEnumerable<string> platform)
    {
        var chosen = new Dictionary<string, (string Path, Version Version, bool Framework)>(StringComparer.OrdinalIgnoreCase);
        var order = new List<string>();
        void Offer(string assembly, bool framework)
        {
            var name = System.IO.Path.GetFileName(assembly);
            var version = VersionOf(assembly);
            if (!chosen.TryGetValue(name, out var held))
            {
                chosen[name] = (assembly, version, framework);
                order.Add(name);
                return;
            }
            var newer = version.CompareTo(held.Version);
            if (newer > 0 || (newer == 0 && framework && !held.Framework)) chosen[name] = (assembly, version, framework);
        }
        foreach (var assembly in packaged) Offer(assembly, false);
        foreach (var assembly in platform) Offer(assembly, true);
        return order.Select(name => chosen[name].Path);
    }

    /// <summary>Every assembly's version, read ONCE for the whole run: the same ~200 framework assemblies are
    /// candidates for every project of a solution, and reading their metadata again per project was the
    /// largest single cost of a one-file edit on a large consumer.</summary>
    private static readonly Dictionary<string, Version> Versions = new(StringComparer.OrdinalIgnoreCase);

    /// <summary>The assembly version a file declares, or 0.0 for a file that is not a managed assembly.</summary>
    private static Version VersionOf(string assembly)
    {
        var identity = Identity(assembly);
        if (Versions.TryGetValue(identity, out var known)) return known;
        return Versions[identity] = ReadVersion(assembly);
    }

    /// <summary>
    /// ONE ASSEMBLY UNDER MANY PATHS IS ONE ASSEMBLY: its file name, size and last write, the same for every copy.
    /// The build of an exe or a test project copies each package it uses into its own `bin`, that copy is read
    /// first, and on a tie `Winners` keeps it - so keyed by PATH, every such project opened its own few hundred
    /// copies of what the project before it had already opened. A large consumer's test and host projects spent many seconds
    /// each in "load references".
    /// </summary>
    private static string Identity(string assembly)
    {
        if (Identities.TryGetValue(assembly, out var known)) return known;
        var identity = assembly;
        try
        {
            var info = new FileInfo(assembly);
            if (info.Exists) identity = $"{info.Name.ToLowerInvariant()}|{info.Length}|{info.LastWriteTimeUtc.Ticks}";
        }
        catch (Exception e) when (e is IOException or UnauthorizedAccessException or ArgumentException or NotSupportedException)
        {
            // Unreadable: the path stays its own identity, and opening it says the rest.
        }
        return Identities[assembly] = identity;
    }

    private static Version ReadVersion(string assembly)
    {
        try
        {
            using var stream = File.OpenRead(assembly);
            using var pe = new PEReader(stream);
            if (!pe.HasMetadata) return new Version(0, 0);
            var reader = System.Reflection.Metadata.PEReaderExtensions.GetMetadataReader(pe);
            return reader.IsAssembly ? reader.GetAssemblyDefinition().Version : new Version(0, 0);
        }
        catch (Exception e) when (e is IOException or UnauthorizedAccessException or BadImageFormatException
            or InvalidOperationException)
        {
            return new Version(0, 0);
        }
    }

    private static void Add(string csproj, List<MetadataReference> references, HashSet<string> seen, string assembly)
    {
        if (!seen.Add(System.IO.Path.GetFileName(assembly))) return;
        var identity = Identity(assembly);
        var shared = Opened.TryGetValue(identity, out var metadata);
        CsTimings.Count(csproj, shared ? CsTimings.Shared : CsTimings.Opened);
        if (!shared)
        {
            try { metadata = MetadataReference.CreateFromFile(assembly); }
            catch (Exception e) when (e is IOException or UnauthorizedAccessException or BadImageFormatException)
            {
                // A native dll sitting in a build folder is not a managed reference. It is skipped, not
                // named: a build folder holds many, and none of them is a fact about the source mapped here.
                metadata = null;
            }
            Opened[identity] = metadata;
        }
        if (metadata is not null) references.Add(metadata);
    }
}
