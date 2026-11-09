# The C# rules over the deep map

Everything in [csharp-sql.md](csharp-sql.md) holds; this is what the task columns add and the rules that read them.

## The task columns

* `calls.dropped`: 1 when the call is a whole statement and nobody takes its result. `_ = Save();` takes it.
* `functions.awaits`: the function's own `await`, `await foreach` and `await using`. A lambda's or a local function's are not counted.
* `functions.body`: 0 for an abstract, interface, `extern` or `partial` declaration.
* `functions.inherited`: 1 for an `override` or an interface member's implementation, explicit or not.

Each is read off the BOUND rows: a resolved `type` is `""` where nothing bound, and an empty type matches no rule.

## The rules

| id | finds |
| --- | --- |
| `cs-task-dropped` | a call returning `Task`/`ValueTask` (or a `ConfigureAwait` of one) whose result nobody takes |
| `cs-task-not-async` | a method or local function with a body that returns a task and is not `async` |
| `cs-async-no-await` | an `async` method or local function that never awaits (CS1998) |
| `cs-task-no-cancellation` | a method returning a task with no `CancellationToken` parameter of its own; an inherited signature and `Main` are left out |
| `cs-task-ienumerable` | a method returning `Task<IEnumerable<T>>` or `ValueTask<IEnumerable<T>>`: return `IAsyncEnumerable<T>` |

`--map-query <db> --rules [FRAGMENT]` lists every rule and what it finds, whatever the config says.

`structuregate.rules.json` (beside the exe, or `--rules-config <file>`) switches each rule:

```json
{"rules": {"cs-task-dropped": "error", "cs-task-no-cancellation": "warn"}}
```

* Each rule is `off` (the default), `warn` or `error`. `true` means `error` and `false` means `off`.
* Every `--map-sqlite` run lists what a rule that is switched on finds.
* A finding of a rule at `error` makes the run exit 1.
* A config that names a rule that does not exist also makes the run exit 1.
* `// async-ok` on the line, or on the line above it, waives that line.
* Test files are left out by path, the same way `--magic` does.
