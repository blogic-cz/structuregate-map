using System.Text;
using System.Text.Json;

namespace StructureGate;

/// <summary>
/// C# TO THE DATABASE: `sql_links`, derived AFTER both halves stored their rows and rewritten whole on
/// every run, because it is a fact about two halves and either may have moved.
///
/// Five kinds, each row naming its C# (or SQL) site and the database object it reaches:
/// `ef_table` (an EF `ToTable` call: entity class to table), `ef_column` (each column of that table to the
/// entity property of the same name, or `no property`), `ef_trigger` (`HasTrigger`), `sql_text` (a table a
/// SQL string in C# touches - Dapper, `FromSqlRaw` - parsed by the same T-SQL parser), and `sql_ref` /
/// `sql_include` (what a view, procedure or seed script touches, and the scripts a deploy script runs).
///
/// A NAME IS RESOLVED, NEVER GUESSED (`SqlResolver`): among the databases the config allows for that site - a
/// context named in `typeof(...)` on the mapping class or holding a `DbSet` of the entity, a connection
/// name written in the same method - the one that has the object. Two that have it fall to `default`;
/// otherwise the row says `ambiguous` and lists them. A SQL string whose holes name what it touches is
/// `unknown`, or `partial` beside the tables that ARE known (`SqlHoles`).
/// </summary>
internal static partial class SqlLinks
{
    private sealed record Link(string Kind, string File, long Line, string From, string Cls, string Func,
        string Entity, string Member, string Action, string Database, string Schema, string Name, string Column,
        string Object, string ColumnId, string Status, string Detail)
    {
        /// <summary>What a fluent mapping says of an `ef_column`'s column (#128, `SqlLinksEfFacets`); null for every other link.</summary>
        public Fluent? Facets { get; init; }
    }

    public static void Run(MapCollector collected, string db, SqlConfig config)
    {
        var objects = Query(db, "SELECT id, database, schema, name, kind, file FROM sql_objects");
        if (objects is null) return;
        var index = new Dictionary<string, List<SqlHit>>(StringComparer.OrdinalIgnoreCase);
        foreach (var row in objects)
        {
            var key = $"{Str(row[2])}.{Str(row[3])}";
            if (!index.TryGetValue(key, out var hits)) index[key] = hits = [];
            hits.Add(new SqlHit(Str(row[0]), Str(row[1]), Str(row[4]), Str(row[5])));
        }
        var columns = new Dictionary<string, Dictionary<string, string>>(StringComparer.Ordinal);
        foreach (var row in Query(db, "SELECT id, object, name FROM sql_columns") ?? [])
        {
            if (!columns.TryGetValue(Str(row[1]), out var named)) columns[Str(row[1])] = named = new(StringComparer.OrdinalIgnoreCase);
            named.TryAdd(Str(row[2]), Str(row[0]));
        }

        var links = new List<Link>();
        var resolver = new SqlResolver(index, config);
        EntityFramework(db, config, resolver, columns, links);
        SqlText(db, config, resolver, columns, links);
        Scripts(db, resolver, columns, links);

        var written = Write(db, links);
        if (written is not null) { collected.Errors.Add($"SQL LINKS were not written — {written}"); return; }
        var counts = links.GroupBy(l => l.Status).OrderBy(g => g.Key, StringComparer.Ordinal).Select(g => $"{g.Count()} {g.Key}");
        collected.Notes.Add($"the sql links: {string.Join(", ", counts)} ({links.Count} row(s) in sql_links)");
        if (config.Problem.Length > 0) collected.Errors.Add($"SQL CONFIG {config.Problem}");
    }

