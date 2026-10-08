# `--ts-discipline` — TypeScript, parsed by the compiler the repo builds with

Opt-in, like `--async-discipline` and `--ps-discipline`. It does two things at once:

* `.ts`, `.tsx`, `.mts` and `.cts` are added to `--ext` and counted **by token** — a source line is a line a
  token sits on, exactly as C# is counted by Roslyn and PowerShell by `[Parser]::ParseFile`.
* twelve rules run, and every one of them fails the build.

```powershell
structuregate.exe --root . --tracked --ts-discipline
structuregate.exe --root . --ts-discipline --ts-host "C:\tools\node\node.exe"
```

Expect a wall of findings on a repo that was not written this way - measured: hundreds over a handful of files in one repo,
over a thousand in another, both dominated by `var` and untyped parameters. That is what `--baseline` is for:
record the debt, and every file may then only shrink. About a second per repo, one node launch for the lot.

`.d.ts` is **declarations**: counted like any other file, never rule-checked. It is usually generated, and a
rule about empty catch blocks has nothing to say about a signature file.

## Where the parser comes from

`typescript` is resolved **out of the checked tree** (`<root>/package.json`, then whatever this script can
reach — a `NODE_PATH` entry, a global install). The gate therefore reads `satisfies`, `const` type
parameters and whatever the next release adds exactly as `tsc` does.

The alternatives were both worse. A TypeScript grammar written in C# would be an approximation, and an
approximation error-recovers into a partial answer instead of stopping — the rule quietly stops covering the
newest files. A copy of `typescript` bundled in the exe would be a **second** compiler version, disagreeing
with the one that produces the build.

**A compiler that cannot be found is a violation, not a skip.** So is a `node` that will not launch. A gate
that passes because it could not find a parser is reporting on checks it never ran.

### Two compilers, one rule set

| version | how the AST arrives |
|---|---|
| 5.x | the in-process JS parser: `ts.createSourceFile` returns the tree |
| 7.x | the native port. The JS package parses nothing — `.` exports the version only. The tree comes from the compiler **server** through `typescript/unstable/sync`, with kinds and trivia helpers in `typescript/unstable/ast` |

Nodes carry the same members either way (`kind`, `forEachChild`, `modifiers`, `getText`), so only the
acquisition differs and the rules are untouched. What the two do **not** agree on is every kind NAME — 7.x
renamed `EndOfFileToken` to `EndOfFile` — and a missing name reads back as `undefined`, which no node's
`kind` ever equals. That would not fail; the rule would go **quiet**. So every name the rules read is
checked against the compiler before a single file is parsed, and a missing one is fatal.

## The rules

`// tsgate-ok` on the line, or on the line above it, waives one — that is where the case which genuinely
needs the shape keeps its reason, next to the code.

| # | rule | what it costs when it is missing |
|---|---|---|
| 0 | the file must PARSE | error recovery hands back a partial tree, so every rule below silently stops covering the rest of the file |
| 1 | no `any` | it is not a type, it is the type system switched off: one `any` parameter makes every call through it unchecked, and `--strict` says nothing because the annotation is legal |
| 2 | no double assertion (`x as unknown as T`) | the same hole in two steps — TypeScript allows it precisely BECAUSE it refuses the single cast as unsound. Banning `any` without this rule just moves the escape |
| 3 | no non-null `!` | it claims what the compiler cannot see, nothing rechecks it when the code around it changes, and it fails as a `TypeError` far from the assertion |
| 4 | no `@ts-ignore` | it silences the next line ENTIRELY, including errors written later, and it outlives the problem. `@ts-expect-error <reason>` fails once the error is gone |
| 5 | no untyped parameter on a **declared** function | an implicit `any` that `noImplicitAny` reports only when it is on — and a repo migrating from JavaScript usually has it off. A callback parameter is left alone: there the type comes from the call site |
| 6 | no empty `catch` | the error is discarded and the code after the `try` runs as if nothing failed. The hardest bug class to find in production, because the only evidence was the exception that was thrown away. A catch holding only a COMMENT still counts - the comment documents the swallow, it does not handle it; `// tsgate-ok` is how a deliberate ignore is written down |
| 7 | no `==` | it coerces: `0 == ''`, `'1' == 1` and `[] == false` are all true. `== null` stays legal — that is the ecosystem idiom for "null or undefined" |
| 8 | no `var` | function-scoped and hoisted, so the name leaks out of its block and every closure in a loop shares it |
| 9 | an exported function declares its return type | inferred at the boundary, an edit inside the body silently changes the signature every caller compiled against, and the error surfaces in THEIR file |
| 10 | no `{}`, `Object` or `Function` as a type | `{}` means "anything but null", `Object` accepts every object shape, `Function` accepts any callable with any arguments. All three type-check a call the runtime rejects |
| 11 | no `namespace` / `require` | both predate ES modules: a namespace merges across files with no import to show it, and `require` is not statically analysable, so its result is unchecked and it cannot be tree-shaken |
| 12 | no call to a file-local `async` function left un-awaited | the statement returns immediately: the work runs unordered, a rejection becomes an unhandled one, and a test that passed did not wait for what it was testing. `void doWork()` is not flagged — that is the written-down "deliberately not awaited" |

Rule 12 resolves only names declared `async` **in the same file**. Without a type checker that is what can
be known, and it is the case that bites in practice.

### What is deliberately NOT here

`tsc --strict` already answers every question about types that are written down, and it is the authority on
them: `noImplicitAny`, `strictNullChecks`, `noUncheckedIndexedAccess`, `exactOptionalPropertyTypes`. Turn
them on in `tsconfig.json` — this gate does not repeat them. What it adds is the shapes people use to get
**out** of the type system, and the runtime bug classes that are legal TypeScript.

The type-aware rules that need a full checker (`no-unsafe-assignment` and the rest of that family, a
project-wide floating-promise rule) are not here either. Those want a program with every dependency loaded,
which is a linter's job — wire it in as `--plugin "npx --no-install eslint . --max-warnings 0"` and this
gate folds its verdict into the same exit code.

## No regex, anywhere

Every decision is read off the AST, off a comment RANGE the parser handed back, or off a keyword kind — and
the build refuses a pattern (the `BanRegex` target in `src/StructureGate.csproj` covers `src/TsGate/*.mjs`).
A pattern over source text would match `any` inside `company`, inside a string and inside a comment. The
line count is taken off the tree for the same reason: a bare scanner cannot tell a `/` that divides from a
`/` that opens a regex literal, and one wrong guess moves every token after it.

## How the halves talk

`src/TsGate/TsGate.mjs` and `src/TsGate/TsGate.Rules.mjs` are **embedded in the exe** and written to a temp
folder named after their content hash on first use, so a consumer still deploys two files — `structuregate.exe`
and `StructureGate.targets` — and cannot end up with a gate whose second half is missing or stale.

One node launch for the whole file set (the process start plus loading the compiler dwarfs the parse), one
line protocol back:

```
TSGATE-LINES|<rel>|<n>                 source lines, by token
TSGATE|error|<rel>|<line>|<message>    one finding
TSGATE-FATAL|<message>                 this half cannot run at all
TSGATE-DONE|<files parsed>             the receipt; its absence means the host died half way
```

The `DONE` line is why a truncated run cannot look like a clean file set, and any listed file that came back
without a count is reported by name. There is **one severity**: a finding that does not fail the build
scrolls past in a log that ends with OK.

Tested by `tests/TsGate.Tests.ps1` — 23 black-box cases over the built binary, including the same rule
firing through both compilers.
