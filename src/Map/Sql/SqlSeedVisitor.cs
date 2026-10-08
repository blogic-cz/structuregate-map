using System.Globalization;
using Microsoft.SqlServer.TransactSql.ScriptDom;

namespace StructureGate;

/// <summary>
/// WHAT A SEED SCRIPT DOES, AS STEPS IN ORDER - read off ScriptDom's tree, never off the text. A step is
/// one of: `values` (a VALUES tuple written into a table, a `#temp` or a `@table`), `set` (a variable
/// DECLAREd or SET), `temp` (a `#temp` or `@table` created, with its columns), `drop` (a `#temp` dropped),
/// and `flow` (a MERGE or an INSERT ... SELECT that moves a `#temp`'s rows into a table or another temp).
///
/// NOTHING IS RESOLVED HERE. The variables a script reads are often declared by the deploy script that
/// runs it with `:r` (a `@demoId` is declared in the post-deploy script and read in
/// a seed script), and a temp is filled in one file and merged in another. SQLCMD inlines each `:r`, so
/// only the whole chain in its run order says what a value is - rust (`rows/seeds/`) walks it after every file.
///
/// ONLY THE SCRIPT ITSELF: the body of a procedure, function, trigger or view runs when it is called, not
/// when the script is deployed, so its INSERTs are no seed.
/// </summary>
internal sealed class SqlSeedVisitor : TSqlFragmentVisitor
{
    public sealed record Step(int Line, int Seq, string Action, string Schema, string Name, string Source,
        List<string> Columns, List<string> Targets, List<string> Kinds, List<string> Exprs, string Text);

    public List<Step> Steps { get; } = [];

    private readonly string original;
    private readonly string prepared;

    /// <param name="original">The file as written - a `$(Var)` in a value is reported as it is spelled.</param>
    /// <param name="prepared">The text ScriptDom parsed (`SqlVisitor.Prepared`); offsets match both.</param>
    public SqlSeedVisitor(string original, string prepared)
    {
        this.original = original;
        this.prepared = prepared;
    }

    /// <summary>Every statement of the script outside a module body.</summary>
    public void Read(TSqlFragment? fragment)
    {
        if (fragment is not TSqlScript script) return;
        foreach (var statement in script.Batches.SelectMany(b => b.Statements))
        {
            if (statement is ProcedureStatementBody or FunctionStatementBody or TriggerStatementBody or ViewStatementBody) continue;
            statement.Accept(this);
        }
    }

    private void Add(TSqlFragment at, string action, string schema, string name, string source = "",
        List<string>? columns = null, List<string>? targets = null, List<string>? kinds = null, List<string>? exprs = null,
        string text = "") =>
        Steps.Add(new Step(at.StartLine, Steps.Count, action, schema, name, source, columns ?? [], targets ?? [],
            kinds ?? [], exprs ?? [], text));

    private string Text(TSqlFragment? fragment, string of) =>
        fragment is null || fragment.StartOffset < 0 || fragment.StartOffset + fragment.FragmentLength > of.Length
            ? "" : of.Substring(fragment.StartOffset, fragment.FragmentLength);

    /// <summary>
    /// One value as `(kind, expr)`: `int`, `num` and `str` carry the literal's value, `null` nothing, `var`
    /// the variable's name, and `expr` the expression's own text - anything a deploy computes at run time.
    /// </summary>
    private (string Kind, string Expr) Value(ScalarExpression? expression)
    {
        var spelled = Text(expression, original);
        // `$(Var)` IS SQLCMD, SUBSTITUTED AT DEPLOY TIME: what ScriptDom read is a stand-in for it.
        if (spelled != Text(expression, prepared)) return ("expr", spelled);
        switch (expression)
        {
            case ParenthesisExpression inner: return Value(inner.Expression);
            case IntegerLiteral integer: return ("int", integer.Value);
            case NumericLiteral or RealLiteral or MoneyLiteral: return ("num", ((Literal)expression).Value);
            // `'café'` WITHOUT `N` is varchar: the server keeps only what the database's code page has, so
            // a seed row reads `cafe` where the collation lacks `é`. The kind says so; the value stays.
            case StringLiteral { IsNational: false } text when text.Value.Any(c => c > 127): return ("vstr", text.Value);
            case StringLiteral text: return ("str", text.Value);
            case NullLiteral: return ("null", "");
            case VariableReference variable: return ("var", variable.Name);
            case UnaryExpression { Expression: IntegerLiteral or NumericLiteral or RealLiteral or MoneyLiteral } signed:
                var (kind, value) = Value(signed.Expression);
                return (kind, signed.UnaryExpressionType == UnaryExpressionType.Negative ? "-" + value : value);
            // `@a + 10`, `@b + @c` (`N'x;y;'` strings):
            // signed terms the chain adds up - or joins, when each is a string - once it knows each variable.
            // A string term is `'` and its value; the terms are split by U+001F, which no script spells.
            case BinaryExpression { BinaryExpressionType: BinaryExpressionType.Add or BinaryExpressionType.Subtract } sum
                when Terms(sum, "+") is { } terms:
                return ("sum", string.Join('\u001f', terms));
            default: return ("expr", spelled);
        }
    }

