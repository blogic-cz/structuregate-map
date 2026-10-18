using System.Text.Json;

namespace StructureGate;

/// <summary>
/// A POSTGRESQL FILE'S ROWS, parsed in rust by PostgreSQL's own parser (`rust/fbtcore/src/sqlpg/`, libpg_query) and
/// written here as a T-SQL file's are - the same tables, the same columns, the same ids from the same counters - so the SQL half stays ONE
/// producer of `lang = 'sql'` rows: two writing separately would each delete the other's. An unqualified name is in
/// `public`, as PostgreSQL resolves it, not in `dbo`.
/// </summary>
internal static class SqlPostgres
{
    public const string Dialect = "postgres";

    public static void Write(CsTables rows, string file, string text, string database)
    {
        using var parsed = JsonDocument.Parse(FbtCore.SqlParse(text) ?? "{}");
        var root = parsed.RootElement;
        IEnumerable<JsonElement> List(string name) =>
            root.TryGetProperty(name, out var list) && list.ValueKind == JsonValueKind.Array ? list.EnumerateArray() : [];
        string Text(JsonElement row, string name) => row.TryGetProperty(name, out var v) && v.ValueKind == JsonValueKind.String ? v.GetString()! : "";
        int Number(JsonElement row, string name) => row.TryGetProperty(name, out var v) && v.ValueKind == JsonValueKind.Number ? v.GetInt32() : -1;
        bool Flag(JsonElement row, string name) => row.TryGetProperty(name, out var v) && v.ValueKind == JsonValueKind.True;

        var errors = 0;
        foreach (var d in List("diagnostics"))
        {
            rows.Add("diagnostics", "dg", ("file", file), ("cls", ""), ("func", ""), ("line", Math.Max(1, Number(d, "line"))),
                ("name", Text(d, "name")), ("kind", "error"), ("source", Text(d, "message")));
            errors++;
        }
        if (errors > 0) rows.Count(file, errors);
        var objects = new List<string>();
        foreach (var o in List("objects"))
        {
            objects.Add(rows.Add("sql_objects", "so", ("file", file), ("line", Number(o, "line")), ("end_line", Number(o, "end_line")),
                ("kind", Text(o, "kind")), ("schema", Schema(Text(o, "schema"))), ("name", Text(o, "name")), ("parent", Text(o, "parent")),
                ("database", database), ("source", Text(o, "source"))));
        }
        string Of(List<string> ids, int index) => index >= 0 && index < ids.Count ? ids[index] : "";
        foreach (var c in List("columns"))
        {
            rows.Add("sql_columns", "sc", ("file", file), ("object", Of(objects, Number(c, "object"))), ("line", Number(c, "line")),
                ("position", Number(c, "position")), ("name", Text(c, "name")), ("type", Text(c, "type")), ("nullable", Flag(c, "nullable") ? 1 : 0),
                ("identity", Flag(c, "identity") ? 1 : 0), ("key", Flag(c, "key") ? 1 : 0), ("default_expr", Text(c, "default")), ("computed", ""));
        }
        var statements = new List<string>();
        foreach (var s in List("statements"))
        {
            statements.Add(rows.Add("sql_statements", "ss", ("file", file), ("object", Of(objects, Number(s, "object"))),
                ("parent", Of(statements, Number(s, "parent"))), ("batch", Number(s, "batch")), ("ordinal", Number(s, "ordinal")),
                ("line", Number(s, "line")), ("end_line", Number(s, "end_line")), ("col", Number(s, "col")), ("kind", Text(s, "kind")),
                ("source", Text(s, "source")), ("fingerprint", "")));
        }
        foreach (var n in List("nodes"))
        {
            var parent = Number(n, "parent");
            rows.Plain("sql_nodes", ("file", file), ("statement", Of(statements, Number(n, "statement"))), ("n", Number(n, "n")),
                ("parent", parent < 0 ? CsTables.Null : parent), ("ord", Number(n, "ord")), ("kind", Text(n, "kind")), ("value", Text(n, "value")),
                ("line", Number(n, "line")));
        }
        foreach (var r in List("refs"))
        {
            rows.Add("sql_refs", "sr", ("file", file), ("object", Of(objects, Number(r, "object"))), ("line", Number(r, "line")), ("lang", SqlDeep.Lang),
                ("action", Text(r, "action")), ("schema", Schema(Text(r, "schema"))), ("name", Text(r, "name")), ("column", Text(r, "column")),
                ("written_database", Text(r, "written_database")), ("schema_written", Text(r, "schema").Length > 0 ? 1 : 0),
                ("database", database), ("call", ""), ("statement", Of(statements, Number(r, "statement"))));
        }
    }

    /// <summary>A name without a schema is `public`'s, as PostgreSQL's default search path resolves it.</summary>
    public static string Schema(string schema) => schema.Length == 0 ? "public" : schema;
}
