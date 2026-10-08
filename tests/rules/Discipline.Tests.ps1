<#
    The opt-in rule sets and the plugin runner - everything that folds a SECOND verdict into the one exit
    code. The shared property under test: a check that could not run is a VIOLATION, never a skip. A check
    nobody noticed was skipped is worse than one that never existed.
#>

# ---------------------------------------------------------------- C# async discipline

Test-Case 'async: a blocking wait inside an async body fails, and the line is named' {
    $tree = Use-Tree @{
        'a.cs' = @"
using System.Threading.Tasks;
class C {
    async Task M() {
        var x = WorkAsync().Result;
        await Task.Yield();
    }
    Task<int> WorkAsync() => Task.FromResult(1);
}
"@
    }
    $clean = Invoke-Gate --root $tree
    Assert-Exit $clean 0                       # opt-in: silent until asked for

    $result = Invoke-Gate --root $tree --async-discipline
    Assert-Exit $result 1
    Assert-Line $result 'a.cs:4'
    Assert-Line $result '.Result'
}

Test-Case 'async: // async-ok waives the line, so the call that cannot be awaited keeps its reason' {
    $tree = Use-Tree @{
        'a.cs' = @"
using System.Threading.Tasks;
class C {
    async Task M() {
        var x = WorkAsync().Result; // async-ok: measured, and this one cannot be awaited
        await Task.Yield();
    }
    Task<int> WorkAsync() => Task.FromResult(1);
}
"@
    }
    Assert-Exit (Invoke-Gate --root $tree --async-discipline) 0
}

Test-Case 'async: async void is allowed for an event handler and refused everywhere else' {
    $tree = Use-Tree @{
        'handler.cs' = @"
using System;
using System.Threading.Tasks;
class C {
    async void OnClick(object sender, EventArgs e) { await Task.Yield(); }
}
"@
        'fire.cs' = @"
using System.Threading.Tasks;
class D {
    async void FireAndForget() { await Task.Yield(); }
}
"@
    }
    $result = Invoke-Gate --root $tree --async-discipline
    Assert-Exit $result 1
    Assert-Line $result 'FireAndForget'
    Assert-NoLine $result 'OnClick'
}

Test-Case 'async: a sync API with an awaitable twin is refused inside an async body' {
    $tree = Use-Tree @{
        'a.cs' = @"
using System.IO;
using System.Threading.Tasks;
class C {
    async Task M() {
        var t = File.ReadAllText("x");
        await Task.Yield();
    }
}
"@
    }
    $result = Invoke-Gate --root $tree --async-discipline
    Assert-Exit $result 1
    Assert-Line $result 'ReadAllTextAsync'
}

# ---------------------------------------------------------------- plugins

# A plugin is a COMMAND LINE, and the commands every machine has differ: `cmd` on Windows, coreutils and
# `sh` elsewhere. What each case asserts is the gate's handling of the command, never the shell.
$script:DisciplineEcho = if ($script:OnWindows) { 'cmd /c echo' } else { 'echo' }
function Get-DisciplineExit([int]$Code) {
    if ($script:OnWindows) { return "cmd /c exit $Code" }
    return "sh -c `"exit $Code`""
}

Test-Case 'plugin: a passing checker is echoed and does not change the verdict' {
    $tree = Use-Tree @{ 'a.cs' = "class A { }`n" }
    $result = Invoke-Gate --root $tree --plugin "$($script:DisciplineEcho) plugin-was-here"
    Assert-Exit $result 0
    Assert-Line $result 'plugin-was-here'
}

Test-Case 'plugin: a non-zero exit is a violation of THIS gate' {
    $tree = Use-Tree @{ 'a.cs' = "class A { }`n" }
    $result = Invoke-Gate --root $tree --plugin (Get-DisciplineExit 3)
    Assert-Exit $result 1
    Assert-Line $result 'failed (exit 3)'
}

Test-Case 'plugin: one that cannot be LAUNCHED fails - a skipped check is the worst outcome' {
    $tree = Use-Tree @{ 'a.cs' = "class A { }`n" }
    $result = Invoke-Gate --root $tree --plugin 'no-such-interpreter-anywhere script.py'
    Assert-Exit $result 1
    Assert-Line $result 'no-such-interpreter-anywhere'
}

Test-Case 'plugin: a quoted path with a space survives the split' {
    $tree = Use-Tree @{ 'a.cs' = "class A { }`n" }
    $folder = Join-Path $tree 'two words'
    [void](New-Item -ItemType Directory -Path $folder -Force)
    if ($script:OnWindows) {
        $checker = Join-Path $folder 'check.cmd'
        [System.IO.File]::WriteAllText($checker, '@echo the plugin ran' + [char]13 + [char]10)
    } else {
        $checker = Join-Path $folder 'check.sh'
        [System.IO.File]::WriteAllText($checker, '#!/bin/sh' + [char]10 + 'echo the plugin ran' + [char]10)
        & chmod +x $checker
    }

    # Invoke-GateRaw, because Windows PowerShell would re-quote the argument this case is about.
    $result = Invoke-GateRaw ('--root "' + $tree + '" --plugin "\"' + $checker + '\""')
    Assert-Exit $result 0
    Assert-Line $result 'the plugin ran'
}

# ---------------------------------------------------------------- async: the rest of the shapes

Test-Case 'async: .GetAwaiter().GetResult() is refused even in a SYNC method' {
    # This is where sync-over-async hides: the call chain that should have been async to the top.
    $tree = Use-Tree @{
        'a.cs' = @"
using System.Threading.Tasks;
class C {
    int M() => WorkAsync().GetAwaiter().GetResult();
    Task<int> WorkAsync() => Task.FromResult(1);
}
"@
    }
    $result = Invoke-Gate --root $tree --async-discipline
    Assert-Exit $result 1
    Assert-Line $result 'GetAwaiter().GetResult()'
}