    private static List<string>? Terms(ScalarExpression expression, string sign)
    {
        switch (expression)
        {
            case ParenthesisExpression inner: return Terms(inner.Expression, sign);
            case IntegerLiteral integer: return [sign + integer.Value];
            case StringLiteral text: return [sign + "'" + text.Value];
            case VariableReference variable: return [sign + variable.Name];
            case BinaryExpression { BinaryExpressionType: BinaryExpressionType.Add or BinaryExpressionType.Subtract } sum:
                var right = sum.BinaryExpressionType == BinaryExpressionType.Add ? sign : sign == "+" ? "-" : "+";
                return Terms(sum.FirstExpression, sign) is { } first && Terms(sum.SecondExpression, right) is { } second
                    ? [.. first, .. second] : null;
            default: return null;
        }
    }

    private void Values(TSqlFragment at, string schema, string name, List<string> columns, IList<ScalarExpression> cells)
    {
        var kinds = new List<string>();
        var exprs = new List<string>();
        foreach (var cell in cells)
        {
            var (kind, expr) = Value(cell);
            kinds.Add(kind);
            exprs.Add(expr);
        }
        Add(at, "values", schema, name, columns: columns, kinds: kinds, exprs: exprs, text: Text(at, original));
    }

    private static string Last(ColumnReferenceExpression? column) =>
        column?.MultiPartIdentifier?.Identifiers is { Count: > 0 } parts ? parts[^1].Value : "";

    /// <summary>A table written to, as `(schema, name)`; `@table` and `#temp` keep their sigil.</summary>
    private static (string Schema, string Name)? Target(TableReference? target) => target switch
    {
        NamedTableReference named => (named.SchemaObject.SchemaIdentifier?.Value ?? "", named.SchemaObject.BaseIdentifier?.Value ?? ""),
        VariableTableReference variable => ("", variable.Variable.Name),
        _ => null,
    };

    private static bool Temp(string name) => name.StartsWith('#') || name.StartsWith('@');

    public override void ExplicitVisit(DeclareVariableStatement node)
    {
        foreach (var declared in node.Declarations)
        {
            var (kind, expr) = declared.Value is null ? ("null", "") : Value(declared.Value);
            Add(declared, "set", "", declared.VariableName.Value, kinds: [kind], exprs: [expr]);
        }
    }

    public override void ExplicitVisit(SetVariableStatement node)
    {
        // `SET @x += 1` depends on what @x was: a value this map does not compute.
        var (kind, expr) = node.AssignmentKind == AssignmentKind.Equals ? Value(node.Expression) : ("expr", Text(node, original));
        Add(node, "set", "", node.Variable.Name, kinds: [kind], exprs: [expr]);
    }

    public override void ExplicitVisit(DeclareTableVariableStatement node) =>
        Add(node, "temp", "", node.Body.VariableName.Value,
            columns: [.. node.Body.Definition?.ColumnDefinitions.Select(c => c.ColumnIdentifier.Value) ?? []]);

    public override void ExplicitVisit(CreateTableStatement node)
    {
        var name = node.SchemaObjectName?.BaseIdentifier?.Value ?? "";
        if (name.StartsWith('#'))
            Add(node, "temp", "", name, columns: [.. node.Definition?.ColumnDefinitions.Select(c => c.ColumnIdentifier.Value) ?? []]);
    }

    public override void ExplicitVisit(DropTableStatement node)
    {
        foreach (var dropped in node.Objects)
        {
            if (dropped.BaseIdentifier?.Value is { } name && name.StartsWith('#')) Add(dropped, "drop", "", name);
        }
    }

