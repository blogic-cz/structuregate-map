//! `--magic [FRAGMENT]`: the literals that carry meaning nothing names - CANDIDATES for a named constant,
//! never a verdict. Four sections, over every half that writes `string_literals` / `number_literals` with a
//! `use` (python, C#, both TypeScript halves, rust):
//!
//!   * MAGIC NUMBERS - each number in code that is not a constant's own value, a parameter default or a format
//!     argument. `0`, `1`, `-1` and `2` are left out: they are counts, flags, ends and halves far more often
//!     than they are a decision somebody should have named.
//!   * MAGIC STRINGS - ONE ROW PER VALUE: a short single-word string (up to 64 characters, no whitespace) that is
//!     COMPARED, or written three times or more, or spells the value of a declared constant (named beside it:
//!     the literal should have been the name). Prose has a space in it and is not listed. Per occurrence, a
//!     schema-heavy tree answered with thousands of rows of `"id"` and `"name"`; per value it is a list a
//!     reader can act on, with how often, in how many files, how often compared, and where it first appears.
//!   * SPLIT POSITIONS - each split whose RESULT is taken by a literal position (`line.split(":")[3]`,
//!     `.nth(7)`): a format parsed by position. `0`, `1` and `-1` - head, second, tail - are the ordinary shapes.
//!   * SEPARATORS - each split or join separator, one row per method and separator. One longer than four
//!     characters is not one: `path.join("package.json")` is a path, and no type says which `join` it is.
//!
//! JavaScript's `typeof x === 'string'` is an idiom, not a magic string: the TypeScript halves mark it `typeof`.
//!
//! TEST CODE IS LEFT OUT: a rust row marked `test`, and any file under a `test`/`tests` folder or named like
//! a test (`*_test.*`, `test_*`, `*.spec.*`, `*.test.*`, `*Tests.cs`). FRAGMENT narrows to the matching paths.

use super::{columns_of, named, query, show, tables_of};
use anyhow::Result;
use rusqlite::Connection;
use std::fmt::Write as _;

/// The callees a separator is handed to, lowercased: python, C#, TypeScript and rust spell them alike.
const SEPARATED: &str = "'split', 'rsplit', 'splitn', 'rsplitn', 'split_once', 'rsplit_once', 'split_terminator', \
                         'join', 'partition', 'rpartition'";

/// The methods that take one element of a sequence by position, the way `[n]` does.
const POSITIONAL: &str = "'nth', 'elementat', 'get', 'at'";

/// `f.path` is not test code - see the module doc.
const NOT_TEST_PATH: &str = "NOT (('/' || lower(f.path)) GLOB '*/test/*' OR ('/' || lower(f.path)) GLOB '*/tests/*' \
    OR lower(f.path) GLOB '*.tests/*' OR lower(f.path) GLOB '*_test.*' OR ('/' || lower(f.path)) GLOB '*/test_*' \
    OR lower(f.path) GLOB '*.spec.*' OR lower(f.path) GLOB '*.test.*' OR lower(f.path) GLOB '*tests.cs')";

