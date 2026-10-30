<#
    A `let` local a method returns reads EVERY `=` written to it, not only its initializer
    (`rust/fbtcore/src/rows/ts/key/fields/key_follow.rs`, `Returned::enter`): the key a later write hands the
    local reaches the caller, whether the method returns the local itself or an arrow reading it.
    Its helpers are `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '..\TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

Test-Case 'tsrows keys: a let local that is written after its initializer hands those keys to the caller' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/ids.ts' = "export enum Kind { Alpha = 1, Beta = 2 }`n"
        'apps/shop/src/translate.service.ts' = "import { Injectable } from '@angular/core';`n" +
            "@Injectable({ providedIn: 'root' })`n" +
            "export class TranslateService {`n  getTranslation(key: string): string {`n    return key;`n  }`n}`n"
        'apps/shop/src/desc.service.ts' = "import { Injectable } from '@angular/core';`n" +
            "import { Kind } from './ids';`n" +
            "import { TranslateService } from './translate.service';`n" +
            "@Injectable({ providedIn: 'root' })`n" +
            "export class DescService {`n" +
            "  constructor(private translate: TranslateService) {}`n" +
            "  branchLet(kind: Kind, flag: boolean): string {`n    let second = '';`n" +
            "    if (flag) { second = this.translate.getTranslation(``demo.branch.`${Kind[kind]}.extra``); }`n" +
            "    else { second = this.translate.getTranslation(``demo.branch2.`${Kind[kind]}``); }`n" +
            "    return this.pick(() => second);`n  }`n" +
            "  private pick(get: () => string): string { return get(); }`n" +
            "  lateLet(kind: Kind): string {`n    let s = '';`n" +
            "    s = this.translate.getTranslation(``demo.late.`${Kind[kind]}``);`n    return s;`n  }`n" +
            "  unusedLet(kind: Kind): string {`n    let u = '';`n" +
            "    u = this.translate.getTranslation(``demo.unusedlet.`${Kind[kind]}``);`n    return '-';`n  }`n" +
            "}`n"
        'apps/shop/src/host.component.ts' = "import { Component, Input, OnInit } from '@angular/core';`n" +
            "import { Kind } from './ids';`n" +
            "import { DescService } from './desc.service';`n" +
            "@Component({ selector: 'demo-host', standalone: true, template: '{{ a }} {{ b }} {{ c }}' })`n" +
            "export class HostComponent implements OnInit {`n" +
            "  @Input() kind: Kind = Kind.Alpha;`n" +
            "  a!: string;`n  b!: string;`n  c!: string;`n" +
            "  constructor(private desc: DescService) {}`n" +
            "  ngOnInit() {`n    this.a = this.desc.branchLet(this.kind, true);`n" +
            "    this.b = this.desc.lateLet(this.kind);`n    this.c = this.desc.unusedLet(this.kind);`n  }`n" +
            "}`n"
        'apps/shop/src/assets/locales/en.json' = '{"shop":{"title":"Shop"},"demo":{' +
            '"branch":{"Alpha":{"extra":"x"},"Beta":{"extra":"y"}},"branch2":{"Alpha":"p","Beta":"q"},' +
            '"late":{"Alpha":"m","Beta":"n"},"unusedlet":{"Alpha":"u","Beta":"v"}}}'
        'structuregate.ts.json' = '{"locales":["apps/shop/src/assets/locales"],"i18nCalls":["getTranslation"]}'
    }
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    $r = Invoke-TsRowsQ $made.Db ("SELECT k.key || ' | ' || k.route || ' | ' || c.name AS reach FROM key_reach k " +
        "JOIN key_reach__components j ON j.owner = '#' || k.rowid AND j.parent = 0 JOIN components c ON c.id = j.value WHERE k.key LIKE 'demo.%'")
    Assert-Line $r 'demo.branch.Alpha.extra | field_binding | HostComponent'
    Assert-Line $r 'demo.branch2.Alpha | field_binding | HostComponent'
    Assert-Line $r 'demo.branch2.Beta | field_binding | HostComponent'
    Assert-Line $r 'demo.late.Alpha | field_binding | HostComponent'
    # written to a local the method never returns: not reached
    Assert-NoLine $r 'demo.unusedlet.Alpha | field_binding | HostComponent'
    Assert-NoLine $r 'demo.unusedlet.Beta | field_binding | HostComponent'
}