    /// <summary>`SELECT TOP 0 [A], [B] INTO #t FROM T` creates `#t` with those columns - or, over `*`,
    /// with T's own, which the map knows by name (`source`). `SELECT @x = 1` with no FROM sets a variable.</summary>
    public override void ExplicitVisit(QuerySpecification node)
    {
        foreach (var assigned in node.SelectElements.OfType<SelectSetVariable>())
        {
            var (kind, expr) = node.FromClause is null && assigned.AssignmentKind == AssignmentKind.Equals
                ? Value(assigned.Expression) : ("expr", Text(assigned, original));
            Add(assigned, "set", "", assigned.Variable.Name, kinds: [kind], exprs: [expr]);
        }
        base.ExplicitVisit(node);
    }

    public override void ExplicitVisit(SelectStatement node)
    {
        if (node.Into?.BaseIdentifier?.Value is { } into && into.StartsWith('#') && node.QueryExpression is QuerySpecification spec)
        {
            var star = spec.SelectElements.Any(e => e is SelectStarExpression);
            var from = spec.FromClause?.TableReferences.Count == 1 ? Target(spec.FromClause.TableReferences[0]) : null;
            Add(node, "temp", "", into, source: star && from is { } table ? $"{table.Schema}.{table.Name}" : "",
                columns: star ? [] : [.. spec.SelectElements.Select(Output)]);
        }
        base.ExplicitVisit(node);
    }

    private static string Output(SelectElement element) => element switch
    {
        SelectScalarExpression { ColumnName.Value: { } alias } => alias,
        SelectScalarExpression { Expression: ColumnReferenceExpression column } => Last(column),
        _ => "",
    };

    public override void ExplicitVisit(InsertSpecification node)
    {
        if (Target(node.Target) is not var (schema, name)) return;
        var columns = node.Columns.Select(Last).ToList();
        switch (node.InsertSource)
        {
            case ValuesInsertSource values:
                foreach (var row in values.RowValues) Values(row, schema, name, columns, row.ColumnValues);
                break;
            case SelectInsertSource select:
                Selected(select.Select, schema, name, columns);
                break;
        }
    }

    /// <summary>`INSERT INTO t (a, b) SELECT ...`: a SELECT of literals with no FROM is a row of values
    /// (`UNION ALL` of them, rows); one reading a temp is a flow of that temp's rows into `t`.</summary>
    private void Selected(QueryExpression? query, string schema, string name, List<string> columns)
    {
        switch (query)
        {
            case BinaryQueryExpression union:
                Selected(union.FirstQueryExpression, schema, name, columns);
                Selected(union.SecondQueryExpression, schema, name, columns);
                break;
            // a seed script: `SELECT r.[Code], r.[Value] FROM (VALUES (...), ...) AS r([Code], [Value])` -
            // each row of the VALUES table, read through the select list.
            case QuerySpecification { FromClause.TableReferences: [InlineDerivedTable derived] } over
                when over.SelectElements.All(e => e is SelectScalarExpression or SelectStarExpression):
                var names = derived.Columns.Select(c => c.Value).ToList();
                var star = over.SelectElements.Any(e => e is SelectStarExpression);
                var into = columns.Count > 0 ? columns : star ? names : [.. over.SelectElements.Select(Output)];
                foreach (var row in derived.RowValues)
                {
                    Values(row, schema, name, into, star ? [.. row.ColumnValues]
                        : [.. over.SelectElements.Cast<SelectScalarExpression>().Select(e => Cell(e.Expression, names, row))]);
                }
                break;
            case QuerySpecification { FromClause: null } literal when literal.SelectElements.All(e => e is SelectScalarExpression):
                Values(literal, schema, name, columns, [.. literal.SelectElements.Cast<SelectScalarExpression>().Select(e => e.Expression)]);
                break;
            case QuerySpecification spec:
                var targets = columns.Count > 0 ? columns : [.. spec.SelectElements.Select(Output)];
                foreach (var (temp, alias, single) in Temps(spec.FromClause))
                {
                    var from = new List<string>();
                    var to = new List<string>();
                    for (var i = 0; i < spec.SelectElements.Count; i++)
                    {
                        if (spec.SelectElements[i] is SelectStarExpression) { from = ["*"]; to = columns; break; }
                        if (Read(spec.SelectElements[i], temp, alias, single) is { } read && i < targets.Count) { from.Add(read); to.Add(targets[i]); }
                    }
                    Add(spec, "flow", schema, name, source: temp, columns: from, targets: to);
                }
                break;
        }
    }

    /// <summary>A select element over a VALUES table: the row's own cell when it names a column of it.</summary>
    private static ScalarExpression Cell(ScalarExpression expression, List<string> names, RowValue row)
    {
        if (expression is not ColumnReferenceExpression column) return expression;
        var at = names.FindIndex(n => string.Equals(n, Last(column), StringComparison.OrdinalIgnoreCase));
        return at >= 0 && at < row.ColumnValues.Count ? row.ColumnValues[at] : expression;
    }

