using System.Text.Json;

namespace StructureGate;

/// <summary>
/// THE DEEP C# HALF of `--map-sqlite`, as rust drives it (`rust/fbtcore/src/mapper/deep/driven.rs`): rust
/// hashes, asks the store what moved, batches and retries; this answers what only Roslyn can - the order files
/// are sent in (one compilation per project) and the rows of one batch.
///
/// ONE SESSION PER RUN, held here between the calls, because a semantic model is a project-sized thing:
/// files arrive project by project and exactly one compilation is ever alive. The ids carry on across
/// batches from the counters the database recorded - two rows sharing one id is a join that silently pulls
/// the wrong row.
///
/// NOTHING IS HELD THAT THE NEXT FILE DOES NOT NEED: the text of a file is read at the moment it is parsed.
/// </summary>
internal static class DeepMap
{
    private static CsModels? models;
    private static SortedDictionary<string, string> paths = new(StringComparer.Ordinal);
    private static SortedDictionary<string, string> shas = new(StringComparer.Ordinal);
    private static Dictionary<string, int> counters = new(StringComparer.Ordinal);
    /// <summary>`--map-exclude`'s matches, decided by rust: listed and never walked.</summary>
    private static HashSet<string> excluded = new(StringComparer.Ordinal);
    /// <summary>Every mapped file by its absolute path - as its tree is named - to its mapped path (`file_refs`).</summary>
    private static Dictionary<string, string> byPath = new(StringComparer.OrdinalIgnoreCase);

    /// <summary>The session: every file that will be sent, its sha, and the ids the database has handed out.</summary>
    public static void Open(JsonElement input)
    {
        paths = new(StringComparer.Ordinal);
        shas = new(StringComparer.Ordinal);
        counters = new(StringComparer.Ordinal);
        foreach (var file in input.GetProperty("paths").EnumerateObject()) paths[file.Name] = file.Value.GetString()!;
        foreach (var file in input.GetProperty("shas").EnumerateObject()) shas[file.Name] = file.Value.GetString()!;
        excluded = input.TryGetProperty("excluded", out var listed)
            ? [.. listed.EnumerateArray().Select(r => r.GetString()!)]
            : new HashSet<string>(StringComparer.Ordinal);
        if (input.GetProperty("counters").ValueKind == JsonValueKind.Object)
        {
            foreach (var prefix in input.GetProperty("counters").EnumerateObject()) counters[prefix.Name] = prefix.Value.GetInt32();
        }
        byPath = new(StringComparer.OrdinalIgnoreCase);
        foreach (var (rel, abs) in paths) byPath[abs] = rel;
        var roots = input.TryGetProperty("roots", out var listedRoots) ? [.. listedRoots.EnumerateArray().Select(r => r.GetString()!)] : new List<string>();
        var skip = input.TryGetProperty("skip", out var skipped)
            ? new HashSet<string>(skipped.EnumerateArray().Select(s => s.GetString()!), StringComparer.OrdinalIgnoreCase)
            : new HashSet<string>(StringComparer.OrdinalIgnoreCase);
        models = new CsModels(paths) { Skipped = new CsSkipped(roots, skip) };
    }

    /// <summary>The files in the order they are sent - project by project.</summary>
    public static List<string> Order(JsonElement rels) => models!.Order(rels.EnumerateArray().Select(r => r.GetString()!));

    /// <summary>The files a batch could not read since it was last asked - see `Batch`.</summary>
    private static readonly List<string> Unreadable = [];

    public static List<string> DrainUnreadable()
    {
        var drained = new List<string>(Unreadable);
        Unreadable.Clear();
        return drained;
    }

    public static void Close()
    {
        models = null;
        paths = new(StringComparer.Ordinal);
        shas = new(StringComparer.Ordinal);
        byPath = new(StringComparer.OrdinalIgnoreCase);
    }

