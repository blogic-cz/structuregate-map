<#
    A partial run writes the map a FULL run of the same edited tree writes: every table, row for row, every cell - ids
    included - and the same `_meta` `spec:typescript`. Each case maps a workspace, appends a comment to one file, runs
    the partial run, moves its map aside and maps the SAME tree again from nothing (a row holds absolute paths, so a copy
    elsewhere would be another tree). The fixture holds what each gap needed to show: two projects (`projects` was minted
    again by every partial run, and `files.project` with it), a constructor's injected parameter (`di` had no claim key
    for one), a base class beside a file read again that has none (a NULL `classes.extends` column the full map never
    has), a template's render edges (`render_graph` read `renders` carried-then-made), a lazy route in a module the run
    keeps (its `load_children` copied the `$module.id` the kept row holds), an `.html` no component names (its
    `files.chars` filled), and a translation key a service call hands over in a tree whose templates hold none
    (`i18n_refs` was a rollup's table, so every other file's refs were deleted). Its helpers are `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '../../TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

function New-TsExactWorkspace {
    $files = @{
        'angular.json' = '{"projects":{"shop":{"projectType":"application","root":"apps/shop","sourceRoot":"apps/shop/src",' +
            '"architect":{"build":{"options":{"tsConfig":"apps/shop/tsconfig.app.json"}}}},' +
            '"ui":{"projectType":"library","root":"libs/ui","sourceRoot":"libs/ui/src",' +
            '"architect":{"build":{"options":{"tsConfig":"libs/ui/tsconfig.lib.json"}}}}}}'
        'apps/shop/tsconfig.app.json' = '{"compilerOptions":{"strict":false},"include":["src/**/*.ts"]}'
        'libs/ui/tsconfig.lib.json' = '{"compilerOptions":{"strict":false},"include":["src/**/*.ts"]}'
        'structuregate.ts.json' = '{"i18nCalls":["t"],"locales":["apps/shop/src/assets/locales"]}'
        'apps/shop/src/assets/locales/en.json' = '{"demo":{"alpha":"A","order":"O"}}'
        'libs/ui/src/label.service.ts' = @'
import { Injectable } from '@angular/core';
@Injectable({ providedIn: 'root' })
export class LabelService {
  t(key: string): string { return key; }
}
'@
        # A LIBRARY FILE THE APPLICATION NEVER REACHES, with an import nothing resolves: the `ui` program is the one that notes it, so a run
        # that leaves that program unbuilt has to say the note again (rows 125).
        'libs/ui/src/orphan.ts' = "import { gone } from 'demo-missing-package';`nexport const orphan = gone;`n"
        'libs/ui/src/base.ts' = @'
export class AlphaBase {
  protected id = 1;
  close(): void {}
}
'@
        # A CALL INTO A FILE THE CALLER NEVER IMPORTS: `alpha` calls `close()` on a `Closer`, which inherits it from `AlphaBase` in `base.ts` -
        # the edge `base.ts` -> `alpha` is the call's alone, and a partial run that kept `base.ts` lost it from `ts_deps`.
        'libs/ui/src/closer.ts' = "import { AlphaBase } from './base';`nexport class Closer extends AlphaBase {}`n"
        'apps/shop/src/alpha.component.ts' = @'
import { Component } from '@angular/core';
import { LabelService } from '../../../libs/ui/src/label.service';
import { Closer } from '../../../libs/ui/src/closer';
@Component({ selector: 'demo-alpha', templateUrl: './alpha.component.html' })
export class AlphaComponent {
  open = true;
  title = '';
  constructor(private readonly labels: LabelService) {
    this.title = labels.t('demo.alpha');
    new Closer().close();
  }
}
'@
        'apps/shop/src/alpha.component.html' = @'
<p *ngIf="open">{{ title }}</p>
<demo-beta *ngIf="open"></demo-beta>
'@
        'apps/shop/src/order.component.ts' = @'
import { Component } from '@angular/core';
import { AlphaBase } from '../../../libs/ui/src/base';
import { LabelService } from '../../../libs/ui/src/label.service';
@Component({ selector: 'demo-order', templateUrl: './order.component.html' })
export class OrderComponent extends AlphaBase {
  ready = true;
  constructor(private readonly labels: LabelService) {
    super();
  }
  name(): string {
    return this.labels.t('demo.order');
  }
}
'@
        'apps/shop/src/order.component.html' = @'
<demo-alpha *ngIf="ready"></demo-alpha>
<demo-beta></demo-beta>
'@
        'apps/shop/src/beta.component.ts' = @'
import { Component } from '@angular/core';
@Component({ selector: 'demo-beta', templateUrl: './beta.component.html' })
export class BetaComponent {
  shown = true;
}
'@
        'apps/shop/src/beta.component.html' = @'
<span *ngIf="shown">beta</span>
'@
        'apps/shop/src/routing.module.ts' = @'
import { NgModule } from '@angular/core';
import { RouterModule } from '@angular/router';
@NgModule({
  imports: [RouterModule.forRoot([{ path: 'lazy', loadChildren: () => import('./lazy.module').then((m) => m.LazyModule) }])],
  exports: [RouterModule],
})
export class RoutingModule {}
'@
        'apps/shop/src/lazy.module.ts' = @'
import { NgModule } from '@angular/core';
@NgModule({})
export class LazyModule {}
'@
        'apps/shop/src/app.module.ts' = @'
import { NgModule } from '@angular/core';
import { AlphaComponent } from './alpha.component';
import { BetaComponent } from './beta.component';
import { OrderComponent } from './order.component';
import { RulesComponent } from './rules.component';
import { HostComponent } from './host.component';
import { RoutingModule } from './routing.module';
@NgModule({ declarations: [AlphaComponent, BetaComponent, OrderComponent, RulesComponent, HostComponent], imports: [RoutingModule], bootstrap: [OrderComponent, HostComponent] })
export class AppModule {}
'@
        # A FUNCTION A TEMPLATE TYPES (rows 107, `TsDerive/TsInputs/TsTplFills.mjs`): `rules()` declares no return type, so its arrow's
        # `target` is found through `[rules]="rules()"` and recomputed on every run - a kept row whose path it spelt absolute read as moved.
        'apps/shop/src/rule.ts' = "export interface Rule { Message: (value: string) => string; }`n"
        'apps/shop/src/rules.component.ts' = @'
