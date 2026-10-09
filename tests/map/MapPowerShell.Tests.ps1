<#
    What the PowerShell file map concludes about a script tree: which file a function name binds to, and which
    files nothing in the tree reaches. Its own suite because `Map` sits at its size limit; the shared helpers are
    `Map.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot 'Map.Helpers.ps1')

# A FUNCTION THE FILE CALLS ITSELF IS NOT UNREAD. A script run by path from outside the tree uses its own helpers,
# and listing them under NO READER read as dead code that was not. Only what nothing calls is listed; a file
# whose every function it calls itself is a file nothing in the tree RUNS.
Test-Case 'mapps: NO READER lists only the functions nothing calls, the file itself included' {
    $tree = Use-Tree @{
        'tools/check-demo.ps1' = "function Get-DemoText([string]`$p) { [System.IO.File]::ReadAllText(`$p) }`n" +
            "`$a = Get-DemoText 'a.txt'`n`$b = Get-DemoText 'b.txt'`n"
        'idle.ps1' = "function Get-Used { 1 }`nfunction Get-Idle { 2 }`nGet-Used`n"
    }
    $found = Get-Map --root $tree --ext .ps1 --map-check
    Assert-Exit $found 0
    Assert-Line $found 'NO READER tools/check-demo.ps1: nothing in the mapped tree runs or imports this file'
    Assert-NoLine $found 'declares Get-DemoText'
    Assert-Line $found 'NO READER idle.ps1: declares Get-Idle and nothing'
    Assert-NoLine $found 'Get-Used'
}

# A FUNCTION DECLARED INSIDE A SCRIPTBLOCK LITERAL is that block's. A runspace or job body must redeclare its
# helpers - it cannot see the caller's - so two bodies each declaring `Send` were AMBIGUOUS, a DUPLICATE, and
# their calls bound to nothing. The call inside binds to the block's own `Send`; the same function declared at file
# level in two files still is both.
Test-Case 'mapps: a function declared inside a scriptblock literal is local to it' {
    $body = "{ `$m = @{ type = `$t; text = `"`$x`" }; `$q.Enqueue(`$m); `$n++ }"
    $tree = Use-Tree @{
        'alpha.ps1' = "`$script:AlphaBody = {`n    function Send(`$t, `$x) $body`n    Send 'log' 'alpha'`n}`n"
        'beta.ps1'  = "`$script:BetaBody = {`n    function Send(`$t, `$x) $body`n    Send 'log' 'beta'`n}`n"
        'gamma.ps1' = "function Share(`$t, `$x) $body`n"
        'delta.ps1' = "function Share(`$t, `$x) $body`n"
        'main.ps1'  = "Share 'log' 'main'`n"
        'omega.ps1' = "function Send { 'elsewhere' }`n"
    }
    $found = Get-Map --root $tree --ext .ps1 --map-check
    Assert-Exit $found 0
    Assert-NoLine $found 'AMBIGUOUS Send'
    Assert-NoLine $found 'alpha.ps1:2'
    Assert-Line $found 'AMBIGUOUS Share'
    Assert-Line $found 'DUPLICATE 2 function bodies'
    Assert-Equal @($found.Map.files.'alpha.ps1'.declares) @('alpha.ps1') 'the block''s Send is not the file''s'
    # ...and the call inside the block binds to the block, never to a file-level Send declared elsewhere.
    Assert-Equal (Get-Imports $found.Map 'alpha.ps1').Count 0 'alpha.ps1 imports nothing'
}
