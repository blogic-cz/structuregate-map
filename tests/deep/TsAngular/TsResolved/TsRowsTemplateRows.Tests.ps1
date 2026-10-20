<#
    FOUR TEMPLATE ROWS THAT SAID MORE, OR LESS, THAN ANGULAR (rows 70): a `*dir` wrapper is matched by its `templateAttrs` alone, as the
    compiler does (`TsTpl/TsTplMatch.mjs`); a read of an accessor pair lands on its getter, a write on its setter; a chain hanging off a
    keyed read, a call, a custom pipe or `a ?? b` resolves (`TsTpl/TsTplReads.mjs::valueOf`); and the condition of `*ngIf="x$ | async as x$"`
    reads the member, not its own alias. Its helpers are `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '../../TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

$script:TsTplFiles = @{
    'apps/shop/src/tpl.component.ts' = "import { Component, Directive, Pipe, TemplateRef } from '@angular/core';`n" +
        "export interface Item { label: string; }`n" +
        "@Directive({ selector: '[ripple]', standalone: true }) export class RippleDirective {}`n" +
        "@Directive({ selector: '[demoIf]', standalone: true }) export class DemoIfDirective { constructor(public t: TemplateRef<unknown>) {} }`n" +
        "@Component({ selector: 'app-leaf', template: '', standalone: true }) export class LeafComponent {}`n" +
        "@Pipe({ name: 'demoPipe', standalone: true }) export class DemoPipe { transform(v: string) { const made: Item = { label: v }; return made; } }`n" +
        "export interface Other { code: number; }`n" +
        "@Pipe({ name: 'demoUnion', standalone: true }) export class DemoUnionPipe { transform(v: string) { const a: Item = { label: v }; const b: Other = { code: 1 }; return v ? a : b; } }`n" +
        "@Component({ selector: 'app-wrap', standalone: true, imports: [RippleDirective, DemoIfDirective, LeafComponent], " +
        "template: '<button ripple *demoIf=`"on`">x</button><app-leaf *demoIf=`"on`"></app-leaf><ng-template [demoIf]=`"on`"></ng-template><app-ghost *ngIf=`"on`"></app-ghost>' })`n" +
        "export class WrapComponent { on = true; }`n" +
        "@Component({ selector: 'app-reads', standalone: true, imports: [DemoPipe, DemoUnionPipe], template: '" +
        "<p>{{ value }}</p><p (click)=`"value = 1`">w</p><p (click)=`"size = 2`">s</p><p>{{ size }}</p>" +
        "<p>{{ items[0].label }}</p><p>{{ load().label }}</p><p>{{ (name | demoPipe).label }}</p><p>{{ (first ?? second).label }}</p><p>{{ (name | demoUnion).code }}</p>" +
        "<p *ngIf=`"x`$ | async as x`$`">{{ x`$ }}</p>" +
        "' })`n" +
        "export class ReadsComponent {`n" +
        "  items: Item[] = []; first: Item = { label: '' }; second: Item = { label: '' }; name = '';`n" +
        "  load() { return this.first; }`n" +
        "  x`$: any;`n" +
        "  get value(): number { return 1; }`n" +
        "  set value(v: number) { void v; }`n" +
        "  set size(v: number) { void v; }`n  get size(): number { return 1; }`n" +
        "}`n"
}

# An inline template written after `template: ` on the decorator line, over two lines (rows 71: its first line's `col` is the .ts column).
$script:TsTplInlineSrc = @("@Component({ selector: 'app-inl', standalone: true, template: ``<div [title]=`"t`">a</div>", '<p>b</p>` })')
$script:TsTplFiles['apps/shop/src/inline.component.ts'] = "import { Component } from '@angular/core';`n" + ($script:TsTplInlineSrc -join "`n") +
    "`nexport class InlComponent { t = ''; }`n"

function Get-TsTplDb {
    $made = New-TsRowsDb (New-TsRowsWorkspace $script:TsTplFiles)
    Assert-Exit $made.Result 0
    return $made.Db
}

# What a template node's matched directives are, `Name@NodeKind` - the cells `template_nodes.directives` holds, WrapComponent's only.
function Get-TsTplMatches([string]$Db) {
    return Invoke-TsRowsQ $Db ("SELECT 'cell=' || d.value || '@' || n.kind || ':' || n.tag AS r FROM template_nodes__directives d " +
        "JOIN template_nodes n ON n.id = d.owner JOIN templates t ON t.id = n.template JOIN classes c ON c.id = t.class " +
        "WHERE d.role = 'name' AND c.name = 'WrapComponent' ORDER BY r")
}

Test-Case 'tsrows: a *dir wrapper matches by its templateAttrs alone, as Angular does - the element keeps its own directives' {
    $db = Get-TsTplDb
    $m = Get-TsTplMatches $db
    # `<button ripple *demoIf>`: the directive on the element once, never on the wrapper; the structural one on the wrapper.
    Assert-Line $m 'cell=RippleDirective@Element:button'
    Assert-NoLine $m 'RippleDirective@Template'
    Assert-Line $m 'cell=DemoIfDirective@Template:button'
    # A component is never on a wrapper (the element is the instance); `<ng-template [demoIf]>` still matches on its own input.
    Assert-Line $m 'cell=LeafComponent@Element:app-leaf'
    Assert-NoLine $m 'LeafComponent@Template'
    Assert-Line $m 'cell=DemoIfDirective@Template:ng-template'
    $r = Invoke-TsRowsQ $db ("SELECT 'ripple=' || count(*) AS r FROM renders r JOIN classes c ON c.id = r.from_class " +
        "WHERE c.name = 'WrapComponent' AND r.to_name = 'RippleDirective'")
    Assert-Line $r 'ripple=1'
    # An unmatched element is ONE diagnostic: its `*ngIf` wrapper carries the same tag and was noted too.
    $u = Invoke-TsRowsQ $db "SELECT 'unmatched=' || count(*) AS r FROM diagnostics WHERE kind = 'element_unmatched' AND tag = 'app-ghost'"
    Assert-Line $u 'unmatched=1'
}

