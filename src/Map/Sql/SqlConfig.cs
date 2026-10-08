using System.Text.Json;

namespace StructureGate;

/// <summary>
/// WHAT THE CODE CANNOT SAY ABOUT ITS DATABASES - `structuregate.sql.json`, beside the exe or at
/// `--sql-config`, READ BY RUST (`mapper/deep/sqlconfig.rs`) and handed over resolved. This is what the SQL
/// half and its links ask of it.
///
/// WHY A FILE. The same table can live in two database projects (`Catalog.Items` can be in both
/// `One.DB` and `Two.DB`), and which one a `DbContext` or a Dapper call reaches is decided at RUN
/// time, from a connection string. No parse tree holds that. So the consumer states it: which `.sqlproj`
/// is which database, which context classes and connection names reach it, which C# methods take SQL text,
/// and the database that holds when nothing else says. WITHOUT the file the map still parses every
/// `.sqlproj` and links a name only where it is unique - fewer links, none of them guessed.
///
/// ```json
/// { "databases": [ { "name": "Main", "project": "../src/X.DB/X.DB.sqlproj",
///                    "contexts": ["My.Data.CoreContext"], "connections": ["db"] } ],
///   "sqlText": ["Dapper.*"], "unattributedContexts": ["My.Data.CoreContext"], "default": "Main" }
/// ```
/// </summary>
internal sealed class SqlConfig
{
    public sealed record Database(string Name, string Project, List<string> Contexts, List<string> Connections);

    public List<Database> Databases { get; } = [];
    public List<string> SqlText { get; } = [];
    /// <summary>The contexts a mapping class with no context in `typeof(...)` on it is applied to (`unattributedContexts`).</summary>
    public List<string> UnattributedContexts { get; } = [];
    public string Default { get; private set; } = "";

    /// <summary>What went wrong reading the file; empty when it was read or absent.</summary>
    public string Problem { get; private set; } = "";

    /// <summary>The config as rust read it (`rust/fbtcore/src/mapper/deep/sqlconfig.rs`): paths already
    /// absolute, the built-in SQL-text methods already first in `sql_text`.</summary>
    public static SqlConfig FromJson(JsonElement root)
    {
        var config = new SqlConfig { Default = root.GetProperty("default").GetString() ?? "", Problem = root.GetProperty("problem").GetString() ?? "" };
        foreach (var entry in root.GetProperty("databases").EnumerateArray())
        {
            config.Databases.Add(new Database(entry.GetProperty("name").GetString()!, entry.GetProperty("project").GetString()!,
                Strings(entry, "contexts"), Strings(entry, "connections")));
        }
        config.SqlText.AddRange(Strings(root, "sql_text"));
        if (root.TryGetProperty("unattributed_contexts", out _)) config.UnattributedContexts.AddRange(Strings(root, "unattributed_contexts"));
        return config;
    }

    private static List<string> Strings(JsonElement owner, string property) =>
        [.. owner.GetProperty(property).EnumerateArray().Select(item => item.GetString()!)];

    /// <summary>The database a `.sqlproj` is: the configured name, else the project's own file name.</summary>
    public string DatabaseOf(string sqlproj) =>
        Databases.FirstOrDefault(d => string.Equals(d.Project, Path.GetFullPath(sqlproj), StringComparison.OrdinalIgnoreCase))?.Name
        ?? Path.GetFileNameWithoutExtension(sqlproj);
}