    /// <summary>
    /// `UPDATE t SET a = 1 WHERE id IN (1, 2)` - a seed script retires rows this way, after the
    /// rows are seeded. Recorded only when the WHERE is keys and literals (`col = value`, `col IN (...)`,
    /// joined by AND) over `t` alone: then the walk can say which seeded rows it changes. `columns` are the SET
    /// columns, `targets` the WHERE columns, and `kinds`/`exprs` the SET values followed by the WHERE values.
    /// </summary>
    public override void ExplicitVisit(UpdateSpecification node)
    {
        var target = node.Target;
        if (node.FromClause is { } from)
        {
            // `UPDATE P SET ... FROM Catalog.Products P`: the alias names the one table of the FROM.
            if (from.TableReferences is not [NamedTableReference only] || target is not NamedTableReference { SchemaObject.SchemaIdentifier: null } alias
                || !string.Equals(only.Alias?.Value, alias.SchemaObject.BaseIdentifier?.Value, StringComparison.OrdinalIgnoreCase)) return;
            target = only;
        }
        if (Target(target) is not var (schema, name) || node.WhereClause?.Cursor is not null) return;
        var where = new List<(string Column, ScalarExpression Value)>();
        if (node.WhereClause?.SearchCondition is { } condition && !Where(condition, where)) return;
        var sets = new List<(string Column, (string Kind, string Expr) Value)>();
        foreach (var clause in node.SetClauses)
        {
            if (clause is not AssignmentSetClause { Column: { } column, Variable: null } set) return;
            sets.Add((Last(column), set.AssignmentKind == AssignmentKind.Equals ? Value(set.NewValue) : ("expr", Text(set, original))));
        }
        var keys = where.Select(w => Value(w.Value)).ToList();
        if (keys.Any(k => k.Kind == "expr")) return;
        Add(node, "update", schema, name, columns: [.. sets.Select(s => s.Column)], targets: [.. where.Select(w => w.Column)],
            kinds: [.. sets.Select(s => s.Value.Kind), .. keys.Select(k => k.Kind)],
            exprs: [.. sets.Select(s => s.Value.Expr), .. keys.Select(k => k.Expr)], text: Text(node, original));
    }

    private static bool Where(BooleanExpression condition, List<(string Column, ScalarExpression Value)> into)
    {
        switch (condition)
        {
            case BooleanParenthesisExpression inner: return Where(inner.Expression, into);
            case BooleanBinaryExpression { BinaryExpressionType: BooleanBinaryExpressionType.And } both:
                return Where(both.FirstExpression, into) && Where(both.SecondExpression, into);
            case BooleanComparisonExpression { ComparisonType: BooleanComparisonType.Equals } equals:
                var (column, value) = equals.FirstExpression is ColumnReferenceExpression first
                    ? (first, equals.SecondExpression) : (equals.SecondExpression as ColumnReferenceExpression, equals.FirstExpression);
                if (column is null || value is ColumnReferenceExpression) return false;
                into.Add((Last(column), value));
                return true;
            case InPredicate { NotDefined: false, Subquery: null, Expression: ColumnReferenceExpression listed } list:
                foreach (var item in list.Values) into.Add((Last(listed), item));
                return true;
            default: return false;
        }
    }

    /// <summary>`IF '$(Mode)' = 'Full' BEGIN :r .\Full\Deploy.sql END ELSE ...`: a branch the
    /// DEPLOY decides, so every row under it is only true of that deploy. The step spans the branch's lines
    /// (`source` is its last line); an IF the data decides (`IF NOT EXISTS`) is no condition.</summary>
    public override void ExplicitVisit(IfStatement node)
    {
        var predicate = Text(node.Predicate, original);
        if (predicate.Contains("$(", StringComparison.Ordinal))
        {
            Add(node.ThenStatement, "if", "", predicate, source: EndLine(node.ThenStatement).ToString(CultureInfo.InvariantCulture));
            if (node.ElseStatement is { } otherwise)
                Add(otherwise, "if", "", $"NOT ({predicate})", source: EndLine(otherwise).ToString(CultureInfo.InvariantCulture));
        }
        base.ExplicitVisit(node);
    }

    private static int EndLine(TSqlFragment fragment) =>
        fragment.LastTokenIndex >= 0 && fragment.ScriptTokenStream is { } tokens && fragment.LastTokenIndex < tokens.Count
            ? tokens[fragment.LastTokenIndex].Line : fragment.StartLine;