import { Component, Input } from '@angular/core';
import { Rule } from './rule';
@Component({ selector: 'demo-rules', template: '<p>{{ rules.length }}</p>' })
export class RulesComponent {
  @Input() rules: Rule[] = [];
}
'@
        'apps/shop/src/host.component.ts' = @'
import { Component } from '@angular/core';
@Component({ selector: 'demo-host', template: '<demo-rules [rules]="rules()"></demo-rules>' })
export class HostComponent {
  rules() { return [{ Message: (value: string) => value }]; }
}
'@
        'apps/shop/src/main.ts' = @'
import { AppModule } from './app.module';
export const boot = AppModule;
'@
        'apps/shop/src/unused.html' = @'
<div>unused</div>
'@
    }
    return Use-Tree $files
}

# EVERY ROW OF EVERY TABLE (but `_meta`, `_cells` & co, SQLite's own and the full-text index of the files), each cell
# `quote`d - ids and all - one line `table=cell,cell,...`, sorted. A line break inside a cell is spelled `\n`, so a row is
# one line. Asked a few tables at a time: one query naming every column of every table is longer than a command line
# may be on Windows.
function Get-TsExactRows([string]$Db) {
    $cols = Invoke-Gate --map-query $Db --width 0 --limit 0 --sql ("SELECT 'c=' || m.name || '|' || p.name AS c " +
        "FROM sqlite_master m, pragma_table_info(m.name) p WHERE m.type = 'table' AND m.name NOT LIKE '\_%' ESCAPE '\' " +
        "AND m.name NOT LIKE 'sqlite\_%' ESCAPE '\' AND m.name NOT LIKE 'file\_text%' ESCAPE '\' ORDER BY m.name, p.cid")
    Assert-Exit $cols 0
    $byTable = [ordered]@{}
    foreach ($line in $cols.Lines) {
        $t = $line.Trim()
        if (-not $t.StartsWith('c=')) { continue }
        $table, $column = $t.Substring(2).Split('|', 2)
        if (-not $byTable.Contains($table)) { $byTable[$table] = [System.Collections.Generic.List[string]]::new() }
        $byTable[$table].Add($column)
    }
    if ($byTable.Count -lt 20) { throw "only $($byTable.Count) table(s) in $Db" }
    $rows = [System.Collections.Generic.List[string]]::new()
    $batch = [System.Collections.Generic.List[string]]::new()
    $flush = {
        if (-not $batch.Count) { return }
        $r = Invoke-Gate --map-query $Db --width 0 --limit 0 --sql ($batch -join ' UNION ALL ')
        Assert-Exit $r 0
        foreach ($line in $r.Lines) {
            $t = $line.Trim()
            $cut = $t.IndexOf('=')
            if ($cut -gt 0 -and $byTable.Contains($t.Substring(0, $cut))) { $rows.Add($t) }
        }
        $batch.Clear()
    }
    foreach ($table in $byTable.Keys) {
        $cells = ($byTable[$table] | ForEach-Object {
            "replace(replace(quote(`"$_`"), char(13), '\r'), char(10), '\n')"
        }) -join " || ',' || "
        $batch.Add("SELECT '$table=' || $cells AS r FROM `"$table`"")
        if (($batch -join ' UNION ALL ').Length -gt 6000) { & $flush }
    }
    & $flush
    return @($rows | Sort-Object)
}

# The workspace mapped, `$Edit` given a comment, mapped again (a PARTIAL run, unless the store reads everything - which
# `$Env` may ask of it), its map moved aside, and the same tree mapped from nothing.
# `-Add` writes `$Edit` as a new file instead.
function Invoke-TsExactRun([string]$Edit, [hashtable]$Env = @{}, [switch]$Add) {
    $tree = New-TsExactWorkspace
    Assert-Exit (New-TsRowsDb $tree).Result 0
    $path = Join-Path $tree $Edit
    if ($Add) { Set-Content -LiteralPath $path -Value 'a note' }
    else { Set-Content -LiteralPath $path -Value ((Get-Content -LiteralPath $path -Raw) + "// edited`n") -NoNewline }
    foreach ($name in $Env.Keys) { Set-Item -Path "Env:$name" -Value $Env[$name] }
    try {
        $partial = New-TsRowsDb $tree
    } finally {
        foreach ($name in $Env.Keys) { Remove-Item -Path "Env:$name" -ErrorAction SilentlyContinue }
    }
    Assert-Exit $partial.Result 0
    Assert-Line $partial.Result 'put back'
    $aside = New-Tree @{}
    $kept = Join-Path $aside 'partial.sqlite'
    Move-Item -LiteralPath $partial.Db -Destination $kept
    foreach ($suffix in @('-wal', '-shm')) {
        if (Test-Path -LiteralPath "$($partial.Db)$suffix") { Move-Item -LiteralPath "$($partial.Db)$suffix" -Destination "$kept$suffix" }
    }
    $full = New-TsRowsDb $tree
    Assert-Exit $full.Result 0
    Assert-NoLine $full.Result 'put back'
    return [pscustomobject]@{ Partial = $partial.Result; PartialDb = $kept; FullDb = $full.Db }
}

# Two sorted row lists, equal - or the first rows each holds alone.
function Assert-TsExactRows([string[]]$Partial, [string[]]$Full, [string]$What) {
    $onlyPartial = @($Partial | Where-Object { $Full -notcontains $_ })
    $onlyFull = @($Full | Where-Object { $Partial -notcontains $_ })
    if ($onlyPartial.Count -or $onlyFull.Count -or $Partial.Count -ne $Full.Count) {
        throw ("$What - partial $($Partial.Count) row(s), full $($Full.Count); partial only:`n  " +
            (($onlyPartial | Select-Object -First 5) -join "`n  ") + "`nfull only:`n  " + (($onlyFull | Select-Object -First 5) -join "`n  "))
    }
}

Test-Case 'tsrows: a partial run writes every row a fresh full map of the edited tree writes, ids included' {
    $run = Invoke-TsExactRun 'apps/shop/src/alpha.component.ts'
    $partial = Get-TsExactRows $run.PartialDb
    # What the fixture is for, there in both maps: two projects, the injected parameter, the service call's key.
    foreach ($needle in @("projects='p:2'", "di=", "i18n_refs=", "render_graph=", "routes__load_children=")) {
        if (-not @($partial | Where-Object { $_.StartsWith($needle) }).Count) { throw "no row starting $needle" }
    }
    Assert-TsExactRows $partial (Get-TsExactRows $run.FullDb) 'every row after editing alpha.component.ts'
}

Test-Case 'tsrows: a partial run publishes the spec a fresh full map of the edited tree publishes' {
    $run = Invoke-TsExactRun 'apps/shop/src/alpha.component.ts'
    $partial = Get-TsRowsMeta $run.PartialDb 'spec:typescript'
    $full = Get-TsRowsMeta $run.FullDb 'spec:typescript'
    # A join to `files` and an anchored table: what a spec derived from the rows the run MADE alone had none of.
    if (-not $partial.joins.'classes.file') { throw 'the partial run''s spec declares no join classes.file' }
    if (-not $partial.anchors.'classes') { throw 'the partial run''s spec anchors no classes' }
    Assert-Equal ($partial | ConvertTo-Json -Depth 100 -Compress) ($full | ConvertTo-Json -Depth 100 -Compress) 'spec:typescript'
}

Test-Case 'tsrows: a partial run over every hop that would leave a kept row naming a row it replaced reads everything' {
    # `classes` deleted WHOLE while node sends the rows it made - a table wrongly rebuilt, as `files` once was - through a
    # door only a test opens (`STRUCTUREGATE_TEST_WHOLE`, `tsapply.rs`). `main.ts` is read again and nothing imports it,
    # so no file the stored graph lists names a class: only a probe of every kept row's joins and structures finds them.
    $run = Invoke-TsExactRun 'apps/shop/src/main.ts' @{ STRUCTUREGATE_TEST_WHOLE = 'classes' }
    Assert-Line $run.Partial 'reads everything instead'
    Assert-TsExactRows (Get-TsExactRows $run.PartialDb) (Get-TsExactRows $run.FullDb) 'every row after the run read everything'
}

# A PROJECT THE EDIT DOES NOT TOUCH IS NOT BUILT (rows 125): the `ui` library owns no file the run reads again, so its program stays unbuilt -
# and the map is still the full run's, its unresolved import (noted by that program) included.
Test-Case 'tsrows: a partial run leaves an untouched project unbuilt and writes the full run''s rows' {
    $run = Invoke-TsExactRun 'apps/shop/src/alpha.component.ts'
    Assert-Line $run.Partial 'left 1 project program(s) unbuilt'
    $partial = Get-TsExactRows $run.PartialDb
    if (-not @($partial | Where-Object { $_.StartsWith('diagnostics=') -and $_.Contains('demo-missing-package') }).Count) { throw 'the unbuilt program''s note is gone' }
    Assert-TsExactRows $partial (Get-TsExactRows $run.FullDb) 'every row after editing alpha.component.ts, the ui program unbuilt'
}

# A NEW FILE NO PROGRAM CAN HOLD reads only what moved and builds no program (`TsShape.mjs::DECLARING`): one `.txt` added in a large
# workspace read every file of it again, and the partial run that replaced that built every project's program.
Test-Case 'tsrows: a new file no program holds is a partial run that builds no program, and writes the full run''s rows' {
    $run = Invoke-TsExactRun 'apps/shop/src/notes.txt' -Add
    Assert-NoLine $run.Partial 'reads everything'
    Assert-Line $run.Partial 'left 2 project program(s) unbuilt'
    # THE ONE DIFFERENCE A PARTIAL RUN KEEPS (typescript-half.md): the new `files` row takes the next free id, where a full run numbers it
    # in path order and every later file one higher - so each map's file ids are read as the paths they name, and every row is compared.
    $byPath = { param($rows)
        $paths = @{}
        foreach ($row in $rows) {
            if (-not $row.StartsWith('files=')) { continue }
            $cells = $row.Substring(6).Split(',')
            $paths[$cells[0]] = $cells[1]
        }
        @($rows | ForEach-Object { $line = $_; foreach ($id in $paths.Keys) { $line = $line.Replace($id, $paths[$id]) }; $line } | Sort-Object)
    }
    Assert-TsExactRows (& $byPath (Get-TsExactRows $run.PartialDb)) (& $byPath (Get-TsExactRows $run.FullDb)) 'every row after a note was added'
}

# A KEPT ROW A PASS RECOMPUTES THE SAME is untouched (`Store.verifyUntouched`, compared as it is written): the arrow `rules()` returns gets
# its `target` again on every run, spelt absolute in node and relative in the map, and every partial run was asked again over every hop.
Test-Case 'tsrows: a kept row a pass recomputes the same does not ask the run again, and the rows are the full run''s' {
    $run = Invoke-TsExactRun 'apps/shop/src/beta.component.ts'
    Assert-NoLine $run.Partial 'reading every hop of dependents again'
    $partial = Get-TsExactRows $run.PartialDb
    if (-not @($partial | Where-Object { $_.StartsWith('functions=') -and $_.Contains("'template'") }).Count) { throw 'no arrow targeted through a template' }
    Assert-TsExactRows $partial (Get-TsExactRows $run.FullDb) 'every row after editing beta.component.ts'
}

# A SECOND EDIT COUNTS ITSELF ALONE: the run before recorded the first one's file as read.
Test-Case 'tsrows: a partial run after a partial run reads only what moved since the last' {
    $tree = New-TsExactWorkspace
    Assert-Exit (New-TsRowsDb $tree).Result 0
    foreach ($edit in 'apps/shop/src/beta.component.ts', 'apps/shop/src/alpha.component.ts') {
        $path = Join-Path $tree $edit
        Set-Content -LiteralPath $path -Value ((Get-Content -LiteralPath $path -Raw) + "// edited`n") -NoNewline
        $run = New-TsRowsDb $tree
        Assert-Exit $run.Result 0
        Assert-Line $run.Result 'reads what moved: 1 file(s) moved since its rows were written'
    }
}

}
