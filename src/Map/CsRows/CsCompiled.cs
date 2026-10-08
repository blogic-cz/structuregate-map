namespace StructureGate;

/// <summary>
/// WHICH FILES A PROJECT COMPILES, decided the way the project itself states it.
///
/// A file on disk under a project folder is not automatically a file the build compiles. An SDK project
/// globs `**/*.cs` and then REMOVES some of it; a project with no SDK attribute lists every file it wants
/// and compiles nothing else. Mapping what the build does not compile is how a map reports edges into code
/// that is not in the assembly — and how the semantic pass binds a file against a compilation it was never
/// part of.
///
/// THE PATTERNS ARE MATCHED HERE, BY HAND, and not by a pattern language. `**` crosses folders, `*` and `?`
/// do not, and a segment is compared whole. That is all MSBuild's item globs use in practice, and a regex
/// translated from a glob is the second, worse parser this repo exists to refuse — the build itself fails
/// on one.
/// </summary>
internal sealed record Compiled(bool Explicit, List<string> Include, List<string> Remove)
{
    /// <summary>A project that says nothing: the SDK glob, nothing removed.</summary>
    public static readonly Compiled Everything = new(false, [], []);

    /// <summary>
    /// Whether this project compiles the file at <paramref name="relative"/> — a path relative to the
    /// project folder, in either slash.
    /// </summary>
    public bool Covers(string relative)
    {
        var path = relative.Replace('\\', '/');
        foreach (var pattern in Remove)
        {
            if (Matches(pattern, path)) return false;
        }
        if (!Explicit) return true;
        foreach (var pattern in Include)
        {
            if (Matches(pattern, path)) return true;
        }
        return false;
    }

    /// <summary>One MSBuild item glob against one path, segment by segment.</summary>
    private static bool Matches(string pattern, string path)
    {
        var wanted = pattern.Replace('\\', '/').Split('/', StringSplitOptions.RemoveEmptyEntries);
        var actual = path.Split('/', StringSplitOptions.RemoveEmptyEntries);
        return Walk(wanted, 0, actual, 0);
    }

    /// <summary>
    /// The match itself. `**` is the only part that needs to look ahead: it stands for any number of
    /// segments, so every position after it is tried until one of them matches the rest of the pattern.
    /// </summary>
    private static bool Walk(string[] pattern, int p, string[] path, int i)
    {
        while (p < pattern.Length)
        {
            if (pattern[p] == "**")
            {
                // A trailing `**` takes everything that is left, which is what `wwwroot/Images/**` means.
                if (p + 1 == pattern.Length) return true;
                for (var skip = i; skip <= path.Length; skip++)
                {
                    if (Walk(pattern, p + 1, path, skip)) return true;
                }
                return false;
            }
            if (i >= path.Length) return false;
            if (!Segment(pattern[p], path[i])) return false;
            p++;
            i++;
        }
        return i == path.Length;
    }

    /// <summary>One segment, where `*` matches any run of characters within it and `?` exactly one.</summary>
    private static bool Segment(string pattern, string name)
    {
        var p = 0;
        var n = 0;
        var star = -1;
        var mark = 0;
        while (n < name.Length)
        {
            if (p < pattern.Length && (pattern[p] == '?'
                || char.ToLowerInvariant(pattern[p]) == char.ToLowerInvariant(name[n])))
            {
                p++;
                n++;
            }
            else if (p < pattern.Length && pattern[p] == '*')
            {
                // Remember where the star was: if what follows it fails further along, the star takes one
                // more character and the rest is tried again.
                star = p++;
                mark = n;
            }
            else if (star >= 0)
            {
                p = star + 1;
                n = ++mark;
            }
            else
            {
                return false;
            }
        }
        while (p < pattern.Length && pattern[p] == '*') p++;
        return p == pattern.Length;
    }
}
