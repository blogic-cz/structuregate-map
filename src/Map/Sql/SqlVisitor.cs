using Microsoft.SqlServer.TransactSql.ScriptDom;

namespace StructureGate;

/// <summary>
/// WHAT ONE T-SQL SCRIPT DECLARES AND TOUCHES, read off Microsoft's own parse tree (ScriptDom) - never off
/// the text. The declarations are `CREATE TABLE/VIEW/PROCEDURE/FUNCTION/TRIGGER/TYPE/SEQUENCE/INDEX` with
/// their columns and keys; the touches are every table, view or procedure a statement reads, writes or
/// runs, with the action (`select`, `insert`, `update`, `delete`, `merge`, `truncate`, `exec`).
///
/// A NAME THAT IS NO OBJECT IS NOT A TOUCH: a `#temp` table, a `@table` variable and a CTE name are local to
/// the script, and a row naming them would read as a table nobody declared.
/// </summary>
internal sealed class SqlVisitor : TSqlFragmentVisitor
{
    public sealed record Object(string Kind, string Schema, string Name, string Parent, int Line, int EndLine, string Source);
    public sealed record Column(int Object, int Position, string Name, string Type, bool Nullable, bool Identity,
        bool Key, string Default, string Computed, int Line);
    public sealed record Key(int Object, string Kind, string Name, string Schema, string Table, List<string> Columns,
        string RefSchema, string RefName, List<string> RefColumns, int Line);
    public sealed record Touch(int Object, string Action, string Schema, string Name, string Column, int Line);
    /// <summary>A `CREATE TRIGGER` inside a string - dynamic SQL. `Open` when the name runs on past the literal
    /// (`'CREATE TRIGGER Demo_' + @Name`): `Name` is then only the prefix every trigger it builds has.</summary>
    public sealed record Dynamic(string Kind, string Name, bool Open, int Line, string Source);

    public List<Object> Objects { get; } = [];
    public List<Column> Columns { get; } = [];
    public List<Key> Keys { get; } = [];
    public List<Touch> Touches { get; } = [];
    public List<Dynamic> Dynamics { get; } = [];

    private readonly string text;
    private readonly HashSet<TSqlFragment> targets = [];
    private readonly HashSet<string> ctes = new(StringComparer.OrdinalIgnoreCase);
    private int owner = -1;

    public SqlVisitor(string text) => this.text = text;

    /// <summary>A fragment's own text, whole.</summary>
    private string Text(TSqlFragment? fragment) =>
        fragment is null || fragment.StartOffset < 0 || fragment.StartOffset + fragment.FragmentLength > text.Length
            ? "" : text.Substring(fragment.StartOffset, fragment.FragmentLength);

    private static int EndLine(TSqlFragment fragment) =>
        fragment.LastTokenIndex >= 0 && fragment.ScriptTokenStream is { } tokens && fragment.LastTokenIndex < tokens.Count
            ? tokens[fragment.LastTokenIndex].Line : fragment.StartLine;

    private int Declare(string kind, SchemaObjectName? name, TSqlFragment node, string parent = "")
    {
        Objects.Add(new Object(kind, name?.SchemaIdentifier?.Value ?? "", name?.BaseIdentifier?.Value ?? "", parent,
            node.StartLine, EndLine(node), Text(node)));
        return Objects.Count - 1;
    }

    /// <summary>Visit a body with the object it belongs to as the owner of every touch in it.</summary>
    private void Owned(int id, Action visit)
    {
        var outer = owner;
        owner = id;
        visit();
        owner = outer;
    }

