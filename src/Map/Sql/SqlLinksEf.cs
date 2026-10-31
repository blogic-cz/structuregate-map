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
        // `classes.bases` is a list, so NODE ROWS (`classes__bases`), one item per base in order - never JSON text.
        var bases = new Dictionary<string, List<string>>(StringComparer.Ordinal);
        foreach (var row in Query(db, "SELECT c.qualname, b.value FROM classes c JOIN classes__bases b ON b.owner = c.id "
            + "AND b.parent = 0 WHERE b.node = 'string' ORDER BY c.rowid, b.ord") ?? [])
        {
            if (!bases.TryGetValue(Str(row[0]), out var list)) bases[Str(row[0])] = list = [];
            list.Add(Str(row[1]).Split('<')[0]);
        }
        string Member(string entity, string column)
        {
            var seen = new HashSet<string>(StringComparer.Ordinal);
            var queue = new Queue<string>([entity]);
            while (queue.Count > 0)
            {
                var type = queue.Dequeue();
                if (!seen.Add(type)) continue;
                if (properties.GetValueOrDefault(type)?.GetValueOrDefault(column) is { } found) return found;
                foreach (var parent in bases.GetValueOrDefault(type) ?? []) queue.Enqueue(parent);
            }
            return "";
        }

        var fluent = FluentColumns(db);
        void Table(string call, string file, long line, string cls, string func, string entity, string written, string table)
        {
            var allowed = Allowed(ClassAllowed($"{file}|{cls}"), byEntity.GetValueOrDefault(entity));
            var schema = SqlDeep.Schema(written);
            List<(string Status, SqlHit? Hit, string Detail)> each = table.Length == 0 ? [("unknown", null, "the table name is not a constant")]
                : resolver.ResolveEach(schema, table, allowed, "table", "view");
            foreach (var (status, hit, detail) in each)
            {
                links.Add(new Link("ef_table", file, line, call, cls, func, entity, "", "",
                    hit?.Database ?? "", schema, table, "", hit?.Id ?? "", "", status, detail));
                if (hit is null) continue;
                foreach (var (column, id) in columns.GetValueOrDefault(hit.Id) ?? [])
                {
                    // A COLUMN THE MAPPING NAMES (#128) is its `Property` call's line and property, with what the call's chain sets.
                    var said = (fluent.GetValueOrDefault($"{file}|{cls}|{func}|{entity}") ?? fluent.GetValueOrDefault(entity))?.GetValueOrDefault(column);
                    var member = Member(entity, said?.Property ?? column);
                    links.Add(new Link("ef_column", file, said?.Line ?? line, call, cls, func, entity, member, "",
                        hit.Database, schema, table, column, hit.Id, id, member.Length > 0 ? "bound" : "no property",
                        member.Length > 0 ? "" : "no property of that name on the entity or its bases - a column EF does not map") { Facets = said });
                }
            }
        }
        var derived = Derived(db);
        var spelled = Named(db);
        const string Arg = "(SELECT a.{1} FROM arguments a WHERE a.call = c.id AND a.position = {0}{2})";
        string Folded(int position) => string.Format(Arg, position, "const", " AND a.const_kind IN ('const', 'folded')");
        string Symbol(int position) => string.Format(Arg, position, "symbol", "");
        foreach (var row in Query(db, "SELECT c.id, c.file, c.line, c.cls, c.func, c.type, " + Folded(0) + ", " + Folded(1) + ", "
            + Symbol(0) + ", " + Symbol(1) + " FROM calls c WHERE c.symbol GLOB 'Microsoft.EntityFrameworkCore.*.ToTable*'") ?? [])
        {
            // A SNAPSHOT NAMES ITS ENTITY IN A STRING (#140): `b.ToTable(..)` inside `modelBuilder.Entity("Demo.Order", b => ..)`.
            var entity = Generic(Str(row[5])) is { Length: > 0 } generic ? generic : spelled(Str(row[1]), Num(row[2]));
            // A NAME THE BASE IS HANDED (#141): one link per class that constructs it with constants, none at the base.
            var through = Str(row[6]).Length == 0 ? derived(Str(row[1]), Str(row[3]), entity, Str(row[8]), Str(row[9]), Str(row[7])) : [];
            foreach (var site in through)
                Table(site.Call, site.File, site.Line, site.Cls, site.Func, site.Entity, site.Schema, site.Table);
            if (through.Count == 0)
                Table(Str(row[0]), Str(row[1]), Num(row[2]), Str(row[3]), Str(row[4]), entity, Str(row[7]), Str(row[6]));
        }
        var dynamic = Dynamic(db);
        foreach (var row in Query(db, "SELECT c.id, c.file, c.line, c.cls, c.func, " + Folded(0)
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
        // `row_values` is a list, so NODE ROWS (`sql_seeds__row_values`): a temp row's string values, in order.
        var temps = new Dictionary<string, (string File, long Line, List<string> Values)>(StringComparer.Ordinal);
        foreach (var row in Query(db, "SELECT s.id, s.file, s.line, v.value, v.node FROM sql_seeds s LEFT JOIN sql_seeds__row_values v "
            + "ON v.owner = s.id AND v.parent = 0 WHERE s.status = 'temp' ORDER BY s.rowid, v.ord") ?? [])
        {
            if (!temps.TryGetValue(Str(row[0]), out var temp)) temps[Str(row[0])] = temp = (Str(row[1]), Num(row[2]), []);
            if (Str(row[4]) == "string") temp.Values.Add(Str(row[3]));
        }
        foreach (var (file, line, values) in temps.Values)
        {
            if (!seeded.TryGetValue(file, out var rows)) seeded[file] = rows = [];
            rows.Add((line, values));
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
        // A MAP BUILT BEFORE `written_database` LACKS THE COLUMN: the query then fails and every ref would go unlinked.
        const string Select = "SELECT id, file, line, object, action, schema, name, column, database, {0} FROM sql_refs WHERE lang = 'sql'";
        var refs = Query(db, string.Format(Select, "written_database")) ?? Query(db, string.Format(Select, "'' AS written_database")) ?? [];
        foreach (var row in refs)
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
            // A DATABASE WRITTEN IN THE REF (`Beta.dbo.Orders`) IS THE SCOPE, not the project's own: otherwise a view
            // reading another project's table binds to the same-named table of its own project.
            var written = Str(row[9]);
            var scope = written.Length > 0 ? written : own;
            var allowed = scope.Length > 0 ? new HashSet<string>(StringComparer.OrdinalIgnoreCase) { scope } : null;
            string[] kinds = Str(row[4]) == "exec" ? ["procedure"] : ["table", "view", "function"];
            // THE FILE'S OWN OBJECT FIRST: a script's procedure reads the table that script creates.
            var elsewhere = written.Length > 0 && !written.Equals(own, StringComparison.OrdinalIgnoreCase);
            var (status, hit, detail) = !elsewhere && resolver.Own(Str(row[5]), Str(row[6]), Str(row[1]), kinds) is { } mine
                ? ("bound", mine, "declared in the same file")
                : resolver.Resolve(Str(row[5]), Str(row[6]), allowed, kinds);
            var column = Str(row[7]).Length > 0 && hit is not null ? columns.GetValueOrDefault(hit.Id)?.GetValueOrDefault(Str(row[7])) ?? "" : "";
            links.Add(new Link("sql_ref", Str(row[1]), Num(row[2]), Str(row[0]), "", "", "", Str(row[3]), Str(row[4]),
                hit?.Database ?? scope, Str(row[5]), Str(row[6]), Str(row[7]), hit?.Id ?? "", column, status, detail));
        }
    }

    /// <summary>
    /// The entity an EF MODEL SNAPSHOT spells as a string - `modelBuilder.Entity("Demo.Order", b => { b.ToTable("Orders"); })` -
    /// for a call at a file and line: the innermost such `Entity` call holding it, "" when none does.
    /// </summary>
    private static Func<string, long, string> Named(string db)
    {
        var spans = (Query(db, "SELECT c.file, c.line, c.end_line, a.const FROM calls c JOIN arguments a ON a.call = c.id AND a.position = 0 "
            + "WHERE c.symbol GLOB 'Microsoft.EntityFrameworkCore.ModelBuilder.Entity*' AND a.const_kind IN ('const', 'folded')") ?? [])
            .GroupBy(r => Str(r[0])).ToDictionary(g => g.Key, g => g.ToList(), StringComparer.Ordinal);
        return (file, line) => spans.GetValueOrDefault(file)?.Where(r => Num(r[1]) <= line && line <= Num(r[2]))
            .OrderByDescending(r => Num(r[1])).Select(r => Str(r[3])).FirstOrDefault() ?? "";
    }

    /// <summary>A derived class's base call that fixes what a base configuration's `ToTable` reads.</summary>
    private sealed record Site(string Call, string File, long Line, string Cls, string Func, string Entity, string Table, string Schema);

    /// <summary>
    /// A `ToTable(tableName, schemaName)` IN A BASE CONFIGURATION reads its own constructor's parameters - a primary
    /// constructor's, or a field its constructor sets from one - and each derived class fixes them in its base call
    /// (`class OrderConfig() : ConfigBase&lt;Order&gt;(Tables.Orders, Schemas.Sales)`, or `: base(...)`), closing the entity
    /// type parameter as it does (#141). One site per such call whose table argument is a constant; none when the
    /// base reads anything else, and then the base's own `unknown` link stands.
    /// </summary>
    private static Func<string, string, string, string, string, string, List<Site>> Derived(string db)
    {
        var classes = Query(db, "SELECT file, name, qualname, symbol FROM classes") ?? [];
        return (file, cls, entity, tableArg, schemaArg, writtenSchema) =>
        {
            var owner = classes.FirstOrDefault(c => Str(c[0]) == file && Str(c[1]) == cls);
            if (owner is null) return [];
            var ctor = $"{Str(owner[2])}.{cls}";
            var parameters = Query(db, $"SELECT qualname, name, position FROM parameters WHERE qualname = {Quote(ctor)}") ?? [];
            var assigned = Query(db, "SELECT file, cls, func, target, symbol FROM assignments WHERE kind = 'assign' "
                + $"AND file = {Quote(file)} AND cls = {Quote(cls)} AND func = {Quote(cls)}") ?? [];
            // The constructor parameter an argument reads: itself, or the one its field was set from in a constructor.
            long Position(string symbol)
            {
                var name = symbol.Split('.')[^1];
                var from = symbol.Contains('.')
                    ? assigned.Where(a => Str(a[0]) == file && Str(a[1]) == cls && Str(a[2]) == cls
                        && (Str(a[3]) == name || Str(a[3]) == "this." + name)).Select(a => Str(a[4])).FirstOrDefault() ?? ""
                    : name;
                return parameters.FirstOrDefault(p => Str(p[0]) == ctor && Str(p[1]) == from) is { } found ? Num(found[2]) : -1;
            }
            var table = Position(tableArg);
            if (table < 0) return [];
            var schema = schemaArg.Length > 0 ? Position(schemaArg) : -1;
            // EVERY CONSTRUCTION OF THE BASE: a derived class's base call, and a `new` of a base that is not abstract.
            var type = Str(owner[3]).Split('<')[0];
            var calls = Query(db, "SELECT c.id, c.file, c.line, c.cls, c.func, c.type, a.position, a.const, a.const_kind FROM calls c "
                + $"JOIN arguments a ON a.call = c.id WHERE (c.type = {Quote(type)} OR c.type GLOB {Quote(type + "<*")}) "
                + "ORDER BY c.rowid, a.position") ?? [];
            var open = TypeArgs(Str(owner[3]));
            var slot = open.IndexOf(entity);
            var sites = new List<Site>();
            foreach (var made in calls.GroupBy(c => Str(c[0])))
            {
                var args = made.ToDictionary(c => Num(c[6]), c => Str(c[8]) is "const" or "folded" ? Str(c[7]) : "");
                var name = args.GetValueOrDefault(table) ?? "";
                if (name.Length == 0) continue;
                var first = made.First();
                var closed = TypeArgs(Str(first[5]));
                var what = slot >= 0 && slot < closed.Count ? closed[slot] : entity;
                sites.Add(new Site(Str(first[0]), Str(first[1]), Num(first[2]), Str(first[3]), Str(first[4]), what, name,
                    schema >= 0 ? args.GetValueOrDefault(schema) ?? "" : writtenSchema));
            }
            return sites;
        };
    }

    /// <summary>`Demo.Base&lt;Demo.Order, int&gt;` -> [`Demo.Order`, `int`]: the top-level type arguments, nested ones whole.</summary>
    private static List<string> TypeArgs(string type)
    {
        var open = type.IndexOf('<');
        if (open < 0 || !type.EndsWith('>')) return [];
        var parts = new List<string>();
        var unclosed = 0;
        var start = open + 1;
        for (var i = start; i < type.Length - 1; i++)
        {
            if (type[i] == '<') unclosed++;
            else if (type[i] == '>') unclosed--;
            else if (type[i] == ',' && unclosed == 0)
            {
                parts.Add(type[start..i].Trim());
                start = i + 1;
            }
        }
        parts.Add(type[start..^1].Trim());
        return parts;
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
