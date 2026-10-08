//! `--dead [FRAGMENT]`: the python defs and module constants nothing in the map can reach -
//! CANDIDATES for deletion, never a verdict.
//!
//! A LENS AND NOT A RECIPE. The query this replaces lived in a skill as SQL to paste, and it was wrong
//! twice over: tens of seconds on a large map, because every def ran its own `LIKE '%name'` scan over every
//! expression, and most of its answers were live - route handlers, dunders, defs named in a dispatch
//! table - kept out of the list only where a longer name happened to end the same way. An agent handed
//! that list deletes code that runs.
//!
//! A def or a constant is REACHED when any of these holds, and each is a SET built once, never a scan
//! per row:
//!   * a call or a read is bound to it (`calls.target_path` + `target_name`, or its `file::qualname` in a
//!     `binds`), or - a def only - a `getattr` with a computed name is bound to its whole file (`*`). A
//!     CONSTANT is not spared by that: `getattr(config, item.attr)` reaches the names its callers
//!     spell as literals, which the literal rule already keeps, and sparing all of config.py hid a dead
//!     `MAX_RETRIES`. A computed `"on_" + event` names defs, which is why they keep the wildcard;
//!   * a launcher calls it (`functions.launched`);
//!   * a DOTTED decorator hands it to an object (`@app.get`, `@router.post`, `@x.setter`);
//!   * it is a dunder, which the language calls by protocol;
//!   * it is a METHOD that overrides one an ancestor defines, or whose class has an ancestor this map does
//!     not hold - a call to the base's method runs it, and a base outside the tree may call anything;
//!   * an import that is USED binds it (`imports.bind`). An import nothing in its file reads is dead code
//!     itself: several defs were kept alive by nothing else, so each candidate now names the unused imports
//!     that go with it. A RE-EXPORT needs no rule of its own - an import of it elsewhere binds through to
//!     the def, so it counts exactly when that downstream import is used;
//!   * its bare name is the LAST SEGMENT of an unbound callee, of an UNBOUND read in any of the seven
//!     tables that carry `reads`, or of a used UNBOUND import, or it is spelled by a string literal - whole
//!     (`walk.run("name")`) or after the colon of an entry point (`"cli:main"`). An `__all__`
//!     entry is a literal.
//! The last rule is deliberately generous: a name the map cannot bind stays alive rather than guessed.
//!
//! A TABLE IS THERE ONLY WHEN A ROW FILLED IT - the schema is derived from the rows - so a tree with no
//! `raise` has no `raises`. Each set is built from the tables this database has, never assumed.

use super::{columns_of, named, query, show, tables_of};
use anyhow::Result;
use rusqlite::Connection;
use std::fmt::Write as _;

/// The part after the last `.`, in SQL: `rtrim` strips everything that is not a dot from the right.
const TAIL: &str = "substr(V, length(rtrim(V, replace(V, '.', ''))) + 1)";

const READS: &[&str] = &[
    "expressions", "assignments", "consts", "returns", "branches", "raises", "parameters", "withs", "deletes", "handlers",
];

/// The part before the first `.`: the name a read or a callee starts from.
const HEAD: &str = "CASE WHEN instr(V, '.') > 0 THEN substr(V, 1, instr(V, '.') - 1) ELSE V END";

/// No rows: what an absent table contributes to a set.
const NONE: &str = "SELECT NULL WHERE 0";

