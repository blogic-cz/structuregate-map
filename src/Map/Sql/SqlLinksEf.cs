using System.Text.Json;

namespace StructureGate;

/// <summary>The EF links (`ef_table`, `ef_column`, `ef_trigger`) and the links of the SQL scripts themselves
/// (`sql_ref`, `sql_include`) - see `SqlLinks`.</summary>
internal static partial class SqlLinks
{
    private static void EntityFramework(string db, SqlConfig config, SqlResolver resolver,
        Dictionary<string, Dictionary<string, string>> columns, List<Link> links)
    {
        // WHICH DATABASE A MAPPING CLASS IS FOR: a configured context in `typeof(...)` on it -
        // `[Demo(typeof(DemoContext))]` - and, per entity, a context holding a `DbSet` of it.
        var byClass = new Dictionary<string, HashSet<string>>(StringComparer.Ordinal);
        // A class with ANY attribute naming a type is not unattributed, whether or not the config knows the type.
        var typed = new HashSet<string>(StringComparer.Ordinal);
        foreach (var row in Query(db, "SELECT file, target, source FROM decorators WHERE target_kind = 'class' AND source GLOB '*typeof(*'") ?? [])
        {
            var key = $"{Str(row[0])}|{Str(row[1])}";
            typed.Add(key);
            foreach (var database in config.Databases)
            {
                if (!database.Contexts.Any(c => Str(row[2]).Contains($"typeof({c})", StringComparison.Ordinal)
                        || Str(row[2]).Contains($"typeof({c.Split('.')[^1]})", StringComparison.Ordinal))) continue;
                if (!byClass.TryGetValue(key, out var set)) byClass[key] = set = new(StringComparer.OrdinalIgnoreCase);
                set.Add(database.Name);
            }
        }
        // A MAPPING WITH NO SUCH ATTRIBUTE goes where the consumer's own registration sends it: a consumer may apply every
        // one to several contexts, so `Demo.ItemMapping` maps Catalog.Items in Main AND in Archive.
        var unattributed = new HashSet<string>(config.Databases
            .Where(d => d.Contexts.Any(c => config.UnattributedContexts.Any(u => u == c || u.Split('.')[^1] == c.Split('.')[^1])))
            .Select(d => d.Name), StringComparer.OrdinalIgnoreCase);
        var byEntity = new Dictionary<string, HashSet<string>>(StringComparer.Ordinal);
        foreach (var row in Query(db, "SELECT qualname, type FROM functions WHERE kind = 'property' AND type GLOB '*DbSet<*'") ?? [])
        {
            var type = Str(row[1]);
            var entity = Generic(type);
            foreach (var database in config.Databases.Where(d => d.Contexts.Any(c => Str(row[0]).StartsWith(c + ".", StringComparison.Ordinal))))
            {
                if (!byEntity.TryGetValue(entity, out var set)) byEntity[entity] = set = new(StringComparer.OrdinalIgnoreCase);
                set.Add(database.Name);
            }
        }
        HashSet<string>? ClassAllowed(string key) => byClass.GetValueOrDefault(key)
            ?? (typed.Contains(key) || unattributed.Count == 0 ? null : unattributed);
        var properties = new Dictionary<string, Dictionary<string, string>>(StringComparer.Ordinal);
        foreach (var row in Query(db, "SELECT qualname, name FROM functions WHERE kind = 'property'") ?? [])
        {
            var qualname = Str(row[0]);
            var name = Str(row[1]);
            if (qualname.Length <= name.Length + 1) continue;
            var owner = qualname[..(qualname.Length - name.Length - 1)];
            if (!properties.TryGetValue(owner, out var named)) properties[owner] = named = new(StringComparer.OrdinalIgnoreCase);
            named.TryAdd(name, qualname);
        }

        // AN INHERITED PROPERTY IS THE ENTITY'S TOO: `IsActive` lives on `EntityBase`, and many columns read
        // as "no property" while only the entity's own body was searched.
        var bases = new Dictionary<string, List<string>>(StringComparer.Ordinal);
        foreach (var row in Query(db, "SELECT qualname, bases FROM classes WHERE bases GLOB '[[]\"*'") ?? [])
        {
            try
            {
                using var list = JsonDocument.Parse(Str(row[1]));
                bases[Str(row[0])] = [.. list.RootElement.EnumerateArray().Select(b => (b.GetString() ?? "").Split('<')[0])];
            }
            catch (JsonException) { }
        }
        string Member(string entity, string column)
        {
            var seen = new HashSet<string>(StringComparer.Ordinal);
            var queue = new Queue<string>([entity]);
            while (queue.Count > 0 && seen.Count < 16)
            {
                var type = queue.Dequeue();
                if (!seen.Add(type)) continue;
                if (properties.GetValueOrDefault(type)?.GetValueOrDefault(column) is { } found) return found;
                foreach (var parent in bases.GetValueOrDefault(type) ?? []) queue.Enqueue(parent);
            }
            return "";
        }

        const string Arg = "(SELECT a.const FROM arguments a WHERE a.call = c.id AND a.position = {0} AND a.const_kind IN ('const', 'folded'))";
        foreach (var row in Query(db, "SELECT c.id, c.file, c.line, c.cls, c.func, c.type, " + string.Format(Arg, 0) + ", "
            + string.Format(Arg, 1) + " FROM calls c WHERE c.symbol GLOB 'Microsoft.EntityFrameworkCore.*.ToTable*'") ?? [])
        {
            var entity = Generic(Str(row[5]));
            var allowed = Allowed(ClassAllowed($"{Str(row[1])}|{Str(row[3])}"), byEntity.GetValueOrDefault(entity));
            var schema = SqlDeep.Schema(Str(row[7]));
            var table = Str(row[6]);
            List<(string Status, SqlHit? Hit, string Detail)> each = table.Length == 0 ? [("unknown", null, "the table name is not a constant")]
                : resolver.ResolveEach(schema, table, allowed, "table", "view");
            foreach (var (status, hit, detail) in each)
            {
                links.Add(new Link("ef_table", Str(row[1]), Num(row[2]), Str(row[0]), Str(row[3]), Str(row[4]), entity, "", "",
                    hit?.Database ?? "", schema, table, "", hit?.Id ?? "", "", status, detail));
                if (hit is null) continue;
                foreach (var (column, id) in columns.GetValueOrDefault(hit.Id) ?? [])
                {
                    var member = Member(entity, column);
                    links.Add(new Link("ef_column", Str(row[1]), Num(row[2]), Str(row[0]), Str(row[3]), Str(row[4]), entity, member, "",
                        hit.Database, schema, table, column, hit.Id, id, member.Length > 0 ? "bound" : "no property",
                        member.Length > 0 ? "" : "no property of that name on the entity or its bases - a column EF does not map"));
                }
            }
        }
        var dynamic = Dynamic(db);
        foreach (var row in Query(db, "SELECT c.id, c.file, c.line, c.cls, c.func, " + string.Format(Arg, 0)
            + " FROM calls c WHERE c.symbol GLOB 'Microsoft.EntityFrameworkCore.*.HasTrigger*'") ?? [])
        {
            var allowed = ClassAllowed($"{Str(row[1])}|{Str(row[3])}");
            var (status, hit, detail) = resolver.ResolveAnySchema(Str(row[5]), allowed, "trigger");
            var database = hit?.Database ?? "";
            if (hit is null && status == "missing" && dynamic(Str(row[5])) is { } built) (status, database, detail) = built;
            links.Add(new Link("ef_trigger", Str(row[1]), Num(row[2]), Str(row[0]), Str(row[3]), Str(row[4]), "", "", "",
                database, "", Str(row[5]), "", hit?.Id ?? "", "", status, detail));
        }
    }

