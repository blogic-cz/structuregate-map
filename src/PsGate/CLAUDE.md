# PsGate: `--ps-discipline`, parsed by the host that will run it

```
structuregate --root . --ps-discipline                  # powershell.exe (5.1) parses
structuregate --root . --ps-discipline --ps-host pwsh    # only if the app is launched by pwsh
```

Two jobs in ONE host launch over the whole file set (per-file launches cost more in startup than in parsing):
`.ps1`/`.psm1`/`.psd1` join `--ext` and are **counted by token** from `[Parser]::ParseFile`, as C# is by
Roslyn, and the rules below run. These scripts are embedded in the exe (`rust/fbtcore/src/embedded/mod.rs`)
and launched by `rust/fbtcore/src/gate/run.rs`.

| file | holds |
|---|---|
| `PsGate.ps1` | entry: parse, count, index, waiver, output |
| `PsGate.Ast.ps1` | the AST vocabulary ("what is this") — no pattern matching |
| `PsGate.Rules.ps1` | rules 1-5, 10-12 |
| `PsGate.Rules.Scope.ps1` | rules 6-9 (closures, modal scope, unrolling) |
| `PsGate.Rules.WinForms.ps1` | rules 13-15 (`VirtualMode` lists) |

**Why the parser is not in this process.** `System.Management.Automation` is reflection-heavy and does not
NativeAOT, and it is the PS7 grammar — a superset that accepts `??`, `?:`, `&&` and would pass a file 5.1
cannot read. Patterns are banned. So the 5.1 host parses, and **a file that does not parse is a violation** —
which enforces the version floor for free.

**`.psd1` is data**: counted, never rule-checked. **A missing count is a violation**: a host that cannot
launch, or returns no count for a file it was given, fails — a check nobody noticed was skipped is worse
than none.

## The rules

| # | rule | what it costs when missing |
|---|---|---|
| 0 | the file must PARSE under `--ps-host` | error recovery returns a partial tree; every rule silently stops covering the rest |
| 1 | no assignment to an automatic variable (`$host`, `$error`, `$matches`, `$input`, `$profile`, …) | `$host` reused for an SMTP host breaks `Write-Host` far away. `$null = …` is untouched: the discard idiom |
| 2 | no `@(… \| ConvertFrom-Json)` | 5.1 emits a JSON array as ONE object: the wrapper yields a one-element array holding an `Object[]`, every field read is off by a level |
| 3 | not two spellings of one hashtable key | keys are case-INSENSITIVE: a `$ctx.ticked` counter overwrote the `$ctx.Ticked` scriptblock (`The term '114' is not recognized`) |
| 4 | no `$x -eq $null` | with an array on the left `-eq` FILTERS: true only when the array contains a `$null` |
| 5 | no `Try…([ref]$x)` whose result is dropped | on `$false` the target keeps its PREVIOUS value, which looks fresh; a bare call also leaks a boolean into the output |
| 6 | no `Add_*` handler capturing a function local without `.GetNewClosure()` | measured: a plain `Add_Tick` saw its captured string AND the timer as empty — `You cannot call a method on a null-valued expression` every tick |
| 7 | no generic local (`$name`, `$path`, `$key`, `$text`, `$note`, …) in a scope that opens a **modal** dialog | while `ShowDialog()` blocks, a handler firing elsewhere resolves unqualified names against the BLOCKED dialog's locals |
| 8 | no `return $set` for a HashSet/List | a scriptblock UNROLLS its return: a one-element set arrives as `[string]`, `.Contains` becomes substring matching |
| 9 | no `@($listOfObject)` | that overload throws `ArgumentException` on 5.1 instead of copying |
| 10 | no `param` used as a COMMAND | `param(...)` declares only as the FIRST statement; elsewhere it calls a command `param`, the block has NO parameters, and the parse is clean |
| 11 | no member access stranded in ARGUMENT mode | `f $x {…} .GetNewClosure()` passes the bareword `.GetNewClosure` as an argument. Reported only after an EXPRESSION and for a PascalCase name, which leaves `.gitignore` and `.\src` alone |
| 12 | no `Add-Type -MemberDefinition` at FILE scope | it COMPILES (csc, hundreds of ms) on every launch's startup path. Inside a function — or a scriptblock literal, which assigning does not run — it is paid on first use |
| 13 | no `CheckBoxes = $true` on a `VirtualMode` list | a click on a virtual list's box raises no ItemCheck and toggles nothing. A list with a mouse handler hit-testing `ListViewHitTestLocations.StateImage` - attached directly or by a function of the file it is handed to - or one passed to a helper in `$VirtualCheckHelpers` is left alone |
| 14 | no `Add_ItemCheck` on a `VirtualMode` list | the event comes from check-box handling a virtual list does not have: dead code |
| 15 | no `.Items` / `.CheckedItems` / `.CheckedIndices` on a `VirtualMode` list | the list holds no items: `.Items.Count` is 0 with rows on screen. Both LIE rather than throw |

## Why the rules are shaped this way

**ONE SEVERITY: every rule FAILS the build.** A `warn` level existed; a finding that does not fail scrolls past
under a log whose last line says OK. The exception is `# psgate-ok` on the line or the one above — free,
and the reason stays next to the code. The price: a rule that stops a build cannot be occasionally wrong, so
the rules stay NARROW, silent wherever only a symbol table or a second file could decide.

**Rules 6 and 7 split the world on purpose.** A scope that opens a modal dialog is still on the stack when its
handlers fire, so its plain scriptblocks are CORRECT — rule 6 is silent there and rule 7's naming convention
covers it. Without the split rule 6 was three false positives in four on its first real tree. Whether a
handler fires while a dialog blocks is not statically decidable, so rule 7 enforces the convention that
makes the collision impossible.

**Narrowings that are exact, not guesses:**

- rule 7 marks only the scope that BLOCKS (whose `ShowDialog()` is on the stack), not every ancestor — marking
  the chain flagged locals in handler scriptblocks that pump nothing;
- rule 5 accepts initialise-and-try on one line (`$ttl = 0; [int]::TryParse($s, [ref]$ttl)`): the init reruns
  with every call. A target initialised elsewhere (`$msg = $null` above a `while` calling
  `TryDequeue([ref]$msg)`) still fails;
- rule 12 defers inside a scriptblock literal as inside a function — it was written against "not inside a
  function", and the first real tree showed a worker compiling its P/Invoke in a `$script:` scriptblock.

**Rules 13-15 are one bug from three sides**: `VirtualMode` stops the list owning its rows, and every member
that assumes it does goes silently wrong. The flag is decidable only as a literal `$true` in the same file.

**A new rule must fire nowhere it is wrong before it may stop a build.** Prove it on a real tree, then pin it
in `tests/fixtures/` (see [../../tests/CLAUDE.md](../../tests/CLAUDE.md)).

**Token counting matters here**: scripts that carry long reason headers are about a fifth comment and blank,
and counting those as source would tell the author to delete the comments the repo keeps on purpose.
