//! THE PASSES OVER THE FINISHED ROWS - SQL seeds, duplicate types, document facts, the row search index, the atlas -
//! each its own span, run after every half (`steps`), and the store's tally of what they wrote.

use super::super::protocol::Collector;
use super::*;

/// One pass over the finished rows, as a span of its own.
pub(super) fn pass(name: &str, work: impl FnOnce()) {
    let _span = crate::trace::stage(name);
    work();
}

/// Every table the store would list that nothing here has counted yet - and the two the passes REWRITE, whose
/// count from an earlier receipt is the count before they ran.
pub(super) fn tally(into: &mut Collector, db: &str) {
    let Ok(conn) = rusqlite::Connection::open(db) else { return };
    // EVERY TABLE, not only those the passes made: a half's receipt counts the tables as they stood when it wrote, before
    // the passes or the stylesheets moved them, so a refresh printed fewer rows than the tables held (#134).
    into.database.clear();
    for table in crate::rows::schema::listable_tables(&conn).unwrap_or_default() {
        let rows = crate::rows::schema::count_of(&conn, &table);
        into.database.insert(table, rows);
    }
    rows_meta(into, &conn);
}

/// `_meta.rows` AS THE `database` LINE SAYS IT, written last: the store's tally was taken before the passes made their tables.
pub(super) fn rows_meta(into: &mut Collector, conn: &rusqlite::Connection) {
    let rows: i64 = into.database.values().filter(|n| **n > 0).sum();
    if let Err(why) = crate::rows::meta::put_moved(conn, "rows", &rows.to_string()) {
        into.notes.push(format!("_meta.rows was not written - {why:#}"));
    }
}

/// WHAT THE DEPLOY SCRIPTS WRITE INTO A TABLE, walked from the SQL half's rows.
pub(super) fn seeds(into: &mut Collector, db: &str) {
    match crate::rows::seeds::run(db) {
        Err(why) => into.errors.push(format!("SQL SEEDS were not written — {why}")),
        Ok(answer) if answer.get("skipped").is_some() => {}
        Ok(answer) => {
            let counts: Vec<String> = answer["counts"].as_array().into_iter().flatten()
                .map(|c| format!("{} {}", c[1].as_i64().unwrap_or(0), c[0].as_str().unwrap_or("")))
                .collect();
            into.notes.push(format!(
                "the sql seeds: {} ({} row(s) in sql_seeds, {} typed view(s) seed_<schema>_<table>)",
                counts.join(", "),
                answer["total"].as_i64().unwrap_or(0),
                answer["views"].as_i64().unwrap_or(0)
            ));
        }
    }
}

/// A TYPE NAME DECLARED IN MORE THAN ONE PLACE - listed, never merged: see `rows/dups.rs`.
pub(super) fn duplicates(into: &mut Collector, db: &str) {
    // ONE TRANSACTION: each row its own commit was an fsync per row on a large database - tens of seconds for a few thousand names.
    let written = rusqlite::Connection::open(db).and_then(|mut conn| {
        let tx = conn.transaction()?;
        let n = crate::rows::dups::write(&tx)?;
        tx.commit()?;
        Ok(n)
    });
    match written {
        Ok(0) => {}
        Ok(n) => into.notes.push(format!("{n} type name(s) are declared in more than one place (duplicate_types)")),
        Err(why) => into.errors.push(format!("DUPLICATES were not written — {why}")),
    }
}

/// A CLASS CHOSEN BY AN ENUM MEMBER'S NAME, under a declared convention - see `rows/dispatch/enumtypes.rs` (#113).
pub(super) fn enum_types(into: &mut Collector, db: &str, specs: &[crate::rows::dispatch::csconfig::EnumTypes]) {
    let written = rusqlite::Connection::open(db).and_then(|mut conn| {
        let tx = conn.transaction()?;
        let made = crate::rows::dispatch::enumtypes::write(&tx, specs)?;
        tx.commit()?;
        Ok(made)
    });
    match written {
        Ok((n, notes)) => {
            if n > 0 {
                into.notes.push(format!("{n} enum member link(s) to the classes their names select (enum_type_links)"));
            }
            into.notes.extend(notes);
        }
        Err(why) => into.errors.push(format!("ENUM TYPE LINKS were not written — {why}")),
    }
}

/// A CLASS CHOSEN BY A NAME STRING, under a declared convention - see `rows/dispatch/nametypes.rs` (#121).
pub(super) fn name_types(into: &mut Collector, db: &str, specs: &[crate::rows::dispatch::csconfig::NameTypes]) {
    let written = rusqlite::Connection::open(db).and_then(|mut conn| {
        let tx = conn.transaction()?;
        let made = crate::rows::dispatch::nametypes::write(&tx, specs)?;
        tx.commit()?;
        Ok(made)
    });
    match written {
        Ok((n, notes)) => {
            if n > 0 {
                into.notes.push(format!("{n} name link(s) from a call's name argument to the classes it selects (name_type_links)"));
            }
            into.notes.extend(notes);
        }
        Err(why) => into.errors.push(format!("NAME TYPE LINKS were not written — {why}")),
    }
}

