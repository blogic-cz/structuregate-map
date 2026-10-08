using System.Runtime.InteropServices;

namespace StructureGate;

/// <summary>
/// THE DOORS INTO `rust/fbtcore`, linked INTO this exe: the whole command (<see cref="Run"/>), what a C# project
/// compiles against (<see cref="CsAsk"/>), and the SQL the links between the C# and SQL rows ask of the map -
/// the last two called while rust waits in a callback.
///
/// SHIPPED AS A STATIC LIBRARY. <c>DirectPInvoke</c> plus <c>NativeLibrary</c> in the csproj link
/// <c>fbtcore.lib</c> into the published binary, so a consumer still receives two files. The matching
/// <c>fbtcore.dll</c> exists only for a managed <c>dotnet build</c>, which cannot see a static library,
/// and is never deployed.
/// </summary>
internal static class FbtCore
{
    /// <summary>The name both the static link and the managed fallback resolve.</summary>
    private const string Library = "fbtcore";

    // DllImport, NOT LibraryImport. The source generator behind LibraryImport emits unsafe code and so
    // needs <AllowUnsafeBlocks> for the WHOLE project - a large permission to grant a few declarations.
    // DirectPInvoke makes these direct calls under NativeAOT either way, so the generator would buy
    // nothing here. ExactSpelling stops the runtime hunting for an `A`/`W` suffixed twin.

    [DllImport(Library, ExactSpelling = true)]
    private static extern IntPtr fbt_main([MarshalAs(UnmanagedType.LPUTF8Str)] string inputJson,
        IntPtr csFile, IntPtr csMap, IntPtr deep, IntPtr free);

    [DllImport(Library, ExactSpelling = true)]
    private static extern IntPtr fbt_cs_ask([MarshalAs(UnmanagedType.LPUTF8Str)] string question);

    [DllImport(Library, ExactSpelling = true)]
    private static extern void fbt_string_free(IntPtr text);

    [DllImport(Library, ExactSpelling = true)]
    private static extern IntPtr fbt_sql_run(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string db,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string script,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string query);

    /// <summary>The whole command, calling back into the parsers only .NET has - see Program.cs.</summary>
    internal static string? Run(string inputJson, IntPtr csFile, IntPtr csMap, IntPtr deep, IntPtr free)
        => Owned(fbt_main(inputJson, csFile, csMap, deep, free));

    /// <summary>What a C# project compiles against, or which project owns a file - see `rust/fbtcore/src/csproj/`.</summary>
    internal static string? CsAsk(string question) => Owned(fbt_cs_ask(question));

    /// <summary>A script, then a query, against the map - see `rows/sqlrun.rs`. JSON rows, or an `error`.</summary>
    internal static string? SqlRun(string db, string script, string query) => Owned(fbt_sql_run(db, script, query));

    /// <summary>A reply from a call that takes NO HANDLE, marshalled and freed by the allocator that made it.</summary>
    private static string? Owned(IntPtr reply)
    {
        if (reply == IntPtr.Zero) return null;
        try { return Marshal.PtrToStringUTF8(reply); }
        finally { fbt_string_free(reply); }
    }
}
