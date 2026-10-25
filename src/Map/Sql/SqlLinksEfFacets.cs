namespace StructureGate;

/// <summary>
/// WHAT A FLUENT MAPPING SAYS OF EACH COLUMN (#128): `builder.Property(x => x.Code).HasMaxLength(20).IsRequired()` is the column's
/// own line and its facets - so an `ef_column` link sits on that `Property` call, not on the mapping's `ToTable`, and carries
/// `max_length`, `required`, `precision`/`scale`, `column_type`, `conversion`, NULL where the mapping sets none. `HasColumnName`
/// renames the column the property maps, so the link joins that column.
///
/// FROM THE BOUND CALLS, never the text alone: a `Property` call is the `calls` row the compiler resolved to EF's
/// `EntityTypeBuilder<E>.Property` (its symbol names `E`); a facet is an EF call whose source extends that call's source by `.`
/// - the chain it hangs off - in the same method. The property is the lambda's member (`x => x.Code`) or the constant name
/// (`Property("Code")`); any other argument names nothing and the call is skipped.
/// </summary>
internal static partial class SqlLinks
{
    private sealed record Fluent(long Line, string Property, string Precision, string Scale, string MaxLength, string Required,
        string ColumnType, string Conversion);

    private static readonly string[] FacetCalls = ["HasMaxLength", "IsRequired", "HasPrecision", "HasColumnType", "HasConversion", "HasColumnName"];

    /// <summary>Entity -> column (as the mapping names it, any case) -> what its `Property` call says.</summary>
    private static Dictionary<string, Dictionary<string, Fluent>> FluentColumns(string db)
    {
        var properties = new List<(string Scope, string Source, string Entity, long Line, string Property)>();
        foreach (var row in Query(db, "SELECT c.file, c.cls, c.func, c.line, c.source, c.symbol, a.source, a.const, a.const_kind FROM calls c "
            + "LEFT JOIN arguments a ON a.call = c.id AND a.position = 0 "
            + "WHERE c.symbol GLOB 'Microsoft.EntityFrameworkCore.*EntityTypeBuilder<*' ORDER BY c.file, c.line") ?? [])
        {
            if (MethodOf(Str(row[5])) != "Property") continue;
            var property = Str(row[8]) is "const" or "folded" ? Str(row[7]) : LambdaMember(Str(row[6]));
            var entity = BuilderEntity(Str(row[5]));
            if (property.Length == 0 || entity.Length == 0) continue;
            properties.Add(($"{Str(row[0])}|{Str(row[1])}|{Str(row[2])}", Str(row[4]), entity, Num(row[3]), property));
        }
        var facets = new Dictionary<int, Dictionary<string, List<string>>>();
        foreach (var row in Query(db, "SELECT c.id, c.file, c.cls, c.func, c.symbol, c.source, a.position, a.const, a.source FROM calls c "
            + "LEFT JOIN arguments a ON a.call = c.id WHERE c.symbol GLOB 'Microsoft.EntityFrameworkCore.*' "
            + "ORDER BY c.file, c.line, c.id, a.position") ?? [])
        {
            var callee = MethodOf(Str(row[4]));
            if (!FacetCalls.Contains(callee)) continue;
            var scope = $"{Str(row[1])}|{Str(row[2])}|{Str(row[3])}";
            var source = Str(row[5]);
            // THE CHAIN IT HANGS OFF: the longest `Property(..)` source this call's source extends.
            var owner = -1;
            for (var i = 0; i < properties.Count; i++)
            {
                var p = properties[i];
                if (p.Scope == scope && source.StartsWith(p.Source + ".", StringComparison.Ordinal)
                    && (owner < 0 || p.Source.Length > properties[owner].Source.Length)) owner = i;
            }
            if (owner < 0) continue;
            if (!facets.TryGetValue(owner, out var said)) facets[owner] = said = new(StringComparer.Ordinal);
            if (!said.TryGetValue(callee, out var args)) said[callee] = args = [];
            if (row[6].ValueKind != System.Text.Json.JsonValueKind.Null) args.Add(Str(row[7]).Length > 0 ? Str(row[7]) : Str(row[8]));
            // A GENERIC CONVERSION (`HasConversion<string>()`) has no argument: its type argument is what it converts to.
            if (callee == "HasConversion" && args.Count == 0) args.Add(TypeArgument(source, "HasConversion"));
        }
        var mapped = new Dictionary<string, Dictionary<string, Fluent>>(StringComparer.Ordinal);
        for (var i = 0; i < properties.Count; i++)
        {
            var p = properties[i];
            var said = facets.GetValueOrDefault(i) ?? new(StringComparer.Ordinal);
            string Arg(string call, int at) => said.TryGetValue(call, out var a) && a.Count > at ? a[at] : "";
            var required = said.ContainsKey("IsRequired") ? (Arg("IsRequired", 0) is "false" or "False" ? "0" : "1") : "";
            var column = Arg("HasColumnName", 0) is { Length: > 0 } renamed ? renamed : p.Property;
            if (!mapped.TryGetValue(p.Entity, out var columns)) mapped[p.Entity] = columns = new(StringComparer.OrdinalIgnoreCase);
            columns.TryAdd(column, new Fluent(p.Line, p.Property, Arg("HasPrecision", 0), Arg("HasPrecision", 1), Arg("HasMaxLength", 0), required,
                Arg("HasColumnType", 0), Arg("HasConversion", 0)));
        }
        return mapped;
    }