    private static void SqlText(string db, SqlConfig config, SqlResolver resolver,
        Dictionary<string, Dictionary<string, string>> columns, List<Link> links)
    {
        var globs = string.Join(" OR ", config.SqlText.Select(p => $"c.symbol GLOB {Quote(p)}"));
        // AN ARGUMENT'S TEMPLATE IS NODE ROWS (`arguments__template`), read once and joined by the argument's id.
        var templates = Templates(db);
        var rows = Query(db, "SELECT c.id, c.file, c.line, c.cls, c.func, a.const, a.const_kind, a.name, a.position, a.type, "
            + $"a.id FROM calls c JOIN arguments a ON a.call = c.id WHERE ({globs}) ORDER BY c.id, a.position") ?? [];
        // WHAT A FIELD FOLDED TO WHERE IT IS DECLARED, by symbol: a `static readonly` built from consts in a REFERENCED
        // project is compiled here without its source, so the template could only keep it as a hole.
        var folded = new Dictionary<string, string>(StringComparer.Ordinal);
        foreach (var row in Query(db, "SELECT c.symbol || '.' || k.name, k.value FROM consts k JOIN classes c ON c.file = k.file AND c.name = k.cls "
            + "WHERE k.value_kind IN ('const', 'folded') AND k.value IS NOT NULL AND c.symbol <> ''") ?? [])
            folded.TryAdd(Str(row[0]), Str(row[1]));
        // A CONNECTION NAME WRITTEN IN THE SAME METHOD says which database the SQL there runs on.
        var names = config.Databases.SelectMany(d => d.Connections.Select(c => (c, d.Name))).ToList();
        var byMethod = new Dictionary<string, HashSet<string>>(StringComparer.Ordinal);
        if (names.Count > 0)
        {
            var list = string.Join(", ", names.Select(n => Quote(n.c)));
            // A literal, or an argument that FOLDS to the name - `GetConnectionString(ConnectionString.Archive)`.
            foreach (var row in Query(db, $"SELECT file, func, value FROM string_literals WHERE value IN ({list}) "
                + $"UNION SELECT c.file, c.func, a.const FROM arguments a JOIN calls c ON c.id = a.call WHERE a.const IN ({list})") ?? [])
            {
                var key = $"{Str(row[0])}|{Str(row[1])}";
                if (!byMethod.TryGetValue(key, out var set)) byMethod[key] = set = new(StringComparer.OrdinalIgnoreCase);
                foreach (var (_, database) in names.Where(n => n.c == Str(row[2]))) set.Add(database);
            }
        }
        foreach (var call in rows.GroupBy(r => Str(r[0])))
        {
            // THE SQL ARGUMENT: the one the callee names `sql`/`commandText`, else the first string.
            var args = call.ToList();
            var sql = args.FirstOrDefault(a => Str(a[7]) is "sql" or "commandText" or "sqlQuery" or "query")
                ?? args.FirstOrDefault(a => Str(a[9]) == "string");
            if (sql is null) continue;
            var first = args[0];
            var allowed = byMethod.GetValueOrDefault($"{Str(first[1])}|{Str(first[4])}");
            Link Row(string action, string database, string schema, string name, string column, string obj, string columnId,
                string status, string detail) => new("sql_text", Str(first[1]), Num(first[2]), call.Key, Str(first[3]), Str(first[4]),
                    "", "", action, database, schema, name, column, obj, columnId, status, detail);
            var strings = Str(sql[6]) is "const" or "folded" ? [[(Str(sql[5]), false)]] : Strings(templates.GetValueOrDefault(Str(sql[10])), folded);
            if (strings.Count == 0)
            {
                links.Add(Row("", "", "", "", "", "", "", "unknown", "the SQL is built at run time"));
                continue;
            }
            foreach (var pieces in strings)
            {
                var reading = SqlHoles.Read(pieces);
                var of = strings.Count > 1 ? $"one of the {strings.Count} strings it runs" : "";
                if (reading.Visitor is null)
                {
                    links.Add(reading.Holes.Count == 0 ? Row("", "", "", "", "", "", "", "unparsed", reading.Failed)
                        : Row("", "", "", "", "", "", "", "unknown",
                            Joined($"the SQL is built at run time, and does not parse with its holes filled ({reading.Failed})", of)));
                    continue;
                }
                var touches = reading.Visitor.Touches.DistinctBy(t => (t.Action, t.Schema, t.Name, t.Column))
                    .Where(t => !Marked(t.Column)).ToList();
                var known = touches.Where(t => !Marked(t.Schema) && !Marked(t.Name)).ToList();
                var open = reading.Holes.Where(h => !h.Value).Select(h => Brief(h.Source)).ToList();
                var values = reading.Holes.Where(h => h.Value).Select(h => Brief(h.Source)).ToList();
                if (open.Count > 0 && known.Count == 0)
                {
                    links.Add(Row("", "", "", "", "", "", "", "unknown",
                        Joined($"the SQL is built at run time: {string.Join(", ", open)} name(s) what it touches", of)));
                    continue;
                }
                var note = Joined(open.Count > 0 ? $"part of the SQL is built at run time: {string.Join(", ", open)}"
                    : values.Count > 0 ? $"its holes are values: {string.Join(", ", values)}" : "", of);
                foreach (var touch in known)
                {
                    var schema = SqlDeep.Schema(touch.Schema);
                    var (status, hit, detail) = touch.Action == "exec"
                        ? resolver.Resolve(schema, touch.Name, allowed, "procedure")
                        : resolver.Resolve(schema, touch.Name, allowed, "table", "view", "function");
                    var column = touch.Column.Length > 0 && hit is not null ? columns.GetValueOrDefault(hit.Id)?.GetValueOrDefault(touch.Column) ?? "" : "";
                    links.Add(Row(touch.Action, hit?.Database ?? "", schema, touch.Name, touch.Column, hit?.Id ?? "", column, status, Joined(detail, note)));
                }
                if (open.Count == 0) continue;
                // WHAT IS NOT KNOWN beside what is: a name built at run time, spelled with its hole - or, where no
                // name holds one, the SQL a hole stands for (`FROM Archive.Events {condition}`).
                var dynamic = touches.Where(t => Marked(t.Schema) || Marked(t.Name)).ToList();
                foreach (var touch in dynamic)
                {
                    links.Add(Row(touch.Action, "", SqlDeep.Schema(Spelled(touch.Schema, reading)), Spelled(touch.Name, reading), "", "", "",
                        "partial", Joined("the name is built at run time; the rest of this SQL is bound", of)));
                }
                if (dynamic.Count == 0)
                {
                    links.Add(Row("", "", "", "", "", "", "", "partial",
                        Joined($"{string.Join(", ", open)} is SQL built at run time, and may touch more than the rows bound beside it", of)));
                }
            }
        }
    }

