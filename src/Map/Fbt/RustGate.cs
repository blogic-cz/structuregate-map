using System.Runtime.InteropServices;
using System.Text;
using System.Text.Json;

namespace StructureGate;

/// <summary>
/// THE GATE'S CALLBACK - the gate is rust's (`rust/fbtcore/src/gate/run.rs`); this is the one thing only .NET
/// can do for it: count a C# file with Roslyn and check it with the async rule. An answer is a UTF-8 string
/// allocated here and freed by the second callback (see <c>Native</c> in Program.cs), so no allocation ever
/// crosses to be freed by the other side's allocator.
/// </summary>
internal static class RustGate
{
    /// <summary>
    /// One C# file: read, checked by the async rule when it is on, counted by Roslyn. NOTHING ESCAPES INTO RUST:
    /// a file that will not open is named by its exception, and anything else is too - an exception unwinding
    /// through a native frame would take the whole build with it.
    /// </summary>
    internal static IntPtr CountCSharp(IntPtr absPtr, IntPtr relPtr, int asyncRules)
    {
        string answer;
        try
        {
            var abs = Marshal.PtrToStringUTF8(absPtr) ?? "";
            var rel = Marshal.PtrToStringUTF8(relPtr) ?? "";
            var text = File.ReadAllText(abs);
            using var buffer = new MemoryStream();
            using (var json = new Utf8JsonWriter(buffer))
            {
                json.WriteStartObject();
                json.WriteNumber("lines", Sources.CSharpLines(text));
                json.WriteStartArray("problems");
                if (asyncRules != 0)
                {
                    foreach (var problem in AsyncRules.Inspect(rel, text)) json.WriteStringValue(problem);
                }
                json.WriteEndArray();
                json.WriteEndObject();
            }
            answer = Encoding.UTF8.GetString(buffer.ToArray());
        }
        catch (Exception e)
        {
            answer = $$"""{"error": "{{e.GetType().Name}}"}""";
        }
        return Marshal.StringToCoTaskMemUTF8(answer);
    }
}
