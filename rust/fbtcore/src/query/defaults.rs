//! `--defaults [FRAGMENT]`: for every python parameter with a default, whether any caller ever passes it.
//!
//! ONE OF FOUR ANSWERS, and the third is the honest one more often than a reader would like:
//!   * `passed`       - at least one bound call fills the parameter; the sites and the values are listed;
//!   * `never passed` - the def has bound callers, none fills it, and nothing else could reach it;
//!   * `cannot tell`  - something the map cannot see may pass it: a call it could not bind that carries the
//!     def's name, a `*args`/`**kwargs` splat at a bound call, a computed `getattr` over the def's file, a
//!     decorator handing the def to an object (`@app.get` - a web framework fills the defaults from the
//!     request), a launcher, or the dunder protocol. The reason is printed;
//!   * `no caller`    - nothing in the map calls the def at all (see `--dead`).
//!
//! `bound` IS THE SAME QUESTION OVER THE BOUND CALLS ALONE - `passed`, `never passed` or `no caller` - so a
//! `cannot tell` that is only a same-named call elsewhere still says what the calls that ARE this def do.
//! `store.open(path, mode)` had both of its real callers bound and neither passing `mode`, and some unbound
//! `open` calls in other files hid that behind `cannot tell`.
//!
//! `never passed` IS A CLAIM ABOUT THIS MAP. A caller in a folder the hook skips (`tests/`) or outside every
//! root is not in it, which is why the reason for every other verdict is spelled out rather than implied.
//!
//! The join is `arguments.param`, the parameter the CALLEE declares, written by PyArgs for bound calls only.

use super::{columns_of, named, query, show, tables_of};
use anyhow::Result;
use rusqlite::Connection;
use std::fmt::Write as _;

/// The part after the last `.`, in SQL: `rtrim` strips everything that is not a dot from the right.
const TAIL: &str = "substr(V, length(rtrim(V, replace(V, '.', ''))) + 1)";

pub(super) fn lens(db: &Connection, out: &mut String, fragment: &str, limit: usize, width: usize) -> Result<()> {
    let tables = tables_of(db);
    let has = |table: &str, column: &str| {
        tables.iter().any(|t| t == table) && columns_of(db, table).iter().any(|c| c == column)
    };
    if !has("arguments", "param") || !has("parameters", "default_expr") || !has("functions", "launched") {
        let _ = writeln!(out, "no argument rows in this map - rebuild it with the current structuregate");
        return Ok(());
    }
    let registered = if has("decorators", "target") {
        "SELECT DISTINCT d.file, d.target, d.name FROM decorators d WHERE instr(d.name, '.') > 0"
    } else {
        "SELECT NULL, NULL, NULL WHERE 0"
    };

    let sql = format!(
        "WITH params(pid, file, path, qual, fname, callname, name, dflt, line) AS ( \
           SELECT p.id, p.file, f.path, p.qualname, p.func, \
             CASE WHEN p.func = '__init__' \
               THEN {class_tail} ELSE p.func END, \
             p.name, p.default_expr, p.line \
           FROM parameters p JOIN files f ON f.id = p.file \
           WHERE p.default_expr <> '' AND p.kind IN ('positional', 'keyword') AND f.path LIKE ?1), \
         reaching(pid, call, site) AS ( \
           SELECT pr.pid, c.id, cf.path || ':' || c.line FROM params pr \
           JOIN calls c ON c.target_path = pr.path \
             AND (c.target_name = pr.qual OR (pr.fname = '__init__' AND c.target_name || '.__init__' = pr.qual)) \
           JOIN files cf ON cf.id = c.file), \
         passing(pid, site, value) AS ( \
           SELECT r.pid, r.site, CASE WHEN a.value <> '' THEN a.value ELSE a.source END \
           FROM reaching r JOIN params pr ON pr.pid = r.pid JOIN arguments a ON a.call = r.call AND a.param = pr.name), \
         splat(pid) AS (SELECT DISTINCT r.pid FROM reaching r JOIN arguments a ON a.call = r.call AND a.star <> ''), \
         tails(name, n) AS (SELECT {callee_tail}, count(*) FROM calls WHERE target_path = '' GROUP BY 1), \
         wild(path) AS (SELECT DISTINCT target_path FROM calls WHERE target_name = '*'), \
         registered(file, target, name) AS ({registered}), \
         judged AS ( \
           SELECT pr.path, pr.line, pr.qual, pr.name, pr.dflt, \
             (SELECT count(DISTINCT call) FROM reaching r WHERE r.pid = pr.pid) AS calls, \
             (SELECT count(*) FROM passing s WHERE s.pid = pr.pid) AS passed, \
             (SELECT group_concat(site || '=' || value, ', ') FROM \
               (SELECT site, value FROM passing s WHERE s.pid = pr.pid LIMIT 4)) AS sites, \
             CASE \
               WHEN pr.fname LIKE '\\_\\_%\\_\\_' ESCAPE '\\' AND pr.fname <> '__init__' THEN 'a dunder, called by protocol' \
               WHEN EXISTS (SELECT 1 FROM functions fn WHERE fn.file = pr.file AND fn.qualname = pr.qual AND fn.launched = 1) \
                 THEN 'a launcher calls it' \
               WHEN EXISTS (SELECT 1 FROM registered g WHERE g.file = pr.file AND g.target = pr.fname) \
                 THEN 'handed to @' || (SELECT g.name FROM registered g WHERE g.file = pr.file AND g.target = pr.fname LIMIT 1) \
               WHEN pr.path IN (SELECT path FROM wild) THEN 'a computed getattr reaches its file' \
               WHEN pr.pid IN (SELECT pid FROM splat) THEN 'a bound call splats *args or **kwargs' \
               WHEN (SELECT n FROM tails t WHERE t.name = pr.callname) > 0 \
                 THEN (SELECT n FROM tails t WHERE t.name = pr.callname) || ' unbound call(s) named ' || pr.callname \
               ELSE '' END AS unseen \
           FROM params pr) \
         SELECT path, line, qual, name, dflt, \
           CASE WHEN passed > 0 THEN 'passed' WHEN unseen <> '' THEN 'cannot tell' \
                WHEN calls = 0 THEN 'no caller' ELSE 'never passed' END, \
           CASE WHEN passed > 0 THEN 'passed' WHEN calls = 0 THEN 'no caller' ELSE 'never passed' END, \
           calls, \
           CASE WHEN passed > 0 THEN passed || 'x: ' || sites WHEN unseen <> '' THEN unseen ELSE '' END \
         FROM judged ORDER BY 1, 2",
        class_tail = TAIL.replace('V', "substr(p.qualname, 1, length(p.qualname) - 9)"),
        callee_tail = TAIL.replace('V', "callee"),
    );
    let like = format!("%{fragment}%");
    let (_, found) = query(db, &sql, &[&like])?;
    show(out, &found, &named(&["file", "line", "def", "param", "default", "verdict", "bound", "calls", "detail"]), limit, width);
    Ok(())
}
