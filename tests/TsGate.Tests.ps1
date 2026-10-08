<#
    --ts-discipline end to end: the exe, the node launch, the counts and the twelve rules folded into one
    verdict.

    THE COMPILER IS THE REPO'S OWN, so these cases need a `typescript` the spawned node can resolve. One is
    installed into a cache under TEMP on first run and reached through NODE_PATH - the same escape hatch a
    pnpm or a hoisted layout uses. The cases that do NOT need a compiler (the flag being opt-in, a host that
    will not launch, a tree with no typescript at all) run either way, because those are the failure modes
    that must never turn into silence.
#>

$script:TsModules = Get-TsModules
$script:TsMajor = Get-TypeScriptMajor $script:TsModules
if (-not $script:TsModules) {
    Write-Host "    (no node/typescript on this machine - the rule cases are not run)"
}

# ---------------------------------------------------------------------------------------------------
# The half itself: opt-in, and never silently absent
# ---------------------------------------------------------------------------------------------------

Test-Case 'ts: the flag is opt-in - `any` is silent until it is passed' {
    $tree = Use-Tree @{ 'a.ts' = "export function f(x: any): number { return 1; }`n" }
    Assert-Exit (Invoke-TsGate --root $tree) 0
}

Test-Case 'ts: a host that cannot be launched is a VIOLATION, not a skip' {
    $tree = Use-Tree @{ 'a.ts' = "export const x: number = 1;`n" }
    $result = Invoke-Gate --root $tree --ts-discipline --ts-host no-such-node.exe
    Assert-Exit $result 1
    Assert-Line $result 'no-such-node.exe'
}

# NODE KEEPS ITS COMPILED CODE BETWEEN RUNS (`hosts/mod.rs`): every node the gate starts is given a compile cache
# under TEMP unless the environment names one, which then wins. Only a node that has the cache (22.1+) is asked.
$script:TsNodeCaches = (& node -p "typeof require('node:module').enableCompileCache" 2>$null) -eq 'function'
if ($script:TsModules -and $script:TsNodeCaches) {
    Test-Case 'ts: node is started with a compile cache - under TEMP, or the one NODE_COMPILE_CACHE names' {
        $tree = Use-Tree @{ 'src/a.ts' = "export const x: number = 1;`n" }
        $temp = Join-Path $tree 'temp'
        New-Item -ItemType Directory -Path $temp | Out-Null
        $names = @('TMPDIR', 'TEMP', 'TMP', 'NODE_COMPILE_CACHE')
        $saved = @{}
        foreach ($name in $names) { $saved[$name] = [Environment]::GetEnvironmentVariable($name) }
        try {
            foreach ($name in 'TMPDIR', 'TEMP', 'TMP') { [Environment]::SetEnvironmentVariable($name, $temp) }
            Remove-Item Env:NODE_COMPILE_CACHE -ErrorAction SilentlyContinue
            Assert-Exit (Invoke-TsGate --root (Join-Path $tree 'src') --ts-discipline --no-gate-cache) 0
            $default = Join-Path $temp 'structuregate-node-cache'
            Assert-Equal (@(Get-ChildItem $default -Recurse -File -ErrorAction SilentlyContinue).Count -gt 0) $true 'a cache under TEMP'

            $own = Join-Path $tree 'own-cache'
            [Environment]::SetEnvironmentVariable('NODE_COMPILE_CACHE', $own)
            Remove-Item -Recurse -Force $default
            Assert-Exit (Invoke-TsGate --root (Join-Path $tree 'src') --ts-discipline --no-gate-cache) 0
            Assert-Equal (@(Get-ChildItem $own -Recurse -File -ErrorAction SilentlyContinue).Count -gt 0) $true 'the cache the environment names'
            Assert-Equal (Test-Path $default) $false 'a second cache beside the one named'
        } finally {
            # AN UNSET VARIABLE IS REMOVED, never set to "": a child then sees `TMPDIR=` and stages into the cwd.
            foreach ($name in $names) {
                if ($null -eq $saved[$name]) { Remove-Item "Env:$name" -ErrorAction SilentlyContinue }
                else { [Environment]::SetEnvironmentVariable($name, $saved[$name]) }
            }
        }
    }
}

