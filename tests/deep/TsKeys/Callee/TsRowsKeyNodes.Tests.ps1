<#
    WHERE A KEY RENDERS (#122): `key_reach.nodes` names the template nodes a key is placed at, whatever route took it there
    (`rust/fbtcore/src/rows/ts/key/keyreach.rs`) - and a field assigned what a method returns holds the key EVERY return of it
    hands back, a `switch` of literal returns included (`key/fields/key_follow.rs::enter_args`), so a key written in TypeScript and
    shown through `{{ field | translate }}` names that interpolation's node. Its helpers are `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '../../TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

Test-Case 'tsrows keys: a key a member carries to a template names the node it renders at (#122)' {
    $made = New-TsRowsDb (New-TsRowsWorkspace @{
        'apps/shop/src/translate.pipe.ts' = "import { Pipe, PipeTransform } from '@angular/core';`n" +
            "@Pipe({ name: 'translate', standalone: true })`nexport class TranslatePipe implements PipeTransform { transform(k: string) { return k; } }`n"
        'apps/shop/src/status.component.ts' = "import { Component, OnInit } from '@angular/core';`nimport { TranslatePipe } from './translate.pipe';`n" +
            "export enum Demo { Alpha = 1, Beta = 2 }`n" +
            "@Component({ selector: 'app-status', standalone: true, imports: [TranslatePipe],`n" +
            "  template: `"<h1>{{ 'demo.title' | translate }}</h1><div class='status'>{{ statusKey | translate }}</div><p>{{ unused }}</p>`" })`n" +
            "export class StatusComponent implements OnInit {`n  state: Demo = Demo.Alpha;`n  statusKey!: string;`n  unused = 'x';`n" +
            "  ngOnInit() { this.statusKey = this.keyOf(); this.log(this.quiet()); }`n" +
            "  private log(k: string) { return k; }`n" +
            "  private quiet(): string { switch (this.state) { case Demo.Alpha: return 'demo.quiet.alpha'; default: return 'demo.quiet.other'; } }`n" +
            "  private keyOf = (): string => {`n    switch (this.state) {`n" +
            "      case Demo.Alpha: return 'demo.status.alpha';`n      case Demo.Beta: return 'demo.status.beta';`n    }`n    return 'demo.status.alpha';`n  };`n}`n"
        'apps/shop/src/assets/locales/en.json' = '{"shop":{"title":"Shop"},"demo":{"title":"T","status":{"alpha":"A","beta":"B"},"quiet":{"alpha":"QA","other":"QO"}}}'
    }) -Shared
    Assert-Exit $made.Result 0
    $r = Invoke-TsRowsQ $made.Db ("SELECT 'at=' || k.key || '|' || k.route || '|' || coalesce(t.tag, '-') || '|' || coalesce(t.kind, '-') AS r FROM key_reach k " +
        "JOIN key_reach__nodes n ON n.owner = '#' || k.rowid AND n.parent IS NOT NULL LEFT JOIN template_nodes t ON t.id = n.value " +
        "LEFT JOIN template_nodes p ON p.id = t.parent")
    $nodes = @($r.Lines | Where-Object { $_.Trim().StartsWith('at=') } | ForEach-Object { $_.Trim() })
    # EVERY CASE'S KEY, at the interpolation that shows the field - and the key written in the template, at its own node.
    $alpha = @($nodes | Where-Object { $_.StartsWith('at=demo.status.alpha|field_binding|') })
    $beta = @($nodes | Where-Object { $_.StartsWith('at=demo.status.beta|field_binding|') })
    Assert-Equal $alpha.Count 1 ($nodes -join "`n")
    Assert-Equal $beta.Count 1 ($nodes -join "`n")
    Assert-Equal (@($nodes | Where-Object { $_.StartsWith('at=demo.title|template_string|') }).Count) 1 ($nodes -join "`n")
    # ...and not the title's node: the two keys are placed apart.
    $at = Invoke-TsRowsQ $made.Db ("SELECT 'same=' || count(*) AS n FROM key_reach__nodes a JOIN key_reach ka ON a.owner = '#' || ka.rowid " +
        "JOIN key_reach__nodes b ON b.value = a.value JOIN key_reach kb ON b.owner = '#' || kb.rowid " +
        "WHERE ka.key = 'demo.status.alpha' AND kb.key = 'demo.title' AND a.parent IS NOT NULL AND b.parent IS NOT NULL")
    Assert-Line $at 'same=0'
    # THE CONTROL: a method whose keys are only handed to another call renders nowhere - no node.
    $q = Invoke-TsRowsQ $made.Db ("SELECT 'quiet=' || count(n.value) AS n FROM key_reach k LEFT JOIN key_reach__nodes n ON n.owner = '#' || k.rowid " +
        "AND n.parent IS NOT NULL WHERE k.key LIKE 'demo.quiet.%'")
    Assert-Line $q 'quiet=0'
}

}
