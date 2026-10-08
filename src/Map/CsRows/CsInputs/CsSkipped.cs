using Microsoft.CodeAnalysis;

namespace StructureGate;

/// <summary>
/// A REFERENCED PROJECT THE CONSUMER SKIPPED is referenced through what its BUILD wrote, not compiled here.
/// `--skip Demo.Legacy` keeps its files out of the map, but a mapped project references it,
/// and binding against it compiled its whole source - seconds of a large consumer's refresh, for rows nobody asked for. Its
/// built `bin/**/&lt;Name&gt;.dll` carries every type and member a mapped file binds to. Without one, it is compiled
/// as before: a skipped folder must never cost a mapped file its binding.
///
/// SKIPPED MEANS UNDER A ROOT: a folder of the project's path BELOW the root that holds it is a `--skip` name. The
/// path above a root is the machine's, and `out` - in the default skip list - is a common folder there.
/// </summary>
internal sealed class CsSkipped(IReadOnlyList<string> roots, IReadOnlySet<string> skip)
{
    private readonly Dictionary<string, MetadataReference?> found = new(StringComparer.OrdinalIgnoreCase);

    /// <summary>The built assembly of a skipped project, or null when it is not skipped or was never built.</summary>
    public MetadataReference? Built(string csproj, string name, string framework)
    {
        if (skip.Count == 0 || !Skipped(csproj)) return null;
        if (found.TryGetValue(csproj, out var known)) return known;
        var bin = Path.Combine(Path.GetDirectoryName(csproj) ?? "", "bin");
        MetadataReference? reference = null;
        if (Directory.Exists(bin))
        {
            var dlls = Directory.EnumerateFiles(bin, name + ".dll", SearchOption.AllDirectories)
                .Select(path => new FileInfo(path))
                // ITS OWN FRAMEWORK FIRST - `bin/Debug/net8.0/` - then the newest build of any.
                .OrderByDescending(dll => string.Equals(dll.Directory?.Name, framework, StringComparison.OrdinalIgnoreCase))
                .ThenByDescending(dll => dll.LastWriteTimeUtc)
                .ToList();
            if (dlls.Count > 0) reference = MetadataReference.CreateFromFile(dlls[0].FullName);
        }
        found[csproj] = reference;
        return reference;
    }

    private bool Skipped(string csproj)
    {
        var folder = Path.GetFullPath(Path.GetDirectoryName(csproj) ?? "");
        foreach (var root in roots)
        {
            var top = Path.GetFullPath(root).TrimEnd(Path.DirectorySeparatorChar, Path.AltDirectorySeparatorChar);
            if (!folder.StartsWith(top + Path.DirectorySeparatorChar, StringComparison.OrdinalIgnoreCase)) continue;
            var below = folder[(top.Length + 1)..].Split(Path.DirectorySeparatorChar, Path.AltDirectorySeparatorChar);
            return below.Any(skip.Contains);
        }
        return false;
    }
}
