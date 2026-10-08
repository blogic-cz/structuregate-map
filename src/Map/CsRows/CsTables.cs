using System.Text.Json;

namespace StructureGate;

/// <summary>
/// THE ROWS OF THE DEEP C# MAP while they are still in this process, and the payload they leave in.
///
/// A ROW IS AN ORDERED LIST OF CELLS, not a type per table. The database derives every table's columns from
/// the rows it is given (see the store in <c>rust/fbtcore</c>), so a record type here would be a second declaration of the
/// same schema — and a schema written twice disagrees with itself the first time a column is added.
///
/// IDS ARE CONTINUED, NEVER RESTARTED. They are handed out from the counters the database already recorded,
/// because an incremental run that started again at 1 would hand a second row the id a joined row already
/// points at. They mean nothing outside one database: a row id is a handle for joining, never a name to
/// keep.
/// </summary>
internal sealed class CsTables
{
    private readonly Dictionary<string, List<(string Key, object Value)[]>> tables = new(StringComparer.Ordinal);

    /// <summary>{prefix -> highest number handed out}, seeded from the database and sent back to it.</summary>
    public Dictionary<string, int> Counters { get; }

    public CsTables(Dictionary<string, int> counters) => Counters = counters;

    public int Rows => tables.Sum(t => t.Value.Count);

    public string Add(string table, string prefix, params (string Key, object Value)[] cells)
    {
        Counters.TryGetValue(prefix, out var used);
        Counters[prefix] = ++used;
        var id = $"{prefix}:{used}";
        if (!tables.TryGetValue(table, out var list)) tables[table] = list = [];
        list.Add([("id", id), .. cells]);
        return id;
    }

    /// <summary>Records how many compile errors the file has.</summary>
    public void Count(string file, int errors) => Stamp(file, "errors", errors);

    private void Stamp(string file, string column, object value)
    {
        foreach (var row in tables.GetValueOrDefault("files") ?? [])
        {
            for (var i = 0; i < row.Length; i++)
            {
                if (row[i].Key == "id" && (string)row[i].Value != file) break;
                if (row[i].Key == column) row[i] = (column, value);
            }
        }
    }

    /// <summary>
    /// The payload the store reads, as BYTES.
    ///
    /// It used to be a FILE — "tens of megabytes over a real tree, which is more than a command line or a
    /// pipe will carry" — because the store was another process. The store is now linked into this one, so
    /// the rows go across as memory this object already holds and nothing is written to disk.
    ///
    /// <paramref name="all"/> states whether this RUN sends every file of the tree or only the ones that
    /// moved; the store refuses to rebuild a database out of a partial payload, and it can only refuse if
    /// it is told. <paramref name="first"/> and <paramref name="final"/> say where this batch sits in that
    /// run: only the first may replace the database, only the last may hold it to the whole tree.
    /// </summary>
    /// <returns>The stream's OWN buffer and how much of it is the payload - never a copy: a batch is tens of
    /// megabytes, and the store reads it in place through a pin (see <c>RustMapper.Pin</c>).</returns>
    public (byte[] Buffer, int Length) ToUtf8(bool all, bool first, bool final, bool reset,
        SortedDictionary<string, string> shas, List<(string Rel, string Abs)> read, string lang = "csharp")
    {
        var stream = new MemoryStream();
        using (var json = new Utf8JsonWriter(stream))
        {
            json.WriteStartObject();
            json.WriteBoolean("all", all);
            json.WriteBoolean("first", first);
            json.WriteBoolean("final", final);
            json.WriteBoolean("reset", reset);
            // WHOSE ROWS THESE ARE: the SQL half stores through this same builder, and the store scopes what is
            // recorded, gone and disagreeing by it.
            json.WriteString("lang", lang);

            json.WriteStartObject("shas");
            foreach (var (rel, sha) in shas) json.WriteString(rel, sha);
            json.WriteEndObject();

            json.WriteStartObject("counters");
            foreach (var (prefix, used) in Counters) json.WriteNumber(prefix, used);
            json.WriteEndObject();

            json.WriteStartArray("read");
            foreach (var (rel, abs) in read)
            {
                json.WriteStartArray();
                json.WriteStringValue(rel);
                json.WriteStringValue(abs);
                json.WriteEndArray();
            }
            json.WriteEndArray();

            json.WriteStartObject("tables");
            foreach (var (table, rows) in tables)
            {
                json.WriteStartArray(table);
                foreach (var row in rows)
                {
                    json.WriteStartObject();
                    foreach (var (key, value) in row) Cell(json, key, value);
                    json.WriteEndObject();
                }
                json.WriteEndArray();
            }
            json.WriteEndObject();
            json.WriteEndObject();
        }
        return (stream.GetBuffer(), (int)stream.Length);
    }

    /// <summary>
    /// One cell. A list stays a LIST here and becomes JSON text in the database, which is what the python
    /// half already does with `reads` and `calls` — so `json_each` opens a C# row the same way it opens a
    /// python one, and a query written for one language runs against the other.
    /// </summary>
    private static void Cell(Utf8JsonWriter json, string key, object value)
    {
        switch (value)
        {
            case string text:
                json.WriteString(key, text);
                break;
            case int number:
                json.WriteNumber(key, number);
                break;
            // A literal's `number`: written as text it compared unequal to every number in SQL.
            case double real when double.IsFinite(real):
                json.WriteNumber(key, real);
                break;
            case long number:
                json.WriteNumber(key, number);
                break;
            case IEnumerable<string> list:
                json.WriteStartArray(key);
                foreach (var item in list) json.WriteStringValue(item);
                json.WriteEndArray();
                break;
            // A LIST OF OBJECTS - an enum's `members` - in the shape the TypeScript half writes, so one query
            // opens both. A null inside one is JSON null: "" would read as a member whose value is empty text.
            case IEnumerable<(string Key, object? Value)[]> objects:
                json.WriteStartArray(key);
                foreach (var item in objects)
                {
                    json.WriteStartObject();
                    foreach (var (name, cell) in item)
                    {
                        if (cell is null) json.WriteNull(name);
                        else Cell(json, name, cell);
                    }
                    json.WriteEndObject();
                }
                json.WriteEndArray();
                break;
            default:
                json.WriteString(key, value?.ToString() ?? "");
                break;
        }
    }
}
