using System.Text.Json;
using System.Xml.Linq;
using Microsoft.SqlServer.TransactSql.ScriptDom;

namespace StructureGate;

/// <summary>
/// THE SQL HALF of the deep map: every `.sql` file, parsed IN THIS PROCESS by ScriptDom and stored through
/// the same store as the C# half, scoped by `lang = 'sql'`.
///
/// A FILE BELONGS TO ITS NEAREST `.sqlproj`, which names the database it builds (`SqlConfig.DatabaseOf`) and
/// what the file is to that build: `build` (a schema object), `predeploy`/`postdeploy` (a deployment script),
/// or `script` - a file the project does not build, such as a seed script a deploy script runs with `:r`.
///
/// INCREMENTAL BY FILE like the C# half: a file's sha is folded with its project file, the config and
/// `SQL_ROWS_VERSION` in `rust/fbtcore/src/mapper/deep/driven.rs` (BUMP IT with every change to what this
/// extractor writes), so a changed database name or a file moved between build items re-reads it.
/// </summary>
internal static class SqlDeep
{
    public const string Lang = "sql";

    private static readonly Dictionary<string, string?> Owners = new(StringComparer.OrdinalIgnoreCase);
    private static readonly Dictionary<string, Dictionary<string, string>> Items = new(StringComparer.OrdinalIgnoreCase);

    /// <summary>Per file: the `.sqlproj` that owns it. The hashing, what moved and the retry are rust's
    /// (`mapper/deep/driven.rs`).</summary>
    public static void Plan(Utf8JsonWriter json, JsonElement files)
    {
        json.WriteStartArray("files");
        foreach (var pair in files.EnumerateArray())
        {
            var project = Owner(pair[1].GetString()!);
            json.WriteStartObject();
            if (project is null) json.WriteNull("project"); else json.WriteString("project", project);
            json.WriteEndObject();
        }
        json.WriteEndArray();
    }

    /// <summary>The stale files parsed into ONE batch, with the whole sha map beside them.</summary>
    public static (List<string> Errors, (byte[] Buffer, int Length) Payload) Batch(JsonElement input, SqlConfig config)
    {
        var paths = new SortedDictionary<string, string>(StringComparer.Ordinal);
        var shas = new SortedDictionary<string, string>(StringComparer.Ordinal);
        var counters = new Dictionary<string, int>(StringComparer.Ordinal);
        foreach (var file in input.GetProperty("paths").EnumerateObject()) paths[file.Name] = file.Value.GetString()!;
        foreach (var file in input.GetProperty("shas").EnumerateObject()) shas[file.Name] = file.Value.GetString()!;
        if (input.GetProperty("counters").ValueKind == JsonValueKind.Object)
        {
            foreach (var prefix in input.GetProperty("counters").EnumerateObject()) counters[prefix.Name] = prefix.Value.GetInt32();
        }
        var rows = new CsTables(counters);
        var read = new List<(string Rel, string Abs)>();
        foreach (var rel in input.GetProperty("rels").EnumerateArray().Select(r => r.GetString()!))
        {
            string text;
            try { text = File.ReadAllText(paths[rel]); }
            catch (Exception e) when (e is IOException or UnauthorizedAccessException) { continue; }
            Read(rows, rel, paths[rel], text, shas[rel], config);
            read.Add((rel, paths[rel]));
        }
        return ([], rows.ToUtf8(input.GetProperty("all").GetBoolean(), first: true, final: true,
            input.GetProperty("reset").GetBoolean(), shas, read, Lang));
    }