    /// <summary>A template piece as stored: a string, or a hole with the symbol it names ("" for none).</summary>
    private sealed record TemplatePiece(string Text, bool Hole, string Symbol);

    /// <summary>Every argument's template, by argument id, rebuilt from its node rows: the root list's items are the
    /// strings, each string's items its pieces - a `string` node is text, an `object` node a hole. A hole's `hole`
    /// and `symbol` are node rows BELOW it, their key in `role`: the columns of a node table are fixed. A node is
    /// (`owner`, `n`), its `parent` the `n` of its parent within the same owner, NULL for the root (`n` = 0).</summary>
    private static Dictionary<string, List<List<TemplatePiece>>> Templates(string db)
    {
        var templates = new Dictionary<string, List<List<TemplatePiece>>>(StringComparer.Ordinal);
        var columns = (Query(db, "SELECT name FROM pragma_table_info('arguments__template')") ?? []).Select(r => Str(r[0])).ToHashSet();
        if (!columns.Contains("owner")) return templates;
        // Keyed `owner` + `n`: a node number is unique only within its owner.
        var strings = new Dictionary<(string, long), List<TemplatePiece>>();
        // A HOLE'S MEMBERS FOLLOW IT in preorder, so each hole is found by its number and filled in place.
        var holes = new Dictionary<(string, long), (List<TemplatePiece> Pieces, int At)>();
        foreach (var row in Query(db, "SELECT owner, n, parent, node, value, role FROM arguments__template ORDER BY owner, n") ?? [])
        {
            var (owner, n, node) = (Str(row[0]), Num(row[1]), Str(row[3]));
            if (row[2].ValueKind != JsonValueKind.Number) { templates[owner] = []; continue; }
            var parent = (owner, Num(row[2]));
            if (!templates.TryGetValue(owner, out var all)) continue;
            if (parent.Item2 == 0) { var one = new List<TemplatePiece>(); strings[(owner, n)] = one; all.Add(one); continue; }
            if (holes.TryGetValue(parent, out var of))
            {
                var piece = of.Pieces[of.At];
                of.Pieces[of.At] = Str(row[5]) switch
                {
                    "hole" => piece with { Text = Str(row[4]) },
                    "symbol" => piece with { Symbol = Str(row[4]) },
                    _ => piece,
                };
                continue;
            }
            if (!strings.TryGetValue(parent, out var pieces)) continue;
            if (node == "object") { holes[(owner, n)] = (pieces, pieces.Count); pieces.Add(new TemplatePiece("", true, "")); }
            else pieces.Add(new TemplatePiece(Str(row[4]), false, ""));
        }
        return templates;
    }

    /// <summary>The strings `arguments.template` says an argument can be, as pieces; none without a template. A hole
    /// naming a field whose declaration folded (`folded`, by symbol) is that value.</summary>
    private static List<List<(string Text, bool Hole)>> Strings(List<List<TemplatePiece>>? template, Dictionary<string, string> folded) =>
        template is null ? [] : [.. template.Select(s => s.Select(p =>
            !p.Hole ? (p.Text, false)
            : p.Symbol.Length > 0 && folded.TryGetValue(p.Symbol, out var value) ? (value, false)
            : (p.Text, true)).ToList())];

    private static bool Marked(string name) => name.Contains(SqlHoles.Marker(0)[..6], StringComparison.OrdinalIgnoreCase);

    /// <summary>A name with each hole marker spelled back as its C# source: `{prefix}_Items`.</summary>
    private static string Spelled(string name, SqlHoles.Reading reading)
    {
        for (var i = reading.Holes.Count - 1; i >= 0; i--)
            name = name.Replace(SqlHoles.Marker(i), "{" + reading.Holes[i].Source + "}", StringComparison.OrdinalIgnoreCase);
        return name;
    }

    private static string Brief(string source) => source.Length <= 60 ? source : source[..57] + "...";