    public override void ExplicitVisit(CreateTableStatement node)
    {
        var id = Declare("table", node.SchemaObjectName, node);
        var position = 0;
        // A KEY WITH ITS OWN COLUMN LIST keys THOSE columns, wherever it is written: after a column with no comma
        // (`[Beta] ... NULL CONSTRAINT PK PRIMARY KEY ([Alpha])`) SQL Server reads it as a table
        // constraint, and filing it under the column it trails keyed `Beta`.
        var named = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
        foreach (var column in node.Definition?.ColumnDefinitions ?? [])
            foreach (var constraint in column.Constraints)
                if (constraint is UniqueConstraintDefinition { IsPrimaryKey: true, Columns.Count: > 0 } listed)
                    foreach (var c in listed.Columns) named.Add(Last(c.Column));
        foreach (var column in node.Definition?.ColumnDefinitions ?? [])
        {
            var nullable = true;
            var key = named.Contains(column.ColumnIdentifier.Value);
            var fallback = column.DefaultConstraint is { } inline ? Text(inline.Expression) : "";
            foreach (var constraint in column.Constraints)
            {
                switch (constraint)
                {
                    case NullableConstraintDefinition n: nullable = n.Nullable; break;
                    case UniqueConstraintDefinition { IsPrimaryKey: true, Columns.Count: > 0 } listed:
                        Keys.Add(new Key(id, "primary", listed.ConstraintIdentifier?.Value ?? "", "", "",
                            [.. listed.Columns.Select(c => Last(c.Column))], "", "", [], listed.StartLine));
                        break;
                    case UniqueConstraintDefinition { IsPrimaryKey: true } primary:
                        key = true;
                        Keys.Add(new Key(id, "primary", primary.ConstraintIdentifier?.Value ?? "", "", "",
                            [column.ColumnIdentifier.Value], "", "", [], primary.StartLine));
                        break;
                    case ForeignKeyConstraintDefinition foreign:
                        Keys.Add(Foreign(id, foreign, foreign.Columns.Count > 0 ? [.. foreign.Columns.Select(c => c.Value)] : [column.ColumnIdentifier.Value]));
                        break;
                }
            }
            // A KEY COLUMN IS NOT NULL, whichever constraint made it one.
            if (key) nullable = false;
            Columns.Add(new Column(id, position++, column.ColumnIdentifier.Value, Text(column.DataType), nullable,
                column.IdentityOptions is not null, key, fallback, Text(column.ComputedColumnExpression), column.StartLine));
        }
        foreach (var constraint in node.Definition?.TableConstraints ?? []) Constraint(id, constraint, "", "");
        foreach (var index in node.Definition?.Indexes ?? [])
        {
            Keys.Add(new Key(id, "index", index.Name?.Value ?? "", "", "",
                [.. index.Columns.Select(c => Last(c.Column))], "", "", [], index.StartLine));
        }
    }

    private void Constraint(int id, ConstraintDefinition constraint, string schema, string table)
    {
        switch (constraint)
        {
            case UniqueConstraintDefinition unique:
                Keys.Add(new Key(id, unique.IsPrimaryKey ? "primary" : "unique", unique.ConstraintIdentifier?.Value ?? "",
                    schema, table, [.. unique.Columns.Select(c => Last(c.Column))], "", "", [], unique.StartLine));
                break;
            case ForeignKeyConstraintDefinition foreign:
                Keys.Add(Foreign(id, foreign, [.. foreign.Columns.Select(c => c.Value)]) with { Schema = schema, Table = table });
                break;
            case CheckConstraintDefinition check:
                Keys.Add(new Key(id, "check", check.ConstraintIdentifier?.Value ?? "", schema, table, [], "", "", [], check.StartLine));
                break;
            case DefaultConstraintDefinition fallback:
                Keys.Add(new Key(id, "default", fallback.ConstraintIdentifier?.Value ?? "", schema, table,
                    fallback.Column is null ? [] : [fallback.Column.Value], "", "", [], fallback.StartLine));
                break;
        }
    }

    private static Key Foreign(int id, ForeignKeyConstraintDefinition foreign, List<string> columns) =>
        new(id, "foreign", foreign.ConstraintIdentifier?.Value ?? "", "", "", columns,
            foreign.ReferenceTableName?.SchemaIdentifier?.Value ?? "", foreign.ReferenceTableName?.BaseIdentifier?.Value ?? "",
            [.. foreign.ReferencedTableColumns.Select(c => c.Value)], foreign.StartLine);

    private static string Last(ColumnReferenceExpression? column) =>
        column?.MultiPartIdentifier?.Identifiers is { Count: > 0 } parts ? parts[^1].Value : "";

    /// <summary>`ALTER TABLE x ADD CONSTRAINT ...` - the key belongs to a table declared elsewhere, so it
    /// carries that table's NAME rather than an object of this file.</summary>
    public override void ExplicitVisit(AlterTableAddTableElementStatement node)
    {
        var schema = node.SchemaObjectName?.SchemaIdentifier?.Value ?? "";
        var table = node.SchemaObjectName?.BaseIdentifier?.Value ?? "";
        foreach (var constraint in node.Definition?.TableConstraints ?? []) Constraint(-1, constraint, schema, table);
    }

    public override void ExplicitVisit(CreateIndexStatement node)
    {
        var id = Declare("index", new SchemaObjectName { Identifiers = { node.Name } }, node,
            $"{node.OnName?.SchemaIdentifier?.Value}.{node.OnName?.BaseIdentifier?.Value}");
        Keys.Add(new Key(id, "index", node.Name?.Value ?? "", node.OnName?.SchemaIdentifier?.Value ?? "",
            node.OnName?.BaseIdentifier?.Value ?? "", [.. node.Columns.Select(c => Last(c.Column))], "", "", [], node.StartLine));
    }

