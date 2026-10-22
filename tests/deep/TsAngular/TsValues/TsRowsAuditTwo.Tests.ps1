<#
    The second audit of the Angular half's derived tables: one workspace, one case per defect, each with the control that
    already held. Every case READS the shared map. Its helpers are `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '../../TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

function New-TsAuditTwoTree {
    $config = '{"locales":["apps/shop/src/assets/locales"],"i18nCarriers":["money"],"gateInputs":["disabled"],' +
        '"featureChecks":["FlagService.isOn"],"featureEnum":"FlagCodes","i18nCalls":["getTranslation"]}'
    New-TsRowsWorkspace @{
        'structuregate.ts.json' = $config
        'apps/shop/src/assets/locales/en.json' = '{"shop":{"title":"Shop"},"demo":{"tip":{"key":"T"},"label":{"key":"L"},"idle":{"key":"I"}}}'
        'apps/shop/src/audit.ts' = @'
import { Component, Directive, EventEmitter, Input, Output } from '@angular/core';
@Directive({ selector: '[demoMask]', standalone: true })
export class DemoMaskDirective { @Input() demoMask = ''; @Output() demoDone = new EventEmitter<string>(); }
@Directive({ selector: '[demoNote]', standalone: true })
export class DemoNoteDirective {
  @Input() demoNote = '';
  @Output() noted = new EventEmitter<string>();
  @Output() quiet = new EventEmitter<string>();
}
@Component({ selector: 'demo-card', standalone: true, imports: [DemoNoteDirective], templateUrl: './audit.html', styleUrls: ['./audit.scss'] })
export class DemoCardComponent { open = true; tone = 'a'; orphanFlag = true; }
export const RX = new RegExp(/a+/g, 'i');
export const RY = new RegExp(/b+/gi);
export const RZ = new RegExp('c+', 'm');
export class I18nService { getTranslation(key: string): string { return key; } }
export class Holder {
  constructor(private svc: I18nService) {}
  all(): string[] { return [{ Name: 'demo.a' }, { Name: 'demo.b' }, { Name: 'demo.c' }].map((t) => this.svc.getTranslation(t.Name)); }
  one(): string { return this.svc.getTranslation('demo.single'); }
}
'@
        'apps/shop/src/audit.html' = @'
<input [demoMask]="tone" (demoDone)="open = false">
<div [demoNote]="tone" (noted)="open = false"></div>
<span tooltipDemo="demo.tip.key" label="demo.label.key"></span>
<p *ngIf="orphanFlag" [ngClass]="[open ? 'btn' : 'plain', tone ? 'hide-mobile' : '']">a</p>
<p [ngClass]="'strA strB'">b</p>
<p [ngClass]="open ? 'condOn' : 'condOff'">c</p>
'@
        'apps/shop/src/orphan.component.html' = "<b *ngIf=`"orphanFlag`">x</b>`n"
        'apps/shop/src/audit.scss' = ".card { --gap: 20px !important; --tone: red; color: blue !important; }`n" +
            ".btn, .hide-mobile, .strA, .condOff { display: none; }`n.plain { color: red; }`n"
        'apps/shop/src/dup-a.ts' = "export class DemoDup {}`nexport interface DemoShape { id: number; }`nexport type DemoId = string;`nexport class DemoSolo {}`n"
        'apps/shop/src/dup-b.ts' = "export class DemoDup {}`nexport interface DemoShape { name: string; }`nexport type DemoId = number;`n"
    }
}

# A DIRECTIVE NO TEMPLATE SCOPE REACHES IS NO TARGET OF ITS BINDINGS: `DemoMaskDirective` is not imported by the card, so every candidate of
# `<input [demoMask]>` is out of scope and the binding stays unattributed. `[demoNote]`, in scope, is the control.
Test-Case 'tsrows: a binding whose every candidate is out of scope fills no input' {
    $made = New-TsRowsDb (New-TsAuditTwoTree) -Shared
    Assert-Exit $made.Result 0
    $r = Invoke-TsRowsQ $made.Db "SELECT 'iu=' || input || '|' || count(*) AS r FROM input_usage WHERE input IN ('demoMask', 'demoNote') GROUP BY input"
    Assert-Line $r 'iu=demoNote|1'
    Assert-NoLine $r 'iu=demoMask'
    $c = Invoke-TsRowsQ $made.Db "SELECT 'bc=' || member || '|' || bound_count AS r FROM io WHERE member = 'demoMask'"
    Assert-Line $c 'bc=demoMask|0'
}

# `io.bound_count` OF AN OUTPUT counts the `(event)` bindings that name it, as an input's counts its bindings. `noted` is bound once, `quiet`
# never; `demoDone` sits on a directive no scope reaches, so it is bound by nobody.
Test-Case 'tsrows: an output counts the event bindings that name it' {
    $made = New-TsRowsDb (New-TsAuditTwoTree) -Shared
    $r = Invoke-TsRowsQ $made.Db "SELECT 'out=' || member || '|' || bound_count AS r FROM io WHERE kind = 'output' AND class IN (SELECT id FROM classes WHERE name LIKE 'Demo%')"
    Assert-Line $r 'out=noted|1'
    Assert-Line $r 'out=quiet|0'
    Assert-Line $r 'out=demoDone|0'
}

