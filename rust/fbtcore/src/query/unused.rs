//! `--unused-imports [FRAGMENT]`: every import line in the map that nothing uses - including a RE-EXPORT
//! that no importer of the package ever takes.
//!
//! WHY A LENS OF ITS OWN. `--dead` lists a dead def with the import lines that go with it. A re-export of a
//! def that IS used - directly, from its own module - is dead just the same, and nothing named it:
//! `pkg/__init__.py` held dozens of from-imports; its own code used most, and of the rest only
//! `resolve` and the submodule `parts` were taken through the package. Finding that took a session of
//! `git grep` over multi-line imports - and a first count from the expression rows alone said far more were
//! unused, because a bare name makes no expression row. `imports.used` reads the whole AST for that reason.
//!
//! An import is LIVE when:
//!   * its file reads the name anywhere (`imports.used`: annotations, quoted annotations and `__all__`
//!     count), or it sits under an `except ImportError` (a presence probe, not a use);
//!   * or an import of it DOWNSTREAM is live - `from pkg import name` elsewhere, taking it from this file
//!     (`from_path`), followed as far as the chain goes;
//!   * or a live `import pkg` elsewhere reads `pkg.name`;
//!   * or its file is star-imported, or reached by a computed `getattr`, which may take any name.
//! AN IMPORT WHOSE HALF CANNOT SAY (`used` NULL) IS NEVER LISTED: a C# `using` in a file with no model or with errors,
//! a `global using`. Read as unused, every C# `using` of a tree was listed - all of them.
//! `from __future__` is never listed: it changes the compiler, and nothing reads it. A package's own
//! `from . import sub` takes `sub` FROM ITSELF, so an import is never its own taker - counted as one, every
//! such line in pkg/__init__.py read as a re-export pointing at itself.
//!
//! A folder the hook skips (`tests/`) and a file outside every root are not in the map. A re-export only a
//! test imports is listed here; check those before deleting the line.

use super::{columns_of, named, query, show, tables_of};
use anyhow::Result;
use rusqlite::Connection;
use std::fmt::Write as _;

const READS: &[&str] = &[
    "expressions", "assignments", "consts", "returns", "branches", "raises", "parameters", "withs", "deletes", "handlers",
];

pub(super) fn lens(db: &Connection, out: &mut String, fragment: &str, limit: usize, width: usize) -> Result<()> {
    let tables = tables_of(db);
    let has = |table: &str, column: &str| {
        tables.iter().any(|t| t == table) && columns_of(db, table).iter().any(|c| c == column)
    };
    if !has("imports", "used") {
        let _ = writeln!(out, "no import usage in this map - rebuild it with the current structuregate");
        return Ok(());
    }
    // A MAP WITH NO PYTHON OR TYPESCRIPT has no re-export to follow: a C#-only tree has neither `from_path` nor a
    // `calls.target_path`, and the lens still answers from `used` alone.
    let from_path = if has("imports", "from_path") { "i.from_path" } else { "''" };
    let star_calls = if has("calls", "target_path") && has("calls", "target_name") {
        " UNION SELECT target_path FROM calls WHERE target_name = '*'"
    } else {
        ""
    };
    // Every dotted value a file reads or calls, for `pkg.name` through a module import.
    let mut values: Vec<String> = READS
        .iter()
        .filter(|t| has(t, "reads"))
        .map(|t| format!("SELECT r.file, j.value FROM {t} r, json_each(r.reads) j"))
        .collect();
    values.push("SELECT file, callee FROM calls".to_string());

    let sql = format!(
        "WITH RECURSIVE \
         imp AS (SELECT i.id, i.file, f.path, i.line, i.module, i.name, i.level, i.used, {from_path} AS from_path, i.guarded, \
             CASE WHEN i.alias <> '' THEN i.alias WHEN i.name <> '' THEN i.name \
                  WHEN instr(i.module, '.') > 0 THEN substr(i.module, 1, instr(i.module, '.') - 1) \
                  ELSE i.module END AS local \
           FROM imports i JOIN files f ON f.id = i.file WHERE i.module <> '__future__'), \
         dotted(file, value) AS ({values}), \
         open_files(path) AS (SELECT from_path FROM imp WHERE name = '*' AND from_path <> ''{star_calls}), \
         live(id) AS ( \
           SELECT id FROM imp WHERE used = 1 OR guarded = 1 OR path IN (SELECT path FROM open_files) \
           UNION \
           SELECT i.id FROM imp i JOIN imp j ON j.from_path = i.path AND j.name = '' AND j.used = 1 \
             WHERE EXISTS (SELECT 1 FROM dotted d WHERE d.file = j.file \
               AND (d.value = j.local || '.' || i.local OR d.value GLOB j.local || '.' || i.local || '.*')) \
           UNION \
           SELECT i.id FROM live l JOIN imp j ON j.id = l.id JOIN imp i ON i.path = j.from_path AND i.local = j.name) \
         SELECT i.path, i.line, \
           CASE WHEN i.name = '' THEN 'import ' || i.module \
             ELSE 'from ' || substr('..........', 1, i.level) || i.module || ' import ' || i.name END, \
           i.local, \
           CASE WHEN EXISTS (SELECT 1 FROM imp j WHERE j.from_path = i.path AND j.name = i.local AND j.id <> i.id) \
             THEN 're-exported, but no importer uses it: ' || (SELECT group_concat(j.path || ':' || j.line, ' ') \
               FROM (SELECT path, line FROM imp j WHERE j.from_path = i.path AND j.name = i.local \
                 AND j.id <> i.id LIMIT 3) j) \
             ELSE 'never used in its file' END \
         FROM imp i WHERE i.path LIKE ?1 AND i.used IS NOT NULL AND i.id NOT IN (SELECT id FROM live) \
         ORDER BY 1, 2",
        values = values.join(" UNION ALL "),
    );
    let like = format!("%{fragment}%");
    let (_, found) = query(db, &sql, &[&like])?;
    show(out, &found, &named(&["file", "line", "import", "name", "why"]), limit, width);
    Ok(())
}
