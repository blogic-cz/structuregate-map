using System.Diagnostics;
using System.Text.Json;

namespace StructureGate;

/// <summary>
/// THE C# HALF'S METRICS - per file, per kind of compiler question - COMPILED IN ONLY WHERE <c>SG_METRICS</c> IS
/// DEFINED (<c>dotnet build -p:Metrics=true</c>). Every method is <see cref="ConditionalAttribute"/>: without the
/// symbol the compiler deletes each CALL, its arguments with it, so a release asks no clock and keeps no tally. A call
/// site holds only a <c>long</c> that nothing then reads.
///
/// TALLIED HERE, sent to rust once a batch (<see cref="Flush"/>): a P/Invoke per compiler question would cost more than
/// the question. The trace writes each name as a <c>metric: &lt;name&gt;</c> span - see
/// <c>rust/fbtcore/src/trace/metrics.rs</c>. TIMING ONLY: nothing here may change a row.
/// </summary>
internal static class CsMetrics
{
    private sealed class Tally
    {
        public long Count, Total, Max;
        public string MaxItem = "";
        public readonly List<(string Item, long Ticks)> Slow = [];
    }

    private static readonly Dictionary<string, Tally> Tallies = new(StringComparer.Ordinal);

    /// <summary>An item slower than this is kept by name as well as counted (`SG_METRICS_SLOW_MS`, 50 by default).</summary>
    private static readonly long SlowTicks = (long.TryParse(Environment.GetEnvironmentVariable("SG_METRICS_SLOW_MS"), out var ms) ? ms : 50)
        * Stopwatch.Frequency / 1000;

    [Conditional("SG_METRICS")]
    public static void Start(ref long started) => started = Stopwatch.GetTimestamp();

    /// <summary>One item of <paramref name="name"/>, timed since <see cref="Start"/>. The item is built only here.</summary>
    [Conditional("SG_METRICS")]
    public static void Stop(string name, string item, long started)
    {
        var took = Stopwatch.GetTimestamp() - started;
        lock (Tallies)
        {
            if (!Tallies.TryGetValue(name, out var tally)) Tallies[name] = tally = new Tally();
            tally.Count++;
            tally.Total += took;
            if (took > tally.Max) { tally.Max = took; tally.MaxItem = item; }
            if (took >= SlowTicks) tally.Slow.Add((item, took));
        }
    }

    /// <summary>Every tally so far, handed to rust and emptied - once a batch.</summary>
    [Conditional("SG_METRICS")]
    public static void Flush()
    {
        static double Ms(long ticks) => ticks * 1000.0 / Stopwatch.Frequency;
        var stream = new MemoryStream();
        lock (Tallies)
        {
            if (Tallies.Count == 0) return;
            using (var json = new Utf8JsonWriter(stream))
            {
                json.WriteStartArray();
                foreach (var (name, tally) in Tallies)
                {
                    json.WriteStartObject();
                    json.WriteString("name", name);
                    json.WriteNumber("count", tally.Count);
                    json.WriteNumber("total_ms", Ms(tally.Total));
                    json.WriteNumber("max_ms", Ms(tally.Max));
                    json.WriteString("max_item", tally.MaxItem);
                    json.WriteStartArray("slow");
                    foreach (var (item, ticks) in tally.Slow)
                    {
                        json.WriteStartArray();
                        json.WriteStringValue(item);
                        json.WriteNumberValue(Ms(ticks));
                        json.WriteEndArray();
                    }
                    json.WriteEndArray();
                    json.WriteEndObject();
                }
                json.WriteEndArray();
            }
            Tallies.Clear();
        }
        FbtCore.Metric(System.Text.Encoding.UTF8.GetString(stream.GetBuffer(), 0, (int)stream.Length));
    }
}
