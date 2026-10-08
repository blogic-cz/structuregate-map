namespace StructureGate;

/// <summary>
/// WHAT THE HALVES FILL IN - the vocabulary, with no join in it. Every producer of map rows writes these
/// and the join in `rust/fbtcore/src/graph/` reads them; keeping the two apart is what stops a question about a
/// FILE (what does this
/// row mean) from being answered in the middle of a question about the TREE (which file does this name
/// reach).
/// </summary>
/// <summary>
/// One mapped file, in the vocabulary every language half fills in. The halves differ in HOW they read a
/// file — Roslyn in this process, node and python out of it — and agree on WHAT they hand back, so the join
/// in rust is written once instead of once per language.
/// </summary>
public sealed class MapFile
{
    public required string Rel { get; init; }
    public int Lines { get; set; }
    public string Summary { get; set; } = "";

    /// <summary>What another file can name to reach this one: a C# type, a python module, a TS module path.</summary>
    public SortedSet<string> Declares { get; } = new(StringComparer.Ordinal);

    /// <summary>Names this file uses, joined against <see cref="Declares"/> across the tree.</summary>
    public SortedSet<string> Uses { get; } = new(StringComparer.Ordinal);

    /// <summary>An entry point is allowed to have no reader — a `Main`, a `__main__` guard, a CLI script.</summary>
    public bool Entry { get; set; }

    /// <summary>Written by a tool, not by a person. It is still in the graph — what it declares and uses is
    /// real — but it is not fingerprinted for duplicates and the deep map holds no rows of it.</summary>
    public bool Generated { get; set; }

    /// <summary>What a framework activates by convention - an MVC controller, a Razor page model, a SignalR hub. Nothing
    /// in the tree names it, and it is still entered: the file is read through it, as a python `@app.route` is.</summary>
    public SortedSet<string> Registered { get; } = new(StringComparer.Ordinal);
}

/// <summary>What the halves fill in, before anything is joined.</summary>
public sealed class MapCollector
{
    /// <summary>fingerprint -> the places that share it. A function body, and an expression, in two lists:
    /// a body only sees whole functions, so an idiom pasted INSIDE larger ones is invisible to it.</summary>
    public Dictionary<string, List<(string Where, int Size)>> Bodies { get; } = new(StringComparer.Ordinal);
    public Dictionary<string, List<(string Where, int Size)>> Expressions { get; } = new(StringComparer.Ordinal);

    /// <summary>A state that cannot be legitimate: a file that does not parse, a half that would not run.</summary>
    public List<string> Errors { get; } = [];

    /// <summary>Something a half wants said that is not a defect - a cache it replaced, a slow path taken.</summary>
    public List<string> Notes { get; } = [];

    /// <summary>
    /// Imports whose target is BUILT AT RUN TIME — a dot-source of a variable, `importlib.import_module(x)`,
    /// an `import(expr)`. Not edges: this pass cannot know what they resolve to, and a guessed edge is worse
    /// than a missing one. Listed rather than dropped, because they are the reason a file with no readers
    /// may still have one, and a blind spot nobody is told about gets acted on as if it were not there.
    /// </summary>
    public List<ComputedImport> Computed { get; } = [];
}

/// <summary>
/// One import whose target is built at RUN TIME. Kept as a RECORD and not a formatted line, because the
/// ratchet keys on the file and the shape and NOT on the line number: a dynamic call does not become a
/// different one because something above it was edited, and a baseline that churned on every unrelated
/// edit would be turned off within a week.
/// </summary>
public sealed record ComputedImport(string Rel, string Line, string What);
