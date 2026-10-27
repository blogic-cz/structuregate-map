//! EACH WAY OF A KEY, ONE PLAIN ROW (#53, rows 127): `key_reach_ways` - (key, ord, path, chain) - and the chains it names
//! once each, `key_reach_chains` (`own`, the ids the way holds beyond its `render_path` row's `gates`/`branches`).
//!
//! The ways were `key_reach.paths`, a list of `{path, own}` objects stored as node rows: an object, a `path` and an `own`
//! list per way and a row per id - most of the closure's node rows, while the same few chains repeat across thousands of
//! ways. A chain's id is a hash of its ids, so the same chain is the same row in every build and a partial run's delta
//! (`derive/delta.rs`) finds the rows that did not move still equal.

use super::store::{Row, Store};
use indexmap::IndexMap;
use serde_json::Value;

/// The chain's id: `ch:` and the first 16 hex digits of a hash of its ids in order.
fn chain_id(own: &[String]) -> String {
    let mut hasher = blake3::Hasher::new();
    for id in own {
        hasher.update(&(id.len() as u64).to_le_bytes());
        hasher.update(id.as_bytes());
    }
    format!("ch:{}", &hasher.finalize().to_hex()[..16])
}

/// One `key_reach_ways` row per way of `key`, in the order given; a way with no ids of its own has no chain (NULL).
/// Each chain named is kept in `chains` for `emit_chains`.
pub fn emit_ways(store: &mut Store, key: &str, ways: Vec<(i64, Vec<String>)>, chains: &mut IndexMap<String, Vec<String>>) {
    for (ord, (path, own)) in ways.into_iter().enumerate() {
        let chain = if own.is_empty() {
            Value::Null
        } else {
            let id = chain_id(&own);
            chains.entry(id.clone()).or_insert(own);
            Value::String(id)
        };
        let mut row = Row::new();
        row.insert("key".to_string(), Value::String(key.to_string()));
        row.insert("ord".to_string(), Value::from(ord as i64));
        row.insert("path".to_string(), Value::from(path));
        row.insert("chain".to_string(), chain);
        store.emit("key_reach_ways", row);
    }
}

/// One `key_reach_chains` row per chain a way named, sorted by id.
pub fn emit_chains(store: &mut Store, mut chains: IndexMap<String, Vec<String>>) {
    chains.sort_keys();
    for (id, own) in chains {
        let mut row = Row::new();
        row.insert("id".to_string(), Value::String(id));
        row.insert("own".to_string(), Value::from(own));
        store.emit("key_reach_chains", row);
    }
}
