using Microsoft.SqlServer.TransactSql.ScriptDom;

namespace StructureGate;

/// <summary>
/// ONE STATEMENT AS A NORMALIZED TREE - `sql_nodes`: a node per clause, table, column, function, operator and literal,
/// in kinds every dialect the SQL half parses can be written in (`select`, `query`, `from`, `table`, `join`, `on`,
/// `where`, `column`, `func`, `literal`, `param`, `op`, `in`, `cte`, `subquery`, ...). Rust reads it for the
/// fingerprint, the column refs and the shape views (`rust/fbtcore/src/rows/sqlshape/`), so a query is asked of one
/// vocabulary whatever parsed it.
///
/// A FRAGMENT NOT MAPPED HERE IS TRANSPARENT: its children hang off the nearest mapped ancestor. Some nodes exist in
/// no ScriptDom type and are made from where a fragment sits (`on`, `target`, `using`, `partition`, `into`) or from
/// a property (`alias`, `schema`, `database`, `distinct`). Nodes are numbered by where they START in the source - a
/// preorder over children sorted by token, with an explicit stack: a tree is as deep as its statement.
/// </summary>
internal sealed class SqlTree
{
    private sealed class Draft(int parent, string kind, string value, int line, int token)
    {
        public int Parent { get; } = parent;
        public string Kind { get; } = kind;
        public string Value { get; } = value;
        public int Line { get; } = line;
        public int Token { get; } = token;
        public List<int> Children { get; } = [];
    }

    private readonly List<Draft> drafts = [];

    public int Add(int parent, string kind, string value, int line, int token)
    {
        drafts.Add(new Draft(parent, kind, value, line, token));
        if (parent >= 0) drafts[parent].Children.Add(drafts.Count - 1);
        return drafts.Count - 1;
    }

    /// <summary>(n, parent n or -1, ord among its siblings, kind, value, line) in source preorder - siblings by their
    /// first token, two made from one token in the order they were added.</summary>
    public IEnumerable<(int N, int Parent, int Ord, string Kind, string Value, int Line)> Numbered()
    {
        if (drafts.Count == 0) yield break;
        var numbers = new int[drafts.Count];
        var stack = new Stack<(int Draft, int Ord)>();
        stack.Push((0, 0));
        var next = 0;
        while (stack.Count > 0)
        {
            var (at, ord) = stack.Pop();
            numbers[at] = next++;
            var draft = drafts[at];
            yield return (numbers[at], draft.Parent < 0 ? -1 : numbers[draft.Parent], ord, draft.Kind, draft.Value, draft.Line);
            var children = draft.Children.OrderBy(c => drafts[c].Token).ThenBy(c => c).ToList();
            for (var i = children.Count - 1; i >= 0; i--) stack.Push((children[i], i));
        }
    }

    /// <summary>A statement's kind: its ScriptDom type in words (`CreateOrAlterProcedureStatement` is
    /// `create or alter procedure`), with the few a reader asks for by a shorter name.</summary>
    public static string KindOf(TSqlStatement statement) => statement switch
    {
        PredicateSetStatement or SetVariableStatement => "set",
        DeclareVariableStatement or DeclareTableVariableStatement => "declare",
        BeginEndBlockStatement => "block",
        _ => Words(statement.GetType().Name),
    };

    private static string Words(string name)
    {
        if (name.EndsWith("Statement", StringComparison.Ordinal)) name = name[..^"Statement".Length];
        var words = new System.Text.StringBuilder();
        foreach (var c in name)
        {
            if (char.IsUpper(c) && words.Length > 0) words.Append(' ');
            words.Append(char.ToLowerInvariant(c));
        }
        return words.ToString();
    }