# (divergence #109) A CLOSURE TABLE'S NODES HAVE NO SHAPE INDEX: every reader starts from the key or a value, and the one index cost
# a tenth of a full map. `--map-index` gives one table its index, kept on every later write - a full re-read included.
Test-Case 'tsrows keys: a closure node table has no shape index until --map-index asks for one, and keeps it' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/translate.component.ts' = "import { Component, Input } from '@angular/core';`n" +
            "@Component({ selector: 'translate', standalone: true, template: '{{ text }}' })`n" +
            "export class TranslateComponent { @Input() text: string = ''; }`n"
        'apps/shop/src/root.component.ts' = "import { Component } from '@angular/core';`nimport { TranslateComponent } from './translate.component';`n" +
            "@Component({ selector: 'demo-root', standalone: true, imports: [TranslateComponent], template: '<translate [text]=`"\'demo.root.title\'`"></translate>' })`n" +
            "export class RootComponent {}`n"
        'apps/shop/src/assets/locales/en.json' = '{"demo":{"root":{"title":"T"}}}'
        'structuregate.ts.json' = '{"locales":["apps/shop/src/assets/locales"],"i18nCarriers":["translate:text"]}'
    }
    $made = New-TsRowsDb $tree
    Assert-Exit $made.Result 0
    $indexQ = "SELECT 'ix=' || name AS ix FROM sqlite_master WHERE type = 'index' AND name IN ('ix_render_path__hops_role', 'ix_render_path__hops_value', 'ix_expressions__ast_role')"
    $ix = Invoke-TsRowsQ $made.Db $indexQ
    Assert-NoLine $ix 'ix=ix_render_path__hops_role'
    Assert-Line $ix 'ix=ix_render_path__hops_value'
    Assert-Line $ix 'ix=ix_expressions__ast_role'
    # A node with no value is not in the value index (divergence #118), and a value compared still reads through it.
    $partial = Invoke-TsRowsQ $made.Db "SELECT 'partial=' || (sql LIKE '%WHERE value IS NOT NULL') AS p FROM sqlite_master WHERE name = 'ix_expressions__ast_value'"
    Assert-Line $partial 'partial=1'
    $plan = Invoke-TsRowsQ $made.Db "EXPLAIN QUERY PLAN SELECT owner FROM expressions__ast WHERE value = 'demo.root.title'"
    if (-not @($plan.Lines | Where-Object { $_.Contains('ix_expressions__ast_value') }).Count) { throw "a compared value does not read the value index: $($plan.Lines -join ' | ')" }
    $asked = Invoke-Gate --map-index $made.Db render_path__hops
    Assert-Exit $asked 0
    Assert-Line (Invoke-TsRowsQ $made.Db $indexQ) 'ix=ix_render_path__hops_role'
    # A FULL RE-READ rewrites the closure tables, and makes the asked-for index again.
    $again = Invoke-Gate --root $tree --map-sqlite $made.Db --ts-node-modules $script:TsRowsModules --ts-config (Join-Path $tree 'structuregate.ts.json') --map-reread typescript
    Assert-Exit $again 0
    Assert-Line (Invoke-TsRowsQ $made.Db $indexQ) 'ix=ix_render_path__hops_role'
    Assert-Line (Invoke-Gate --map-index $made.Db expressions__ast) 'already has its shape index'
    Assert-Exit (Invoke-Gate --map-index $made.Db no__such) 2
}

}
