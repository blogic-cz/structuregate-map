using System.Text.Json;

namespace StructureGate;

/// <summary>
/// WHAT RUST HANDS A .NET HALF, read in one place: the deep map's C# session and the SQL batch both get the same
/// `paths` / `shas` / `counters` members, and a config's string lists are read the same way everywhere.
/// </summary>
internal static class JsonInput
{
    /// <summary>A member that is a list of strings.</summary>
    public static List<string> Strings(JsonElement owner, string name) =>
        [.. owner.GetProperty(name).EnumerateArray().Select(item => item.GetString()!)];

    /// <summary>Every file that will be sent (relative path to absolute), its sha, and the ids the database has handed out.</summary>
    public static (SortedDictionary<string, string> Paths, SortedDictionary<string, string> Shas, Dictionary<string, int> Counters) Batch(JsonElement input)
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
        return (paths, shas, counters);
    }
}