Test-Case 'ts: a tree with no typescript installed is REPORTED, not passed' {
    # The rules key on the repo's own compiler. With none resolvable there is no second-best parser, and a
    # gate that passes because it could not find one is reporting on checks it never ran.
    $tree = Use-Tree @{ 'a.ts' = "export const x: any = 1;`n" }
    $previous = $env:NODE_PATH
    $env:NODE_PATH = ''
    try { $result = Invoke-Gate --root $tree --ts-discipline }
    finally { $env:NODE_PATH = $previous }
    Assert-Exit $result 1
    Assert-Line $result 'typescript could not be resolved'
}

# ---------------------------------------------------------------------------------------------------
# The rules
# ---------------------------------------------------------------------------------------------------

if ($script:TsModules) {
    # A TYPESCRIPT 7 COMPILER ANSWERS PER PROJECT, so a changed tsconfig asks every file again - though no
    # file of it moved.
    Test-Case 'ts: a kept answer is asked again when the project around it changes' {
        $tree = Use-Tree @{ 'a.ts' = "export const x: number = 1;`n"; 'tsconfig.json' = '{ "include": ["*.ts"] }' }
        Assert-Exit (Invoke-TsGate --root $tree --ts-discipline --max-lines 10) 0
        & $script:Python -c "import sqlite3,sys; c = sqlite3.connect(sys.argv[1]); c.execute('UPDATE gate_counts SET lines = 99'); c.execute('DELETE FROM gate_pass'); c.commit()" (Join-Path $tree '.fbt\gate.sqlite')
        $forged = Invoke-TsGate --root $tree --ts-discipline --max-lines 10
        Assert-Exit $forged 1
        Assert-Line $forged 'a.ts'
        Set-Content -LiteralPath (Join-Path $tree 'tsconfig.json') -Value '{ "include": ["*.ts"], "compilerOptions": {} }'
        Assert-Exit (Invoke-TsGate --root $tree --ts-discipline --max-lines 10) 0
    }


Test-Case 'ts: `any` fails the build, wherever it is written' {
    $tree = Use-Tree @{
        'any.ts' = @"
export function read(raw: string): unknown { return JSON.parse(raw); }
export function bad(input: any): void { report(input); }
export function cast(value: unknown): string { return (value as any).name; }
export function box(rows: Array<any>): number { return rows.length; }
function report(x: unknown): void { void x; }
"@
    }
    $result = Invoke-TsGate --root $tree --ts-discipline
    Assert-Exit $result 1
    Assert-Line $result 'any.ts:2'
    Assert-Line $result 'any.ts:3'
    Assert-Line $result 'any.ts:4'
    Assert-Line $result 'the `any` type'
}

Test-Case 'ts: `// tsgate-ok` waives the line it sits on, or the line under it' {
    # Both placements, because that is how people write the comment - and nothing further down is waived.
    $tree = Use-Tree @{
        'waived.ts' = @"
export function bad(input: any): void { void input; }  // tsgate-ok: third-party callback signature

// tsgate-ok: the shim is generated with this signature
export function above(other: any): void { void other; }

export function worse(third: any): void { void third; }
"@
    }
    $result = Invoke-TsGate --root $tree --ts-discipline
    Assert-Exit $result 1
    Assert-NoLine $result 'waived.ts:1'
    Assert-NoLine $result 'waived.ts:4'
    Assert-Line $result 'waived.ts:6'
}

Test-Case 'ts: there is ONE severity - every rule fails the build' {
    $tree = Use-Tree @{ 'one.ts' = "export function f(x: any): void { void x; }`n" }
    $result = Invoke-TsGate --root $tree --ts-discipline
    Assert-Exit $result 1
    Assert-Line $result 'error:'
    Assert-NoLine $result 'warning:'
}

Test-Case 'ts: the escape hatches from `any` are closed too' {
    $tree = Use-Tree @{
        'escape.ts' = @"
export function force(value: unknown): string { return value as unknown as string; }
export function shout(name: string | null): number { return name!.length; }
"@
    }
    $result = Invoke-TsGate --root $tree --ts-discipline
    Assert-Exit $result 1
    Assert-Line $result 'a double assertion'
    Assert-Line $result 'a non-null assertion'
}

Test-Case 'ts: `@ts-ignore` is a violation, `@ts-expect-error` is not' {
    $tree = Use-Tree @{
        'ignore.ts' = @"
// @ts-ignore
export const a: number = later();
// @ts-expect-error the shim is typed loosely upstream
export const b: number = later();
function later(): number { return 1; }
"@
    }
    $result = Invoke-TsGate --root $tree --ts-discipline
    Assert-Exit $result 1
    Assert-Line $result 'ignore.ts:1'
    Assert-Line $result '@ts-expect-error'
    Assert-NoLine $result 'ignore.ts:3'
}

Test-Case 'ts: an untyped parameter on a DECLARED function fails; a callback parameter does not' {
    $tree = Use-Tree @{
        'params.ts' = @"
export function total(rows: number[]): number { return rows.reduce((sum, row) => sum + row, 0); }
export function count(rows): number { return rows.length; }
"@
    }
    $result = Invoke-TsGate --root $tree --ts-discipline
    Assert-Exit $result 1
    Assert-Line $result 'params.ts:2'
    Assert-NoLine $result 'params.ts:1'
}

Test-Case 'ts: an empty catch, a `==`, a `var` and a missing return type each fail' {
    $tree = Use-Tree @{
        'shapes.ts' = @"
export function load(): void {
    try { work(); } catch {}
}
export function same(a: string, b: number): boolean { return a == b; }
export function loop(): number {
    var total = 0;
    return total;
}
export function guess(a: number) { return a; }
function work(): void { return; }
"@
    }
    $result = Invoke-TsGate --root $tree --ts-discipline
    Assert-Exit $result 1
    Assert-Line $result 'an empty catch'
    Assert-Line $result 'a coercing comparison'
    Assert-Line $result '`var`'
    Assert-Line $result 'has NO return type'
}

Test-Case 'ts: a catch holding only a COMMENT is still empty - and the waiver is how a deliberate ignore is written' {
    # Decided, not an oversight: the comment documents the swallow, it does not handle it. `catch {}` with a
    # note beside it still discards the error and still runs the code after the try as if nothing failed.
    $tree = Use-Tree @{
        'catches.ts' = @"
export function noted(): void {
    try { work(); } catch (error) { /* the icon stays at its default */ }
}
export function waived(): void {
    try { work(); } catch (error) { /* tsgate-ok: the probe is best-effort, a miss is the answer */ }
}
function work(): void { return; }
"@
    }
    $result = Invoke-TsGate --root $tree --ts-discipline
    Assert-Exit $result 1
    Assert-Line $result 'catches.ts:2'
    Assert-Line $result 'an empty catch'
    Assert-NoLine $result 'catches.ts:5'
}

Test-Case 'ts: `== null` stays legal - it is the ecosystem idiom for "null or undefined"' {
    $tree = Use-Tree @{ 'nullish.ts' = "export function set(x: string | null): boolean { return x == null; }`n" }
    Assert-Exit (Invoke-TsGate --root $tree --ts-discipline) 0
}

Test-Case 'ts: `{}`, `Object` and `Function` as a type fail' {
    $tree = Use-Tree @{
        'weak.ts' = @"
export function a(value: {}): void { void value; }
export function b(value: Object): void { void value; }
export function c(handler: Function): void { void handler; }
"@
    }
    $result = Invoke-TsGate --root $tree --ts-discipline
    Assert-Exit $result 1
    Assert-Line $result 'weak.ts:1'
    Assert-Line $result 'weak.ts:2'
    Assert-Line $result 'weak.ts:3'
}

Test-Case 'ts: `namespace` and `require` fail - both sit outside the ES module graph' {
    $tree = Use-Tree @{
        'legacy.ts' = @"
namespace Legacy { export const one = 1; }
export const fs = require('node:fs');
export const used: number = Legacy.one;
"@
    }
    $result = Invoke-TsGate --root $tree --ts-discipline
    Assert-Exit $result 1
    Assert-Line $result '`namespace`/`module`'
    Assert-Line $result '`require(...)` in TypeScript'
}

Test-Case 'ts: a call to a file-local async function that is not awaited fails; `void` and `await` do not' {
    $tree = Use-Tree @{
        'promise.ts' = @"
async function save(): Promise<void> { return; }
export async function good(): Promise<void> {
    await save();
}
export async function deliberate(): Promise<void> {
    void save();
}
export async function bad(): Promise<void> {
    save();
}
"@
    }
    $result = Invoke-TsGate --root $tree --ts-discipline
    Assert-Exit $result 1
    Assert-Line $result 'promise.ts:9'
    Assert-Line $result 'is called and NOT awaited'
    Assert-NoLine $result 'promise.ts:3'
    Assert-NoLine $result 'promise.ts:6'
}

Test-Case 'ts: a file that does not PARSE is a violation, not a partial check' {
    # A partial tree is the failure this rule exists for: the parser error-recovers and every rule below
    # quietly stops covering the rest of the file.
    $tree = Use-Tree @{ 'broken.ts' = "export function f(: number { return 1; }`n" }
    $result = Invoke-TsGate --root $tree --ts-discipline
    Assert-Exit $result 1
    Assert-Line $result 'does not parse as TypeScript'
}

Test-Case 'ts: a clean, fully typed module passes - no rule fires on idiomatic code' {
    $tree = Use-Tree @{
        'clean.ts' = @"
export interface Row { id: string; total: number; }

export function sum(rows: readonly Row[]): number {
    return rows.reduce((acc, row) => acc + row.total, 0);
}

export async function save(rows: readonly Row[]): Promise<number> {
    const written = sum(rows);
    if (written === 0) { return 0; }
    try {
        await flush(written);
    } catch (error) {
        console.error(error);
        throw error;
    }
    return written;
}

async function flush(count: number): Promise<void> {
    void count;
}
"@
    }
    Assert-Exit (Invoke-TsGate --root $tree --ts-discipline) 0
}

# A DAMAGED NATIVE COMPILER IS NAMED, not reported as "node did not finish". 7.x keeps its executable and its
# lib files in a per-platform package; with `lib.d.ts` gone it panics on start, and the caller used to see only
# node's own stack. The install is the cache's, copied, with that one file removed.
Test-Case 'ts: a native compiler missing its lib.d.ts is reported by name' {
    if ($script:TsMajor -lt 7) { return }
    $tree = Use-Tree @{ 'a.ts' = "export const a = 1;`n" }
    $modules = Join-Path $tree 'node_modules'
    [void](New-Item -ItemType Directory -Path $modules -Force)
    Copy-Item (Join-Path $script:TsModules 'typescript') $modules -Recurse
    Copy-Item (Join-Path $script:TsModules '@typescript') $modules -Recurse
    Get-ChildItem (Join-Path $modules '@typescript') -Directory -Filter 'typescript-*' |
        ForEach-Object { Remove-Item (Join-Path $_.FullName 'lib\lib.d.ts') -Force }
    $result = Invoke-Gate --root $tree --ts-discipline
    Assert-Exit $result 1
    Assert-Line $result 'is damaged'
    Assert-Line $result 'lib.d.ts does not exist'
}

# THE HARNESS REUSES ITS COMPILER CACHE ONLY WHEN THE COMPILER CAN START: a cache that had lost the native
# compiler's lib.d.ts failed every TypeScript case in the suite, because a package.json was all it checked.
Test-Case 'ts: a compiler cache missing its lib.d.ts is not taken as complete' {
    $root = Join-Path ([System.IO.Path]::GetTempPath()) ('sgtest-tscache-' + [guid]::NewGuid().ToString('N').Substring(0, 8))
    try {
        $platform = Join-Path $root '@typescript\typescript-win32-x64\lib'
        [void](New-Item -ItemType Directory -Path (Join-Path $root 'typescript'), $platform -Force)
        Set-Content (Join-Path $root 'typescript\package.json') '{}'
        if (Test-TypeScriptComplete $root) { throw 'a cache with no lib.d.ts anywhere was taken as complete' }
        Set-Content (Join-Path $platform 'lib.d.ts') ''
        if (-not (Test-TypeScriptComplete $root)) { throw 'the 7.x layout with its lib.d.ts was refused' }
    } finally { Remove-Item $root -Recurse -Force -ErrorAction SilentlyContinue }
}

Test-Case 'ts: the SAME rule fires through the 5.x in-process parser, not just the 7.x native one' {
    # Two compilers, one rule set. 5.x parses in this process (`createSourceFile`); 7.x is the native port,
    # whose JS package parses nothing and answers over its own protocol. A rule that only fires on one of
    # them is a rule half the repos do not have.
    $five = Get-TypeScriptPath 'typescript@5'
    if (-not $five) { return }
    $tree = Use-Tree @{ 'five.ts' = "export function f(x: any): void { void x; }`n" }
    $previous = $env:NODE_PATH
    $env:NODE_PATH = $five
    try { $result = Invoke-Gate --root $tree --ts-discipline }
    finally { $env:NODE_PATH = $previous }
    Assert-Exit $result 1
    Assert-Line $result 'five.ts:1'
    Assert-Line $result 'the `any` type'
}

# ---------------------------------------------------------------------------------------------------
# What is measured, and how
# ---------------------------------------------------------------------------------------------------

Test-Case 'ts: lines are counted BY TOKEN - a comment and a blank line are not source' {
    $tree = Use-Tree @{
        'count.ts' = @"
// a measured reason, three lines long
// second line
// third line

export const one: number = 1;

/* a block comment
   over two lines */
export const two: number = 2;
"@
    }
    $dump = Get-TsDump --root $tree --ts-discipline
    Assert-Equal (Get-Count $dump 'files' 'count.ts') 2 'token lines'
}

Test-Case 'ts: `.tsx`, `.mts` and `.cts` are added by the flag and rule-checked' {
    $tree = Use-Tree @{
        'ui.tsx' = "export function View(props: any): null { void props; return null; }`n"
        'mod.mts' = "export const x: any = 1;`n"
        'old.cts' = "export const y: any = 2;`n"
    }
    $result = Invoke-TsGate --root $tree --ts-discipline
    Assert-Exit $result 1
    Assert-Line $result 'ui.tsx:1'
    Assert-Line $result 'mod.mts:1'
    Assert-Line $result 'old.cts:1'
}

Test-Case 'ts: `.d.ts` is DECLARATIONS - counted, never rule-checked' {
    $tree = Use-Tree @{ 'shim.d.ts' = "export declare function anything(input: any): any;`n" }
    $dump = Get-TsDump --root $tree --ts-discipline
    Assert-Equal (Get-Count $dump 'files' 'shim.d.ts') 1 'declaration source lines'
    Assert-Exit (Invoke-TsGate --root $tree --ts-discipline) 0
}

Test-Case 'ts: the line limit applies to what the compiler counted' {
    $body = (1..40 | ForEach-Object { "export const v$_`: number = $_;" }) -join "`n"
    $tree = Use-Tree @{ 'long.ts' = $body }
    $result = Invoke-TsGate --root $tree --ts-discipline --max-lines 10
    Assert-Exit $result 1
    Assert-Line $result 'long.ts'
}

Test-Case 'ts: node_modules is not measured, so a dependency cannot fail the gate' {
    $tree = Use-Tree @{
        'app.ts' = "export const x: number = 1;`n"
        'node_modules/dep/index.ts' = "export const y: any = 2;`n"
    }
    Assert-Exit (Invoke-TsGate --root $tree --ts-discipline) 0
}

}