    /// <summary>`SET NOCOUNT ON` and `SET XACT_ABORT ON` are both `set`: the root says which, as its tokens after
    /// `SET` spell it (`nocount on`).</summary>
    public static string RootValue(TSqlStatement statement)
    {
        if (statement is not PredicateSetStatement || statement.ScriptTokenStream is not { } tokens) return "";
        var words = new List<string>();
        for (var i = statement.FirstTokenIndex + 1; i <= statement.LastTokenIndex && i < tokens.Count; i++)
        {
            if (tokens[i].TokenType is TSqlTokenType.WhiteSpace or TSqlTokenType.SingleLineComment or TSqlTokenType.MultilineComment
                or TSqlTokenType.Semicolon or TSqlTokenType.Comma) continue;
            words.Add(tokens[i].Text.ToLowerInvariant());
        }
        return string.Join(' ', words);
    }

    /// <summary>A node no ScriptDom type is: what a fragment is to the one holding it.</summary>
    public static string? Wrapper(TSqlFragment parent, TSqlFragment f) => parent switch
    {
        QualifiedJoin join when ReferenceEquals(f, join.SearchCondition) => "on",
        MergeSpecification merge when ReferenceEquals(f, merge.SearchCondition) => "on",
        MergeSpecification merge when ReferenceEquals(f, merge.TableReference) => "using",
        MergeSpecification merge when ReferenceEquals(f, merge.Target) => "target",
        InsertSpecification insert when ReferenceEquals(f, insert.Target) => "target",
        UpdateSpecification update when ReferenceEquals(f, update.Target) => "target",
        DeleteSpecification delete when ReferenceEquals(f, delete.Target) => "target",
        SelectStatement select when ReferenceEquals(f, select.Into) => "into",
        OverClause over when over.Partitions.Any(p => ReferenceEquals(p, f)) => "partition",
        _ => null,
    };

