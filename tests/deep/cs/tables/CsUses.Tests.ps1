<#
    WHO USES A PROPERTY, from `refs` alone: a write and a read inside a lambda are uses as much as a read
    is, and a use the compiler could not bind is a MAYBE that must still be findable - an unbound call already is a
    `calls` row with `symbol = ''`. Silently absent, "who uses this property" answered fewer than half on a consumer solution.

    Its helpers are its own - `-Only CsUses` runs this suite alone.
#>

$script:CsUsesProject = '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework></PropertyGroup></Project>'

# Lines as numbered in the cases: the write on 12, the lambda read on 15, the expression trees on 18-19, the
# initializer on 21, and the call the compiler cannot resolve on 22.
$script:CsUsesSource = @'
using System;
using System.Collections.Generic;
using System.Linq;
using System.Linq.Expressions;
namespace Demo;
public class Item { public DateTime? Start { get; set; } public int Id { get; set; } }
public class Ctx { public DateTime Now { get; set; } }
public class Uses
{
    public void Assign(Item item, Ctx ctx)
    {
        item.Start = ctx.Now;
    }
    public List<Item> Filter(IQueryable<Item> items, DateTime now) =>
        items.Where(m => m.Start != null && m.Start <= now).ToList();
    public void Build(Builder<Item> b)
    {
        b.Property(x => x.Start);
        b.Include(x => new { x.Id, x.Start });
    }
    public Item Make(Ctx c) => new Item { Start = c.Now };
    public void Broken(Builder<Item> b) { b.Missing(y => y.Start); }
}
public class Builder<T> { public void Property<P>(Expression<Func<T, P>> e) { } public void Include(Expression<Func<T, object>> e) { } }
'@

function Get-CsUsesLines([string]$Db, [string]$Where) {
    $r = Invoke-Gate --map-query $Db --sql "SELECT 'L' || line || ':' || name || ':' || kind AS r FROM refs WHERE $Where ORDER BY line"
    Assert-Exit $r 0
    return @($r.Lines | Where-Object { $_.Trim().StartsWith('L') } | ForEach-Object { $_.Trim() })
}

Test-Case 'csuses: a property written, read in a lambda and in an expression tree is a refs row each time' {
    $tree = Use-Tree @{ 'Item.cs' = $script:CsUsesSource; 'Demo.csproj' = $script:CsUsesProject }
    $db = Join-Path $tree 'map.sqlite'
    Invoke-Gate --root $tree --ext .cs --map-sqlite $db | Out-Null
    $bound = (Get-CsUsesLines $db "symbol = 'Demo.Item.Start'") -join ' '
    Assert-Equal $bound 'L12:item.Start:property L15:m.Start:property L18:x.Start:property L19:x.Start:property L21:Start:property' 'the bound uses'
}

Test-Case 'csuses: a use the compiler could not bind is still a row - unbound, by the name it is written as' {
    $tree = Use-Tree @{ 'Item.cs' = $script:CsUsesSource; 'Demo.csproj' = $script:CsUsesProject }
    $db = Join-Path $tree 'map.sqlite'
    Invoke-Gate --root $tree --ext .cs --map-sqlite $db | Out-Null
    # `b.Missing` does not exist, so `y` has no type and `y.Start` binds to nothing: a MAYBE, never a guess.
    $unbound = (Get-CsUsesLines $db "symbol = ''") -join ' '
    Assert-Equal $unbound 'L22:y.Start:unbound' 'the unbound use'
}
