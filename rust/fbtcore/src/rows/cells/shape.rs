//! A NODE TABLE'S SHAPE INDEX - `(role, parent, owner, n)`, which answers a shape across every owner (each root `$ref`, each
//! `$template` list, #71). Every node table has one but a CLOSURE table's (`tsderive::CLOSURE_TABLES`, divergence #109): every
//! reader of those starts from its row (`owner`, the key) or from a value, and the ways' shape index alone (then
//! `key_reach__paths`, #110) was a tenth of a full map's store. A consumer whose query does need it asks for it once (`--map-index <db> <table>`): the request
//! is kept in `_meta` (`index:<child table>`), so every later write makes it again - a full run included.

use super::super::schema;
use anyhow::{bail, Result};
use rusqlite::Connection;

const KEY: &str = "index:";

/// The statement that gives `child_table` (a node table of `table`) its shape index, or None when it goes without.
pub fn shape_index(db: &Connection, table: &str, child_table: &str) -> Option<String> {
    if crate::rows::tsderive::CLOSURE_TABLES.contains(&table) && super::super::meta::get(db, &format!("{KEY}{child_table}")).is_none() {
        return None;
    }
    Some(role_index(child_table))
}

fn role_index(child_table: &str) -> String {
    let t = schema::escape(child_table);
    format!("CREATE INDEX IF NOT EXISTS \"ix_{t}_role\" ON \"{t}\" (role, parent, owner, n)")
}

/// `--map-index <db> <table>`: the shape index of one closure node table, made now and on every later write. A table that
/// already has one, or is no node table of the map, is said so.
pub fn request(db: &Connection, child_table: &str) -> Result<String> {
    let table: Option<String> = db
        .query_row(&format!("SELECT tbl FROM {} WHERE child = ?1", super::REGISTRY), [child_table], |r| r.get(0))
        .ok();
    let Some(table) = table else {
        bail!("{child_table} is no node table of this map (`--map-query <db> --tables` lists them: <table>__<column>)");
    };
    if !crate::rows::tsderive::CLOSURE_TABLES.contains(&table.as_str()) {
        return Ok(format!("{child_table} already has its shape index: only a closure table's node tables go without"));
    }
    super::super::meta::put(db, &format!("{KEY}{child_table}"), "role")?;
    let made = role_index(child_table);
    super::super::schema::sorted_on_every_core(db, 2000, || Ok(db.execute_batch(&made)?))?;
    Ok(format!("{child_table}: shape index (role, parent, owner, n) made, and kept on every later write"))
}