    /// <summary>The temp column a select element reads, when it reads one of THAT temp.</summary>
    private static string? Read(SelectElement element, string temp, string alias, bool single)
    {
        if (element is not SelectScalarExpression { Expression: ColumnReferenceExpression column }) return null;
        var parts = column.MultiPartIdentifier?.Identifiers;
        if (parts is not { Count: > 0 }) return null;
        if (parts.Count == 1) return single ? parts[0].Value : null;
        var qualifier = parts[^2].Value;
        return string.Equals(qualifier, alias, StringComparison.OrdinalIgnoreCase)
            || string.Equals(qualifier, temp, StringComparison.OrdinalIgnoreCase) ? parts[^1].Value : null;
    }

    /// <summary>Every temp a FROM clause reads, with its alias and whether it is the only table there.</summary>
    private static List<(string Temp, string Alias, bool Single)> Temps(FromClause? from)
    {
        var all = new List<(string Name, string Alias)>();
        void Walk(TableReference reference)
        {
            switch (reference)
            {
                case JoinTableReference join: Walk(join.FirstTableReference); Walk(join.SecondTableReference); break;
                case NamedTableReference named: all.Add((named.SchemaObject.BaseIdentifier?.Value ?? "", named.Alias?.Value ?? "")); break;
                case VariableTableReference variable: all.Add((variable.Variable.Name, variable.Alias?.Value ?? "")); break;
                default: all.Add(("", "")); break;
            }
        }
        foreach (var reference in from?.TableReferences ?? []) Walk(reference);
        return [.. all.Where(t => Temp(t.Name)).Select(t => (t.Name, t.Alias, all.Count == 1))];
    }

    /// <summary>
    /// `MERGE t USING s`: the merge's own INSERT and UPDATE clauses say which column of `s` lands in which
    /// column of `t`. `s` is a temp (a flow), `(VALUES ...) AS s(a, b)` (rows written into `t` here), or a
    /// SELECT over a temp - a `USING (SELECT ... FROM #Items JOIN ...)` - whose
    /// select list is followed back to the temp's columns.
    /// </summary>
    public override void ExplicitVisit(MergeSpecification node)
    {
        if (Target(node.Target) is not var (schema, name)) return;
        var map = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase);
        foreach (var clause in node.ActionClauses)
        {
            switch (clause.Action)
            {
                case InsertMergeAction insert when insert.Source?.RowValues is [var row]:
                    // `THEN INSERT VALUES (...)` with no column list fills the table's columns in order: `#3` is
                    // its third column an INSERT may write, which only the map knows.
                    for (var i = 0; i < row.ColumnValues.Count && (insert.Columns.Count == 0 || i < insert.Columns.Count); i++)
                    {
                        if (row.ColumnValues[i] is ColumnReferenceExpression from)
                            map.TryAdd(Last(from), insert.Columns.Count == 0 ? $"#{i + 1}" : Last(insert.Columns[i]));
                    }
                    break;
                case UpdateMergeAction update:
                    foreach (var set in update.SetClauses.OfType<AssignmentSetClause>())
                    {
                        if (set.NewValue is ColumnReferenceExpression from) map.TryAdd(Last(from), Last(set.Column));
                    }
                    break;
            }
        }
        string Mapped(string column) => map.GetValueOrDefault(column) ?? column;
        switch (node.TableReference)
        {
            case InlineDerivedTable inline:
                var columns = inline.Columns.Select(c => Mapped(c.Value)).ToList();
                foreach (var row in inline.RowValues) Values(row, schema, name, columns, row.ColumnValues);
                break;
            case NamedTableReference or VariableTableReference when Target(node.TableReference) is var (_, temp) && Temp(temp):
                Add(node.TableReference, "flow", schema, name, source: temp, columns: [.. map.Keys], targets: [.. map.Values]);
                break;
            case QueryDerivedTable { QueryExpression: QuerySpecification spec } derived:
                foreach (var (temp, alias, single) in Temps(spec.FromClause))
                {
                    var from = new List<string>();
                    var to = new List<string>();
                    for (var i = 0; i < spec.SelectElements.Count; i++)
                    {
                        var output = derived.Columns.Count > i ? derived.Columns[i].Value : Output(spec.SelectElements[i]);
                        if (Read(spec.SelectElements[i], temp, alias, single) is not { } read || !map.TryGetValue(output, out var landed)) continue;
                        from.Add(read);
                        to.Add(landed);
                    }
                    Add(derived, "flow", schema, name, source: temp, columns: from, targets: to);
                }
                break;
        }
    }
}