    public static (string Kind, string Value)? Map(TSqlFragment parent, TSqlFragment f) => f switch
    {
        QuerySpecification => ("query", ""),
        BinaryQueryExpression b => (b.BinaryQueryExpressionType switch
        {
            BinaryQueryExpressionType.Union => "union",
            BinaryQueryExpressionType.Except => "except",
            _ => "intersect",
        }, b.All ? "all" : ""),
        FromClause => ("from", ""),
        WhereClause => ("where", ""),
        GroupByClause => ("groupby", ""),
        HavingClause => ("having", ""),
        OrderByClause => ("orderby", ""),
        TopRowFilter => ("top", ""),
        OffsetClause => ("offset", ""),
        ExpressionWithSortOrder sort => ("sort", sort.SortOrder switch
        {
            SortOrder.Descending => "desc",
            SortOrder.Ascending => "asc",
            _ => "",
        }),
        SelectScalarExpression or SelectStarExpression or SelectSetVariable => ("item", ""),
        NamedTableReference table => ("table", table.SchemaObject?.BaseIdentifier?.Value ?? ""),
        VariableTableReference variable => ("table", variable.Variable?.Name ?? ""),
        QueryDerivedTable or ScalarSubquery => ("subquery", ""),
        QualifiedJoin join => ("join", join.QualifiedJoinType switch
        {
            QualifiedJoinType.Inner => "inner",
            QualifiedJoinType.LeftOuter => "left",
            QualifiedJoinType.RightOuter => "right",
            _ => "full",
        }),
        UnqualifiedJoin join => ("join", join.UnqualifiedJoinType switch
        {
            UnqualifiedJoinType.CrossJoin => "cross",
            UnqualifiedJoinType.CrossApply => "cross apply",
            _ => "outer apply",
        }),
        CommonTableExpression cte => ("cte", cte.ExpressionName?.Value ?? ""),
        ColumnReferenceExpression column => ("column", ColumnName(column)),
        FunctionCall call => ("func", FunctionName(call)),
        OverClause => ("over", ""),
        NullLiteral => ("null", ""),
        Literal literal => ("literal", literal.Value ?? ""),
        VariableReference variable => ("param", variable.Name ?? ""),
        GlobalVariableExpression global => ("param", global.Name ?? ""),
        BooleanComparisonExpression comparison => ("op", Comparison(comparison.ComparisonType)),
        BinaryExpression binary => ("op", Arithmetic(binary.BinaryExpressionType)),
        BooleanBinaryExpression both => ("op", both.BinaryExpressionType == BooleanBinaryExpressionType.And ? "and" : "or"),
        BooleanNotExpression => ("op", "not"),
        UnaryExpression unary => ("op", unary.UnaryExpressionType switch
        {
            UnaryExpressionType.Negative => "-",
            UnaryExpressionType.BitwiseNot => "~",
            _ => "+",
        }),
        BooleanIsNullExpression isNull => ("op", isNull.IsNot ? "is not null" : "is null"),
        LikePredicate like => ("op", like.NotDefined ? "not like" : "like"),
        BooleanTernaryExpression ternary => ("op", ternary.TernaryExpressionType == BooleanTernaryExpressionType.NotBetween ? "not between" : "between"),
        ExistsPredicate => ("exists", ""),
        InPredicate inList => ("in", inList.NotDefined ? "not" : ""),
        SearchedCaseExpression or SimpleCaseExpression => ("case", ""),
        SearchedWhenClause or SimpleWhenClause => ("when", ""),
        CastCall => ("func", "cast"),
        TryCastCall => ("func", "try_cast"),
        ConvertCall => ("func", "convert"),
        TryConvertCall => ("func", "try_convert"),
        ParseCall => ("func", "parse"),
        IIfCall => ("func", "iif"),
        CoalesceExpression => ("func", "coalesce"),
        NullIfExpression => ("func", "nullif"),
        LeftFunctionCall => ("func", "left"),
        RightFunctionCall => ("func", "right"),
        DataTypeReference type => ("type", (type.Name?.BaseIdentifier?.Value ?? "").ToLowerInvariant()),
        ValuesInsertSource => ("values", ""),
        RowValue => ("row", ""),
        AssignmentSetClause => ("set", ""),
        OutputClause or OutputIntoClause => ("output", ""),
        MergeActionClause action => ("action", action.Condition switch
        {
            MergeCondition.Matched => "matched",
            MergeCondition.NotMatchedBySource => "not matched by source",
            _ => "not matched",
        }),
        InsertMergeAction => ("insert", ""),
        UpdateMergeAction => ("update", ""),
        DeleteMergeAction => ("delete", ""),
        DeclareVariableElement declared => ("var", declared.VariableName?.Value ?? ""),
        ExecutableProcedureReference exec => ("proc", Dotted(exec.ProcedureReference?.ProcedureReference?.Name)),
        ColumnDefinition column => ("columndef", column.ColumnIdentifier?.Value ?? ""),
        SchemaObjectName name when parent is TSqlStatement => ("name", Dotted(name)),
        ProcedureReference procedure when parent is TSqlStatement => ("name", Dotted(procedure.Name)),
        _ => null,
    };

    /// <summary>The nodes a fragment's PROPERTIES make: a table's schema, database and alias, a select item's alias
    /// and star, a query's DISTINCT. A MERGE keeps its target's alias on itself (`MERGE dbo.Orders AS T`).</summary>
    public void Synthesized(TSqlFragment parent, TSqlFragment f, int node)
    {
        if (parent is MergeSpecification { TableAlias: { } merged } merge && ReferenceEquals(merge.Target, f))
            Add(node, "alias", merged.Value, merged.StartLine, merged.FirstTokenIndex);
        if (f is NamedTableReference { SchemaObject: { } name })
        {
            if (name.DatabaseIdentifier is { } database) Add(node, "database", database.Value, database.StartLine, database.FirstTokenIndex);
            if (name.SchemaIdentifier is { } schema) Add(node, "schema", schema.Value, schema.StartLine, schema.FirstTokenIndex);
        }
        switch (f)
        {
            case TableReferenceWithAlias { Alias: { } alias }:
                Add(node, "alias", alias.Value, alias.StartLine, alias.FirstTokenIndex);
                break;
            case SelectScalarExpression { ColumnName: { } named }:
                Add(node, "alias", named.Value, named.StartLine, named.FirstTokenIndex);
                break;
            case SelectStarExpression star:
                var qualifier = star.Qualifier?.Identifiers is { Count: > 0 } parts ? string.Join('.', parts.Select(p => p.Value)) + "." : "";
                Add(node, "column", qualifier + "*", star.StartLine, star.FirstTokenIndex);
                break;
            case QuerySpecification { UniqueRowFilter: UniqueRowFilter.Distinct } query:
                Add(node, "distinct", "", query.StartLine, query.FirstTokenIndex);
                break;
        }
    }