    /// <summary>
    /// A TRIGGER NO `CREATE TRIGGER` DECLARES, matched against the ones dynamic SQL builds (`sql_dynamic`):
    /// dynamic SQL that builds the name from a prefix and a value (`'CREATE TRIGGER Demo_' + @Name`, the value
    /// from a VALUES list) creates `Demo_Items` when a row says `Items` - `dynamic`, with the script's line and the row's.
    /// A name the list does NOT hold stays `missing`, and says so: that trigger is never built. Where the script
    /// lists no rows, the prefix is all there is to match, and the row is `dynamic` on it alone.
    /// </summary>
    private static Func<string, (string Status, string Database, string Detail)?> Dynamic(string db)
    {
        var built = Query(db, "SELECT d.file, d.line, d.name, d.open, d.database, f.path FROM sql_dynamic d JOIN files f ON f.id = d.file "
            + "WHERE d.kind = 'trigger' ORDER BY f.path, d.line") ?? [];
        var seeded = new Dictionary<string, List<(long Line, List<string> Values)>>(StringComparer.Ordinal);
        foreach (var row in Query(db, "SELECT file, line, row_values FROM sql_seeds WHERE status = 'temp'") ?? [])
        {
            List<string> values;
            try
            {
                using var document = JsonDocument.Parse(Str(row[2]));
                values = [.. document.RootElement.EnumerateArray().Where(v => v.ValueKind == JsonValueKind.String).Select(v => v.GetString() ?? "")];
            }
            catch (JsonException) { continue; }
            if (!seeded.TryGetValue(Str(row[0]), out var rows)) seeded[Str(row[0])] = rows = [];
            rows.Add((Num(row[1]), values));
        }
        return name =>
        {
            foreach (var row in built)
            {
                var prefix = Str(row[2]);
                var open = Num(row[3]) == 1;
                var at = $"{Str(row[5])}:{Num(row[1])}";
                if (!open && string.Equals(prefix, name, StringComparison.OrdinalIgnoreCase))
                    return ("dynamic", Str(row[4]), $"created by dynamic SQL at {at}");
                if (!open || name.Length <= prefix.Length || !name.StartsWith(prefix, StringComparison.OrdinalIgnoreCase)) continue;
                var rest = name[prefix.Length..];
                var rows = seeded.GetValueOrDefault(Str(row[0])) ?? [];
                if (rows.Count == 0)
                    return ("dynamic", Str(row[4]), $"created by dynamic SQL at {at} as '{prefix}' + a value the script does not list");
                var listed = rows.FirstOrDefault(r => r.Values.Contains(rest, StringComparer.OrdinalIgnoreCase));
                return listed.Values is not null
                    ? ("dynamic", Str(row[4]), $"created by dynamic SQL at {at} as '{prefix}' + '{rest}', a row of the list at {Str(row[5])}:{listed.Line}")
                    : ("missing", "", $"the dynamic SQL at {at} builds '{prefix}' + a row of the script's VALUES list, and no row is '{rest}'");
            }
            return null;
        };
    }

