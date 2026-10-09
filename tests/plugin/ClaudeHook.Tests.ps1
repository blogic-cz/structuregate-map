<#
    `--claude-hook`: what the plugin's hooks (`hooks/hooks.json`) run. Its stdout is JSON Claude Code parses, so the
    cases read it back as JSON: a session-start note in a mapped tree, a NUDGE on a symbol search that never carries
    a permission decision, a refusal only when STRUCTUREGATE_HOOK=deny, and silence where there is no map or no symbol.
#>

# The hook with its stdin and its project, the way Claude Code runs it. Named for this suite alone - every suite is
# dot-sourced into one scope.
function Invoke-ClaudeHookGate([string]$Event, [string]$Project, [hashtable]$Stdin, [string]$Mode = '') {
    $gate = Get-GateInvocation
    $previous = @($env:CLAUDE_PROJECT_DIR, $env:STRUCTUREGATE_HOOK)
    $env:CLAUDE_PROJECT_DIR = $Project
    $env:STRUCTUREGATE_HOOK = $Mode
    try {
        $Stdin['session_id'] = [Guid]::NewGuid().ToString('N')
        $text = ($Stdin | ConvertTo-Json -Compress -Depth 5) | & $gate.File @($gate.Lead) --claude-hook $Event 2>$null
        $exit = $LASTEXITCODE
    } finally {
        $env:CLAUDE_PROJECT_DIR, $env:STRUCTUREGATE_HOOK = $previous
    }
    $joined = (@($text) -join "`n").Trim()
    return [pscustomobject]@{ Exit = $exit; Text = $joined; Json = if ($joined) { $joined | ConvertFrom-Json } else { $null } }
}

# A REAL deep map at `$Rel` under the tree: the hook knows a map by its `_meta`, not by a file name.
function New-ClaudeHookMap([string]$Tree, [string]$Rel) {
    $db = Join-Path $Tree $Rel
    [void](New-Item -ItemType Directory -Force -Path (Split-Path $db -Parent))
    Assert-Exit (Invoke-Gate --root $Tree --ext .py --map-sqlite $db) 0
}

Test-Case 'claudehook: a mapped tree gets a session note, and a symbol search a nudge that decides nothing' {
    $tree = Use-Tree @{ 'app.py' = "X = 1`n" }
    New-ClaudeHookMap $tree 'buildmap.sqlite'
    $start = Invoke-ClaudeHookGate 'session-start' $tree @{ hook_event_name = 'SessionStart' }
    Assert-Equal $start.Exit 0 'exit'
    Assert-Equal $start.Json.hookSpecificOutput.hookEventName 'SessionStart' 'event'
    if ($start.Json.hookSpecificOutput.additionalContext -notlike '*--map-query buildmap.sqlite --find*') { throw "no query named: $($start.Text)" }

    $grep = Invoke-ClaudeHookGate 'pre-tool-use' $tree @{ tool_name = 'Grep'; tool_input = @{ pattern = 'OrderService' } }
    $said = $grep.Json.hookSpecificOutput
    if ($said.additionalContext -notlike '*--find OrderService*') { throw "no nudge: $($grep.Text)" }
    if ($said.PSObject.Properties.Name -contains 'permissionDecision') { throw "a nudge must not decide: $($grep.Text)" }

    $shell = Invoke-ClaudeHookGate 'pre-tool-use' $tree @{ tool_name = 'Bash'; tool_input = @{ command = 'rg -n get_user src' } }
    if ($shell.Json.hookSpecificOutput.additionalContext -notlike '*--find get_user*') { throw "no nudge for rg: $($shell.Text)" }
}

Test-Case 'claudehook: deny only when asked, and silence with no map or no symbol' {
    $tree = Use-Tree @{ 'app.py' = "X = 1`n" }
    New-ClaudeHookMap $tree 'buildmap.sqlite'
    $denied = Invoke-ClaudeHookGate 'pre-tool-use' $tree @{ tool_name = 'Grep'; tool_input = @{ pattern = 'OrderService' } } 'deny'
    Assert-Equal $denied.Json.hookSpecificOutput.permissionDecision 'deny' 'STRUCTUREGATE_HOOK=deny'

    $prose = Invoke-ClaudeHookGate 'pre-tool-use' $tree @{ tool_name = 'Grep'; tool_input = @{ pattern = 'connection refused' } }
    Assert-Equal $prose.Text '' 'prose is grep''s'

    # ...and a .sqlite that is not a map is no map.
    $bare = Use-Tree @{ 'app.py' = "X = 1`n"; 'cache.sqlite' = 'not a map' }
    $none = Invoke-ClaudeHookGate 'pre-tool-use' $bare @{ tool_name = 'Grep'; tool_input = @{ pattern = 'OrderService' } }
    Assert-Equal $none.Exit 0 'exit'
    Assert-Equal $none.Text '' 'no map, nothing to point at'
}

# A MAP IS FOUND WHEREVER THE WIRING WROTE IT, and every map is named: a tree that runs `--map-sqlite data/build/...`
# and keeps a second map elsewhere was silent, because only `buildmap.sqlite` was looked for.
Test-Case 'claudehook: maps under any name and folder are found, the nearest named first' {
    $tree = Use-Tree @{ 'app.py' = "X = 1`n" }
    New-ClaudeHookMap $tree 'data/build/demo.sqlite'
    New-ClaudeHookMap $tree 'web/.map/front.sqlite'
    $start = Invoke-ClaudeHookGate 'session-start' $tree @{ hook_event_name = 'SessionStart' }
    $note = $start.Json.hookSpecificOutput.additionalContext
    if (-not $note.Contains('data/build/demo.sqlite') -or -not $note.Contains('web/.map/front.sqlite')) { throw "not both maps: $note" }
    $grep = Invoke-ClaudeHookGate 'pre-tool-use' $tree @{ tool_name = 'Grep'; tool_input = @{ pattern = 'computeTotals' } }
    if ($grep.Json.hookSpecificOutput.additionalContext -notlike '*--map-query data/build/demo.sqlite --find computeTotals*') { throw "no query on the found map: $($grep.Text)" }
}
