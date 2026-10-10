using Microsoft.CodeAnalysis.CSharp;
using System.Runtime.InteropServices;
using System.Text;
using System.Text.Json;

namespace StructureGate;

/// <summary>
/// THE MAP'S CALLBACKS - the map is rust's (`rust/fbtcore/src/mapper/`); this answers what only .NET can: a C#
/// file's graph (Roslyn), and the deep halves of C# and T-SQL, each run over a scratch collector whose
/// additions are handed back. Nothing thrown here may unwind into rust, so every callback catches everything
/// and says so.
/// </summary>
internal static class RustMapper
{
    /// <summary>One C# file's graph, by Roslyn: its own fields, and what it adds to the collector.</summary>
    internal static IntPtr MapCSharp(IntPtr absPtr, IntPtr relPtr)
    {
        string answer;
        try
        {
            var abs = Marshal.PtrToStringUTF8(absPtr) ?? "";
            var rel = Marshal.PtrToStringUTF8(relPtr) ?? "";
            string text;
            try { text = File.ReadAllText(abs); }
            catch (Exception e) when (e is IOException or UnauthorizedAccessException)
            {
                return Marshal.StringToCoTaskMemUTF8($$"""{"error": "{{e.GetType().Name}}"}""");
            }
            var file = new MapFile { Rel = rel };
            var scratch = new MapCollector();
            // ONE PARSE, read twice: the count and the edges each parsed the file again.
            var tree = CSharpSyntaxTree.ParseText(text);
            file.Lines = Sources.CSharpLines(tree);
            CSharpMap.Read(file, tree, scratch);
            answer = Written(json =>
            {
                json.WriteNumber("lines", file.Lines);
                json.WriteBoolean("generated", file.Generated);
                json.WriteString("summary", file.Summary);
                json.WriteBoolean("entry", file.Entry);
                Strings(json, "declares", file.Declares);
                Strings(json, "uses", file.Uses);
                Strings(json, "registered", file.Registered);
                Strings(json, "errors", scratch.Errors);
                json.WriteStartArray("computed");
                foreach (var computed in scratch.Computed) Strings(json, null, [computed.Line, computed.What]);
                json.WriteEndArray();
                Places(json, "bodies", scratch.Bodies);
                Places(json, "expressions", scratch.Expressions);
            });
        }
        catch (Exception e)
        {
            answer = $$"""{"error": "{{e.GetType().Name}}"}""";
        }
        return Marshal.StringToCoTaskMemUTF8(answer);
    }

    /// <summary>The SQL config of this run, read once for the plan, the batch and the links.</summary>
    private static SqlConfig sqlConfig = new();

    /// <summary>
    /// A deep half only .NET can parse, asked one question at a time by rust (`mapper/deep/driven.rs`): the C#
    /// half's plan, session, order and batches, and the SQL half's plan, batch and links. A batch answers with
    /// one JSON line and then the payload's bytes, which rust hands to the store as they are.
    /// </summary>
    internal static IntPtr RunDeep(IntPtr inputPtr)
    {
        // WHICH QUESTION FAILED, said with the failure: "the deep map failed" alone named no step and no file.
        string? mode = "?";
        try
        {
            using var document = JsonDocument.Parse(Marshal.PtrToStringUTF8(inputPtr) ?? "{}");
            var input = document.RootElement;
            mode = input.GetProperty("mode").GetString();
            switch (mode)
            {
                case "csharp-open":
                    DeepMap.Open(input);
                    return Text("{}");
                case "csharp-order":
                    return Text(Written(json => Strings(json, "order", DeepMap.Order(input.GetProperty("rels")))));
                case "csharp-close":
                    DeepMap.Close();
                    return Text("{}");
                case "csharp-batch":
                {
                    var (taken, final, errors, notes, payload, spent) = DeepMap.Batch(input);
                    return Text(Written(json =>
                    {
                        json.WriteStartArray("spent");
                        foreach (var ms in spent) json.WriteNumberValue(ms);
                        json.WriteEndArray();
                        // [csproj, load references, parse sources, razor, declarations, files, opened, shared] - see CsTimings.
                        json.WriteStartArray("projects");
                        foreach (var (project, phases) in CsTimings.Drain())
                        {
                            json.WriteStartArray();
                            json.WriteStringValue(project);
                            foreach (var ms in phases) json.WriteNumberValue(ms);
                            json.WriteEndArray();
                        }
                        json.WriteEndArray();
                        json.WriteNumber("taken", taken);
                        json.WriteBoolean("final", final);
                        Strings(json, "errors", errors);
                        Strings(json, "notes", notes);
                        Strings(json, "unreadable", DeepMap.DrainUnreadable());
                        Pin(json, payload);
                    }));
                }
                case "sql-open":
                    sqlConfig = SqlConfig.FromJson(input.GetProperty("config"));
                    return Text("{}");
                case "sql-plan":
                    return Text(Written(json => SqlDeep.Plan(json, input.GetProperty("files"))));
                case "sql-batch":
                {
                    var (errors, payload) = SqlDeep.Batch(input, sqlConfig);
                    return Text(Written(json =>
                    {
                        Strings(json, "errors", errors);
                        Pin(json, payload);
                    }));
                }
                case "release":
                    // The store has read the batch: the buffer may move, and be collected, again.
                    if (Pinned.Remove(input.GetProperty("handle").GetInt64(), out var pinned)) pinned.Free();
                    return Text("{}");
                default:
                    return Text(Links(input));
            }
        }
        catch (Exception e)
        {
            return Text(Written(json => Strings(json, "errors", [$"HALF      deep: the deep map failed in `{mode}` — {DeepMap.Thrown(e)}"])));
        }
    }

