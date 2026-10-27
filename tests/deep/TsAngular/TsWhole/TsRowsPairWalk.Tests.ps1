<#
    THE FIRST TWO PROJECTS WALKED AT ONCE (`src/TsRows/TsSetup/TsPairWalk.mjs`) store what the walk one after the other
    stores: every row of every table, its id and its place, and every counter. The two projects share a library through
    a path alias - the first owns its files, the second folds a constant declared there, notes the same unresolved
    import and reads a branch of it - so each thing the merge has to give back to the first is in the fixture.
    `STRUCTUREGATE_TS_PAIR=0` is the walk one after the other. Its helpers are `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '../../TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

# Every table of two maps, row by row in rowid order (a table without one, as a sorted set), and the id counters.
$script:PairCompare = @'
import sqlite3, sys
a, b = (sqlite3.connect(f'file:{p}?mode=ro', uri=True) for p in sys.argv[1:3])
def tables(c):
    return {n: 'WITHOUT ROWID' in (s or '').upper() for n, s in c.execute("SELECT name, sql FROM sqlite_master WHERE type = 'table'")
            if not n.startswith('sqlite_') and 'fts' not in n and n not in ('_meta', '_meta__value')}
ta, tb = tables(a), tables(b)
print('tables=%d/%d' % (len(ta), len(tb)))
for t in sorted(set(ta) | set(tb)):
    if t not in ta or t not in tb:
        print('DIFF ' + t); continue
    # `_cells` is written in an order the closure does not keep from run to run: compared as a set.
    if tb[t] or t == '_cells':
        ra, rb = (sorted(c.execute(f'SELECT * FROM "{t}"').fetchall(), key=repr) for c in (a, b))
    else:
        ra, rb = (c.execute(f'SELECT rowid, * FROM "{t}" ORDER BY rowid').fetchall() for c in (a, b))
    if ra != rb: print('DIFF ' + t)
ca, cb = (dict(c.execute("SELECT key, value FROM _meta WHERE key LIKE 'counter:%'").fetchall()) for c in (a, b))
print('counters=' + ('same' if ca == cb and ca else 'DIFF'))
'@

Test-Case 'tsrows: the first two projects walked at once store every row the walk one after the other does' {
    $tree = New-TsRowsWorkspace @{
        'angular.json' = '{"projects":{"admin":{"projectType":"application","root":"apps/admin","sourceRoot":"apps/admin/src",' +
            '"architect":{"build":{"options":{"tsConfig":"apps/admin/tsconfig.app.json"}}}},' +
            '"shop":{"projectType":"application","root":"apps/shop","sourceRoot":"apps/shop/src",' +
            '"architect":{"build":{"options":{"tsConfig":"apps/shop/tsconfig.app.json"}}}}}}'
        'tsconfig.base.json' = '{"compilerOptions":{"strict":true,"baseUrl":".","paths":{"@demo/core":["libs/core/src/index.ts"]}}}'
        'apps/admin/tsconfig.app.json' = '{"extends":"../../tsconfig.base.json","include":["src/**/*.ts","../../libs/core/src/**/*.ts"]}'
        'apps/shop/tsconfig.app.json' = '{"extends":"../../tsconfig.base.json","include":["src/**/*.ts"]}'
        'libs/core/src/index.ts' = "import { Missing } from 'demo-not-installed';`n" +
            "export const HUES = { alpha: 'hue.alpha', beta: 'hue.beta' } as const;`n" +
            "export enum Kind { Home = 1, Flat = 2 }`n" +
            "export const pick = (on: boolean) => (on ? HUES.alpha : HUES.beta);`n" +
            "export type Holder = Missing;`n" +
            "const base = 'core';`nexport const VERSION = ``core: `${base}``;`n"
        'apps/admin/src/admin.component.ts' = "import { Component } from '@angular/core';`nimport { HUES, Kind, VERSION } from '@demo/core';`n" +
            "@Component({ selector: 'demo-admin', standalone: true, template: '<b *ngIf=`"kind === 1`">{{ title }}</b>' })`n" +
            "export class AdminComponent {`n  kind = Kind.Home;`n  title = HUES.alpha;`n  version = VERSION;`n}`n"
        'apps/shop/src/shop.component.ts' = "import { Component, Input } from '@angular/core';`nimport { HUES, Kind, pick, VERSION } from '@demo/core';`n" +
            "@Component({ selector: 'demo-shop', standalone: true, template: '<i>{{ on ? label : other }}</i>' })`n" +
            "export class ShopComponent {`n  @Input() on = false;`n  kind = Kind.Flat;`n  label = this.on ? HUES.alpha : HUES.beta;`n" +
            "  other = pick(this.kind === Kind.Home);`n  version = VERSION;`n}`n"
    }
    # OUTSIDE THE TREE, or the second map would list the first one's database among the files.
    $dbs = New-Tree @{ 'keep.txt' = '' }
    $paired = Join-Path $dbs 'paired.sqlite'
    $alone = Join-Path $dbs 'alone.sqlite'
    $run = Invoke-Gate --root $tree --map-sqlite $paired --ts-node-modules $script:TsRowsModules
    Assert-Exit $run 0
    Assert-Line $run 'walked admin and shop at once'
    $env:STRUCTUREGATE_TS_PAIR = '0'
    try { $one = Invoke-Gate --root $tree --map-sqlite $alone --ts-node-modules $script:TsRowsModules }
    finally { Remove-Item Env:STRUCTUREGATE_TS_PAIR }
    Assert-Exit $one 0
    Assert-NoLine $one 'at once'
    # THE FIXTURE CARRIES WHAT THE MERGE GIVES BACK: a library file the second project's walk referenced, owned by the first.
    Assert-Line (Invoke-TsRowsQ $alone ("SELECT 'lib=' || p.name AS r FROM files f JOIN projects p ON p.id = f.project " +
        "WHERE f.path = 'libs/core/src/index.ts'")) 'lib=admin'
    $lines = @(& $script:Python -c $script:PairCompare $alone $paired)
    $compared = [pscustomobject]@{ Lines = $lines; Text = ($lines -join "`n") }
    Assert-Line $compared 'counters=same'
    Assert-NoLine $compared 'DIFF'
}

}