pub(super) fn lens(db: &Connection, out: &mut String, fragment: &str, limit: usize, width: usize) -> Result<()> {
    let tables = tables_of(db);
    let has = |table: &str, column: &str| tables.iter().any(|t| t == table) && columns_of(db, table).iter().any(|c| c == column);
    let numbers = has("number_literals", "use");
    let strings = has("string_literals", "use");
    if !numbers && !strings {
        let _ = writeln!(out, "no literal uses in this map - rebuild it with the current structuregate");
        return Ok(());
    }
    let not_test = |alias: &str, table: &str| {
        let flag = if has(table, "test") { format!("coalesce({alias}.test, 0) = 0") } else { "1".to_string() };
        format!("{flag} AND {NOT_TEST_PATH}")
    };
    let like = format!("%{fragment}%");
    // The split-then-index shape: a literal position into whatever a split produced.
    let split_index = format!("(n.use = 'index' OR (n.use = 'argument' AND lower(n.callee) IN ({POSITIONAL}))) \
                               AND lower(n.target) GLOB '*split*'");

    if numbers {
        // The Angular half's rows carry no scope: a column only some halves write is asked for only when it is there.
        let func = if has("number_literals", "func") { "coalesce(n.func, '')" } else { "''" };
        let sql = format!(
            "SELECT f.path, n.line, n.value, n.use, n.callee, {func} FROM number_literals n \
             JOIN files f ON f.id = n.file WHERE f.path LIKE ?1 AND {not_test} \
               AND n.use NOT IN ('declared', 'default', 'format') AND CAST(n.number AS REAL) NOT IN (0, 1, -1, 2) \
               AND NOT ({split_index}) ORDER BY 1, 2",
            not_test = not_test("n", "number_literals"),
        );
        section(db, out, "MAGIC NUMBERS", &sql, &like, &["file", "line", "value", "use", "callee", "func"], limit, width)?;
    }
    if strings {
        let not_test = not_test("s", "string_literals");
        let sql = format!(
            "WITH cand AS (SELECT s.file, s.line, s.value, s.use, f.path FROM string_literals s \
               JOIN files f ON f.id = s.file WHERE f.path LIKE ?1 AND {not_test} \
                 AND s.use NOT IN ('declared', 'doc', 'format', 'default', 'typeof') AND length(s.value) <= 64 \
                 AND instr(s.value, ' ') = 0 AND instr(s.value, char(9)) = 0 AND instr(s.value, char(10)) = 0 \
                 AND NOT (s.use IN ('argument', 'receiver') AND lower(s.callee) IN ({SEPARATED}))), \
             declared AS (SELECT s.value, min(k.name) AS name FROM string_literals s \
               JOIN consts k ON k.file = s.file AND k.line = s.line WHERE s.use = 'declared' GROUP BY s.value), \
             grouped AS (SELECT value, count(*) AS uses, count(DISTINCT file) AS files, \
               sum(use = 'compare') AS compared, min(path || ':' || line) AS first FROM cand GROUP BY value) \
             SELECT g.value, g.uses, g.files, g.compared, coalesce(d.name, ''), g.first FROM grouped g \
             LEFT JOIN declared d ON d.value = g.value \
             WHERE g.compared > 0 OR g.uses >= 3 OR d.name IS NOT NULL \
             ORDER BY g.uses DESC, g.value"
        );
        section(db, out, "MAGIC STRINGS", &sql, &like, &["value", "uses", "files", "compared", "constant", "first"], limit, width)?;
    }
    if numbers {
        let sql = format!(
            "SELECT f.path, n.line, n.value, n.target FROM number_literals n JOIN files f ON f.id = n.file \
             WHERE f.path LIKE ?1 AND {} AND {split_index} AND CAST(n.number AS REAL) NOT IN (0, 1, -1) ORDER BY 1, 2",
            not_test("n", "number_literals")
        );
        section(db, out, "SPLIT POSITIONS", &sql, &like, &["file", "line", "position", "of"], limit, width)?;
    }
    if strings {
        let sql = format!(
            "SELECT s.callee, quote(s.value), count(*), count(DISTINCT s.file), min(f.path || ':' || s.line) \
             FROM string_literals s JOIN files f ON f.id = s.file WHERE f.path LIKE ?1 AND {} \
               AND s.use IN ('argument', 'receiver') AND lower(s.callee) IN ({SEPARATED}) AND length(s.value) <= 4 \
             GROUP BY s.callee, s.value ORDER BY 3 DESC, 1, 2",
            not_test("s", "string_literals")
        );
        section(db, out, "SEPARATORS", &sql, &like, &["method", "separator", "uses", "files", "first"], limit, width)?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn section(db: &Connection, out: &mut String, title: &str, sql: &str, like: &str, columns: &[&str], limit: usize, width: usize) -> Result<()> {
    let (_, found) = query(db, sql, &[&like])?;
    let _ = writeln!(out, "{title} ({})", found.len());
    show(out, &found, &named(columns), limit, width);
    let _ = writeln!(out);
    Ok(())
}