    /// <summary>The method a call's symbol names: its last name, past any type arguments written after it
    /// (`PropertyBuilder&lt;int&gt;.HasConversion&lt;string&gt;` is `HasConversion`).</summary>
    private static string MethodOf(string symbol)
    {
        var end = symbol.Length;
        if (end > 0 && symbol[end - 1] == '>')
        {
            var open = 0;
            for (var i = end - 1; i >= 0; i--)
            {
                if (symbol[i] == '>') open++;
                else if (symbol[i] == '<' && --open == 0) { end = i; break; }
            }
        }
        var dot = symbol.LastIndexOf('.', Math.Max(0, end - 1));
        return symbol[(dot + 1)..end];
    }

    /// <summary>The member a one-parameter lambda reads off its parameter (`x => x.Code`, `(x) => x.Code`); "" for any other body.</summary>
    private static string LambdaMember(string lambda)
    {
        var arrow = lambda.IndexOf("=>", StringComparison.Ordinal);
        if (arrow < 0) return "";
        var parameter = lambda[..arrow].Trim().Trim('(', ')').Trim();
        var body = lambda[(arrow + 2)..].Trim();
        if (parameter.Length == 0 || !body.StartsWith(parameter + ".", StringComparison.Ordinal)) return "";
        var member = body[(parameter.Length + 1)..];
        return member.Length > 0 && member.All(c => char.IsLetterOrDigit(c) || c == '_') ? member : "";
    }

    /// <summary>`E` of the `EntityTypeBuilder&lt;E&gt;` a symbol is a member of; "" when it names none.</summary>
    private static string BuilderEntity(string symbol)
    {
        const string Builder = "EntityTypeBuilder<";
        var start = symbol.IndexOf(Builder, StringComparison.Ordinal);
        if (start < 0) return "";
        start += Builder.Length;
        var open = 1;
        for (var i = start; i < symbol.Length; i++)
        {
            if (symbol[i] == '<') open++;
            else if (symbol[i] == '>' && --open == 0) return symbol[start..i];
        }
        return "";
    }

    /// <summary>The type argument written on `name&lt;T&gt;(` in a call's source; "" when none is.</summary>
    private static string TypeArgument(string source, string name)
    {
        var at = source.LastIndexOf("." + name + "<", StringComparison.Ordinal);
        if (at < 0) return "";
        var start = at + name.Length + 2;
        var end = source.IndexOf(">(", start, StringComparison.Ordinal);
        return end > start ? source[start..end] : "";
    }
}
