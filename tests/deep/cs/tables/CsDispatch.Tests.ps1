<#
    WHICH HANDLER A MEDIATOR CALL RUNS (`rust/fbtcore/src/rows/dispatch.rs`): `dispatch` joins a `Send`/`Publish` call to
    every class whose closed base `IRequestHandler<T, ..>` / `INotificationHandler<T>` names the call's argument type T
    exactly. A call of another name, and a message no handler is closed over, are no row.

    Its helpers are its own - `-Only CsDispatch` runs this suite alone.
#>

$script:CsDispatchProject = '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework>' +
    '<ImplicitUsings>enable</ImplicitUsings></PropertyGroup></Project>'
$script:CsDispatchMediator = @'
namespace MediatR
{
    public interface IRequest<out TResponse> { }
    public interface INotification { }
    public interface IRequestHandler<in TRequest, TResponse> where TRequest : IRequest<TResponse> { Task<TResponse> Handle(TRequest request, CancellationToken ct); }
    public interface INotificationHandler<in TNotification> where TNotification : INotification { Task Handle(TNotification n, CancellationToken ct); }
    public interface ISender { Task<TResponse> Send<TResponse>(IRequest<TResponse> request, CancellationToken ct = default); }
    public interface IPublisher { Task Publish<TNotification>(TNotification n, CancellationToken ct = default) where TNotification : INotification; }
}
'@
$script:CsDispatchApp = @'
using MediatR;
namespace Demo;
public record ArchivedOrdersQuery(int Page) : IRequest<string[]>;
public record OtherQuery(int Page) : IRequest<string[]>;
public class ArchivedOrdersQueryHandler : IRequestHandler<ArchivedOrdersQuery, string[]>
{
    public Task<string[]> Handle(ArchivedOrdersQuery q, CancellationToken ct) => Task.FromResult(new string[0]);
}
public record OrderPlaced(int Id) : INotification;
public class MailOnPlaced : INotificationHandler<OrderPlaced> { public Task Handle(OrderPlaced n, CancellationToken ct) => Task.CompletedTask; }
public class LogOnPlaced : INotificationHandler<OrderPlaced> { public Task Handle(OrderPlaced n, CancellationToken ct) => Task.CompletedTask; }
public class OrdersEndpoint(ISender sender, IPublisher publisher)
{
    public Task<string[]> Get(int page) => sender.Send(new ArchivedOrdersQuery(page));
    public Task<string[]> Other(int page) => sender.Send(new OtherQuery(page));
    public Task Placed(OrderPlaced e) => publisher.Publish(e);
}
'@

function Get-CsDispatchRows([string]$Db, [string]$Sql) {
    $r = Invoke-Gate --map-query $Db --width 0 --sql $Sql
    Assert-Exit $r 0
    return $r
}

Test-Case 'csdispatch: a Send reaches the request handler of its argument type, a Publish every notification handler' {
    $tree = Use-Tree @{ 'Demo.csproj' = $script:CsDispatchProject; 'Mediator.cs' = $script:CsDispatchMediator; 'App.cs' = $script:CsDispatchApp }
    $db = Join-Path $tree 'map.sqlite'
    Assert-Exit (Invoke-Gate --root $tree --ext .cs --map-sqlite $db) 0
    $r = Get-CsDispatchRows $db "SELECT 'd=' || func || '|' || kind || '|' || message || '|' || handler AS d FROM dispatch"
    Assert-Line $r 'd=Get|request|Demo.ArchivedOrdersQuery|Demo.ArchivedOrdersQueryHandler.Handle'
    Assert-Line $r 'd=Placed|notification|Demo.OrderPlaced|Demo.MailOnPlaced.Handle'
    Assert-Line $r 'd=Placed|notification|Demo.OrderPlaced|Demo.LogOnPlaced.Handle'
    # `OtherQuery` has no handler: no row, never the nearest one
    $n = Get-CsDispatchRows $db "SELECT 'n=' || count(*) AS n FROM dispatch"
    Assert-Line $n 'n=3'
    # the row joins its call
    $j = Get-CsDispatchRows $db "SELECT 'j=' || c.callee AS j FROM dispatch d JOIN calls c ON c.id = d.call WHERE d.kind = 'request'"
    Assert-Line $j 'j=sender.Send'
}