pub(super) fn lens(db: &Connection, out: &mut String, fragment: &str, limit: usize, width: usize) -> Result<()> {
    let tables = tables_of(db);
    let has = |table: &str, column: &str| {
        tables.iter().any(|t| t == table) && columns_of(db, table).iter().any(|c| c == column)
    };
    if !has("functions", "launched") || !has("calls", "target_path") {
        let _ = writeln!(out, "no bound calls in this map - rebuild it with the current structuregate");
        return Ok(());
    }

    let mut names = vec![format!("SELECT {} FROM calls WHERE target_path = ''", TAIL.replace('V', "callee"))];
    if has("string_literals", "value") {
        names.push("SELECT value FROM string_literals".to_string());
        // `"cli:main"` in a step table is python's own entry-point syntax, and the def is the
        // part after the colon - a whole-string match listed every one of those entry points as dead.
        names.push(format!(
            "SELECT {} FROM (SELECT substr(value, instr(value, ':') + 1) AS attr FROM string_literals \
             WHERE instr(value, ':') > 0)",
            TAIL.replace('V', "attr")
        ));
    }
    // AN IMPORT REACHES WHAT IT BINDS: `from scanner import SKIP_DIRS` that never uses it still
    // fails the day SKIP_DIRS goes. Bound, it reaches that one symbol; unbound, it protects by name.
    let imports_bound = has("imports", "bind");
    if has("imports", "name") {
        names.push(if imports_bound {
            // ONLY WHERE THE MODULE COULD BE IN THIS TREE. `from extlib import make_id as
            // _w` names an EXTERNAL package's function; by name it kept the file's own dead
            // `make_id` alive. An unbound import of a module some mapped file is named after
            // is a path this map could not follow, and that one still protects by name.
            format!(
                "SELECT name FROM used_imports WHERE name <> '' AND bind = '' \
                 AND {} IN (SELECT module FROM files)",
                TAIL.replace('V', "module")
            )
        } else {
            "SELECT name FROM imports WHERE name <> ''".to_string()
        });
    }
    // WHAT EACH FILE USES, by the first segment of every read and callee: the name an import binds in it.
    let mut uses: Vec<String> = READS
        .iter()
        .filter(|t| has(t, "reads"))
        .map(|t| format!("SELECT r.file, {} FROM {t} r, json_each(r.reads) j", HEAD.replace('V', "j.value")))
        .collect();
    uses.push(format!("SELECT file, {} FROM calls", HEAD.replace('V', "callee")));
    let used_imports = if imports_bound {
        "SELECT i.* FROM imports i WHERE EXISTS (SELECT 1 FROM uses u WHERE u.file = i.file \
         AND u.head = CASE WHEN i.alias <> '' THEN i.alias ELSE i.name END)"
    } else {
        "SELECT * FROM imports WHERE 0"
    };
    // A BOUND READ REACHES ITS SYMBOL AND NOTHING ELSE. `binds` holds, position for position, the
    // `file::qualname` each read names; only a read the map could not place still protects by NAME.
    // Counted by name, `run()` in main.py kept a dead `legacy.py::run` alive for as long as anything called
    // ANY `run` - which is the bare-name join binding exists to replace.
    let reads: Vec<String> = READS
        .iter()
        .filter(|t| has(t, "reads") && has(t, "binds"))
        .map(|t| format!("SELECT reads, binds FROM {t}"))
        .collect();
    let mut bound = vec!["SELECT DISTINCT target_path, target_name FROM calls WHERE target_path <> ''".to_string()];
    if !reads.is_empty() {
        let rows = reads.join(" UNION ALL ");
        names.push(format!(
            "SELECT {} FROM ({rows}) r, json_each(r.reads) j, json_each(r.binds) b \
             WHERE b.key = j.key AND b.value = ''",
            TAIL.replace('V', "j.value")
        ));
        bound.push(format!(
            "SELECT substr(b.value, 1, instr(b.value, '::') - 1), substr(b.value, instr(b.value, '::') + 2) \
             FROM ({rows}) r, json_each(r.binds) b WHERE b.value <> ''"
        ));
    }
    if imports_bound {
        bound.push(
            "SELECT substr(bind, 1, instr(bind, '::') - 1), substr(bind, instr(bind, '::') + 2) \
             FROM used_imports WHERE bind <> ''"
                .to_string(),
        );
    }
    let registered = if has("decorators", "target") {
        "SELECT DISTINCT file, target FROM decorators WHERE instr(name, '.') > 0"
    } else {
        "SELECT NULL, NULL WHERE 0"
    };

    // THE CLASS TREE, from the BOUND bases. `parents` is every resolved edge; `outside` is every class with
    // a base the map does not hold (`object` excepted, which defines nothing a method overrides by name).
    let (parents, outside) = if has("classes", "bases_bind") {
        (
            "SELECT f.path || '::' || k.qualname, bb.value FROM classes k JOIN files f ON f.id = k.file, \
             json_each(k.bases_bind) bb WHERE bb.value <> ''",
            "SELECT f.path || '::' || k.qualname FROM classes k JOIN files f ON f.id = k.file, \
             json_each(k.bases) b, json_each(k.bases_bind) bb \
             WHERE b.key = bb.key AND bb.value = '' AND b.value <> 'object'",
        )
    } else {
        ("SELECT NULL, NULL WHERE 0", NONE)
    };
    // The class a method sits in, as `file::qualname`: its own qualname less the last segment.
    let owner = "f.path || '::' || substr(fn.qualname, 1, length(fn.qualname) - length(fn.name) - 1)";

    // THE IMPORTS THAT GO WITH IT: every `file:line` importing the candidate, none of them used.
    let also = |symbol: &str| {
        if imports_bound {
            format!(
                "coalesce((SELECT group_concat(g.path || ':' || i.line, ' ') FROM imports i \
                 JOIN files g ON g.id = i.file WHERE i.bind = {symbol}), '')"
            )
        } else {
            "''".to_string()
        }
    };
    let consts = if has("consts", "name") {
        format!("UNION ALL SELECT f.path, k.line, 'const', {also}, k.name FROM consts k JOIN files f ON f.id = k.file \
         WHERE f.path LIKE ?1 AND k.func = '' AND k.cls = '' \
           AND NOT EXISTS (SELECT 1 FROM bound b WHERE b.path = f.path AND b.qual = k.name) \
           AND k.name NOT IN (SELECT name FROM named WHERE name IS NOT NULL)",
            also = also("f.path || '::' || k.name"))
    } else {
        String::new()
    };

    let sql = format!(
        "WITH RECURSIVE uses(file, head) AS ({uses}), \
         used_imports AS ({used_imports}), \
         named(name) AS ({names}), \
         bound(path, qual) AS ({bound}), \
         wild(path) AS (SELECT DISTINCT target_path FROM calls WHERE target_name = '*'), \
         registered(file, name) AS ({registered}), \
         parents(child, parent) AS ({parents}), \
         outside(cls) AS ({outside}), \
         ancestors(child, anc) AS (SELECT child, parent FROM parents \
           UNION SELECT a.child, p.parent FROM ancestors a JOIN parents p ON p.child = a.anc), \
         defs(key) AS (SELECT f.path || '::' || fn.qualname FROM functions fn JOIN files f ON f.id = fn.file) \
         SELECT f.path, fn.line, 'def', {also_def}, fn.qualname FROM functions fn JOIN files f ON f.id = fn.file \
         WHERE f.path LIKE ?1 AND fn.launched = 0 \
           AND fn.name NOT LIKE '\\_\\_%\\_\\_' ESCAPE '\\' \
           AND f.path NOT IN (SELECT path FROM wild) \
           AND NOT EXISTS (SELECT 1 FROM bound b WHERE b.path = f.path AND b.qual = fn.qualname) \
           AND NOT EXISTS (SELECT 1 FROM registered g WHERE g.file = fn.file AND g.name = fn.name) \
           AND fn.name NOT IN (SELECT name FROM named WHERE name IS NOT NULL) \
           AND NOT (fn.cls <> '' AND fn.func = '' AND ( \
             {owner} IN (SELECT cls FROM outside) \
             OR EXISTS (SELECT 1 FROM ancestors a WHERE a.child = {owner} \
               AND (a.anc IN (SELECT cls FROM outside) OR a.anc || '.' || fn.name IN (SELECT key FROM defs))))) \
         {consts} \
         ORDER BY 1, 2",
        names = names.join(" UNION "),
        bound = bound.join(" UNION "),
        uses = uses.join(" UNION "),
        also_def = also("f.path || '::' || fn.qualname"),
    );
    let like = format!("%{fragment}%");
    let (_, found) = query(db, &sql, &[&like])?;
    show(out, &found, &named(&["file", "line", "kind", "unused imports", "name"]), limit, width);
    Ok(())
}