# THE ORPHAN TEMPLATE IS NO COMPONENT: `orphanFlag` is read by the card's template and by a template no component names, and one
# component reads it.
Test-Case 'tsrows: gate_index counts no component for an orphan template' {
    $made = New-TsRowsDb (New-TsAuditTwoTree) -Shared
    $r = Invoke-TsRowsQ $made.Db "SELECT 'gi=' || identifier || '|' || gate_count || '|' || component_count AS r FROM gate_index WHERE identifier = 'orphanFlag'"
    Assert-Line $r 'gi=orphanFlag|2|1'
}

# A STATIC ATTRIBUTE'S TEXT IS A LITERAL a key can be spelled in: `tooltipDemo="demo.tip.key"` and `label="demo.label.key"` reference their
# keys; `demo.idle.key` is spelled nowhere (the control).
Test-Case 'tsrows: a translation key spelled in a static attribute is referenced' {
    $made = New-TsRowsDb (New-TsAuditTwoTree) -Shared
    $r = Invoke-TsRowsQ $made.Db "SELECT 'tr=' || key || '|' || referenced AS r FROM translations WHERE key LIKE 'demo.%'"
    Assert-Line $r 'tr=demo.tip.key|1'
    Assert-Line $r 'tr=demo.label.key|1'
    Assert-Line $r 'tr=demo.idle.key|0'
}

# A CUSTOM PROPERTY IS IMPORTANT WHEN IT SAYS SO, and its value is the value alone; `--tone` and `color` are the controls.
Test-Case 'tsrows: a custom property marked important is important' {
    $made = New-TsRowsDb (New-TsAuditTwoTree) -Shared
    $r = Invoke-TsRowsQ $made.Db "SELECT 'sr=' || property || '|' || value || '|' || important AS r FROM style_rules WHERE selector = '.card'"
    Assert-Line $r 'sr=--gap|20px|1'
    Assert-Line $r 'sr=--tone|red|0'
    Assert-Line $r 'sr=color|blue|1'
}

# `new RegExp(/lit/, 'flags')` IS A CALL ROW TOO, its flags the argument's (JavaScript overrides the literal's), the literal keeps its own row; without
# a flags argument the literal's are copied. `new RegExp('c+', 'm')` is the control.
Test-Case 'tsrows: new RegExp over a literal is a call row with the flags argument' {
    $made = New-TsRowsDb (New-TsAuditTwoTree) -Shared
    $r = Invoke-TsRowsQ $made.Db "SELECT 'rx=' || kind || '|' || api || '|' || pattern || '|' || flags AS r FROM regexes WHERE pattern IN ('a+', 'b+', 'c+')"
    Assert-Line $r 'rx=call|RegExp|a+|i'
    Assert-Line $r 'rx=literal||a+|g'
    Assert-Line $r 'rx=call|RegExp|b+|gi'
    Assert-Line $r 'rx=call|RegExp|c+|m'
}

# EVERY KEY A SERVICE CALL CAN READ IS A REF: a parameter over a literal array of objects reads each element's property. `demo.single` is the control.
Test-Case 'tsrows: a service call over the elements of a literal array yields a ref per element' {
    $made = New-TsRowsDb (New-TsAuditTwoTree) -Shared
    $r = Invoke-TsRowsQ $made.Db "SELECT 'ref=' || key || '|' || kind AS r FROM i18n_refs WHERE key LIKE 'demo.%'"
    foreach ($key in 'demo.a', 'demo.b', 'demo.c', 'demo.single') { Assert-Line $r "ref=$key|service_call" }
}

# `[ngClass]` NAMES EVERY CLASS ITS FORM CAN PRODUCE: the arms of a ternary in an array, a string, a ternary. `plain` does not hide (the control).
Test-Case 'tsrows: an ngClass array, string and ternary hide through every class they name' {
    $made = New-TsRowsDb (New-TsAuditTwoTree) -Shared
    $r = Invoke-TsRowsQ $made.Db "SELECT DISTINCT 'ch=' || h.via || '|' || h.class AS r FROM class_hides h"
    foreach ($class in 'btn', 'hide-mobile', 'strA', 'condOff') { Assert-Line $r "ch=ngClass|$class" }
    Assert-NoLine $r 'plain'
}

# A TYPE NAME DECLARED TWICE IS ONE ROW WHATEVER IT IS: class, interface and alias, not only enums. `DemoSolo` is declared once.
Test-Case 'tsrows: duplicate_types lists a class, an interface and an alias' {
    $made = New-TsRowsDb (New-TsAuditTwoTree) -Shared
    $r = Invoke-TsRowsQ $made.Db "SELECT 'dt=' || name || '|' || kinds || '|' || places AS r FROM duplicate_types"
    Assert-Line $r 'dt=DemoDup|class|2'
    Assert-Line $r 'dt=DemoShape|interface|2'
    Assert-Line $r 'dt=DemoId|alias|2'
    Assert-NoLine $r 'DemoSolo'
}

# AN IMPORT ONLY CASE-INSENSITIVE RESOLUTION FINDS says so: `./Cart` beside `cart.ts` resolves on Windows and macOS, not where the build ships.
Test-Case 'tsrows: an unresolved import notes when only the casing differs' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/casing.ts' = "import { Cart } from './Cart';`nimport { Gone } from './gone';`nexport const c = Cart;`nexport const g = Gone;`n"
    }
    $made = New-TsRowsDb $tree -Shared
    Assert-Exit $made.Result 0
    $r = Invoke-TsRowsQ $made.Db ("SELECT 'ui=' || module || '|' || case_insensitive_match AS r FROM diagnostics " +
        "WHERE kind = 'unresolved_import' AND module IN ('./Cart', './gone')")
    Assert-Line $r 'ui=./Cart|1'
    Assert-Line $r 'ui=./gone|0'
}

}