    /// <summary>`sql-links`: the joins between the C# and SQL rows, over a scratch collector that starts with
    /// rust's notes so a note said once stays said once, handing back only what it added.</summary>
    private static string Links(JsonElement input)
    {
        var scratch = new MapCollector();
        foreach (var note in input.GetProperty("notes").EnumerateArray()) scratch.Notes.Add(note.GetString()!);
        var had = scratch.Notes.Count;
        SqlLinks.Run(scratch, input.GetProperty("db").GetString()!, sqlConfig);
        return Written(json =>
        {
            Strings(json, "errors", scratch.Errors);
            Strings(json, "notes", scratch.Notes.Skip(had));
        });
    }

    private static IntPtr Text(string answer) => Marshal.StringToCoTaskMemUTF8(answer);

    /// <summary>The batches rust is reading, pinned until it says it is done with them.</summary>
    private static readonly Dictionary<long, GCHandle> Pinned = [];
    private static long pins;

    /// <summary>
    /// A batch's payload handed over IN PLACE: the buffer is pinned, and rust is given its address and length
    /// and reads it where it lies, then sends `release`. NO COPY on either side - a C# batch is tens of
    /// megabytes, and copying it into native memory and again into a rust string held it three times over.
    /// </summary>
    private static void Pin(Utf8JsonWriter json, (byte[] Buffer, int Length) payload)
    {
        var handle = GCHandle.Alloc(payload.Buffer, GCHandleType.Pinned);
        var id = ++pins;
        Pinned[id] = handle;
        json.WriteStartObject("payload");
        json.WriteNumber("address", handle.AddrOfPinnedObject().ToInt64());
        json.WriteNumber("length", payload.Length);
        json.WriteNumber("handle", id);
        json.WriteEndObject();
    }

    private static string Written(Action<Utf8JsonWriter> body)
    {
        using var buffer = new MemoryStream();
        using (var json = new Utf8JsonWriter(buffer))
        {
            json.WriteStartObject();
            body(json);
            json.WriteEndObject();
        }
        return Encoding.UTF8.GetString(buffer.ToArray());
    }

    private static void Strings(Utf8JsonWriter json, string? name, IEnumerable<string> values)
    {
        if (name is null) json.WriteStartArray(); else json.WriteStartArray(name);
        foreach (var value in values) json.WriteStringValue(value);
        json.WriteEndArray();
    }

    private static void Places(Utf8JsonWriter json, string name, Dictionary<string, List<(string Where, int Size)>> found)
    {
        json.WriteStartArray(name);
        foreach (var (digest, at) in found)
        {
            foreach (var (where, size) in at)
            {
                json.WriteStartArray();
                json.WriteStringValue(digest);
                json.WriteStringValue(where);
                json.WriteNumberValue(size);
                json.WriteEndArray();
            }
        }
        json.WriteEndArray();
    }
}
