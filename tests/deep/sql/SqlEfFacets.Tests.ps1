<#
    WHAT A FLUENT MAPPING SAYS OF EACH COLUMN (#128, `src/Map/Sql/SqlLinksEfFacets.cs`): an `ef_column` link sits on its column's own
    `Property(..)` call, carries the facets the call's chain sets - `precision`/`scale`, `max_length`, `required`, `column_type`,
    `conversion`, NULL where none is set - and follows `HasColumnName` to the column it renames. A column no `Property` call names
    keeps the mapping's `ToTable` line.

    Its helpers are its own - `-Only SqlEfFacets` runs this suite alone.
#>

$script:SqlEfFacetsFiles = @{
    'Db/Db.sqlproj' = '<Project DefaultTargets="Build"><ItemGroup><Build Include="Orders.sql" /></ItemGroup></Project>'
    'Db/Orders.sql' = "CREATE TABLE [Sales].[Orders] ([OrderID] INT NOT NULL PRIMARY KEY, [Price] DECIMAL(18, 4) NULL, [Code] NVARCHAR(20) NOT NULL, " +
        "[LEGACY_NAME] NVARCHAR(10) NULL, [Kind] NVARCHAR(10) NOT NULL, [Note] NVARCHAR(100) NULL)`n"
    'App/App.csproj' = '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework></PropertyGroup></Project>'
    'App/StandIns.cs' = @'
namespace Microsoft.EntityFrameworkCore.Metadata.Builders
{
    public class PropertyBuilder<P>
    {
        public PropertyBuilder<P> HasMaxLength(int length) => this;
        public PropertyBuilder<P> IsRequired(bool required = true) => this;
        public PropertyBuilder<P> HasPrecision(int precision, int scale) => this;
        public PropertyBuilder<P> HasColumnType(string type) => this;
        public PropertyBuilder<P> HasColumnName(string name) => this;
        public PropertyBuilder<P> HasConversion<TProvider>() => this;
    }
    public class EntityTypeBuilder<T> { public PropertyBuilder<P> Property<P>(System.Linq.Expressions.Expression<System.Func<T, P>> member) => new(); }
}
namespace Microsoft.EntityFrameworkCore
{
    using Microsoft.EntityFrameworkCore.Metadata.Builders;
    public static class RelationalEntityTypeBuilderExtensions
    {
        public static EntityTypeBuilder<T> ToTable<T>(this EntityTypeBuilder<T> builder, string name, string schema) => builder;
    }
}
'@
    'App/OrderMapping.cs' = @'
namespace Demo
{
    using Microsoft.EntityFrameworkCore;
    using Microsoft.EntityFrameworkCore.Metadata.Builders;
    public class Order { public int OrderID { get; set; } public decimal? Price { get; set; } public string Code { get; set; } public string Name { get; set; } public int Kind { get; set; } public string Note { get; set; } }
    public class OrderMapping
    {
        public void Configure(EntityTypeBuilder<Order> builder)
        {
            builder.ToTable("Orders", "Sales");
            builder.Property(x => x.Price).HasPrecision(18, 4);
            builder.Property(x => x.Code).HasMaxLength(20).IsRequired();
            builder.Property(x => x.Name).HasColumnName("LEGACY_NAME").IsRequired(false);
            builder.Property(x => x.Kind).HasConversion<string>().HasColumnType("nvarchar(10)");
            builder.Property(x => x.Note);
        }
    }
}
'@
}

Test-Case 'sqlefacets: an ef_column link sits on its Property call and carries the facets the mapping sets (#128)' {
    $tree = Use-Tree $script:SqlEfFacetsFiles
    $db = Join-Path $tree 'links.sqlite'
    Assert-Exit (Invoke-Gate --root $tree --ext '.cs,.sql' --map-sqlite $db) 0
    $r = Invoke-Gate --map-query $db --width 0 --limit 0 --sql ("SELECT 'v=' || column || '|' || line || '|' || member || '|' || status || '|' || coalesce(precision, '-') || '|' || " +
        "coalesce(scale, '-') || '|' || coalesce(max_length, '-') || '|' || coalesce(required, '-') || '|' || coalesce(column_type, '-') || '|' || coalesce(conversion, '-') AS v " +
        "FROM sql_links WHERE kind = 'ef_column'")
    Assert-Exit $r 0
    # A COLUMN NO `Property` CALL NAMES keeps the mapping's line and has no facet.
    Assert-Line $r 'v=OrderID|10|Demo.Order.OrderID|bound|-|-|-|-|-|-'
    Assert-Line $r 'v=Price|11|Demo.Order.Price|bound|18|4|-|-|-|-'
    Assert-Line $r 'v=Code|12|Demo.Order.Code|bound|-|-|20|1|-|-'
    # `HasColumnName` maps the property to the column it names - no longer `no property` - and `IsRequired(false)` is 0.
    Assert-Line $r 'v=LEGACY_NAME|13|Demo.Order.Name|bound|-|-|-|0|-|-'
    Assert-Line $r 'v=Kind|14|Demo.Order.Kind|bound|-|-|-|-|nvarchar(10)|string'
    # A BARE `Property` call: its own line, every facet NULL - EF's defaults, which the SQL column may not match.
    Assert-Line $r 'v=Note|15|Demo.Order.Note|bound|-|-|-|-|-|-'
}