Test-Case 'async: Task.WaitAll and Task.WaitAny name the awaitable twin' {
    $tree = Use-Tree @{
        'a.cs' = @"
using System.Threading.Tasks;
class C {
    void M(Task[] all) {
        Task.WaitAll(all);
        Task.WaitAny(all);
    }
}
"@
    }
    $result = Invoke-Gate --root $tree --async-discipline
    Assert-Exit $result 1
    Assert-Line $result 'await Task.WhenAll instead'
    Assert-Line $result 'await Task.WhenAny instead'
}

Test-Case 'async: Thread.Sleep and .Wait() inside an async body are refused' {
    $tree = Use-Tree @{
        'a.cs' = @"
using System.Threading;
using System.Threading.Tasks;
class C {
    async Task M(Task other) {
        Thread.Sleep(10);
        other.Wait();
        await Task.Yield();
    }
}
"@
    }
    $result = Invoke-Gate --root $tree --async-discipline
    Assert-Exit $result 1
    Assert-Line $result 'await Task.Delay instead'
    Assert-Line $result 'blocks on .Wait() inside an async body'
}

Test-Case 'async: a BOUNDED WaitForExit(ms) is left alone, the no-argument one is not' {
    $tree = Use-Tree @{
        'bounded.cs' = @"
using System.Diagnostics;
using System.Threading.Tasks;
class C {
    async Task M(Process p) { p.WaitForExit(500); await Task.Yield(); }
}
"@
        'blocking.cs' = @"
using System.Diagnostics;
using System.Threading.Tasks;
class D {
    async Task M(Process p) { p.WaitForExit(); await Task.Yield(); }
}
"@
    }
    $result = Invoke-Gate --root $tree --async-discipline
    Assert-Exit $result 1
    Assert-Line $result 'blocking.cs'
    Assert-NoLine $result 'bounded.cs'
}

Test-Case 'async: a SYNC lambda nested in an async method is where the author chose to block' {
    $tree = Use-Tree @{
        'a.cs' = @"
using System.IO;
using System.Threading.Tasks;
class C {
    async Task M() {
        System.Func<string> read = () => File.ReadAllText("x");
        var t = read();
        await Task.Yield();
    }
}
"@
    }
    # The nearest enclosing function is the sync lambda, so the rule stays silent - it reports where the
    # author asked to be asynchronous, not everywhere inside a file that has an async method in it.
    Assert-Exit (Invoke-Gate --root $tree --async-discipline) 0
}

Test-Case 'async: a plain x.Result on something not task-shaped is not this rule''s business' {
    $tree = Use-Tree @{
        'a.cs' = @"
using System.Threading.Tasks;
record Outcome(int Result);
class C {
    async Task M(Outcome outcome) {
        var n = outcome.Result;
        await Task.Yield();
    }
}
"@
    }
    Assert-Exit (Invoke-Gate --root $tree --async-discipline) 0
}

Test-Case 'async: the rule is C# only - a .ts file with .Result in it is untouched' {
    $tree = Use-Tree @{ 'a.ts' = "const x = getThing().Result;`n" }
    Assert-Exit (Invoke-Gate --root $tree --async-discipline) 0
}

Test-Case 'plugin: the command splitter keeps a quoted argument WHOLE' {
    $tree = Use-Tree @{ 'a.cs' = "class A { }`n" }
    # `cmd /c echo` prints its arguments back, and the difference between the two runs is the whole point:
    # a quoted `one two` reaches the process as ONE argument (re-quoted on the way, which is how it shows),
    # while unquoted it is two. If the splitter ignored quotes, both would print identically.
    if ($script:OnWindows) {
        $quoted = Invoke-GateRaw ('--root "' + $tree + '" --plugin "cmd /c echo \"one two\" three"')
        Assert-Exit $quoted 0
        Assert-Line $quoted '"one two" three'

        $bare = Invoke-GateRaw ('--root "' + $tree + '" --plugin "cmd /c echo one two three"')
        Assert-Exit $bare 0
        Assert-Line $bare 'one two three'
        Assert-NoLine $bare '"one two"'
        return
    }
    # Off Windows `echo` joins its arguments with a space, so one argument and two print alike; `printf`
    # brackets each argument it was handed, which shows the split itself.
    $quoted = Invoke-GateRaw ('--root "' + $tree + '" --plugin "printf [%s] \"one two\" three"')
    Assert-Exit $quoted 0
    Assert-Line $quoted '[one two][three]'

    $bare = Invoke-GateRaw ('--root "' + $tree + '" --plugin "printf [%s] one two three"')
    Assert-Exit $bare 0
    Assert-Line $bare '[one][two][three]'
}

Test-Case 'plugin: several --plugin flags all run, and one failure is enough to fail the gate' {
    $tree = Use-Tree @{ 'a.cs' = "class A { }`n" }
    $result = Invoke-Gate --root $tree --plugin "$($script:DisciplineEcho) first-ran" --plugin (Get-DisciplineExit 7) --plugin "$($script:DisciplineEcho) third-ran"
    Assert-Exit $result 1
    Assert-Line $result 'exit 7'
    # The third one still ran: a gate that stops at the first failure hides the rest of the work.
    Assert-Line $result 'third-ran'
}

Test-Case 'plugin: an empty --plugin string is a violation, not a silent no-op' {
    $tree = Use-Tree @{ 'a.cs' = "class A { }`n" }
    $result = Invoke-Gate --root $tree --plugin '   '
    Assert-Exit $result 1
    Assert-Line $result 'empty command'
}