    private static string Dotted(SchemaObjectName? name) =>
        name?.Identifiers is { Count: > 0 } parts ? string.Join('.', parts.Select(p => p.Value)) : "";

    private static string ColumnName(ColumnReferenceExpression column) => column.ColumnType switch
    {
        ColumnType.Regular or ColumnType.Wildcard when column.MultiPartIdentifier?.Identifiers is { Count: > 0 } parts =>
            string.Join('.', parts.Select(p => p.Value)) + (column.ColumnType == ColumnType.Wildcard ? ".*" : ""),
        ColumnType.Wildcard => "*",
        ColumnType.PseudoColumnAction => "$action",
        ColumnType.PseudoColumnIdentity => "$identity",
        ColumnType.PseudoColumnRowGuid => "$rowguid",
        ColumnType.IdentityCol => "identitycol",
        ColumnType.RowGuidCol => "rowguidcol",
        _ => column.MultiPartIdentifier?.Identifiers is { Count: > 0 } other ? string.Join('.', other.Select(p => p.Value)) : "",
    };

    /// <summary>A function by its name, lowercased - SQL's names are not case-sensitive - with what it is called on
    /// (`dbo.fn`, `x.method`).</summary>
    private static string FunctionName(FunctionCall call)
    {
        var name = call.FunctionName?.Value ?? "";
        var target = call.CallTarget switch
        {
            MultiPartIdentifierCallTarget { MultiPartIdentifier.Identifiers: { Count: > 0 } parts } => string.Join('.', parts.Select(p => p.Value)) + ".",
            _ => "",
        };
        return (target + name).ToLowerInvariant();
    }

    private static string Comparison(BooleanComparisonType type) => type switch
    {
        BooleanComparisonType.Equals => "=",
        BooleanComparisonType.GreaterThan => ">",
        BooleanComparisonType.LessThan => "<",
        BooleanComparisonType.GreaterThanOrEqualTo => ">=",
        BooleanComparisonType.LessThanOrEqualTo => "<=",
        BooleanComparisonType.NotEqualToBrackets or BooleanComparisonType.NotEqualToExclamation => "<>",
        BooleanComparisonType.NotLessThan => ">=",
        BooleanComparisonType.NotGreaterThan => "<=",
        BooleanComparisonType.IsDistinctFrom => "is distinct from",
        BooleanComparisonType.IsNotDistinctFrom => "is not distinct from",
        BooleanComparisonType.NotLike => "not like",
        BooleanComparisonType.LeftOuterJoin => "*=",
        _ => "=*",
    };

    private static string Arithmetic(BinaryExpressionType type) => type switch
    {
        BinaryExpressionType.Add => "+",
        BinaryExpressionType.Subtract => "-",
        BinaryExpressionType.Multiply => "*",
        BinaryExpressionType.Divide => "/",
        BinaryExpressionType.Modulo => "%",
        BinaryExpressionType.BitwiseAnd => "&",
        BinaryExpressionType.BitwiseOr => "|",
        BinaryExpressionType.BitwiseXor => "^",
        BinaryExpressionType.LeftShift => "<<",
        BinaryExpressionType.RightShift => ">>",
        _ => "||",
    };
}
