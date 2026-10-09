<#
    GateSettings.ps1 - every entry the wiring writes into a consumer's `.claude/settings.json`: the Stop hook that runs
    the gate each turn, and the plugin whose hooks point a session at the map. Dot-sourced by GateWiring.ps1; apart
    from it so that file stays under its own line limit.
#>
Set-StrictMode -Version 2.0

<#
    STOP HOOK. For a tree with no build step, which is where a rule is otherwise only enforced when somebody
    remembers to run it.

    THE WRAPPER EXISTS FOR ONE REASON: EXIT 2. Claude Code treats exit 2 from a Stop hook as something to
    fix and hands the output back; exit 1 only prints, and the turn ends anyway with the violation in the
    tree. The gate exits 1, correctly, because from MSBuild that is a failed build. So the translation lives
    here, next to the exe, and not in the exe.
#>
function Set-HookWiring([string]$Path, [string]$GateDir, [string]$GateArgs, [bool]$Deep) {
    $wrapper = Join-Path $GateDir 'StructureGate.Hook.ps1'
    $verb = 'present'
    if (-not (Test-Path $wrapper)) {
        [System.IO.File]::WriteAllText($wrapper, (Get-HookWrapperText $GateArgs $Deep))
        $verb = 'added'
    }
    $settingsDir = Join-Path $Path '.claude'
    if (-not (Test-Path $settingsDir)) { [void](New-Item -ItemType Directory -Path $settingsDir -Force) }
    $settingsPath = Join-Path $settingsDir 'settings.json'
    $settings = New-Object psobject
    if (Test-Path $settingsPath) { $settings = Get-Content $settingsPath -Raw | ConvertFrom-Json }
    $command = $script:GateShell + ' -NoProfile -ExecutionPolicy Bypass -File "' + $wrapper + '"'
    if ((ConvertTo-Json $settings -Depth 20).Contains('StructureGate.Hook.ps1')) { return $verb }
    $entry = [pscustomobject]@{ matcher = ''
                                hooks = @([pscustomobject]@{ type = 'command'; command = $command }) }
    $hooks = $settings.PSObject.Properties | Where-Object { $_.Name -eq 'hooks' }
    if (-not $hooks) { Add-Member -InputObject $settings -MemberType NoteProperty -Name 'hooks' -Value (New-Object psobject) }
    $stop = $settings.hooks.PSObject.Properties | Where-Object { $_.Name -eq 'Stop' }
    if ($stop) { $settings.hooks.Stop = @(@($stop.Value) + $entry) }
    else { Add-Member -InputObject $settings.hooks -MemberType NoteProperty -Name 'Stop' -Value @($entry) }
    [System.IO.File]::WriteAllText($settingsPath, (ConvertTo-Json $settings -Depth 20))
    return 'added'
}

<#
    THE PLUGIN, FOR EVERYONE WHO OPENS THE TREE. Its hooks point a session at the map before it greps for a symbol
    (`hooks/hooks.json` in the release's repo), and a plugin a project's settings declare is offered to every
    teammate who trusts the folder - nobody has to know to install it. An entry already there is left as it is.
#>
function Set-PluginWiring([string]$Path) {
    $settingsDir = Join-Path $Path '.claude'
    if (-not (Test-Path $settingsDir)) { [void](New-Item -ItemType Directory -Path $settingsDir -Force) }
    $settingsPath = Join-Path $settingsDir 'settings.json'
    $settings = New-Object psobject
    if (Test-Path $settingsPath) { $settings = Get-Content $settingsPath -Raw | ConvertFrom-Json }
    $verb = 'present'
    $wanted = @{
        extraKnownMarketplaces = @('structuregate', [pscustomobject]@{ source = [pscustomobject]@{ source = 'github'; repo = 'blogic-cz/structuregate-map' } })
        enabledPlugins         = @('structuregate@structuregate', $true)
    }
    foreach ($key in @('extraKnownMarketplaces', 'enabledPlugins')) {
        if ($null -eq $settings.PSObject.Properties[$key]) {
            Add-Member -InputObject $settings -MemberType NoteProperty -Name $key -Value (New-Object psobject)
        }
        $name, $value = $wanted[$key]
        if ($null -ne $settings.$key.PSObject.Properties[$name]) { continue }
        Add-Member -InputObject $settings.$key -MemberType NoteProperty -Name $name -Value $value
        $verb = 'added'
    }
    if ($verb -eq 'added') { [System.IO.File]::WriteAllText($settingsPath, (ConvertTo-Json $settings -Depth 20)) }
    return $verb
}
