using System.Runtime.InteropServices;
using System.Text;
using System.Text.Json;
using StructureGate;

// THE WHOLE COMMAND IS RUST'S - `rust/fbtcore/src/cli/`: the arguments, which question this run asks, and
// every answer. This side hands it the parsers only .NET has, as callbacks - Roslyn for a C# file's count,
// async rule, graph and deep rows (`RustGate`, `RustMapper`), ScriptDom for T-SQL's - and prints what it says.
//
// THE CALLBACKS ARE DELEGATES, not `[UnmanagedCallersOnly]` function pointers, because taking the address of
// one needs unsafe code, and AllowUnsafeBlocks is a permission for the whole project. They are held in static
// fields so nothing collects them while rust holds their pointers.
var input = new MemoryStream();
using (var json = new Utf8JsonWriter(input))
{
    json.WriteStartObject();
    json.WriteStartArray("argv");
    foreach (var arg in args) json.WriteStringValue(arg);
    json.WriteEndArray();
    // WHAT ONLY THIS SIDE KNOWS, for the gate's pass key: its own version and its command line, verbatim.
    json.WriteString("command_line", string.Join(" ", Environment.GetCommandLineArgs()));
    json.WriteString("version", typeof(Native).Assembly.GetName().Version?.ToString() ?? "0");
    // Where `structuregate.sql.json` is looked for when --sql-config is not given: beside the exe.
    json.WriteString("base_dir", AppContext.BaseDirectory);
    // WHICH BUILD IS RUNNING, for what the deep map derives from its rows: the process, and under `dotnet` the
    // managed assembly and the rust library beside it - each by size and time.
    json.WriteString("build", string.Join(";", new[] { Environment.ProcessPath,
        Path.Combine(AppContext.BaseDirectory, "structuregate.dll"), Path.Combine(AppContext.BaseDirectory, "fbtcore.dll") }
        .Where(p => !string.IsNullOrEmpty(p) && File.Exists(p))
        .Select(p => $"{p}|{new FileInfo(p!).Length}|{File.GetLastWriteTimeUtc(p!).Ticks}")));
    json.WriteEndObject();
}
var reply = FbtCore.Run(Encoding.UTF8.GetString(input.ToArray()), Native.CsFile, Native.CsMap, Native.Deep, Native.Free)
    ?? """{"out": [["e", "structuregate: the run returned nothing"]], "exit": 2}""";
using var document = JsonDocument.Parse(reply);
var root = document.RootElement;
// A DIAGNOSTIC OR A LENS WRITES UTF-8: what it prints is a path or a translation key read back by a script.
// Everything else is left in the console's code page, which is what MSBuild reads.
if (root.TryGetProperty("utf8", out var utf8) && utf8.GetBoolean())
{
    try { Console.OutputEncoding = new UTF8Encoding(false); }
    catch (IOException) { /* a redirected console that refuses an encoding is not a reason to stop */ }
}
foreach (var line in root.GetProperty("out").EnumerateArray())
{
    switch (line[0].GetString())
    {
        case "e": Console.Error.WriteLine(line[1].GetString()); break;
        case "w": Console.Out.Write(line[1].GetString()); break;
        case "j":
            // Indented by .NET's own writer, as every JSON this tool printed always was.
            using (var buffer = new MemoryStream())
            {
                using (var json = new Utf8JsonWriter(buffer, new JsonWriterOptions { Indented = true })) line[1].WriteTo(json);
                var text = Encoding.UTF8.GetString(buffer.ToArray());
                if (line[2].GetBoolean()) Console.Out.WriteLine(text); else Console.Out.Write(text);
            }
            break;
        default: Console.Out.WriteLine(line[1].GetString()); break;
    }
}
return root.GetProperty("exit").GetInt32();

namespace StructureGate
{
    /// <summary>The callbacks rust is handed, alive for the whole process.</summary>
    internal static class Native
    {
        [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
        private delegate IntPtr CsFileFn(IntPtr abs, IntPtr rel, int asyncRules);

        [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
        private delegate IntPtr CsMapFn(IntPtr abs, IntPtr rel);

        [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
        private delegate IntPtr DeepFn(IntPtr question);

        [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
        private delegate void FreeFn(IntPtr text);

        private static readonly CsFileFn CsFileCallback = RustGate.CountCSharp;
        private static readonly CsMapFn CsMapCallback = RustMapper.MapCSharp;
        private static readonly DeepFn DeepCallback = RustMapper.RunDeep;
        private static readonly FreeFn FreeCallback = Marshal.FreeCoTaskMem;

        internal static IntPtr CsFile => Marshal.GetFunctionPointerForDelegate(CsFileCallback);
        internal static IntPtr CsMap => Marshal.GetFunctionPointerForDelegate(CsMapCallback);
        internal static IntPtr Deep => Marshal.GetFunctionPointerForDelegate(DeepCallback);
        internal static IntPtr Free => Marshal.GetFunctionPointerForDelegate(FreeCallback);
    }
}
