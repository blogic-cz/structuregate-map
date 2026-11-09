//! THE C# RULES: tasks, read off the BOUND rows (`files.semantic = 1`) - a resolved type is `""` where nothing bound,
//! and an empty type matches no rule, so a file without a compilation is never accused. The columns they read are
//! written by `src/Map/CsRows`: `calls.dropped` (the call is a whole statement), `functions.awaits` (its own
//! awaits), `functions.body` and `functions.inherited` (an override or an interface member's implementation).

use super::{Rule, NOT_TEST_PATH};

/// A type `t` that is a task: `Task`, `ValueTask`, generic or not, as Roslyn names them.
fn task(t: &str) -> String {
    format!("({t} = 'System.Threading.Tasks.Task' OR {t} GLOB 'System.Threading.Tasks.Task<*' \
             OR {t} = 'System.Threading.Tasks.ValueTask' OR {t} GLOB 'System.Threading.Tasks.ValueTask<*')")
}

/// The functions of the C# files that are not test code, aliased `fn`.
fn functions(kinds: &str, also: &str) -> String {
    format!("SELECT f.path AS path, fn.line AS line, fn.qualname AS name, fn.type AS type FROM functions fn \
             JOIN files f ON f.id = fn.file WHERE f.lang = 'csharp' AND fn.kind IN ({kinds}) AND {NOT_TEST_PATH} AND {also}")
}

fn dropped() -> String {
    format!("SELECT f.path AS path, c.line AS line, c.callee AS name, c.type AS type FROM calls c \
             JOIN files f ON f.id = c.file WHERE f.lang = 'csharp' AND c.dropped = 1 AND {NOT_TEST_PATH} \
             AND ({} OR c.type GLOB 'System.Runtime.CompilerServices.Configured*TaskAwaitable*')", task("c.type"))
}

fn not_async() -> String {
    functions("'method', 'local'", &format!("fn.body = 1 AND fn.is_async = 0 AND {}", task("fn.type")))
}

fn never_awaits() -> String {
    functions("'method', 'local'", "fn.body = 1 AND fn.is_async = 1 AND fn.awaits = 0")
}

/// A parameter of the function's own declaration: its qualname, inside its lines (an overload shares the qualname).
fn no_token() -> String {
    functions("'method'", &format!("{} AND fn.inherited = 0 AND fn.name <> 'Main' AND NOT EXISTS (SELECT 1 FROM parameters p \
               WHERE p.file = fn.file AND p.qualname = fn.qualname AND p.line BETWEEN fn.line AND fn.end_line \
               AND p.type GLOB 'System.Threading.CancellationToken*')", task("fn.type")))
}

fn enumerable() -> String {
    functions("'method', 'local'", "fn.inherited = 0 AND (fn.type GLOB 'System.Threading.Tasks.Task<System.Collections.Generic.IEnumerable<*>>' \
               OR fn.type GLOB 'System.Threading.Tasks.ValueTask<System.Collections.Generic.IEnumerable<*>>')")
}

const FUNCTIONS: &[(&str, &str)] = &[("functions", "type"), ("functions", "body"), ("functions", "inherited")];

pub(super) const RULES: &[Rule] = &[
    Rule {
        id: "cs-task-dropped",
        says: "a task nobody awaits - its exception reaches no one",
        fix: "await it, or write `_ = ` to drop it on purpose",
        needs: &[("calls", "dropped"), ("calls", "type")],
        sql: dropped,
    },
    Rule {
        id: "cs-task-not-async",
        says: "returns a task but is not async",
        fix: "make it async and await what it returns",
        needs: FUNCTIONS,
        sql: not_async,
    },
    Rule {
        id: "cs-async-no-await",
        says: "async but never awaits - it runs synchronously (CS1998)",
        fix: "await the work, or drop async and return a completed task",
        needs: &[("functions", "awaits"), ("functions", "body")],
        sql: never_awaits,
    },
    Rule {
        id: "cs-task-no-cancellation",
        says: "returns a task but takes no CancellationToken",
        fix: "add a CancellationToken parameter and pass it on",
        needs: &[("functions", "type"), ("functions", "inherited"), ("parameters", "type")],
        sql: no_token,
    },
    Rule {
        id: "cs-task-ienumerable",
        says: "returns a task of IEnumerable",
        fix: "return IAsyncEnumerable<T> and yield each item as it arrives",
        needs: FUNCTIONS,
        sql: enumerable,
    },
];