    public override void ExplicitVisit(CreateViewStatement node) =>
        Owned(Declare("view", node.SchemaObjectName, node), () => node.SelectStatement?.Accept(this));

    public override void ExplicitVisit(CreateProcedureStatement node) =>
        Owned(Declare("procedure", node.ProcedureReference?.Name, node), () => node.StatementList?.Accept(this));

    public override void ExplicitVisit(CreateOrAlterProcedureStatement node) =>
        Owned(Declare("procedure", node.ProcedureReference?.Name, node), () => node.StatementList?.Accept(this));

    public override void ExplicitVisit(CreateFunctionStatement node) =>
        Owned(Declare("function", node.Name, node), () => node.StatementList?.Accept(this));

    public override void ExplicitVisit(CreateTriggerStatement node) =>
        Owned(Declare("trigger", node.Name, node,
                $"{node.TriggerObject?.Name?.SchemaIdentifier?.Value}.{node.TriggerObject?.Name?.BaseIdentifier?.Value}"),
            () => node.StatementList?.Accept(this));

    public override void ExplicitVisit(CreateSequenceStatement node) => Declare("sequence", node.Name, node);

    public override void ExplicitVisit(CreateTypeTableStatement node) => Declare("type", node.Name, node);

    public override void ExplicitVisit(CreateTypeUddtStatement node) => Declare("type", node.Name, node);

    public override void ExplicitVisit(CommonTableExpression node)
    {
        ctes.Add(node.ExpressionName.Value);
        base.ExplicitVisit(node);
    }

    public override void ExplicitVisit(InsertSpecification node)
    {
        Target(node.Target, "insert");
        foreach (var column in node.Columns) Written(node.Target, "insert", column);
        base.ExplicitVisit(node);
    }

    public override void ExplicitVisit(UpdateSpecification node)
    {
        var target = Aliased(node.Target, node.FromClause);
        Target(target, "update");
        foreach (var set in node.SetClauses.OfType<AssignmentSetClause>()) Written(target, "update", set.Column);
        base.ExplicitVisit(node);
    }

    public override void ExplicitVisit(DeleteSpecification node)
    {
        Target(Aliased(node.Target, node.FromClause), "delete");
        base.ExplicitVisit(node);
    }

    /// <summary>`UPDATE t SET ... FROM Sales.Orders t` - the target is an ALIAS, and the table it
    /// names is the one in the FROM clause. The alias itself is marked as a target, so it is no read.</summary>
    private TableReference? Aliased(TableReference? target, FromClause? from)
    {
        if (target is not NamedTableReference { SchemaObject.SchemaIdentifier: null } named || from is null) return target;
        var alias = named.SchemaObject.BaseIdentifier?.Value;
        foreach (var candidate in Tables(from.TableReferences))
        {
            if (string.Equals(candidate.Alias?.Value, alias, StringComparison.OrdinalIgnoreCase))
            {
                targets.Add(named);
                return candidate;
            }
        }
        return target;
    }

    private static IEnumerable<NamedTableReference> Tables(IEnumerable<TableReference> references)
    {
        foreach (var reference in references)
        {
            switch (reference)
            {
                case NamedTableReference named: yield return named; break;
                case JoinTableReference join:
                    foreach (var inner in Tables([join.FirstTableReference, join.SecondTableReference])) yield return inner;
                    break;
            }
        }
    }

    public override void ExplicitVisit(MergeSpecification node)
    {
        Target(node.Target, "merge");
        base.ExplicitVisit(node);
    }

    public override void ExplicitVisit(TruncateTableStatement node)
    {
        Record("truncate", node.TableName, "", node.StartLine);
        base.ExplicitVisit(node);
    }

    public override void ExplicitVisit(ExecutableProcedureReference node)
    {
        if (node.ProcedureReference?.ProcedureReference?.Name is { } name) Record("exec", name, "", node.StartLine);
        base.ExplicitVisit(node);
    }

    public override void ExplicitVisit(NamedTableReference node)
    {
        if (!targets.Contains(node)) Record("select", node.SchemaObject, "", node.StartLine);
        base.ExplicitVisit(node);
    }

    private void Target(TableReference? target, string action)
    {
        if (target is not NamedTableReference named) return;
        targets.Add(named);
        Record(action, named.SchemaObject, "", named.StartLine);
    }