/// WHICH C# ACTION AN ANGULAR HTTP CALL REACHES - see `rows/http/`.
/// NOT RUN AGAIN OVER THE SAME ROWS: what it read is keyed (`rows/http/inputs.rs`), and what it counted is said again.
pub(super) fn http(into: &mut Collector, db: &str, build: &str) {
    use crate::rows::http::inputs;
    let written = rusqlite::Connection::open(db).and_then(|mut conn| {
        let key = inputs::key(&conn, build);
        if let Some(counts) = key.as_deref().and_then(|k| inputs::replayed(&conn, k)) {
            crate::trace::set("structuregate.deep.http_replayed", true);
            return Ok(counts);
        }
        let tx = conn.transaction()?;
        let counts = crate::rows::http::write(&tx)?;
        tx.commit()?;
        if let Some(key) = &key {
            // A KEY NOT KEPT is a pass run again next time, never a wrong replay.
            let _ = inputs::remember(&conn, key, &counts);
        }
        Ok(counts)
    });
    match written {
        Ok(c) if c.endpoints + c.calls == 0 => {}
        Ok(c) => into.notes.push(format!(
            "the http links: {} endpoint(s), {} call(s), {} linked, {} unmatched, {} ambiguous",
            c.endpoints, c.calls, c.linked, c.unmatched, c.ambiguous
        )),
        Err(why) => into.errors.push(format!("HTTP      links were not written — {why}")),
    }
}

/// Which handler a mediator `Send`/`Publish` runs - see `rows/dispatch.rs`.
pub(super) fn dispatch(into: &mut Collector, db: &str) {
    let written = rusqlite::Connection::open(db).and_then(|mut conn| {
        let tx = conn.transaction()?;
        let n = crate::rows::dispatch::write(&tx)?;
        tx.commit()?;
        Ok(n)
    });
    match written {
        Ok(0) => {}
        Ok(n) => into.notes.push(format!("the mediator dispatch: {n} call-to-handler row(s)")),
        Err(why) => into.errors.push(format!("DISPATCH  rows were not written — {why}")),
    }
}

/// The facts of the consumer's documents, and how each list stands against the code - see `facts/`.
pub(super) fn documented(into: &mut Collector, db: &str, facts: &crate::facts::config::Config) {
    match crate::facts::run(db, facts) {
        Err(why) => into.errors.push(format!("FACTS     were not written — {why}")),
        Ok(made) => {
            into.notes.extend(made.notes);
            let [bound, in_doc_only, in_code_only] = made.links;
            into.notes.push(format!(
                "the document facts: {} row(s) in doc_facts; doc_links {bound} bound, {in_doc_only} missing in code, {in_code_only} missing in doc",
                made.facts
            ));
        }
    }
}

/// The search index over every row, on request. A MISSING DATABASE OR A SQLITE WITHOUT FTS5 IS A NOTE: the
/// map is complete either way.
pub(super) fn search(into: &mut Collector, db: &str) {
    let receipt = match crate::rows::calls::search(Path::new(db), false) {
        Ok(receipt) => receipt,
        Err(error) => return into.errors.push(format!("SEARCH    the row index was not built — {error}")),
    };
    match serde_json::from_str::<Value>(&receipt) {
        Err(e) => into.errors.push(format!("SEARCH    the row index receipt could not be read — {e}")),
        Ok(receipt) => match receipt["skipped"].as_str() {
            Some(why) => into.notes.push(format!("the row search index was not built: {why}")),
            None => into.notes.push(format!("the row search index holds {} row(s)", receipt["rows"].as_i64().unwrap_or(0))),
        },
    }
}

/// The overview, on request. A DATABASE WITH NO ANGULAR IN IT IS A NOTE, NOT AN ERROR.
pub(super) fn atlas(into: &mut Collector, db: &str, dir: &str) {
    let receipt = match crate::rows::calls::atlas(Path::new(db), dir) {
        Ok(receipt) => receipt,
        Err(error) => return into.errors.push(format!("ATLAS     was not written — {error}")),
    };
    match serde_json::from_str::<Value>(&receipt) {
        Err(e) => into.errors.push(format!("ATLAS     receipt could not be read — {e}")),
        Ok(receipt) => match receipt["skipped"].as_str() {
            Some(why) => into.notes.push(format!("the atlas was not written: {why}")),
            None => into.notes.push(format!(
                "the atlas covers {} project(s), {} route(s) and names {} boundary(ies) -> {dir}",
                receipt["projects"].as_i64().unwrap_or(0),
                receipt["routes"].as_i64().unwrap_or(0),
                receipt["boundaries"].as_i64().unwrap_or(0)
            )),
        },
    }
}

/// A KEY FOR EVERY ENTITY, the same in every build (#130) - see `rows/keys/mod.rs`. After every other pass: it keys what they wrote.
pub(super) fn entity_keys(into: &mut Collector, db: &str, roots: &[String]) {
    let written = rusqlite::Connection::open(db).and_then(|mut conn| {
        let tx = conn.transaction()?;
        let n = crate::rows::keys::write(&tx, roots)?;
        tx.commit()?;
        Ok(n)
    });
    match written {
        Ok(n) => into.notes.push(format!("{n} entit(ies) keyed for joins across builds (entity_keys)")),
        Err(why) => into.errors.push(format!("ENTITY KEYS were not written — {why}")),
    }
}

/// WHICH SOURCE THE MAP DESCRIBES (#130): each root's commit, branch and dirty flag, the gate's version and build - and `built_at`
/// when the passes ran (`derived`). Written only where a value differs.
pub(super) fn source_state(into: &mut Collector, db: &str, roots: &[String], build: &str, derived: bool) {
    let written = rusqlite::Connection::open(db).and_then(|conn| {
        if !crate::rows::meta::exists(&conn) {
            return Ok(());
        }
        crate::rows::keys::source_state(&conn, roots, build, derived)
    });
    if let Err(why) = written {
        into.notes.push(format!("the map's source state was not kept in _meta - {why}"));
    }
}
