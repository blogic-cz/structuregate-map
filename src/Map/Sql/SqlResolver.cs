namespace StructureGate;

/// <summary>An object a name can reach: its id, the database and file it is declared in, and its kind.</summary>
internal sealed record SqlHit(string Id, string Database, string Kind, string File);

/// <summary>
/// RESOLVES A NAME TO ONE OBJECT, never by a guess: among the databases the site may reach, the one that
/// has it; when several do and nothing narrows them, the configured `default` - a decision the consumer
/// wrote down, so the row is `bound` and its `detail` reads `default of A, B`; with no default among them,
/// `ambiguous`, the `detail` listing them and `database` empty.
///
/// A SCRIPT OUTSIDE EVERY `.sqlproj` IS NO DATABASE: its objects have database "", and they are candidates
/// only where no database has the name - `default of , Main` once listed one.
/// </summary>
internal sealed class SqlResolver(Dictionary<string, List<SqlHit>> index, SqlConfig config)
{
    /// <summary>SQL Server's own catalog is no object of any project, and not a gap in the map.</summary>
    private static readonly string[] System = ["sys", "INFORMATION_SCHEMA"];

    public (string Status, SqlHit? Hit, string Detail) Resolve(string schema, string name, HashSet<string>? allowed, params string[] kinds) =>
        IsSystem(schema, name, kinds) ? ("system", null, "SQL Server's own catalog")
            : Choose(index.GetValueOrDefault($"{schema}.{name}") ?? [], allowed, kinds);

    /// <summary>
    /// EVERY database of `allowed` that has the object, one result each - an EF mapping is applied to every
    /// context it is registered with, so each of their databases holds the table it maps. A database
    /// without it is left out; none with it is the one `missing` result `Resolve` gives.
    /// </summary>
    public List<(string Status, SqlHit? Hit, string Detail)> ResolveEach(string schema, string name, HashSet<string>? allowed, params string[] kinds)
    {
        if (allowed is not { Count: > 1 } || IsSystem(schema, name, kinds)) return [Resolve(schema, name, allowed, kinds)];
        var hits = (index.GetValueOrDefault($"{schema}.{name}") ?? [])
            .Where(h => kinds.Contains(h.Kind) && allowed.Contains(h.Database))
            .DistinctBy(h => h.Database, StringComparer.OrdinalIgnoreCase).OrderBy(h => h.Database, StringComparer.OrdinalIgnoreCase).ToList();
        if (hits.Count < 2) return [Resolve(schema, name, allowed, kinds)];
        var all = string.Join(", ", hits.Select(h => h.Database));
        return [.. hits.Select(h => ("bound", (SqlHit?)h, $"each database its contexts reach that has it: {all}"))];
    }

    public (string Status, SqlHit? Hit, string Detail) ResolveAnySchema(string name, HashSet<string>? allowed, params string[] kinds) =>
        Choose([.. index.Where(e => e.Key.EndsWith("." + name, StringComparison.OrdinalIgnoreCase)).SelectMany(e => e.Value)], allowed, kinds);

    /// <summary>The object of that name a FILE declares itself: a script's procedure reads the table the same
    /// script creates, whatever other database also has one.</summary>
    public SqlHit? Own(string schema, string name, string file, params string[] kinds) =>
        (index.GetValueOrDefault($"{schema}.{name}") ?? []).FirstOrDefault(h => h.File == file && kinds.Contains(h.Kind));

    private static bool IsSystem(string schema, string name, string[] kinds) =>
        System.Contains(schema, StringComparer.OrdinalIgnoreCase)
        || (kinds.Contains("procedure") && (name.StartsWith("sp_", StringComparison.OrdinalIgnoreCase) || name.StartsWith("xp_", StringComparison.OrdinalIgnoreCase)));

    private (string, SqlHit?, string) Choose(List<SqlHit> all, HashSet<string>? allowed, string[] kinds)
    {
        var hits = all.Where(h => kinds.Contains(h.Kind)).ToList();
        if (hits.Any(h => h.Database.Length > 0)) hits = [.. hits.Where(h => h.Database.Length > 0)];
        if (hits.Count == 0) return ("missing", null, kinds.Contains("trigger")
            ? "no CREATE TRIGGER declares it - a trigger built by dynamic SQL has no name to find"
            : "not in any database of the map");
        var scoped = allowed is { Count: > 0 } ? hits.Where(h => allowed.Contains(h.Database)).ToList() : hits;
        if (scoped.Count == 0) return ("missing", null, $"not in {string.Join(", ", allowed!.Order())}; it is in {string.Join(", ", hits.Select(h => h.Database).Distinct().Order())}");
        var databases = scoped.Select(h => h.Database).Distinct(StringComparer.OrdinalIgnoreCase).ToList();
        if (databases.Count == 1) return ("bound", scoped[0], allowed is { Count: > 0 } ? "" : "the only database that has it");
        var fallback = scoped.FirstOrDefault(h => string.Equals(h.Database, config.Default, StringComparison.OrdinalIgnoreCase));
        if (fallback is not null) return ("bound", fallback, $"default of {string.Join(", ", databases.Order())}");
        return ("ambiguous", null, $"ambiguous: {string.Join(", ", databases.Order())}");
    }
}