    /// <summary>One file's rows: its `files` row, what it declares, and what it touches.</summary>
    public static void Read(CsTables rows, string rel, string abs, string text, string sha, SqlConfig config)
    {
        var project = Owner(abs);
        var database = project is null ? "" : config.DatabaseOf(project);
        var kind = project is null ? "script" : KindOf(project, abs);
        var file = rows.Add("files", "f",
            ("path", rel), ("lang", Lang), ("sha", sha), ("lines", text.Count(c => c == '\n') + 1),
            ("project", project is null ? "" : Path.GetFileNameWithoutExtension(project)),
            ("database", database), ("kind", kind), ("errors", 0));

        var included = new List<(string Path, int Line)>();
        var prepared = SqlVisitor.Prepared(text, included);
        var parser = new TSql170Parser(initialQuotedIdentifiers: true);
        TSqlFragment fragment;
        IList<ParseError> errors;
        using (var reader = new StringReader(prepared)) fragment = parser.Parse(reader, out errors);
        foreach (var parseError in errors)
        {
            rows.Add("diagnostics", "dg", ("file", file), ("cls", ""), ("func", ""), ("line", parseError.Line),
                ("name", $"SQL{parseError.Number}"), ("kind", "error"), ("source", parseError.Message));
        }
        if (errors.Count > 0) rows.Count(file, errors.Count);

        var visitor = new SqlVisitor(prepared);
        fragment?.Accept(visitor);
        var ids = new List<string>();
        foreach (var o in visitor.Objects)
        {
            ids.Add(rows.Add("sql_objects", "so", ("file", file), ("line", o.Line), ("end_line", o.EndLine),
                ("kind", o.Kind), ("schema", Schema(o.Schema)), ("name", o.Name), ("parent", o.Parent),
                ("database", database), ("source", o.Source)));
        }
        string Id(int index) => index >= 0 && index < ids.Count ? ids[index] : "";
        foreach (var c in visitor.Columns)
        {
            rows.Add("sql_columns", "sc", ("file", file), ("object", Id(c.Object)), ("line", c.Line),
                ("position", c.Position), ("name", c.Name), ("type", c.Type), ("nullable", c.Nullable ? 1 : 0),
                ("identity", c.Identity ? 1 : 0), ("key", c.Key ? 1 : 0), ("default_expr", c.Default), ("computed", c.Computed));
        }
        foreach (var k in visitor.Keys)
        {
            var owner = k.Object >= 0 ? visitor.Objects[k.Object] : null;
            rows.Add("sql_keys", "sk", ("file", file), ("object", Id(k.Object)), ("line", k.Line), ("kind", k.Kind),
                ("name", k.Name), ("schema", Schema(owner?.Schema ?? k.Schema)), ("table", owner?.Name ?? k.Table),
                ("columns", k.Columns), ("ref_schema", k.RefName.Length > 0 ? Schema(k.RefSchema) : ""),
                ("ref_name", k.RefName), ("ref_columns", k.RefColumns), ("database", database));
        }
        foreach (var t in visitor.Touches)
        {
            rows.Add("sql_refs", "sr", ("file", file), ("object", Id(t.Object)), ("line", t.Line), ("lang", Lang),
                ("action", t.Action), ("schema", Schema(t.Schema)), ("name", t.Name), ("column", t.Column),
                ("database", database), ("call", ""));
        }
        foreach (var d in visitor.Dynamics)
        {
            rows.Add("sql_dynamic", "sy", ("file", file), ("line", d.Line), ("kind", d.Kind), ("name", d.Name),
                ("open", d.Open ? 1 : 0), ("source", d.Source), ("database", database));
        }
        // WHAT A SEED SCRIPT DOES, in order - rust (`rows/seeds/`) walks the steps of a whole `:r` chain into `sql_seeds`.
        var seeds = new SqlSeedVisitor(text, prepared);
        seeds.Read(fragment);
        foreach (var s in seeds.Steps)
        {
            rows.Add("sql_steps", "st", ("file", file), ("line", s.Line), ("seq", s.Seq), ("action", s.Action),
                ("schema", s.Schema), ("name", s.Name), ("source", s.Source), ("columns", s.Columns), ("targets", s.Targets),
                ("kinds", s.Kinds), ("exprs", s.Exprs), ("text", s.Text));
        }
        // THE SEED CHAIN: a deploy script is a list of the files it runs.
        foreach (var (path, line) in included)
        {
            rows.Add("sql_refs", "sr", ("file", file), ("object", ""), ("line", line), ("lang", Lang),
                ("action", "include"), ("schema", ""), ("name", path.Replace('\\', '/')), ("column", ""),
                ("database", database), ("call", ""));
        }
    }

    /// <summary>A name without a schema is `dbo`'s, as SQL Server resolves it for a schema-less reference.</summary>
    public static string Schema(string schema) => schema.Length == 0 ? "dbo" : schema;

    /// <summary>The nearest `.sqlproj` at or above a file.</summary>
    public static string? Owner(string fileAbs)
    {
        var folder = Path.GetDirectoryName(fileAbs);
        if (folder is null) return null;
        if (Owners.TryGetValue(folder, out var known)) return known;
        string? found = null;
        for (var at = folder; at is not null; at = Path.GetDirectoryName(at))
        {
            string[] projects;
            try { projects = Directory.GetFiles(at, "*.sqlproj"); }
            catch (Exception e) when (e is IOException or UnauthorizedAccessException) { break; }
            if (projects.Length > 0) { found = projects[0]; break; }
        }
        Owners[folder] = found;
        return found;
    }

    /// <summary>What a file is to its project's build. A classic `.sqlproj` lists every file; an SDK-style
    /// one (`Microsoft.Build.Sql`) builds every `.sql` it does not name as a deploy script.</summary>
    private static string KindOf(string project, string fileAbs)
    {
        if (!Items.TryGetValue(project, out var items))
        {
            items = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase);
            try
            {
                var document = XDocument.Load(project);
                foreach (var element in document.Root?.Descendants() ?? [])
                {
                    var include = element.Attribute("Include")?.Value;
                    if (include is null || !include.EndsWith(".sql", StringComparison.OrdinalIgnoreCase)) continue;
                    var kind = element.Name.LocalName switch
                    {
                        "Build" => "build",
                        "PostDeploy" => "postdeploy",
                        "PreDeploy" => "predeploy",
                        _ => "script",
                    };
                    // `Scripts\Post.sql`: MSBuild reads a backslash as a separator on every OS, and so must this.
                    var relative = include.Replace('\\', Path.DirectorySeparatorChar);
                    items[Path.GetFullPath(Path.Combine(Path.GetDirectoryName(project)!, relative))] = kind;
                }
                items["*sdk"] = document.Root?.Attribute("Sdk") is not null
                    || document.Root?.Elements().Any(e => e.Name.LocalName == "Sdk") == true ? "1" : "";
            }
            catch (Exception e) when (e is IOException or UnauthorizedAccessException or System.Xml.XmlException) { }
            Items[project] = items;
        }
        if (items.TryGetValue(Path.GetFullPath(fileAbs), out var listed)) return listed;
        return items.GetValueOrDefault("*sdk") == "1" ? "build" : "script";
    }
}