    /// <summary>
    /// One batch: files taken from the front of `rels` until `bound` rows are held, and the payload the store
    /// reads. EVERY BATCH CARRIES THE WHOLE SHA MAP and only its own rows: what has LEFT the tree is decided
    /// against that map, so a batch naming only its own files would say every other file had been deleted.
    /// </summary>
    public static (int Taken, bool Final, List<string> Errors, List<string> Notes, (byte[] Buffer, int Length) Payload, long[] Spent) Batch(JsonElement input)
    {
        var rels = input.GetProperty("rels").EnumerateArray().Select(r => r.GetString()!).ToList();
        var bound = input.GetProperty("bound").GetInt64();
        if (input.GetProperty("first").GetBoolean()) CsRows.GeneratorGaps = 0;
        var rows = new CsTables(counters);
        var read = new List<(string Rel, string Abs)>();
        var errors = new List<string>();
        var taken = 0;
        // WHERE THE TIME WENT: building compilations, binding and extracting rows, writing the payload.
        var compile = new System.Diagnostics.Stopwatch();
        var extract = new System.Diagnostics.Stopwatch();
        // READING THE SOURCE OFF THE DISK is its own line: on a cold disk it is minutes, and it was in nobody's total.
        var reading = new System.Diagnostics.Stopwatch();
        while (taken < rels.Count && rows.Rows < bound)
        {
            var rel = rels[taken++];
            string text;
            reading.Start();
            try { text = File.ReadAllText(paths[rel]); }
            catch (Exception e) when (e is IOException or UnauthorizedAccessException)
            {
                // A FILE THAT WILL NOT OPEN leaves the sha map, so no later batch records a sha with no rows, and is
                // named back to the caller - which no longer opens every moved file first to find out.
                shas.Remove(rel);
                Unreadable.Add(rel);
                continue;
            }
            finally { reading.Stop(); }
            // ONE FILE THAT THROWS IS ONE FILE UNPARSED, whatever it throws and wherever - building its compilation or
            // reading its rows. Escaping the batch, a NullReferenceException on one file ended the whole C# half with
            // no file named and no row of any other file stored.
            try
            {
                compile.Start();
                var ready = models!.For(rel);
                compile.Stop();
                extract.Start();
                if (rel == Environment.GetEnvironmentVariable("STRUCTUREGATE_TEST_THROW_ON"))
                    throw new NullReferenceException("thrown by STRUCTUREGATE_TEST_THROW_ON");
                CsRows.Read(rows, rel, text, shas[rel], ready.Tree, ready.Model, ready.Project, ready.Compiled, ready.Razor, excluded.Contains(rel), ready.Declared, byPath);
            }
            catch (Exception e) when (e is not OutOfMemoryException)
            {
                errors.Add($"UNPARSED  {rel}: the deep C# pass could not read it ({Thrown(e)})");
                continue;
            }
            finally
            {
                compile.Stop();
                extract.Stop();
            }
            read.Add((rel, paths[rel]));
        }
        var final = taken >= rels.Count;
        var written = System.Diagnostics.Stopwatch.StartNew();
        var payload = rows.ToUtf8(input.GetProperty("all").GetBoolean(), input.GetProperty("first").GetBoolean(), final,
            input.GetProperty("reset").GetBoolean(), shas, read);
        var notes = CsRazor.Skipped.ToList();
        CsRazor.Skipped.Clear();
        if (final && CsRows.GeneratorGaps > 0)
            notes.Add($"{CsRows.GeneratorGaps} partial-method diagnostic(s) (CS8795/CS0762) are a source generator's missing output - "
                + "labelled `generator`, not counted as errors; build with <EmitCompilerGeneratedFiles>true</EmitCompilerGeneratedFiles> "
                + "and the map compiles what it wrote under obj/**/generated");
        return (taken, final, errors, notes, payload, [compile.ElapsedMilliseconds, extract.ElapsedMilliseconds, written.ElapsedMilliseconds, reading.ElapsedMilliseconds]);
    }

    /// <summary>An exception as one line: its type, its message and the frame that threw it - the method, which a
    /// NativeAOT trace keeps when it keeps no line numbers - so a report from a tree nobody else has says where.</summary>
    public static string Thrown(Exception e)
    {
        var frame = e.StackTrace?.Split('\n').Select(f => f.Trim()).FirstOrDefault(f => f.Length > 0);
        return frame is null ? $"{e.GetType().Name}: {e.Message}" : $"{e.GetType().Name}: {e.Message} {frame}";
    }
}