# Every Read, SafeRead and Write of the expressions whose source holds `$Like`: `source:name=<member kind | 1/0 when the target is no member>`.
function Get-TsTplTargets([string]$Db, [string]$Like) {
    return Invoke-TsRowsQ $Db ("SELECT 'read=' || x.source || ':' || nm.value || '=' || (CASE WHEN rw.value IS NULL THEN 'none' ELSE ifnull(m.kind, 'type') END) AS r " +
        "FROM expressions x JOIN expressions__ast k ON k.owner = x.id AND k.role = 'k' AND k.value IN ('Read', 'SafeRead', 'Write') " +
        "JOIN expressions__ast nm ON nm.owner = x.id AND nm.parent = k.parent AND nm.role = 'name' " +
        "LEFT JOIN expressions__ast t ON t.owner = x.id AND t.parent = k.parent AND t.role = 'target' " +
        "LEFT JOIN expressions__ast rw ON rw.owner = x.id AND rw.parent = t.n AND rw.role = 'row' LEFT JOIN members m ON m.id = rw.value " +
        "WHERE x.source LIKE '$Like' ORDER BY r")
}

Test-Case 'tsrows: an accessor pair - a template read lands on the getter, whichever accessor is declared last' {
    $db = Get-TsTplDb
    Assert-Line (Get-TsTplTargets $db '{{ value }}') 'read={{ value }}:value=getter'
    Assert-Line (Get-TsTplTargets $db '{{ size }}') 'read={{ size }}:size=getter'
}

Test-Case 'tsrows: an accessor pair - a template write lands on the setter, whichever accessor is declared last' {
    $db = Get-TsTplDb
    Assert-Line (Get-TsTplTargets $db 'value = 1') 'read=value = 1:value=setter'
    # The pair declared the other way round: the setter first, the getter last.
    Assert-Line (Get-TsTplTargets $db 'size = 2') 'read=size = 2:size=setter'
}

# The control of the four cases below: `items` is a plain property read, resolved before a chain hung from a value was.
Test-Case 'tsrows: a read resolves past a keyed read - the element of what it indexes' {
    $db = Get-TsTplDb
    Assert-Line (Get-TsTplTargets $db '{{ items[0].label }}') 'read={{ items[0].label }}:items=property'
    Assert-Line (Get-TsTplTargets $db '{{ items[0].label }}') 'read={{ items[0].label }}:label=type'
}

Test-Case 'tsrows: a read resolves past a call - what the method returns' {
    $db = Get-TsTplDb
    Assert-Line (Get-TsTplTargets $db '{{ load().label }}') 'read={{ load().label }}:load=method'
    Assert-Line (Get-TsTplTargets $db '{{ load().label }}') 'read={{ load().label }}:label=type'
}

Test-Case 'tsrows: a read resolves past an in-tree pipe - what its transform returns' {
    $db = Get-TsTplDb
    Assert-Line (Get-TsTplTargets $db '{{ (name | demoPipe).label }}') ':name=property'
    Assert-Line (Get-TsTplTargets $db '{{ (name | demoPipe).label }}') ':label=type'
}

Test-Case 'tsrows: a read resolves past an in-tree pipe whose transform returns SEVERAL types - the one that declares the name' {
    $db = Get-TsTplDb
    # `transform` infers `Item | Other` (`return_refs`); `code` is declared by `Other` alone, so it lands there.
    Assert-Line (Get-TsTplTargets $db '{{ (name | demoUnion).code }}') ':name=property'
    Assert-Line (Get-TsTplTargets $db '{{ (name | demoUnion).code }}') ':code=type'
}

Test-Case 'tsrows: an inline template starting on the decorator line has the .ts column on its first line' {
    $db = Get-TsTplDb
    $first = $script:TsTplInlineSrc[0]
    $r = Invoke-TsRowsQ $db ("SELECT 'n=' || n.tag || ':' || n.line || ':' || n.col AS r FROM template_nodes n JOIN templates t ON t.id = n.template " +
        "JOIN classes c ON c.id = t.class WHERE c.name = 'InlComponent' UNION ALL SELECT 'b=' || b.name || ':' || b.line || ':' || b.col " +
        "FROM bindings b JOIN templates t ON t.id = b.template JOIN classes c ON c.id = t.class WHERE c.name = 'InlComponent'")
    Assert-Line $r ("n=div:2:" + ($first.IndexOf('<div') + 1))
    Assert-Line $r ("b=title:2:" + ($first.IndexOf('[title]') + 1))
    # The control: a later line of the template starts at column 1 of the file's line, unshifted.
    Assert-Line $r 'n=p:3:1'
}

Test-Case 'tsrows: a read resolves past a ?? whose sides name one owner' {
    $db = Get-TsTplDb
    Assert-Line (Get-TsTplTargets $db '{{ (first ?? second).label }}') ':first=property'
    Assert-Line (Get-TsTplTargets $db '{{ (first ?? second).label }}') ':label=type'
}

Test-Case 'tsrows: the condition of an as alias named like a member reads the member, not its own alias' {
    $db = Get-TsTplDb
    Assert-Line (Get-TsTplTargets $db 'x_ | async%') 'x$ | async:x$=property'
    # The control: inside the element the name IS the alias, and names no member.
    Assert-Line (Get-TsTplTargets $db '{{ x$ }}') ':x$=none'
}

}
