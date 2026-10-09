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

Test-Case 'claudehook: a mapped tree gets a session note, and a symbol search a nudge that decides nothing' {
    $tree = Use-Tree @{ 'buildmap.sqlite' = 'x'; 'app.py' = "X = 1`n" }
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
    $tree = Use-Tree @{ 'buildmap.sqlite' = 'x' }
    $denied = Invoke-ClaudeHookGate 'pre-tool-use' $tree @{ tool_name = 'Grep'; tool_input = @{ pattern = 'OrderService' } } 'deny'
    Assert-Equal $denied.Json.hookSpecificOutput.permissionDecision 'deny' 'STRUCTUREGATE_HOOK=deny'

    $prose = Invoke-ClaudeHookGate 'pre-tool-use' $tree @{ tool_name = 'Grep'; tool_input = @{ pattern = 'connection refused' } }
    Assert-Equal $prose.Text '' 'prose is grep''s'

    $bare = Use-Tree @{ 'app.py' = "X = 1`n" }
    $none = Invoke-ClaudeHookGate 'pre-tool-use' $bare @{ tool_name = 'Grep'; tool_input = @{ pattern = 'OrderService' } }
    Assert-Equal $none.Exit 0 'exit'
    Assert-Equal $none.Text '' 'no map, nothing to point at'
}