    private static string Joined(string a, string b) => a.Length == 0 ? b : b.Length == 0 ? a : $"{a}; {b}";

    /// <summary>
    /// Dapper's LIST PARAMETER, `WHERE ID IN @ids`, as Dapper itself sends it - `IN (@ids)`. It is not T-SQL,
    /// and a few of a large consumer's SQL strings failed to parse over it. Lines do not move: only a pair of parentheses
    /// is inserted on the same line.
    /// </summary>
    public static string Lists(string sql)
    {
        var built = new StringBuilder(sql.Length + 8);
        for (var i = 0; i < sql.Length; i++)
        {
            built.Append(sql[i]);
            var word = i + 2 < sql.Length && char.ToLowerInvariant(sql[i]) == 'i' && char.ToLowerInvariant(sql[i + 1]) == 'n'
                && (i == 0 || char.IsWhiteSpace(sql[i - 1])) && char.IsWhiteSpace(sql[i + 2]);
            if (!word) continue;
            var at = i + 2;
            while (at < sql.Length && char.IsWhiteSpace(sql[at])) at++;
            if (at >= sql.Length || sql[at] != '@') continue;
            var end = at + 1;
            while (end < sql.Length && (char.IsLetterOrDigit(sql[end]) || sql[end] == '_')) end++;
            built.Append(sql[i + 1]).Append(sql, i + 2, at - i - 2).Append('(').Append(sql, at, end - at).Append(')');
            i = end - 1;
        }
        return built.ToString();
    }

    /// <summary>The table, rewritten whole, in one transaction.</summary>
    private static string? Write(string db, List<Link> links)
    {
        var script = new StringBuilder("BEGIN; DROP TABLE IF EXISTS sql_links; CREATE TABLE sql_links (id, kind, file, line, from_id, cls, func, "
            + "entity, member, action, database, schema, name, column, object, column_id, status, detail, "
            + "precision, scale, max_length, required, column_type, conversion);\n");
        for (var start = 0; start < links.Count; start += 400)
        {
            script.Append("INSERT INTO sql_links VALUES ");
            var first = true;
            for (var i = start; i < Math.Min(start + 400, links.Count); i++)
            {
                var l = links[i];
                if (!first) script.Append(',');
                first = false;
                script.Append('(').Append(Quote($"lnk:{i + 1}"));
                foreach (var cell in new[] { l.Kind, l.File }) script.Append(',').Append(Quote(cell));
                script.Append(',').Append(l.Line);
                foreach (var cell in new[] { l.From, l.Cls, l.Func, l.Entity, l.Member, l.Action, l.Database, l.Schema,
                    l.Name, l.Column, l.Object, l.ColumnId, l.Status, l.Detail }) script.Append(',').Append(Quote(cell));
                // A FACET THE MAPPING DOES NOT SET IS NULL, never '' - "no max length" is not "max length ''".
                var f = l.Facets;
                foreach (var cell in new[] { f?.Precision, f?.Scale, f?.MaxLength, f?.Required, f?.ColumnType, f?.Conversion })
                    script.Append(',').Append(string.IsNullOrEmpty(cell) ? "NULL" : Quote(cell));
                script.Append(')');
            }
            script.Append(";\n");
        }
        script.Append("CREATE INDEX ix_sql_links_object ON sql_links(object); CREATE INDEX ix_sql_links_from ON sql_links(from_id); "
            + "CREATE INDEX ix_sql_links_entity ON sql_links(entity); COMMIT;");
        var reply = FbtCore.SqlRun(db, script.ToString(), "");
        if (reply is null) return "the store did not answer";
        using var document = JsonDocument.Parse(reply);
        return document.RootElement.TryGetProperty("error", out var error) ? error.GetString() : null;
    }

    /// <summary>Rows of one query, or null when it failed (a table that is not there: nothing to link).</summary>
    internal static List<JsonElement[]>? Query(string db, string sql)
    {
        var reply = FbtCore.SqlRun(db, "", sql);
        if (reply is null) return null;
        using var document = JsonDocument.Parse(reply);
        if (document.RootElement.TryGetProperty("error", out _)) return null;
        return [.. document.RootElement.GetProperty("rows").EnumerateArray().Select(r => r.EnumerateArray().Select(c => c.Clone()).ToArray())];
    }

    internal static string Str(JsonElement cell) => cell.ValueKind switch
    {
        JsonValueKind.String => cell.GetString() ?? "",
        JsonValueKind.Number => cell.GetRawText(),
        _ => "",
    };

    internal static long Num(JsonElement cell) => cell.ValueKind == JsonValueKind.Number && cell.TryGetInt64(out var n) ? n
        : long.TryParse(Str(cell), out var parsed) ? parsed : 0;

    internal static string Quote(string text) => "'" + text.Replace("'", "''") + "'";
}