    private static void Scripts(string db, SqlResolver resolver, Dictionary<string, Dictionary<string, string>> columns, List<Link> links)
    {
        var files = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase);
        var paths = new Dictionary<string, string>(StringComparer.Ordinal);
        foreach (var row in Query(db, "SELECT id, path FROM files WHERE lang = 'sql'") ?? [])
        {
            files[Str(row[1])] = Str(row[0]);
            paths[Str(row[0])] = Str(row[1]);
        }
        foreach (var row in Query(db, "SELECT id, file, line, object, action, schema, name, column, database FROM sql_refs WHERE lang = 'sql'") ?? [])
        {
            if (Str(row[4]) == "include")
            {
                // `:r .\Data\Seed.sql` is relative to the script that runs it.
                var from = paths.GetValueOrDefault(Str(row[1])) ?? "";
                var target = Normal(Path.Combine(Path.GetDirectoryName(from) ?? "", Str(row[6])));
                var id = files.GetValueOrDefault(target) ?? "";
                links.Add(new Link("sql_include", Str(row[1]), Num(row[2]), Str(row[0]), "", "", "", "", "include", Str(row[8]),
                    "", target, "", id, "", id.Length > 0 ? "bound" : "missing", id.Length > 0 ? "" : "no mapped file at that path"));
                continue;
            }
            var own = Str(row[8]);
            var allowed = own.Length > 0 ? new HashSet<string>(StringComparer.OrdinalIgnoreCase) { own } : null;
            string[] kinds = Str(row[4]) == "exec" ? ["procedure"] : ["table", "view", "function"];
            // THE FILE'S OWN OBJECT FIRST: a script's procedure reads the table that script creates.
            var (status, hit, detail) = resolver.Own(Str(row[5]), Str(row[6]), Str(row[1]), kinds) is { } mine
                ? ("bound", mine, "declared in the same file")
                : resolver.Resolve(Str(row[5]), Str(row[6]), allowed, kinds);
            var column = Str(row[7]).Length > 0 && hit is not null ? columns.GetValueOrDefault(hit.Id)?.GetValueOrDefault(Str(row[7])) ?? "" : "";
            links.Add(new Link("sql_ref", Str(row[1]), Num(row[2]), Str(row[0]), "", "", "", Str(row[3]), Str(row[4]),
                hit?.Database ?? own, Str(row[5]), Str(row[6]), Str(row[7]), hit?.Id ?? "", column, status, detail));
        }
    }

    /// <summary>EVERY database a site may reach, from both sources: a mapping class without an attribute
    /// binds to the DEFAULT contexts, so a `DbSet` in one context does not exclude the others - a
    /// `Catalog.Items` read as "not in Archive" when its DbSet alone decided.</summary>
    private static HashSet<string>? Allowed(HashSet<string>? a, HashSet<string>? b)
    {
        if (a is not { Count: > 0 } && b is not { Count: > 0 }) return null;
        var all = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
        if (a is not null) all.UnionWith(a);
        if (b is not null) all.UnionWith(b);
        return all;
    }

    private static string Generic(string type)
    {
        var open = type.IndexOf('<');
        var close = type.LastIndexOf('>');
        return open >= 0 && close > open ? type[(open + 1)..close] : "";
    }

    internal static string Normal(string path)
    {
        var parts = new List<string>();
        foreach (var part in path.Replace('\\', '/').Split('/'))
        {
            if (part is "" or ".") continue;
            if (part == ".." && parts.Count > 0) parts.RemoveAt(parts.Count - 1);
            else parts.Add(part);
        }
        return string.Join('/', parts);
    }
}