    private void Written(TableReference? target, string action, ColumnReferenceExpression? column)
    {
        if (target is NamedTableReference named && Last(column) is { Length: > 0 } name)
            Record(action, named.SchemaObject, name, column!.StartLine);
    }

    private void Record(string action, SchemaObjectName? name, string column, int line)
    {
        var base_ = name?.BaseIdentifier?.Value ?? "";
        var schema = name?.SchemaIdentifier?.Value ?? "";
        if (base_.Length == 0 || base_.StartsWith('#') || base_.StartsWith('@')) return;
        if (schema.Length == 0 && ctes.Contains(base_)) return;
        // A TRIGGER'S `inserted` AND `deleted` are the rows of the statement that fired it, not tables.
        if (schema.Length == 0 && base_.ToLowerInvariant() is "inserted" or "deleted") return;
        Touches.Add(new Touch(owner, action, schema, base_, column, line));
    }

    /// <summary>
    /// SQL INSIDE A STRING is not parsed - its name is often built by `+` - but a `CREATE TRIGGER` in one is
    /// what a deploy runs: dynamic SQL that builds an object name from a prefix and a value creates objects
    /// no `CREATE` declares, and a mapping that names one would read it as missing. The name's literal part is kept.
    /// </summary>
    public override void ExplicitVisit(StringLiteral node)
    {
        if (CreatedTrigger(node.Value) is { } created)
            Dynamics.Add(new Dynamic("trigger", created.Name, created.Open, node.StartLine, Text(node)));
        base.ExplicitVisit(node);
    }

    /// <summary>The name after `CREATE [OR ALTER] TRIGGER` in a string, and whether the string ends inside it.
    /// The last part of a dotted name, brackets dropped; null without one or with no literal part.</summary>
    internal static (string Name, bool Open)? CreatedTrigger(string text)
    {
        for (var at = text.IndexOf("CREATE", StringComparison.OrdinalIgnoreCase); at >= 0;
             at = text.IndexOf("CREATE", at + 6, StringComparison.OrdinalIgnoreCase))
        {
            if (at > 0 && (char.IsLetterOrDigit(text[at - 1]) || text[at - 1] == '_')) continue;
            var i = Word(text, at + 6, "OR") is { } or && Word(text, or, "ALTER") is { } alter ? alter : at + 6;
            if (Word(text, i, "TRIGGER") is not { } name) continue;
            var start = name;
            while (name < text.Length && (char.IsLetterOrDigit(text[name]) || text[name] is '_' or '.' or '[' or ']' or '"')) name++;
            var spelled = text[start..name].Split('.')[^1].Trim('[', ']', '"');
            if (spelled.Length > 0) return (spelled, name == text.Length);
        }
        return null;
    }

    /// <summary>Past whitespace and then `word` (ignoring case) followed by whitespace: the index after that
    /// whitespace, or null.</summary>
    private static int? Word(string text, int at, string word)
    {
        var i = at;
        while (i < text.Length && char.IsWhiteSpace(text[i])) i++;
        if (i == at || string.Compare(text, i, word, 0, word.Length, StringComparison.OrdinalIgnoreCase) != 0) return null;
        i += word.Length;
        var after = i;
        while (i < text.Length && char.IsWhiteSpace(text[i])) i++;
        return i > after ? i : null;
    }

    /// <summary>
    /// A script as ScriptDom can read it: SQLCMD lines (`:r`, `:setvar`) blanked and `$(Var)` spelled as an
    /// identifier of the SAME LENGTH, so every line and offset still points into the original file. The
    /// `:r` targets are returned - a deploy script is a list of the seed files it runs.
    /// </summary>
    public static string Prepared(string source, List<(string Path, int Line)> included)
    {
        var lines = source.Split('\n');
        for (var i = 0; i < lines.Length; i++)
        {
            var trimmed = lines[i].TrimStart();
            if (!trimmed.StartsWith(':')) continue;
            if (trimmed.StartsWith(":r ", StringComparison.OrdinalIgnoreCase))
                included.Add((trimmed[3..].Trim().TrimEnd('\r').Trim('"'), i + 1));
            lines[i] = new string(' ', lines[i].TrimEnd('\r').Length) + (lines[i].EndsWith('\r') ? "\r" : "");
        }
        var joined = string.Join('\n', lines).ToCharArray();
        for (var i = 0; i + 1 < joined.Length; i++)
        {
            if (joined[i] != '$' || joined[i + 1] != '(') continue;
            var end = Array.IndexOf(joined, ')', i + 2);
            if (end < 0) break;
            joined[i] = '_';
            joined[i + 1] = '_';
            joined[end] = '_';
            i = end;
        }
        return new string(joined);
    }
}
