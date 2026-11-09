<#
    The deep-map RULES over C# (`rust/fbtcore/src/rules/`): each read off the bound rows after a `--map-sqlite`
    run, each switched by `structuregate.rules.json` (`--rules-config` here) - off, warn or error - and listed
    whatever the file says by `--map-query <db> --rules`. The columns they read are `calls.dropped`,
    `functions.awaits`, `functions.body` and `functions.inherited` (`src/Map/CsRows`).

    Its helpers are its own - `-Only CsRules` runs this suite alone.
#>

$script:CsRulesProject = '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework></PropertyGroup></Project>'

# Lines as the cases name them: each statement's line is its number here.
$script:CsRulesStore = @'
using System.Collections.Generic;
using System.Threading;
using System.Threading.Tasks;
namespace Demo;
public interface IStore { Task Put(int id); }
public class Store : IStore
{
    public Task Put(int id) => Task.CompletedTask;
    public async Task Save(CancellationToken token)
    {
        Load();
        _ = Load();
        await Load();
        Load(); // async-ok: warms the cache
    }
    public async Task<int> Load() => 1;
    public async Task<IEnumerable<int>> All(CancellationToken token) { await Task.Yield(); return new[] { 1 }; }
    public async Task Each(CancellationToken token) { await foreach (var x in Stream(token)) { } }
    private async IAsyncEnumerable<int> Stream([System.Runtime.CompilerServices.EnumeratorCancellation] CancellationToken token) { await Task.Yield(); yield return 1; }
    public void Kick(Store other) { other?.Save(default); }
    public int Count() { Total(); return 1; }
    private int Total() => 2;
}
'@

# A tree with the store, a project and (when given) a rules config; the map run's result and its database.
function Invoke-CsRulesMap([string]$Rules) {
    $files = @{ 'Store.cs' = $script:CsRulesStore; 'Demo.csproj' = $script:CsRulesProject }
    if ($Rules) { $files['rules.json'] = $Rules }
    $tree = Use-Tree $files
    $db = Join-Path $tree 'map.sqlite'
    $run = @('--root', $tree, '--ext', '.cs', '--map-sqlite', $db)
    if ($Rules) { $run += @('--rules-config', (Join-Path $tree 'rules.json')) }
    return @{ Result = (Invoke-Gate @run); Db = $db }
}

# The `file:line` of every finding a rule printed, in order.
function Get-CsRulesHits($Result, [string]$Rule) {
    return @($Result.Lines | Where-Object { $_ -like "*rule * $Rule *" } | ForEach-Object { ($_ -split '\s+' | Where-Object { $_ -like '*Store.cs:*' }) -replace '^.*Store\.cs:', '' })
}

function Assert-CsRulesHits($Result, [string]$Rule, [string[]]$Lines) {
    $got = (Get-CsRulesHits $Result $Rule) -join ','
    if ($got -ne ($Lines -join ',')) { throw "$Rule found lines '$got', expected '$($Lines -join ',')'. Output:`n$($Result.Text)" }
}

Test-Case 'csrules: every rule is off without a config and the run passes' {
    $run = Invoke-CsRulesMap ''
    Assert-Exit $run.Result 0
    if ($run.Result.Text -like '*rule *cs-*') { throw "a rule ran with no config. Output:`n$($run.Result.Text)" }
}

Test-Case 'csrules: each rule at warn lists what it finds and the run passes' {
    $run = Invoke-CsRulesMap '{"rules": {"cs-task-dropped": "warn", "cs-task-not-async": "warn", "cs-async-no-await": "warn", "cs-task-no-cancellation": "warn", "cs-task-ienumerable": "warn"}}'
    Assert-Exit $run.Result 0
    # 12 takes it with `_ =`, 13 awaits it, 14 is waived, 22 drops an int.
    Assert-CsRulesHits $run.Result 'cs-task-dropped' @('11', '20')
    # The interface member has no body; `Put` returns a task straight through.
    Assert-CsRulesHits $run.Result 'cs-task-not-async' @('8')
    # `Each` waits in an `await foreach`.
    Assert-CsRulesHits $run.Result 'cs-async-no-await' @('16')
    # `Store.Put` implements the interface's, whose own declaration is the one to change.
    Assert-CsRulesHits $run.Result 'cs-task-no-cancellation' @('5', '16')
    Assert-CsRulesHits $run.Result 'cs-task-ienumerable' @('17')
}

Test-Case 'csrules: a rule at error fails the run and one left off says nothing' {
    $run = Invoke-CsRulesMap '{"rules": {"cs-task-dropped": "error"}}'
    Assert-Exit $run.Result 1
    Assert-CsRulesHits $run.Result 'cs-task-dropped' @('11', '20')
    if ($run.Result.Text -like '*cs-task-ienumerable*') { throw "a rule left off was listed. Output:`n$($run.Result.Text)" }
}

Test-Case 'csrules: a config naming no rule fails the run' {
    $run = Invoke-CsRulesMap '{"rules": {"cs-no-such-rule": "warn"}}'
    Assert-Exit $run.Result 1
    Assert-Line $run.Result 'no rule is called "cs-no-such-rule"'
}

Test-Case 'csrules: the --rules lens lists every rule whatever the config says' {
    $run = Invoke-CsRulesMap ''
    $lens = Invoke-MapQuery $run.Db --rules --width 0
    Assert-Exit $lens 0
    Assert-Line $lens 'cs-task-ienumerable - returns a task of IEnumerable (1)'
    Assert-Line $lens 'cs-task-dropped - a task nobody awaits - its exception reaches no one (2)'
}
